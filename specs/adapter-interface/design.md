# Design: adapter-interface

## API and schema changes

- New `harness/adapters/types.ts`: the `Adapter` interface (`start`, `status`, `cancel`,
  `collect`), `RunRequest` (task ID, deadline, continuation flag only; worktree, base SHA,
  profile, egress class and session ID are resolved from the task record, A-2),
  `RunResult` (status, exit code, artifact digests labelled as runtime claims) and the
  result schema used by A-5.
- Per-task directories: `<local-artifacts-directory>/requests/<task-id>/` and a
  harness-owned task home; host-side commit step with repository-controlled Git code
  disabled (A-16).
- New `harness/orchestration/worktree.ts`: create a worktree for a task at the base SHA in
  its task record, acquire the per-worktree writer lock and release it only after the
  run's processes are confirmed gone (A-10, A-11).
- A per-run OS user or container for the runtime process (A-12, A-15); choice in T2.
- Harness-run validation uses independently trusted entrypoints configured outside the
  candidate; every script they invoke runs inside the harness-enforced boundary with a
  read-only candidate mount and isolated writable scratch space, without adapter/controller
  credentials or write access to artifacts, evidence stores or ledgers, and with reads and
  network access limited by the validation job's permission profile (A-17, A-22;
  `docs/local-workflow.md` "Trust and installation", "Operator configuration"). The harness
  captures validation output outside that boundary; unavailable isolation fails closed
  before validation code runs (A-20). T2 chooses the backend, not whether to isolate validation.
- New run-ledger record shape (A-13), append-only; storage is an open question.
- No change to `harness/policy/transitions.ts` or the approval ledger.

## Dependencies

- `harness/policy/transitions.ts` (task states) and the approval-ledger design.
- `specs/ledger-write-guard/` for the guard that keeps agents off the ledger.
- Node >= 22.6 for `node --experimental-strip-types --test` (README "Testing"); open PR #4
  requires Node 24, to be reconciled in T2.
- The migration classifier (`<migration-classifier-command>`), which does not exist yet.
- Dependents: `specs/kiro-adapter/` and future per-runtime adapter packets. A dependent
  task names a task in this packet as `adapter-interface:<task-id>` in `depends_on`; it
  should depend on T4 (the reviewed implementation), not T3.

## Migration plan

No migration: no database schema or deployed-service data. The run ledger and artifacts
are new local append-only files. The release gate (`docs/architecture/release-gate.md`)
still applies: migration evidence may be skipped only if the classifier reports `clean`.
The classifier does not exist yet, and release-gate.md treats a missing classifier as
`migration`, so until it exists a release needs full migration evidence.

## Rollout and rollback

- **Rollout:** the interface and worktree manager ship with a fake test adapter only; no
  real runtime is wired until its own packet passes review.
- **Rollback:** remove the modules; no runtime depends on them until a later packet.
