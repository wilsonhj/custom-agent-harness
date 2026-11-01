# Acceptance tests: adapter-interface

Each item names the command or procedure that proves it and the requirement it covers.
Check an item only when the result is recorded in `handoff.md`.

The test files below do not exist yet; they are created in T3 and run with the README
"Testing" command, extended to the `.mjs` suite: `node --experimental-strip-types --test tests/*.test.ts tests/*.test.mjs`. Nothing here
has been run.

## Behavior

- [ ] A fake adapter implementing the interface can be started, polled, cancelled and collected — `tests/adapter-interface.test.ts` — covers: A-1
- [ ] `start` rejects a raw path, SHA or command and resolves worktree, base SHA and session ID from the task record — `tests/adapter-interface.test.ts` — covers: A-2
- [ ] An unmappable permission profile is refused — `tests/adapter-interface.test.ts` — covers: A-3
- [ ] Deadline and cancel kill a fake run's child and grandchild processes; the run is failed — `tests/adapter-process.test.ts` — covers: A-4
- [ ] Invalid, unparseable and "approved"-claiming output fails the run and changes no task state — `tests/adapter-interface.test.ts` — covers: A-5
- [ ] Artifacts are written by the adapter with correct sha256 digests — `tests/adapter-interface.test.ts` — covers: A-6
- [ ] The first run's runtime-assigned session ID is recorded; continuation passes only that ID and ignores a different one from caller or runtime output — `tests/adapter-interface.test.ts` — covers: A-7
- [ ] A canary secret in the parent environment is absent from the fake runtime's environment — `tests/adapter-process.test.ts` — covers: A-8
- [ ] The compatibility check fails on a version other than the declared one — `tests/adapter-interface.test.ts` — covers: A-9
- [ ] Each run appends exactly one run-ledger record with the listed fields, and it is not accepted as an approval — `tests/run-ledger.test.ts` — covers: A-13
- [ ] Harness-launched delegation depth and parallel-job caps are enforced — `tests/adapter-interface.test.ts` — covers: A-14

## Invariants

- [ ] A worktree is created at the base SHA in the task record, outside other tasks' worktrees — `tests/worktree.test.ts` — covers: A-10
- [ ] Two concurrent writers on one worktree: exactly one acquires the lock; after exit and cancel it is released only once a surviving grandchild (detached with `setsid`) is also gone; a lock with an unconfirmable owner stays held until a human releases it — `tests/worktree.test.ts` — covers: A-4, A-11
- [ ] Under the per-run OS user or container, an `analysis` run cannot modify the worktree and a `workspace` run cannot write an unrelated path, tested with a fake runtime that tries to — `tests/worktree.test.ts` — covers: A-12
- [ ] The fake runtime cannot write the artifacts directory, evidence store or run ledger, but can write the requests directory — `tests/adapter-process.test.ts` — covers: A-15
- [ ] Existing suites still pass: `tests/transitions.test.ts`, `tests/ledger-guard.test.mjs` — README "Testing"

## Release

- [ ] The migration classifier reports the release diff as `clean` (docs/architecture/release-gate.md) — covers: design.md "Migration plan"
