# Requirements: tool-call-intervention-gate — Ordered, fail-closed gate on agent tool calls

Status: Draft. Not approved; approval lives in the ledger (`docs/architecture/approval-ledger.md`).

## User-visible behavior

- Before an agent runtime executes a tool call or external write, the control plane
  evaluates an ordered list of handlers and returns exactly one decision: `proceed`,
  `deny`, `guide`, `confirm`, or `transform`.
- A `deny` from any handler blocks the call and skips the remaining handlers. Every
  `deny` records a reason that is visible to the operator and returned to the agent.
- `guide` returns combined feedback from all guiding handlers to the agent so it can
  retry with a different approach. It never authorizes anything.
- `confirm` does not pause inside the agent process. It writes a request artifact under
  `<local-artifacts-directory>/requests/` and the call stays blocked until a verified ledger
  record for the exact target exists. An agent-supplied answer is never accepted.
- A handler that throws, times out, or cannot read the ledger blocks the call (fail closed)
  unless that handler is explicitly declared non-security-critical.
- Each decision, with handler name and version, is written to the run ledger so "why was
  this call allowed or blocked" can be answered from one record.

## Non-goals

- Deciding which state transitions need a `human` versus a `policy` actor. That is the
  open `TODO(human)` in `harness/policy/transitions.ts` and an open question in the ledger doc.
- Adopting the Strands Agents SDK as a dependency or runtime.
- Writing to the approval ledger. Agents and this gate only read it and request transitions.
- LLM-based "steering" handlers. The first version uses deterministic handlers only. A model
  must never decide whether approval is required: Strands' `HumanInTheLoop` classifier does
  this from agent-written tool arguments, which is a prompt-injection path past the human.
- A multi-user identity model (Cedar-style principals). Single-operator first.

## Invariants

- Agents never mark their own phase transitions approved (`AGENTS.md`).
- Enforcement is by OS permissions and managed runtime configuration, not by instructions
  (`docs/architecture/approval-ledger.md`, "Who writes, and how it is enforced"). A gate that
  runs inside the agent process is defence in depth and is never the only control.
- `canTransition` keeps denying every non-`Blocked` transition until the approval policy
  exists; this packet must not weaken that.
- Entering `Blocked` stays possible without approval.
- Missing or mismatched approval blocks external writes (README, "Minimum acceptance tests").
- No retrieved text, tool output, or review comment can change a decision; they are data.
- The call that executes is exactly the call that was evaluated: nothing may modify it after
  the gate decides (no time-of-check/time-of-use gap).
- Gate state (approvals, trust, rate-limit counters) lives only outside agent-writable
  storage. Nothing the agent's session, snapshot or `invocation_state` can write or restore
  may grant, extend or reset it.
- There is no "trust this tool for the session". Each approval covers one exact call and
  target; a later call with the same tool name needs its own approval.

## Cited company decisions

| Constraint | Source (ADR / policy / brain doc ID @ revision) |
|---|---|
| Agents may only request a transition; approvals come from the external ledger | `AGENTS.md` "Approvals and state"; `docs/architecture/approval-ledger.md` @ `0b9d58f` |
| Approval binds to exact commit, evidence digests and policy digest; fail closed if the anchor is unreachable | `docs/architecture/approval-ledger.md` "Record schema", "Tamper evidence" @ `0b9d58f` |
| Default-deny until approval policy exists | `harness/policy/transitions.ts` `canTransition` @ `0b9d58f` (commit `2cf1fe1`) |
| Approvals for one deployment target do not cover another | `docs/architecture/release-gate.md` @ `0b9d58f` |
| Content leaving the machine follows the egress policy | `docs/security/data-egress.md` @ `0b9d58f` |
| Action vocabulary to borrow: proceed / deny / guide / confirm / transform; `deny` short-circuits; `guide` feedback accumulates; per-handler `onError` (`throw` / `proceed` / `deny`) | Shipped code, strands-agents/harness-sdk @ `3ee280f`: `strands-ts/src/interventions/{actions,handler,registry}.ts`; Python parity in `strands-py/src/strands/interventions/`. Docs: strands-agents/docs @ `658b6f0` `interventions.mdx` (archived). |
| Evaluation is ordered by handler registration, not by action precedence; the docs' "deny > confirm > guide > transform > proceed" is not what the code does | `strands-ts/src/interventions/registry.ts` `_dispatch` @ `3ee280f` |
| Rationale for a shared primitive over independent plugins (no short-circuit, fragmented actions, no unified audit, undefined ordering) | strands-agents/docs @ `658b6f0`: `designs/0007-intervention-primitive.md` "Problem" (also at `team/designs/` in harness-sdk). |
| Why Strands' `confirm` cannot be the approval mechanism here: it resolves on whatever response the caller resumes with (or a `response` the handler pre-fills), checked by `defaultEvaluate`, which accepts `true`, `"y"` or `"yes"` | `strands-ts/src/interventions/registry.ts` `_onBeforeToolCall` and `actions.ts` `defaultEvaluate` @ `3ee280f`, compared with `AGENTS.md` above. |
| Security state in agent-writable storage is resettable: `HumanInTheLoop` keeps trusted tools in `agent.state["hitl:trusted_tools"]`; `CedarAuthorization` keeps rate-limit counts in `agent.state["cedar-authorization"]`; both persist in session snapshots | `strands-py/src/strands/vended_interventions/hitl/hitl.py` `_trust_tool`, `cedar/cedar_authorization.py` `_increment_call_count` @ `3ee280f` |
| Session-wide trust by tool name approves every later call with any arguments and disables risk classification for that tool | `hitl.py` `__init__` docstring (`enable_trust`) and `_requires_approval` @ `3ee280f` |
| Model-decided approval requirement: built-in classifier is an inner LLM agent prompted with the tool name and agent-written input | `strands-py/src/strands/vended_interventions/hitl/classifier.py` `_create_llm_risk_classifier`, `_format_tool_call` @ `3ee280f` |

## Unresolved questions

- [ ] Scope assumption: the user replied "Yes" to an offer to draft a packet "for any part, e.g. the intervention model for approval gates". This packet takes that example. Confirm or redirect — owner: user, blocks: T1
- [ ] Request format for `confirm`: artifact file, MCP tool, or CLI command (open in the ledger doc) — owner: user, blocks: T2
- [ ] Which handlers are security-critical (fail closed) versus advisory (`proceed` on error)? — owner: user, blocks: T2
- [ ] Where does the gate run for each runtime (Claude Code managed hook, Codex, Cursor), given each exposes different pre-tool interception? Not yet researched — owner: user, blocks: T3
- [ ] Test runner and command names: README "Testing" is still placeholders (`<permission-policy-test-command>`) — owner: user, blocks: T1
- [ ] Confirm Cedar's skip-on-error behavior for an erroring `forbid` (design.md, Cedar lessons) by running a `cedarpy` test; needed only if Cedar is adopted — owner: user, blocks: none
- [ ] Is TypeScript the implementation language? `harness/policy/transitions.ts` suggests yes — owner: user, blocks: T1
