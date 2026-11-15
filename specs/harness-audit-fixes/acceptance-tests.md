# Acceptance tests: harness-audit-fixes

Each item names the command or procedure that proves it and the requirement it covers.
Check an item only when the result is recorded in `handoff.md`.

There is no `package.json`. The README "Testing" section documents the unit test command.
For this packet, run from the repository root:

```bash
node --experimental-strip-types --test tests/*.test.ts
```

(Node >= 22.6. `--experimental-strip-types` prints an ExperimentalWarning; that is expected.)

## Behavior

- [x] Resume from `Blocked` needs the entering record and the current commit; mismatched state, mismatched commit, missing/empty/malformed record or missing commit are denied — `node --experimental-strip-types --test tests/*.test.ts` (`resume from Blocked` suite) — covers: requirements "canTransition resumes a Blocked task only when..."
- [x] Approval whose `from_state`/`to_state` mismatch the arguments, and missing/malformed approval, are rejected — same command (`approval shape` suite) — covers: "rejects an approval whose from_state or to_state differs"
- [x] Fail-closed preserved: no transition other than entering `Blocked` returns `ok: true` — same command (`fail closed` suite) — covers: "stays fail-closed"
- [x] `Failed` edges from `Verified`, `PreviewVerified`, `MigrationGated`; `Released`/`Observed` -> `Blocked`; `Implementing`/`Repair` -> `Superseded` — same command (`edges` suite, full 16x16 pair table) — covers: edge bullets
- [x] Every state reachable from `Draft`; only `Closed`, `Cancelled`, `Superseded` are dead ends; `Released` unreachable without `MigrationGated` — same command (`graph` suite) — covers: reachability bullet
- [x] Terminology: no `untrusted-origin` left; README successful-path names equal `TaskState` — `grep -rn "untrusted-origin" README.md docs; grep -n "Preview verified\|Migration gated" README.md` (expect no output) — covers: terminology bullet
- [x] README no longer ticks the Codex review and no longer says the release decision is "enforced from Phase 1" — `grep -n "\[x\]" README.md; grep -n "enforced from Phase 1" README.md` (expect no output) — covers: README consistency bullet

## Invariants

- [x] A `Blocked` task may resume only to the state named on the entering `BlockedRecord` (`from_state`), at the same commit and matching approval target; `Cancelled` and `Superseded` need no record — same command (`resume from Blocked` suite). Forward progress after resume uses that state's own edges.
- [x] Nothing under an approval-ledger directory was written — `git status --short` lists only `README.md`, `docs/`, `harness/`, `specs/harness-audit-fixes/`, `tests/`

## Migration

No migration (design.md). Section removed.

## Preview

Not applicable: no deployable artifact.