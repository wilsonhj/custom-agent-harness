# PR13 functional review fixes implementation plan

> Execution: implement inline in the assigned worktree with test-first changes; the parent session arranges independent review. No pushes, credentials, or live agents.

**Goal:** Keep Node tools available with native Codex, read the same committed packet from nested repository paths, and allow status/cancel during a run.

**Architecture:** Preserve the one Store writer (design §5.4) and CLI → engine boundary (design §5.1). A private, transient local control endpoint belongs to the running Engine and accepts only status/cancel; this does not introduce a daemon or generic command API (design §5.5; rust-spine non-goals). Cancellation keeps process supervision and `TreeGone` release (R-11).

**Spec:** `specs/rust-spine/` and `docs/superpowers/specs/2026-11-09-unified-harness-design.md`.

## Task 1: Node tools and Codex invocation

- [x] Add failing native-Codex/Node verification regression and launch/login coverage.
- [x] Preserve discovered `EngineConfig.node_bin`; add `codex_uses_node` to choose the Codex argv prefix independently. Update fixtures and affected docs only.
- [x] Run engine and CLI tests; record red/green evidence.

## Task 2: Committed packets from nested paths

- [x] Add a failing workspace regression comparing root/nested cwd packet contents and digest.
- [x] Add `ls-tree --full-tree`; run workspace packet tests.

## Task 3: Active-run status/cancel

- [x] Add failing fake-agent conformance coverage for active status/list/cancel and second-writer rejection.
- [x] Add engine-owned private control transport with bounded requests/timeouts and endpoint cleanup. Reuse Engine submission and cancellation tokens; keep SQLite ownership unchanged. Adapt CLI status rendering to an engine snapshot.
- [x] Cover stale endpoint/restart, invalid requests and atomic status snapshots under concurrent submissions; run covering tests and required workspace checks.

## Review focus

- Native/shell Codex must run directly even when Node is available; JS Codex still uses Node.
- Nested `--repo` paths must not produce empty packet digests.
- A runner keeps serving control during hanging agents and verification; cancellation must kill descendants before unlocking.
- Endpoint files stay private and stale endpoints cannot create a second writer or replay cancellation.
- Relative/long state paths and malformed/partial clients must not hang the runner.

## Validation and delivery

- [x] Run packet acceptance commands: dependency check; format; clippy; workspace tests; workspace build; conformance; existing Node suites.
- [x] Update `specs/rust-spine/handoff.md` with assumptions, exact evidence and next action; write `docs/reviews/pr13-functional-review-fixes.md`; commit scoped changes.

## Execution evidence

- RED: native-Codex verification reached `repair_limit_reached`; active status exited 70 with the writer-lock error; nested packet test returned 0 files instead of 5. Native login regression selected Node instead of Codex.
- GREEN: workspace 246 passed / 5 existing ignores; CLI conformance 13 passed; fmt, clippy, dependency and syntax checks passed. Node pilot: 44 passed / 44 failed / 2 skipped on unsupported macOS (system-runtime path restrictions and Linux Bubblewrap requirement).
- Ruling: status/cancel use only a transient owner-private Unix socket, with a short private `/tmp` directory referenced by a private state file — supports long state paths without adding a daemon; if wrong, the IPC boundary would need redesign.
- External whole-branch review and security integration belong to the parent session; no task phase or release is approved here.

- Additional RED→GREEN: concurrent status initially returned projections totaling version 8 with sequence 6; collecting both in one Store writer job makes the boundary consistent (design §5.5).
