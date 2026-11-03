# Requirements: run-ledger — Run ledger with decision-ready records

<!-- Status: Draft. Not approved; approval is a ledger record, not this file. -->

## Background

The README "Run ledger" section lists what to record (task/job IDs, runtime and skill
versions, permission profile, knowledge revision, session IDs, commit SHAs, evidence,
approvals, timing, usage) but has no schema, storage, or writer. The approval ledger
(`docs/architecture/approval-ledger.md`) records *who approved what*.

The local evidence workflow pilot (`harness/local/`, `docs/local-workflow.md`) already
records *one run* in detail: `run.json` plus content-addressed evidence objects (command,
stage, times, exit code, output digests) in its run store. What is missing is a record
*across* runs: which runs belong to a task, why each one moved on or stopped, and how each
ended, in one place an operator or an analysis can read. The local workflow controller is
the first producer of run-ledger records; its run store stays the source of evidence.

This packet also shapes the record so that a later experiment can compare the current
fixed repair policy (`execution.max_repair_rounds`) with alternatives using logged
history. Recording the right fields now is cheap; reconstructing them later is not
possible. Learning anything from the data is out of scope here.

## User-visible behavior

- An operator can list every job for a task with: runtime and version, adapter and
  version, model ID as reported (or `null`), permission profile, session ID, base and head
  SHA, start and end time, termination reason, and usage where the runtime reports it.
- An operator can see, for each job, every **control decision point** (continue, request
  review, repair, verify, stop, escalate): what the controller observed as structured
  fields, which actions were available, which it chose, and what the following verifier
  run reported.
- Every job ends with exactly one `termination_reason` from a closed set that separates
  `succeeded`, `task_failure`, `budget_exhausted`, `infrastructure_failure`,
  `policy_violation`, and `cancelled`.
- A job's terminal outcome (acceptance tests passed / failed / not run) comes from commands
  the control plane ran and is bound to evidence digests. An agent's statement that it
  finished is never recorded as an outcome.
- An operator can run a verify command that reports whether the ledger's hash chain is
  intact and whether every line validates against its declared schema version.
- An operator can export a task's or project's records as JSONL for offline analysis; the
  export carries the most restrictive egress class of its inputs.
- A write attempted from an agent process is refused, and a record that fails schema
  validation (including any unknown field) is refused, with the reason reported to the
  caller.

## Non-goals

- Learning a controller, training any model, or changing the repair policy. Phase 1 logs
  a **static** controller only.
- Capturing per-token log probabilities or raw model contexts. The vendor CLIs used here
  do not expose them, so model-weight RL is not a target of this ledger.
- Recording transcripts, prompts, tool output text, file contents, or any free-text
  field. Content lives in the evidence store and is referenced by digest.
