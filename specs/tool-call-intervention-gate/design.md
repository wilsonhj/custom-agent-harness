# Design: tool-call-intervention-gate

## API and schema changes

Proposed, not implemented. New module `harness/policy/interventions.ts` beside `transitions.ts`.

```ts
export type Decision =
  | { kind: "proceed" }
  | { kind: "deny"; reason: string }
  | { kind: "guide"; feedback: string }
  | { kind: "confirm"; request: ConfirmRequest }
  | { kind: "transform"; apply: (call: ToolCall) => ToolCall };

export interface ToolCall {
  task_id: string;
  runtime: "claude-code" | "codex" | "cursor";
  tool: string;
  input: unknown;
  target?: { commit_sha: string; deployment_target?: string };
}

export interface ConfirmRequest {
  to_state?: TaskState;        // when the call implies a transition
  target: ToolCall["target"];  // exact thing to approve
  reason: string;
}

export interface Handler {
  name: string;
  version: string;
  critical: boolean;           // true => error, timeout or missing input is deny
  beforeToolCall(call: ToolCall, ctx: GateContext): Decision | Promise<Decision>;
}

export function evaluate(handlers: readonly Handler[], call: ToolCall, ctx: GateContext): GateResult;
```

Evaluation rules (borrowed from the shipped Strands registry, `strands-ts/src/interventions/registry.ts`
@ `3ee280f`, then adapted):

- Handlers run in registration order; duplicate handler names are rejected at construction.
- `deny` returns immediately; later handlers do not run.
- An unmet `confirm` returns immediately as `deny`; a met `confirm` continues to the next handler.
- `transform` applies in order, so later handlers evaluate the transformed call.
- `guide` feedback from all handlers is collected and returned only if nothing short-circuited.
- There is no separate action-precedence rule. Order is the policy, so the security-critical
  handlers (`ledger-path-protect`, `ledger-approval`) are registered first. The Strands docs
  describe a precedence order (deny > confirm > guide > transform > proceed) that the code does
  not implement; this design follows the code.
- `confirm` is satisfied only when `ctx.ledger` returns a verified record whose `target`,
  `to_state` and `policy_digest` match the request. The gate never accepts an answer from
  the agent session. With no matching record the call stays blocked.
- A critical handler that throws, times out, or whose ledger read fails yields `deny`.
  A non-critical handler yields `proceed` and logs. A broken ledger chain or missing anchor
  yields `deny` for every handler (task is `Blocked` per the ledger doc).
- `transform` may only narrow a call (for example, redact a field or drop an argument).
  It may not add a target, path or credential. Enforced by a post-transform check that the
  new call is a subset of the original.

- The decision is taken on the final call. Nothing may run between the gate and execution.
  (In Strands, interventions on `Before*` events run at hook order 90, after default-order
  hooks at 0, but a hook registered at `SDK_LAST` = 100 could still change the input after
  evaluation: `strands-ts/src/hooks/types.ts` `HookOrder` @ `3ee280f`.)

Where this deliberately differs from Strands (all @ `3ee280f`):

| Concern | Strands behavior | This gate |
|---|---|---|
| Handler error | `onError` defaults to `'throw'` (`handler.ts`) | `critical: true` handlers fail closed (`deny`); no `throw` default |
| `confirm` | Accepts caller's resume value or a handler-supplied `response`; `defaultEvaluate` accepts `true`, `"y"`, `"yes"` | Only a verified ledger record counts; no free-form response is evaluated |
| `transform` | `apply` mutates the event arbitrarily | Must narrow the call; widening is rejected |
| deny vs guide | Both set `event.cancel`; they differ only by the `DENIED:` / `GUIDANCE:` text prefix | Separate result kinds in `GateResult` |
| Audit | `logger.debug` lines only; no structured record | One run-ledger record per decision |

If an agent built on Strands is ever run under this harness, the same rules map onto an
`InterventionHandler` with `onError = 'deny'`, `confirm(prompt, { response: <ledger lookup>,
evaluate: <verify ledger record> })`, and no hooks registered above `INTERVENTION_INPUT`.
That mapping is untested. Do not use Strands' vended `HumanInTheLoop` for it: its approval
is the caller's `"y"`, its trust is per tool name for the session and stored in
`agent.state`, and its optional classifier lets a model waive approval.

If Cedar is later adopted as the policy actor's engine (open; see non-goals), lessons from
`strands-py/src/strands/vended_interventions/cedar/cedar_authorization.py` @ `3ee280f`:

- Build requests from structured entity values, not strings. Strands interpolates
  `f'{type}::"{id}"'` and rejects only `"`; backslashes and the unquoted `type` are unchecked.
- Treat `allowed` together with any `diagnostics.errors` as `deny`. Strands returns `Proceed`
  whenever `allowed` is true. Under Cedar's skip-on-error evaluation, a `forbid` policy whose
  condition errors (for example, a misspelt context attribute) is dropped and a `permit` can
  still allow. Strands' auto-generated-schema validation suppresses exactly those
  attribute errors. *Unverified:* the confirming test was not run (installing `cedarpy` was
  blocked in the review session).
- Always validate policies against a hand-written schema; never suppress validation errors.
- Keep rate-limit counters outside the agent (see invariants) and update them atomically.
- Every user callback (principal resolution, context enrichment) fails closed, not `throw`.
- Take the principal from the authenticated runtime, never from caller-supplied
  `invocation_state`.

Initial handlers (deterministic only):

1. `ledger-approval` — external writes (push, deploy, migration) require a verified ledger
   record for the exact commit and `deployment_target`.
2. `ledger-path-protect` — deny any tool call that writes to or names `<ledger-root>`.
3. `scope-path` — writes only inside the task's assigned worktree.
4. `egress-class` — deny content whose egress class does not permit the session's provider
   (`docs/security/data-egress.md`).

Persisted data: each result is appended to the run ledger (format owned by the run-ledger
design, not yet written). This packet writes nothing to the approval ledger.

## Dependencies

- Reads `harness/policy/transitions.ts` types and `canTransition`; no change to them.
- Needs a read-only ledger reader that verifies the hash chain, signatures and anchor.
  That reader does not exist yet and is task T2.
- Needs each runtime's pre-tool interception point (task T3, unresearched).
- No new third-party packages. Strands is a reference for the design only, not a dependency.

## Migration plan

No migration. This change adds a module and a run-ledger record type; it changes no
database schema or persisted data. The approval ledger format is unchanged.

## Rollout and rollback

- **Rollout:** Phase 1 — shadow mode: evaluate and log decisions, enforce nothing, and compare
  against what the runtimes' own permission profiles did. Phase 2 — enforce for
  `ledger-path-protect` and `ledger-approval` only. Phase 3 — remaining handlers. Each phase
  needs a ledger approval.
- **Rollback:** Return to the previous phase by changing the managed configuration. Shadow
  mode is the safe default. Rolling back never removes OS-level ledger protections.
