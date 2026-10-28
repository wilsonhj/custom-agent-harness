# Acceptance tests: tool-call-intervention-gate

Each item names the command or procedure that proves it and the requirement it covers.
Check an item only when the result is recorded in `handoff.md`.

No test runner is defined yet. README "Testing" lists `<permission-policy-test-command>`
as a placeholder; T1 must replace it before any item below can be checked. Do not substitute
an ad-hoc command.

## Behavior

- [ ] `deny` from the first handler skips later handlers — `<permission-policy-test-command>` — covers: "A `deny` from any handler blocks the call and skips the remaining handlers"
- [ ] `guide` feedback from two handlers is combined into one message — `<permission-policy-test-command>` — covers: "`guide` returns combined feedback"
- [ ] `confirm` with no ledger record keeps the call blocked — `<permission-policy-test-command>` — covers: "`confirm` ... stays blocked until a verified ledger record"
- [ ] `confirm` with an agent-supplied "yes" is still blocked — `<permission-policy-test-command>` — covers: "An agent-supplied answer is never accepted"
- [ ] Ledger record for a different commit or `deployment_target` does not satisfy `confirm` — `<permission-policy-test-command>` — covers: "exact target"
- [ ] Critical handler that throws, times out or cannot read the ledger yields `deny` — `<permission-policy-test-command>` — covers: "fails closed"
- [ ] Broken hash chain, bad signature, wrong namespace or missing anchor yields `deny` — `<permission-policy-test-command>` — covers: "fails closed"
- [ ] `deny` registered after a `guide` still blocks, and the guide feedback is not returned — `<permission-policy-test-command>` — covers: registration-order evaluation
- [ ] Duplicate handler names are rejected at construction — `<permission-policy-test-command>` — covers: design "duplicate handler names"
- [ ] `confirm` given `true`, `"y"` or `"yes"` with no ledger record is still blocked — `<permission-policy-test-command>` — covers: "An agent-supplied answer is never accepted"
- [ ] `transform` that adds a path, target or credential is rejected — `<permission-policy-test-command>` — covers: design "transform may only narrow"
- [ ] Every result is recorded with handler name and version — `<permission-policy-test-command>` — covers: "Each decision ... is written to the run ledger"

## Invariants

- [ ] A call modified after the gate's decision does not execute (time-of-check/time-of-use) — `<permission-policy-test-command>`
- [ ] A tool call naming `<ledger-root>` is denied, including via a shell command string — `<permission-policy-test-command>`
- [ ] `canTransition` still denies every non-`Blocked` transition — `<permission-policy-test-command>`
- [ ] Entering `Blocked` still needs no approval — `<permission-policy-test-command>`
- [ ] Text inside tool output or retrieved documents cannot change a decision (injection fixtures) — `<permission-policy-test-command>`
- [ ] Missing or mismatched approval blocks external writes — README minimum acceptance test, `<permission-policy-test-command>`
- [ ] Denied actions are tested, not only allowed ones (README "Required controls") — `<permission-policy-test-command>`

## Migration

No migration (see `design.md`).

## Preview

- [ ] Shadow mode run on a real task produces a decision log with no behavior change — `<end-to-end-preview-test-command>`
- [ ] In a sandbox, an agent process cannot write `<ledger-root>` regardless of the gate (OS permission is the boundary) — `<procedure to be written in T3>`
