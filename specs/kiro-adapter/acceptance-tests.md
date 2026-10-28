# Acceptance tests: kiro-adapter

Each item names the command or procedure that proves it and the requirement it covers.
Check an item only when the result is recorded in `handoff.md`.

Test commands do not exist yet (no adapter, no test harness). Replace the procedures
below with runnable commands as T2-T4 land; do not report them as passing before then.

## Behavior

- [ ] Kiro run in a task worktree records exit status and output — procedure: adapter compatibility test (TBD) — covers: K-1, K-11
- [ ] Second writer on the same worktree is refused — procedure: TBD — covers: K-2
- [ ] Harness-supplied agent runs despite a same-named global agent — procedure: TBD — covers: K-8
- [ ] Subagent from a restricted agent cannot use a tool the parent lacks — procedure: TBD — covers: K-7

## Invariants

- [ ] Kiro session cannot create or modify files under `<ledger-root>` — procedure: TBD — covers: K-3
- [ ] Canary `local-only` file is denied via file read, grep, shell `cat` and MCP — procedure per data-egress.md "Tests" — covers: K-5, K-6
- [ ] Session on an unpinned account is refused — procedure: TBD — covers: K-4
- [ ] Kiro session is treated as `any-provider` while no BAA allowlist entry exists — procedure: TBD — covers: K-10

## Migration

Not applicable: design.md says "No migration".

## Preview

Not applicable: no deployed surface.
