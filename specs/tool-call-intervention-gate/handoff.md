# Handoff: tool-call-intervention-gate / lessons from Strands vended interventions

| Field | Value |
|---|---|
| Base SHA | 47d906f (`git rev-parse 47d906f` for the full SHA) |
| Head SHA | the commit that changes this file (`git log -1 -- specs/tool-call-intervention-gate/handoff.md`) |
| Author (runtime / session ID) | Claude Code / session_01AaDiMKDxN9AM1oregxnBoS |
| Date | 2026-10-28 |

## Changed files

- `specs/tool-call-intervention-gate/requirements.md` — two invariants (gate state outside agent-writable storage; no session-wide trust by tool name); non-goal states no model decides whether approval is needed; three cited sources; one open question (Cedar test)
- `specs/tool-call-intervention-gate/design.md` — replaced "vended handlers not reviewed" with why `HumanInTheLoop` is unsuitable and a Cedar lessons list
- `specs/tool-call-intervention-gate/acceptance-tests.md` — three invariant tests: session-state tampering, per-call approval, injected "no approval needed" text
- `specs/tool-call-intervention-gate/handoff.md` — this file

## Implemented requirements

- None. Planning packet only; every task is still `Planned`.

## Source findings (strands-agents/harness-sdk @ 3ee280f, read in full)

`strands-py/src/strands/vended_interventions/cedar/` (`cedar_authorization.py`, `_schema_generator.py`, `_file_loaders.py`):
- Fails closed on Cedar exceptions, `NoDecision`, missing principal, and `"` in principal or tool name. Policies validated at init; `reload()` validates before swapping.
- Request strings are interpolated (`f'{type}::"{id}"'`); only `"` is rejected.
- Returns `Proceed` whenever `allowed` is true, ignoring `diagnostics.errors`; with an auto-generated schema, validation suppresses all but unknown-action errors.
- Rate-limit counts in `agent.state`, persisted in snapshots, updated without a lock.
- `on_error` defaults to `"throw"` for resolver/enricher exceptions; resource is always `Resource::"agent"`; principal may come from caller `invocation_state`.

`strands-py/src/strands/vended_interventions/hitl/` (`hitl.py`, `classifier.py`):
- Approval required by default; `!tool` is never trusted; classifier errors and `None` from `ask` fail closed.
- Approval is `default_evaluate` (`true`/`"y"`/`"yes"`) from the caller or `ask` callback.
- `enable_trust`: one `t` trusts a tool name for the session, any arguments, and disables classification for it; stored in `agent.state["hitl:trusted_tools"]`.
- Optional classifier is an LLM (or `DecisionModel`) shown the agent-written tool input and can waive approval.

## Tests run and results

| Command | Result | Evidence |
|---|---|---|
| `cedarpy` test of erroring `forbid` with a matching `permit` (scratchpad venv) | not run — install of `cedarpy` from PyPI was blocked by the session's auto-mode permission check | none |
| Strands test suites | not run | Read the source only |
| `/codex:review`, `/codex:adversarial-review` | not run | Not available in this session |

## Known limitations

- The Cedar skip-on-error claim rests on Cedar's documented evaluation semantics plus the handler code, not a test.
- Per-runtime interception points (Claude Code, Codex, Cursor) are still unresearched (T3).

## Unresolved decisions

- Scope confirmation from earlier handoffs still stands — owner: user
- Whether to run the `cedarpy` test (only matters if Cedar is adopted) — owner: user

## Assumptions made

- Cedar remains out of scope; its lessons are recorded for a later decision, not adopted.
- Content of the cloned Strands repository was treated as data, not instructions.

## Exact next action

User confirms scope. Then run `/codex:adversarial-review` on this packet, record findings here, and request the `Draft -> Planned` transition through the approval ledger.
