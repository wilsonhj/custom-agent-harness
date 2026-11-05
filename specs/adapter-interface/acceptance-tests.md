# Acceptance tests: adapter-interface

Each item names the command or procedure that proves it and the requirement it covers.
Check an item only when the result is recorded in `handoff.md`.

The test files below do not exist yet; they are created in T3 and run with
`npm test` (runs every suite under `tests/`, added to main with the local workflow). Nothing here
has been run. "Fake runtime" is a test executable that tries the forbidden action.

## Behavior

- [ ] A fake adapter implementing the interface can be started, polled, cancelled and collected — `tests/adapter-interface.test.ts` — covers: A-1
- [ ] A `start` call carrying a path, SHA, profile, egress class or session ID field is rejected; for a reviewer task the profile resolved from its record is `analysis` — `tests/adapter-interface.test.ts` — covers: A-2
- [ ] An unmappable permission profile is refused — `tests/adapter-interface.test.ts` — covers: A-3
- [ ] Deadline and cancel kill a fake run's child, grandchild and a `setsid`-detached descendant; the run is failed — `tests/adapter-process.test.ts` — covers: A-4
- [ ] Invalid, unparseable and "approved"-claiming output fails the run and changes no task state — `tests/adapter-interface.test.ts` — covers: A-5
- [ ] Artifacts are written by the adapter with correct sha256 digests, labelled as runtime claims, with a canary secret redacted — `tests/adapter-interface.test.ts` — covers: A-6
- [ ] The first run's session ID is recorded; continuation passes only that ID; a session ID already recorded for another task is refused — `tests/adapter-interface.test.ts` — covers: A-7
- [ ] A canary secret in the parent environment and in `~/.ssh`, `~/.aws` is not reachable by the fake runtime — `tests/adapter-process.test.ts` — covers: A-8
- [ ] The compatibility check fails on a version other than the declared one — `tests/adapter-interface.test.ts` — covers: A-9
- [ ] Each run appends exactly one run-ledger record with the listed fields, including outcome `failed` for a cancelled run; a head SHA printed by the runtime is ignored in favour of Git's; the record is not accepted as an approval — `tests/run-ledger.test.ts` — covers: A-13
- [ ] Harness-launched delegation depth and parallel-job caps are enforced — `tests/adapter-interface.test.ts` — covers: A-14
- [ ] A fake runtime that prints "tests passed" produces no `evidence_refs` entry; harness-run validation satisfying A-22 and schema-valid reviewer findings do — `tests/run-ledger.test.ts` — covers: A-17
- [ ] A task whose egress class exceeds the runtime's ceiling is refused before launch — `tests/adapter-interface.test.ts` — covers: A-19
- [ ] Missing runtime, unavailable sandbox, version mismatch and a required tool failing to start each fail the run before the runtime receives the task — `tests/adapter-interface.test.ts` — covers: A-20

## Invariants

- [ ] A worktree is created at the base SHA in the task record, outside other tasks' worktrees — `tests/worktree.test.ts` — covers: A-10
- [ ] Two concurrent writers on one worktree: exactly one acquires the lock; after exit and cancel it is released only once a `setsid`-detached descendant is also gone; a lock with an unconfirmable owner stays held until a human releases it — `tests/worktree.test.ts` — covers: A-4, A-11
- [ ] Under the runtime user, an `analysis` run cannot modify the worktree; a `workspace` run cannot write an unrelated path, `.git`, the common `.git` directory, refs, config or hooks — `tests/worktree.test.ts` — covers: A-12
- [ ] The fake runtime cannot write the artifacts directory, evidence store, run ledger or another task's request directory, but can write its own — `tests/adapter-process.test.ts` — covers: A-15
- [ ] A fake runtime that plants a nested repository with a `filter.*.clean` command, a hook or a gitlink: the adapter rejects the run before any host Git call and the command never runs on the host — `tests/adapter-commit.test.ts` — covers: A-16
- [ ] The fake runtime cannot read another task's worktree, home or request directory, and its network access matches its profile — `tests/adapter-process.test.ts` — covers: A-18
- [ ] Existing suites still pass under `npm test` — `npm test`
- [ ] Neither the adapter's user nor the fake runtime's user can write `<ledger-root>`, the signing keys or the anchor deploy key — `tests/adapter-process.test.ts` — covers: A-21
- [ ] An externally configured trusted verifier invokes malicious candidate build, test and dependency scripts: candidate/evidence/artifact/run-ledger/approval-ledger writes, controller credential reads, unrelated file reads and forbidden network access are denied; isolated scratch writes succeed and the harness captures output outside the boundary; candidate configuration cannot replace the verifier entrypoint, and unavailable isolation fails before validation code runs — `tests/adapter-validation.test.ts` — covers: A-17, A-20, A-22

## Release

- [ ] Either the migration classifier reports `clean`, or, while it does not exist, full migration evidence is supplied (docs/architecture/release-gate.md) — covers: design.md "Migration plan"
