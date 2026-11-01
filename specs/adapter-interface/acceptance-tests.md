# Acceptance tests: adapter-interface

Each item names the command or procedure that proves it and the requirement it covers.
Check an item only when the result is recorded in `handoff.md`.

The test files below do not exist yet; they are created in T3 and run with the README
"Testing" command `node --experimental-strip-types --test tests/*.test.ts`. Nothing here
has been run.

## Behavior

- [ ] A fake adapter implementing the interface can be started, polled, cancelled and collected — `tests/adapter-interface.test.ts` — covers: A-1
- [ ] `start` rejects a raw path or command and resolves the worktree from the task ID — `tests/adapter-interface.test.ts` — covers: A-2
- [ ] An unmappable permission profile is refused — `tests/adapter-interface.test.ts` — covers: A-3
- [ ] Deadline and cancel kill a fake run's child and grandchild processes; the run is failed — `tests/adapter-process.test.ts` — covers: A-4
- [ ] Invalid, unparseable and "approved"-claiming output fails the run and changes no task state — `tests/adapter-interface.test.ts` — covers: A-5
- [ ] Artifacts are written by the adapter with correct sha256 digests — `tests/adapter-interface.test.ts` — covers: A-6
- [ ] Continuation uses the recorded session ID and ignores a different one from the runtime — `tests/adapter-interface.test.ts` — covers: A-7
- [ ] A canary secret in the parent environment is absent from the fake runtime's environment — `tests/adapter-process.test.ts` — covers: A-8
- [ ] The compatibility check fails on a version other than the declared one — `tests/adapter-interface.test.ts` — covers: A-9
- [ ] Each run appends exactly one run-ledger record with the listed fields, and it is not accepted as an approval — `tests/run-ledger.test.ts` — covers: A-13
- [ ] Delegation depth and parallel-job caps are enforced — `tests/adapter-interface.test.ts` — covers: A-14

## Invariants

- [ ] A worktree is created at the recorded base SHA, outside other tasks' worktrees — `tests/worktree.test.ts` — covers: A-10
- [ ] Two concurrent writers on one worktree: exactly one acquires the lock; it is released after exit, crash and cancel — `tests/worktree.test.ts` — covers: A-11
- [ ] An `analysis` run cannot modify the worktree; a `workspace` run cannot write an unrelated path — `tests/worktree.test.ts` — covers: A-12
- [ ] Existing suites still pass: `tests/transitions.test.ts`, `tests/ledger-guard.test.mjs` — README "Testing"

## Release

- [ ] The migration classifier reports the release diff as `clean` (docs/architecture/release-gate.md) — covers: design.md "Migration plan"
