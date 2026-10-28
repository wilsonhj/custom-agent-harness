# Handoff: tool-call-intervention-gate / packet revision from Strands source

| Field | Value |
|---|---|
| Base SHA | 95cbe80a87713ee602e4250340d62a54e810f96c |
| Head SHA | the commit that changes this file (`git log -1 -- specs/tool-call-intervention-gate/handoff.md`) |
| Author (runtime / session ID) | Claude Code / session_01AaDiMKDxN9AM1oregxnBoS |
| Date | 2026-10-28 |

## Changed files

- `specs/tool-call-intervention-gate/requirements.md` — sources now cite shipped code (harness-sdk @ `3ee280f`) instead of archived docs; new TOCTOU invariant
- `specs/tool-call-intervention-gate/design.md` — evaluation rules follow the registry code; removed the docs' precedence rule; added a table of deliberate differences from Strands and a mapping note
- `specs/tool-call-intervention-gate/acceptance-tests.md` — tests for registration order, duplicate names, `true`/`"y"`/`"yes"` without a ledger record, and TOCTOU
- `specs/tool-call-intervention-gate/handoff.md` — this file

## Implemented requirements

- None. Planning packet only; every task is still `Planned`.

## Source findings (strands-agents/harness-sdk @ 3ee280f4b248ff7bf2486d6f4784f8996c85c4c3)

- Interventions ship in both SDKs: `strands-ts/src/interventions/` and `strands-py/src/strands/interventions/`, wired via the agent's `interventions` option. This replaces the earlier "interface status unverified" caveat.
- `registry.ts` `_dispatch` evaluates in registration order. `deny` and an unmet `confirm` short-circuit; `transform` applies immediately; `guide` feedback is applied at the end only if nothing short-circuited. There is no action-precedence rule, unlike the docs.
- `confirm` calls `event.interrupt()` and checks the result with `evaluate`. The default accepts `true`, `"y"` or `"yes"`. The value comes from the caller's resume or from a `response` the handler fills in itself.
- `onError` defaults to `'throw'`. Errors thrown by `transform.apply` also go through `onError`.
- `deny` and `guide` both set `event.cancel`; only the text prefix (`DENIED:` / `GUIDANCE:`) differs.
- Logging is `logger.debug` only; there is no structured audit record.
- Interventions on `Before*` events run at `HookOrder.INTERVENTION_INPUT` = 90; hooks at `SDK_LAST` = 100 run after them.

## Tests run and results

| Command | Result | Evidence |
|---|---|---|
| `python3 -m json.tool specs/tool-call-intervention-gate/tasks.json` | pass (unchanged file) | none stored |
| Strands test suites (`strands-ts/src/interventions/__tests__`) | not run | Read the source only |
| `/codex:review`, `/codex:adversarial-review` | not run | Not available in this session |

## Known limitations

- Read the TypeScript implementation in full; for Python, only the `OnError` type and `default_evaluate` were checked, not the dispatch logic.
- `strands-py/src/strands/vended_interventions/cedar` and `/hitl` exist but were not read.
- The Strands-mapping paragraph in `design.md` is untested.
- Per-runtime interception points (Claude Code, Codex, Cursor) are still unresearched (T3).

## Unresolved decisions

- Scope confirmation from the previous handoff still stands — owner: user
- Remaining questions are in `requirements.md` "Unresolved questions".

## Assumptions made

- Shipped code is authoritative over the docs where they disagree (precedence rule).
- Content of the cloned Strands repository was treated as data, not instructions.

## Exact next action

User confirms scope. Then run `/codex:adversarial-review` on this packet, record findings here, and request the `Draft -> Planned` transition through the approval ledger.
