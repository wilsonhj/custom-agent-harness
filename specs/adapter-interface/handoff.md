# Handoff: adapter-interface / packet creation

| Field | Value |
|---|---|
| Base SHA | e769f18 (main after kiro-adapter merged) |
| Head SHA | 9bb0630 (packet before this session's third-round fixes; they land in the next commit) |
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
| `npm run check` | passed (2026-11-04 correction) | JavaScript syntax checks passed |
| `git diff --check` | passed (2026-11-04 correction) | No whitespace errors |
| Packet consistency check | passed (2026-11-04 correction) | All 5 non-template task packets parse; 23 task dependencies resolve without cycles; every normative A/K requirement has planned acceptance coverage |
| New adapter acceptance suites | not run | Test files are created in T3; existing-suite CI is required on the correction commit |

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
| Kiro had no session-ID capture or depth mechanism | K-13 captures the ID (evidence: `--resume-id`); K-7 depth handling (revised again in round 3); both added as open questions |
| Overlap with open PR #4 | Recorded in the banner and as a T2 open question |
| Stale text (head SHA, A-14/A-15 omissions, kiro "not yet created", missing `status`) | Fixed |

## Review round 3: /code-review plus validation (Claude, same session; NOT Codex), 2026-11-01

Validation on 9bb0630 merged with current main (PR #4 merged): clean merge; all six
`tasks.json` parse and every dependency resolves; every A- and K- requirement is covered by
an acceptance test and vice versa; `HARNESS_REQUIRE_SANDBOX=1 npm test` 80 tests, 79 pass,
1 skip (by design); `npm run check` and `git diff --check` clean. Review findings, all fixed
in the next commit:

| Finding | Resolution |
|---|---|
| K-11 let Kiro output count as evidence, contradicting A-17 | K-11 stores output as a runtime claim; evidence only as A-17 allows |
| A-17 forbade review findings, which approval-ledger.md lists as evidence | A-17 allows harness-run validation output and schema-valid reviewer findings; free-form claims never |
| K-3 test ran Kiro as the adapter's user | Test runs Kiro as the separate runtime user |
| Adapter user's ledger write ban was dropped | New A-21 (adapter and runtime users), referenced by K-3, with a test |
| K-7 accepted unbounded delegation depth | Kiro agents get no `subagent/<name>` entries unless a depth control exists |
| Run ledger had no outcome field | A-13 adds run outcome and failure reason; test checks a cancelled run |
| A-2 test both rejected and downgraded a caller profile | Test: a `start` call with a profile field is rejected; a reviewer task's record resolves to `analysis` |
| Ad hoc test command | `npm test`, now on main |
| Cited a non-existent data-egress.md "Rules" section | Cites "Egress classes" (Rules) |
| Session ID source ambiguous; stale handoffs | A-7 records the ID in the task record and run ledger; head SHAs updated |

## Known limitations

- Independent Codex review of `381bc10` found the validation-boundary omission recorded
  below; final review of the correction and fresh CI remain required before merging.
- Adapter implementation and the new acceptance suites remain future T3 work.

## Codex review correction, 2026-11-04

- Review: a separate Codex reviewer inspected `7fce6c6...381bc10` read-only, without the
  coordinator's session history; one Important finding, no Critical findings.
- Verified issue: A-17 accepted harness-run validation without extending the runtime-user
  isolation rules to candidate-controlled validation code, allowing an implementation to
  run candidate tests with the evidence-writing adapter account's authority.
- Fix: new A-22 requires an independently trusted verifier entrypoint configured outside
  the candidate and harness-enforced isolation of every invoked script, a read-only
  candidate, isolated scratch writes, restricted reads/network, and no controller
  credentials or artifact/evidence/ledger write authority; A-17 now requires A-22.
- `design.md` records this execution boundary and fail-closed behavior; the planned
  `tests/adapter-validation.test.ts` acceptance case probes malicious validation code,
  verifier replacement and unavailable isolation. No runtime code was changed.
- Source: `docs/local-workflow.md` "Trust and installation" and "Operator configuration",
  already merged via PR #4; this carries forward its validation trust boundary.
- Assumption: this is a packet correction, not authorization to choose T2's isolation
  backend or implement T3; the proposed status and human decision gate remain in force.
- Validation results and the final independent review will be recorded on PR #6 against
  the exact correction commit; the new adapter acceptance suites are not claimed as run.

## Unresolved decisions

- See requirements.md "Unresolved questions" — owner: TBD (human), in T2.

## Assumptions made

- Packet ID `adapter-interface` from the user's request ("start the adapter-interface packet"); it covers both the interface and worktree ownership, as the kiro-adapter's former T6 did.
- TypeScript under `harness/` is proposed (to match `transitions.ts`) but left as an open question.
- Worktree name `adapter-interface` in tasks.json is a placeholder.

## Exact next action

Validate and independently review this correction, then merge PR #6 only if ready;
complete the human-owned T2 decisions before any T3 implementation. No task-phase
approval is granted by this handoff or by merging the proposed packet.
