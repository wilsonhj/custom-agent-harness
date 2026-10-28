# Acceptance tests: strands-agentcore-slice

Each item names the command or procedure that proves it and the requirement it covers.
Check an item only when the result is recorded in `handoff.md`.

**No test commands exist yet.** `README.md` "Testing" is still placeholders and no slice
code exists. Commands below are `<placeholders>` that T2-T4 must define and record in this
file before the item can be checked. Do not substitute ad-hoc commands and report them as
passing (`AGENTS.md`, "While working").

## Behavior

- [ ] Local run answers via exactly one tool call with a stubbed model, no AWS credentials in the environment — `<local-test-command>` — covers: local run bullet
- [ ] Bedrock path returns an answer for a synthetic prompt using the pinned model ID and API mode — `<bedrock-smoke-command>` — covers: Bedrock path bullet
- [ ] Tenant A principal requesting a tenant B record gets `Denied` from the backend and the agent output contains no tenant B field — `<authz-negative-test-command>` — covers: authorization bullet
- [ ] Same denial holds when the prompt instructs the agent to ignore the restriction (injection case) — `<authz-injection-test-command>` — covers: authorization bullet
- [ ] Non-string prompt, missing prompt, and an over-length prompt are rejected before any model call (call count is zero) — `<input-validation-test-command>` — covers: typed boundary bullet
- [ ] Tool argument failing its schema is rejected before the backend is called — `<tool-schema-test-command>` — covers: typed boundary bullet
- [ ] Deployed agent returns an answer to a JSON payload via the pinned CLI's invoke command — `<agentcore-invoke-command>` — covers: deployment bullet
- [ ] Tier 1 (deterministic, no AWS): task completion, tool-call log, and unauthorized-action rate over the fixed case set; unauthorized-action rate is zero — `<eval-tier1-command>` — covers: evaluation bullet
- [ ] Tier 2 (Strands Evals, synthetic inputs only): tool-selection and trajectory scores reported separately; not a gate for the authorization criteria — `<eval-tier2-command>` — covers: evaluation bullet
- [ ] The principal cannot be changed by prompt text or tool arguments (injection and argument-tampering cases keep the original tenant) — `<principal-integrity-test-command>` — covers: authorization bullet
- [ ] Teardown leaves zero resources carrying the slice's tag, and the CloudFormation stack is gone — `<inventory-command>` — covers: teardown bullet
- [ ] AWS Budgets alerts at the approved thresholds exist before the first deploy — `<budget-check-command>` — covers: cost invariant

## Invariants

- [ ] Static check: no `*` action or resource in any IAM policy in the CDK synth output — `<iam-lint-command>`
- [ ] Deployment role and runtime role are different principals — `<iam-roles-check-command>`
- [ ] Only synthetic fixtures are present in prompts, traces, and eval inputs (fixture manifest matches) — `<fixture-check-command>`
- [ ] Version manifest has no `null` field and every version is the installed one — `<manifest-check-command>`
- [ ] No path under `<ledger-root>` is referenced or written by any slice command — `<ledger-reference-check-command>`
- [ ] Missing or mismatched approval blocks the AWS write: deploy command refuses without the recorded approval for this commit and target (README minimum acceptance test) — `<approval-block-test-command>`
- [ ] Deployment evidence names the intended commit SHA and `<provider>/<project>/<environment>` target (README minimum acceptance test) — `<evidence-target-check-command>`

## Migration

Delete only if Unresolved question 5 resolves to "gate not applicable". Until then:

- [ ] Reviewer confirms in writing that no persisted data is created, and records the classifier outcome (`migration` by fail-closed default) — `<link to review finding>`
- [ ] Recovery procedure is documented (teardown steps, residual telemetry, owner) — `<link>`

## Preview

- [ ] Smoke checks pass on the sandbox AgentCore deployment (same cases as the local smoke test, real model) — `<agentcore-smoke-command>`
