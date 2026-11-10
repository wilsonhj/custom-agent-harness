# Design: rust-spine

The design is `docs/superpowers/specs/2026-11-09-unified-harness-design.md`; the task-level
plan with exact interfaces and tests is `docs/superpowers/plans/2026-11-09-rust-spine.md`.
This file records only what the packet template asks for.

## API and schema changes

- New Cargo workspace at the repository root: `crates/{core,store,exec,adapters,engine,cli}`;
  binary `eng`.
- New local SQLite database at `~/Library/Application Support/eng/state.db` (tables: tasks,
  commands, events, outbox, artifacts, workspaces) and a content-addressed object directory
  beside it.
- New Git refs in target repositories: `refs/harness/tasks/<task-id>/cp/<n>`; on accept,
  branch `harness/<task-id>`.
- New `conformance/` suite driven through the CLI; `conformance/golden/transitions.json` is
  exported from `harness/policy/transitions.ts`.
- No change to `harness/local/*.mjs` or `harness/policy/transitions.ts` in this packet.

## Dependencies

- A Rust stable toolchain, installed by the owner (not present on the owner's Mac on
  2026-11-09).
- Claude Code 2.1.318 and codex-cli 0.166.0, logged in by the owner through `eng login`.
- Node 24 for the conformance runner (already required by `package.json`).
- Supersedes for this slice: adapter-interface D1 (Linux only) and D4 (TypeScript modules);
  claude-adapter C-2, C-7, C-8. Those packets are not edited here.

## Migration plan

No migration: no deployed service or shared database. The state database is new and local.
The release gate (`docs/architecture/release-gate.md`) still applies to any release; this
packet releases nothing.

## Rollout and rollback

- **Rollout:** `eng` is built locally; the Node pilot stays in place and unchanged. The first
  real run is supervised by the owner (plan Task 11).
- **Rollback:** delete the `crates/` and `conformance/` directories and the state directory;
  `git update-ref -d` the `refs/harness/` refs in any target repository.
