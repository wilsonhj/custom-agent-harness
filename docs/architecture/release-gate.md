# Release path and migration gate

Status: Design (Phase 1). States: `harness/policy/transitions.ts`. Approvals:
`docs/architecture/approval-ledger.md`.

## Release path

```text
Verified -> PreviewVerified -> MigrationGated -> Released -> Observed -> Closed
```

`MigrationGated` sits between `PreviewVerified` and `Released`. Every release passes
through it, including releases with no schema change, so the step cannot be skipped
by mislabelling a change. A change without a migration passes with a "no migration"
record (see below).

## Why a separate gate

On Vercel (and similar platforms):

- A git-triggered auto-deploy builds and serves the new code; it **does not run
  database migrations**. Code that expects a column the database lacks ships anyway.
- If the application crashes at boot (e.g. a startup query against a missing column),
  **every route fails, including `/healthz`**, so health checks cannot distinguish
  "bad migration ordering" from a total outage.
- An **application rollback does not reverse schema changes**. Promoting the previous
  deployment brings back old code against the new schema; any data a migration
  dropped or rewrote stays that way.

Preview verification proves the new code works on a preview database. It does not prove
the production migration will be applied in the right order, or that the old code
survives the new schema during the deploy window. The gate checks exactly that.

## Required evidence for `PreviewVerified -> MigrationGated`

Each item is an `evidence_refs` entry on the ledger record.

| # | Evidence | Satisfied by |
|---|---|---|
| 1 | Migration plan reviewed | `design.md` "Migration plan" section (expand/contract, ordering vs deploy, backward compatibility, rollback limits) with a recorded review finding set and resolutions. |
| 2 | Migration applied and verified in preview | Command output showing the migration ran against the preview database, plus a schema check (e.g. column/index present) and the preview smoke tests run *after* it. |
| 3 | Backward compatibility | Test run of the **currently released** commit against the **new** schema (the state production is in between migration and deploy, and after any rollback). |
| 4 | Recovery procedure | Written steps for: migration fails midway; new code fails after migration; rollback needed. States which steps are forward-fixes because the schema cannot be rolled back, and the restore point (backup/snapshot ID placeholder) if data is changed. |

### No-migration releases: machine-checked, fail-closed

A release skips items 2-4 only when a deterministic classifier, not an agent or a
person's label, reports the diff as `clean`:

- The classifier is a project-specific command (`<migration-classifier-command>`) that
  inspects the diff between the released commit and `target.commit_sha`. At minimum it
  checks `<migrations-dir>`, ORM schema/model files (`<schema-paths>`), and SQL or
  data-rewrite statements in any other changed file (backfills, seed scripts, one-off jobs).
- It returns exactly one of `clean`, `migration`, or `uncertain`, with the paths and
  rules that produced the result. Its output and version are the evidence ref.
- **`uncertain`, a crash, or a missing classifier is treated as `migration`**: the full
  evidence (items 1-4) is required. There is no "no migration" path without a `clean`
  result.
- A `policy` actor may approve `PreviewVerified -> MigrationGated` only on a `clean`
  result. Any `migration` or `uncertain` result needs a `human` actor.

A wrong `clean` result is a classifier bug: fix the classifier and add the diff as a
regression case under `evaluations/`.

## Required evidence for `MigrationGated -> Released`

- The production migration has been applied **before** the code deploy (or the plan
  documents why a different order is safe), with command output.
- The ledger `target` names the exact `commit_sha` and `deployment_target`
  (`<provider>/<project>/production`). Approval for one target does not cover another.
- Release evidence from the README "Release evidence" list.

## Failure handling

- Gate evidence missing or failing: transition to `Repair` (fix and re-run preview) or
  `Blocked` (waiting on a human decision).
- Production migration applied but release fails: do not roll back the schema
  reflexively; follow the recovery procedure from item 4. Record the outcome as a
  transition to `Failed` with the evidence, then `Failed -> Repair`. `Released -> Repair`
  is not an edge. `Failed` is allowed from `Implementing`, `Reviewing`, `Repair`,
  `Verified`, `PreviewVerified`, `MigrationGated`, `Released`, and `Observed`.

