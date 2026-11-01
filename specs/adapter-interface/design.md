# Design: adapter-interface

## API and schema changes

- New `harness/adapters/types.ts`: the `Adapter` interface (`start`, `status`, `cancel`,
  `collect`), `RunRequest` (task ID, base SHA, profile, deadline, session ID), `RunResult`
  (status, exit code, artifact digests) and the result schema used by A-5.
- New `harness/orchestration/worktree.ts`: create a worktree for a task at the base SHA in
  its task record, acquire the per-worktree writer lock and release it only after the
  run's processes are confirmed gone (A-10, A-11).
- A per-run OS user or container for the runtime process (A-12, A-15); choice in T2.
- New run-ledger record shape (A-13), append-only; storage is an open question.
- No change to `harness/policy/transitions.ts` or the approval ledger.

## Dependencies

- `harness/policy/transitions.ts` (task states) and the approval-ledger design.
- `specs/ledger-write-guard/` for the guard that keeps agents off the ledger.
- Node >= 22.6 for `node --experimental-strip-types --test` (README "Testing").
- Dependents: `specs/kiro-adapter/` and future per-runtime adapter packets.

## Migration plan

No migration: no database schema or deployed-service data. The run ledger and artifacts
are new local append-only files. The release gate (`docs/architecture/release-gate.md`)
still applies: migration evidence may be skipped only if the classifier reports `clean`.

## Rollout and rollback

- **Rollout:** the interface and worktree manager ship with a fake test adapter only; no
  real runtime is wired until its own packet passes review.
- **Rollback:** remove the modules; no runtime depends on them until a later packet.
