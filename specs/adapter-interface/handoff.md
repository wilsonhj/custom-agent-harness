# Handoff: adapter-interface / packet creation

| Field | Value |
|---|---|
| Base SHA | e769f18 (main after kiro-adapter merged) |
| Head SHA | e769f18 (edits in this session land in the next commit) |
| Author (runtime / session ID) | Claude Code / session_01AaDiMJVSVxQVfoGPARPgBn |
| Date | 2026-11-01 |

## Changed files

- `specs/adapter-interface/*` — new packet: adapter interface (A-1 to A-9, A-13, A-14) and worktree ownership (A-10 to A-12), split out of `kiro-adapter` (its former T6).
- `specs/kiro-adapter/design.md`, `tasks.json`, `handoff.md` — point the dependency at this packet.

## Implemented requirements

- None. Packet only; no code.

## Tests run and results

| Command | Result | Evidence |
|---|---|---|
| (none) | not run | Test files are created in T3 |

## Known limitations

- No independent review yet (T1). Requirements are drawn from README "Adapter contract", "Minimum acceptance tests", "Run ledger" and AGENTS.md; no runtime facts.

## Unresolved decisions

- See requirements.md "Unresolved questions" — owner: TBD (human), in T2.

## Assumptions made

- Packet ID `adapter-interface` from the user's request ("start the adapter-interface packet"); it covers both the interface and worktree ownership, as the kiro-adapter's former T6 did.
- TypeScript under `harness/` is proposed (to match `transitions.ts`) but left as an open question.
- Worktree name `adapter-interface` in tasks.json is a placeholder.

## Exact next action

A human runs `/codex:adversarial-review` on `specs/adapter-interface/` and records the findings here (T1).
