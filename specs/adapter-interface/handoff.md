# Handoff: adapter-interface / packet creation

| Field | Value |
|---|---|
| Base SHA | e769f18 (main after kiro-adapter merged) |
| Head SHA | 88be84f (packet before this session's second-round fixes; they land in the next commit) |
| Author (runtime / session ID) | Claude Code / session_01AaDiMJVSVxQVfoGPARPgBn |
| Date | 2026-11-01 |

## Changed files

- `specs/adapter-interface/*` — new packet: adapter interface (A-1 to A-9, A-13 to A-20) and worktree ownership (A-10 to A-12), split out of `kiro-adapter` (its former T6).
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

## Review: six-agent parallel review (Claude subagents; NOT Codex), 2026-11-01

An independent Claude subagent review and a cross-packet validation of 88be84f. All
findings fixed in the next commit:

| Finding | Resolution |
|---|---|
| High: A-12 ignored Git metadata; runtime could rewrite shared `.git`, refs, hooks | A-12 forbids Git metadata writes; new A-16: the adapter commits on the host with repository-controlled Git code disabled and rejects nested repos and gitlinks first (PR #4's sandbox escape went through exactly this path) |
| High: caller chose the permission profile | A-2 resolves profile and egress class from the task record and policy |
| design.md `RunRequest` contradicted A-2 | `RunRequest` is task ID, deadline and continuation flag only |
| Session ID from runtime output could be another task's | A-7 refuses an ID recorded for another task |
| Runtime output hashed and citable as evidence; shared `requests/`; head SHA source | A-6 labels artifacts as runtime claims; new A-17 (evidence only from harness-run validation); per-task request directories; A-13 reads head SHA from Git |
| Missing controls: network, read isolation, redaction, egress ceiling, required-integration failures, run-ledger fields | New A-18, A-19, A-20; redaction in A-6; knowledge revision and skill versions in A-13 |
| Cancel test lacked a detached descendant; macOS cgroups; credential files | A-4 test adds `setsid`; lock question notes macOS; A-8 covers `~/.ssh`, `~/.aws`, keychain |
| kiro T4 depended on unreviewed T3; `<packet>:<task>` undocumented | Now `adapter-interface:T4`; form documented in design.md "Dependencies" |
| Release test needs a classifier that does not exist | Release item allows full migration evidence while it is missing |
| A-15 cited a source that does not state it | Marked *new*, with the sources it derives from |
| Kiro K-2 crash release and K-3 shared OS user contradicted A-11 and A-15; K-16 too narrow | K-2, K-3, K-16 aligned with A-11, A-12, A-15 |
| Kiro had no session-ID capture or depth mechanism | K-13 captures the ID (evidence: `--resume-id`); K-7 states depth is unbounded unless T1 finds a control; both added as open questions |
| Overlap with open PR #4 | Recorded in the banner and as a T2 open question |
| Stale text (head SHA, A-14/A-15 omissions, kiro "not yet created", missing `status`) | Fixed |

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