## Project profile: canary-backend (researched 2026-10-26)

Drizzle ORM on PostgreSQL (Neon), deployed to Vercel. Read-only inspection of `main`.

**Existing controls the gate should require evidence from, not rebuild:**

- Git auto-deploy is off for `main` (`vercel.json`); `scripts/vercel-build.ts` never touches
  the database.
- `.github/workflows/deploy-staging.yml` and `deploy-production.yml` run, in order:
  `pnpm assert-prod-schema` (static), `pnpm db:migrate`, `pnpm assert-prod-schema:live`,
  then `vercel deploy --prod`. Gate evidence item 2 and the `MigrationGated -> Released`
  "migration before deploy" item can be satisfied by that workflow run's logs.
- `assert-prod-schema` is a PHI-coverage check (classified tables, ciphertext columns), not a
  full schema-correctness check. `src/db/assert-schema-current.ts` is a boot gate that
  compares migration *counts* only.
- `docs/deploy-production.md` §3: migrations are forward-only, expand → migrate → contract,
  and Instant Rollback must not cross a migration boundary.

**Proposed classifier rules:**

| Result | Trigger |
|---|---|
| `migration` | Any change under `src/db/migrations/**`, `src/db/schema.ts`, or `drizzle.config.ts`. A *modification* of an existing migration file also raises a hard warning (see gap 1). |
| `uncertain` (paths) | `scripts/migrate.ts`, `scripts/phi-backfill*.ts`, `scripts/seed-wheelchairs.ts`, `scripts/build-wheelchair-seed.ts`, `src/domains/wheelchairs/{seed-data,fallbacks}.ts`, `src/lib/phi-crypto.ts`, `scripts/phi-column-policy.ts`, `src/db/{client,assert-schema-current}.ts`, `vercel.json`, `src/domains/emails/{outbox,dispatcher,cron-routes}.ts`, `.github/workflows/deploy-*.yml`, `package.json` scripts or Drizzle versions |
| `uncertain` (content, non-test files) | Added `migrate(` or a Drizzle migrator import; DDL keywords (`CREATE`, `ALTER`, `DROP`, `TRUNCATE`, `GRANT`, `REVOKE`, `DO $`); non-SELECT `db.execute(` / `query(`; new `sql` templates with `UPDATE`/`DELETE`/`INSERT`; `pgTable`/`pgEnum` outside `schema.ts` |
| `uncertain` (invariants) | The candidate's `meta/_journal.json` is not a strict append-only extension of the released one; a new entry lacks its `.sql`, or lacks a snapshot without being INSERT-only |
| `clean` | No rule fires, the journal is byte-identical, and `drizzle-kit generate` in a scratch worktree of the candidate produces no new migration (offline; catches schema/snapshot drift) |

**Known gaps (cannot be closed by diff rules):**

1. **Edited applied migrations are skipped in production.** Drizzle's migrator applies only
   entries newer than the last applied one and does not re-hash, and the boot gate compares
   counts. This has happened before (an existing `0014_*.sql` was edited in place to add a
   backfill). Treat it as `migration` with a hard warning, and require a new migration instead.
2. **Snapshot drift** is not checked in CI today; only the `drizzle-kit generate` cross-check
   catches it.
3. **Operator scripts** (backfill, seed) change data out of band; code changes to them can
   only be flagged.
4. **Changed meaning of stored data** (encryption key, AAD, or encoding changes) alters
   persisted data with no schema change.
5. **Manual changes in the database console** and **database URL configuration** in Vercel or
   GitHub environments are invisible to any diff; `assert-prod-schema:live` only partly detects
   the former.

## Open questions

- Who runs production migrations: a human with release credentials, or a privileged
  release job (README permission profile "Privileged release job")?
- Exact human-vs-policy rules per transition are decided in the `TODO(human)` block of
  `harness/policy/transitions.ts`; this document only fixes that a policy actor needs a
  `clean` classifier result.
- Classifier rules for projects other than canary-backend.
- canary-backend: should CI add the `drizzle-kit generate` drift check and a rule rejecting
  edits to applied migration files? That change belongs in canary-backend, not here.