- Signing records or anchoring the run ledger remotely. The run ledger is not an
  authority; approvals stay in the approval ledger (see design.md "Relation to the
  approval ledger").
- A dashboard or query service. Phase 1 is a file plus CLI commands.
- Splitting tasks into train / selection / evaluation sets (see Unresolved questions).

## Invariants

- No agent process (any runtime) can append to, edit, or delete the run ledger. Same
  boundary as the approval ledger: OS ownership, not instructions.
- Lines are append-only; a correction is a new record referencing the corrected one.
- No record contains free text or file content. Every string field is an enum, an ID, a
  version, a SHA, a digest, or a timestamp, enforced by the schema
  (`additionalProperties: false`).
- `null` means "not reported"; it is never written as `0`. Usage and cost fields that a
  runtime does not report stay `null`.
- A behavior probability is recorded only for a decision drawn at random by a stochastic
  controller. Deterministic decisions record `policy_kind: "deterministic"` and no
  probability; probabilities are never imputed after the fact.
- The run ledger never changes task state. `canTransition` reads only the approval ledger.
- Existing approval-ledger, release-gate, and egress rules are unchanged.

## Cited company decisions

| Constraint | Source (ADR / policy / brain doc ID @ revision) |
|---|---|
| Ledgers live outside every worktree; agents get no write access; enforcement is OS ownership, with managed hooks as defence in depth | `docs/architecture/approval-ledger.md` "Storage", "Who writes, and how it is enforced" @ `7fce6c6` |
| Evidence is referenced by `{ name, digest.sha256 }` in the content-addressed store; contents never enter a ledger | `docs/architecture/approval-ledger.md` "Record schema" (`evidence_refs`) @ `7fce6c6`; `EvidenceRef` in `harness/policy/transitions.ts` @ `7fce6c6` |
| Task states and allowed transitions | `harness/policy/transitions.ts` @ `7fce6c6` |
| Derived artifacts inherit the most restrictive egress class of their inputs; unknown class means `local-only` | `docs/security/data-egress.md` "Egress classes" @ `7fce6c6` |
| Completion requires validated evidence, not an assertion | `AGENTS.md` "Approvals and state" @ `7fce6c6` |
| Fields the run ledger must cover | `README.md` "Run ledger" @ `7fce6c6` |
| Run states (`prepared`, `running`, `awaiting-decision`, `failed`, `decision-recorded`) are local execution facts, not `TaskState`; a run store lives outside the source checkout; interrupted runs are never replayed | `docs/local-workflow.md`, `specs/local-evidence-workflow/design.md` "Architecture" @ `7fce6c6` |
| Node 24 ESM, no npm dependencies; tests run with `npm test` (`node --experimental-strip-types --test tests/*`), syntax checks with `npm run check` | `package.json`, `scripts/test.mjs`, `scripts/check-local.mjs`, `.github/workflows/local-workflow.yml` @ `7fce6c6` |
| Fixed repair and budget limits (`max_repair_rounds`, `deadline_seconds`) | `README.md` "Example configuration contract" @ `7fce6c6` (proposed schema, not yet implemented) |

Research input, **not** a company decision and not independently verified: the uploaded
note "Reinforcement Learning for an Agent Harness" (2026-10-28) sections 5, 6 and 9
(transition-record fields, the four failure types, outcome-only rewards, no invented
behavior probabilities). Nothing in this packet depends on its numerical claims.

## Unresolved questions

- [x] Test runner and toolchain: resolved on `main` (`npm test`, `npm run check`, Node 24,
  no npm dependencies; see Cited company decisions).
- [ ] Writer identity: the local workflow's controller account, the approval ledger's
  `<ledger-user>`, or a separate `<run-ledger-user>`? The controller account is simplest
  (it already writes the run store); a separate user limits a writer bug to one ledger.
  — owner: <maintainer>, blocks: T2
- [ ] Evidence location: local-workflow evidence lives in `<store>/<run-id>/objects/<sha256>`,
  not `<local-artifacts-directory>/evidence/sha256/` as the approval ledger assumes
  (`docs/local-workflow.md` "Known limitations"). Draft assumes the run ledger resolves
  digests in the run store named by the job. Should one layout win instead? — owner:
  <maintainer>, blocks: T2
- [ ] Events from interactive sessions (Claude Code with the `codex` plugin) can only come
  from hooks that run as the session's user, so the session could forge them. Accept them
  as `provenance: "session-hook"` (lower trust, never used as an outcome) or exclude them
  until a control-plane adapter exists? Draft assumes the former. — owner: <maintainer>,
  blocks: T4
- [ ] Retention period and deletion procedure for the run ledger and the evidence it
  references (README "Run ledger": `Retention: <Policy>`). — owner: <maintainer>,
  blocks: none (T2 can ship with "retain all")
- [ ] Task-family taxonomy used in `task_family`, and when and how a task is assigned to a
  train / selection / held-out split so the assignment cannot be influenced by results.
  — owner: <maintainer>, blocks: none for Phase 1
- [ ] File rotation: one file per project (as the approval ledger) or per project and
  month? — owner: <maintainer>, blocks: T2
