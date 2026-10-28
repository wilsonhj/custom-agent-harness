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
- LLM-based "steering" handlers. The first version uses deterministic handlers only.
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

## Cited company decisions

| Constraint | Source (ADR / policy / brain doc ID @ revision) |
|---|---|
| Agents may only request a transition; approvals come from the external ledger | `AGENTS.md` "Approvals and state"; `docs/architecture/approval-ledger.md` @ `0b9d58f` |
| Approval binds to exact commit, evidence digests and policy digest; fail closed if the anchor is unreachable | `docs/architecture/approval-ledger.md` "Record schema", "Tamper evidence" @ `0b9d58f` |
| Default-deny until approval policy exists | `harness/policy/transitions.ts` `canTransition` @ `0b9d58f` (commit `2cf1fe1`) |
| Approvals for one deployment target do not cover another | `docs/architecture/release-gate.md` @ `0b9d58f` |
| Content leaving the machine follows the egress policy | `docs/security/data-egress.md` @ `0b9d58f` |
| Action vocabulary and ordering to borrow: proceed / deny / guide / confirm / transform; `deny` short-circuits; `guide` feedback accumulates; per-handler `onError` (`throw` / `proceed` / `deny`) | strands-agents/docs @ `658b6f0` (archived; moved to strands-agents/harness-sdk): `site/src/content/docs/user-guide/concepts/agents/interventions.mdx`, `designs/0007-intervention-primitive.md` |
| Rationale for a shared primitive over independent plugins (no short-circuit, fragmented actions, no unified audit, undefined ordering) | strands-agents/docs @ `658b6f0`: `designs/0007-intervention-primitive.md` "Problem". Proposal text; interface status in the SDK is unverified. |
| Why Strands' in-process `interrupt` cannot be the approval mechanism here | Derived by comparing `site/src/content/docs/user-guide/concepts/interrupts.mdx` @ `658b6f0` (the caller of the agent supplies the response) with `AGENTS.md` above. |

## Unresolved questions

- [ ] Scope assumption: the user replied "Yes" to an offer to draft a packet "for any part, e.g. the intervention model for approval gates". This packet takes that example. Confirm or redirect — owner: user, blocks: T1
- [ ] Request format for `confirm`: artifact file, MCP tool, or CLI command (open in the ledger doc) — owner: user, blocks: T2
- [ ] Which handlers are security-critical (fail closed) versus advisory (`proceed` on error)? — owner: user, blocks: T2
- [ ] Where does the gate run for each runtime (Claude Code managed hook, Codex, Cursor), given each exposes different pre-tool interception? Not yet researched — owner: user, blocks: T3
- [ ] Test runner and command names: README "Testing" is still placeholders (`<permission-policy-test-command>`) — owner: user, blocks: T1
- [ ] Is TypeScript the implementation language? `harness/policy/transitions.ts` suggests yes — owner: user, blocks: T1
