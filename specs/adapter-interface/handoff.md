# Handoff: adapter-interface / packet creation

| Field | Value |
|---|---|
| Base SHA | e769f18 (main after kiro-adapter merged) |
| Head SHA | 45edc0c (packet as reviewed; review fixes land in the next commit on this branch) |
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

## Review: /code-review (Claude, same session, not independent; NOT Codex)

Run 2026-11-01 on 45edc0c without a separate verify pass. All ten findings fixed:

| Finding | Resolution |
|---|---|
| A-7 forbade the runtime-assigned session ID it must record | A-7 now records the runtime's ID from the first run and continues only with that recorded ID |
| Lock released before the process tree is confirmed dead | A-11 releases only after every run process is confirmed gone; test adds a `setsid` grandchild |
| A-11 promised crash release with no mechanism | A lock with an unconfirmable owner is released only by a human, never by timeout |
| Caller-supplied base SHA vs recorded base SHA | A-2 resolves worktree, base SHA and session ID from the task record; A-10 uses the task record |
| Runtime could write artifacts and the run ledger | New A-15 plus test |
| A-12 unprovable with a fake adapter | A-12 is now a harness-level OS boundary (per-run user or container, T2 choice), tested with a fake runtime that tries to escape |
| Delegation cap unenforceable inside runtime subagents | A-14 scoped to harness-launched jobs; each runtime packet must state its internal bound |
| Test command skipped the `.mjs` suite | Command now `tests/*.test.ts tests/*.test.mjs` |
| Head SHA was the base | Recorded 45edc0c |
| kiro-adapter T4's cross-packet dependency was title-only | `depends_on` now includes `adapter-interface:T3`; `<packet>:<task>` is the cross-packet form |

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
