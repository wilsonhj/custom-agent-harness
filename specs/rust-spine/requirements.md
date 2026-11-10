# Requirements: rust-spine — Rust core and first slice (implement, review, repair)

> Status: design approved by the repository owner on 2026-11-09; no task phase is approved
> by this file. Source of every row: `docs/superpowers/specs/2026-11-09-unified-harness-design.md`
> (cited as "design §n"). Graded evidence: `docs/superpowers/specs/research-2026-11-09/`.

## User-visible behavior

- `eng task new`, `eng decide <task> approve-spec`, `eng run <task>` carry a hand-written
  packet through Claude Code implementation, harness verification, Codex review and up to two
  automatic repair rounds, then stop for the human (R-1 to R-6).
- `eng decide <task> accept|reject|send-back` is the only way a task becomes verified (R-7).
- Nothing runs if the macOS sandbox is unavailable (R-8).

## Non-goals

- Research, spec drafting, deployment triage, counsel, routing, the daemon and clients.
- Linux, Cursor, session resume, skills rendering.
- The signed approval ledger (`docs/architecture/approval-ledger.md`); see design §6.5.
- Releasing or deploying anything.

## Invariants

- Agent output is a claim; only harness-run verification output and schema-valid reviewer
  findings are evidence (design §6.5).
- The harness never reads, stores or forwards a subscription login token (design §4 row 6).
- Agents never run Git or commit; the harness commits under `refs/harness/` (design §4 row 9).
- Implementer and reviewer come from different model families (design §2 goal 3).

## Requirements

| ID | Requirement | Source |
|---|---|---|
| R-1 | The workspace has six crates with the dependency direction `cli -> engine -> {store, exec, adapters} -> core`, checked in CI. | design §5, §5.1 |
| R-2 | `harness-core` contains no I/O, async, clock or randomness. | design §5 |
| R-3 | A command, its events and its pending work commit in one SQLite transaction before any work runs, and a repeated command returns the first receipt. | design §4 row 10, §5.4 |
| R-4 | The event log is hash-chained and append-only, and `eng audit verify` detects a changed event. | design §4 row 11 |
| R-5 | After a crash, an agent run is never replayed: it fails with `LostOnRestart`, the worktree resets to the last checkpoint, and the task stops for the human; verification is retried. | design §4 row 12, §5.4 |
| R-6 | Repair starts automatically when harness verification fails or a finding is at or above the severity threshold, for at most `max_repair_rounds` (default 2). | design §4 row 19, §6.5 |
| R-7 | `Planned -> Approved` and `Reviewing -> Verified` require a human decision bound to the exact packet digest or commit, entered at a terminal. | design §4 row 13, §6.5 |
| R-8 | Every agent and verification process runs under a generated macOS sandbox profile; a failed probe fails the run before any process starts. | design §6.1 |
| R-9 | Sandbox paths are canonicalised before use; reads of `~/.ssh`, `~/.aws`, the state store, Git metadata and repository agent configuration are denied. | design §6.1 |
| R-10 | Agent network access is one localhost port to a harness proxy that passes only listed hosts and logs every decision; verification has no network. | design §6.1 |
| R-11 | A worktree lock is released only after every descendant process is confirmed gone. | design §6.2 |
| R-12 | The harness refuses to commit a diff containing a credential-shaped string. | design §6.3 |
| R-13 | Each adapter checks its pinned runtime version before any task content is sent. | design §6.4 |
| R-14 | A run succeeds only with exit code 0, a non-error final event, and schema-valid output. | design §6.4 |
| R-15 | Each agent's own sandbox is disabled, and each run confirms it is off. | design §4 row 7 |
| R-16 | A subscription rate limit pauses the task for the human without consuming a repair round. | design §4 row 18 |

## Cited company decisions

| Constraint | Source |
|---|---|
| Rust core; subscription logins; CLI first; Jev optional | Owner, 2026-11-09 (design §4 rows 1–4) |
| One writer per worktree | `AGENTS.md` |
| Runtime output is data, never evidence | `specs/adapter-interface/requirements.md` A-5, A-17 |

## Unresolved questions

- [ ] Open-source license (design §14 Q5). — owner: repository owner, blocks: copying any code from another project
- [ ] Whether a sandboxed shell child can read the Keychain login (design §11.2). — owner: architect with owner present, blocks: none (measured in T11)
- [ ] Exact vendor host lists for the egress allowlist (design §11.3). — owner: architect with owner present, blocks: T11
