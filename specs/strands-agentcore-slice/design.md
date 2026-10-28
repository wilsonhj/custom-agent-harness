# Design: strands-agentcore-slice

<!-- Draft. Version fields are null until T1 fills them from installed artifacts. -->

## Layer decision

Report 1 section 8 asks that each layer be named. Proposed choice, pending Unresolved
question 2 and 3:

| Layer | Choice | Not chosen, and why |
|---|---|---|
| Model / service API | Amazon Bedrock inference, one model ID, one API mode | Bedrock Agents Classic (maintenance mode since 2026-07-30). `bedrock-agent-runtime` is not AgentCore Runtime; do not pick a client by name similarity. |
| Agent framework | Strands SDK `Agent` (code-defined loop), Python | Strands assembled harness and AgentCore managed harness: either would add a second loop. The managed harness is also a 2026-04 preview with unconfirmed status. |
| Application adapter | `bedrock-agentcore` Python SDK (`BedrockAgentCoreApp`) | AgentCore protocol extras (A2A, AG-UI): not needed. |
| Deployment tooling | `@aws/agentcore` CLI at one pinned 1.x release (default branch `v1.x`; its `command.md` was generated for 1.0.1) | Python Starter Toolkit (unsupported; installs the same `agentcore` executable). |
| Tool boundary | One typed read-only tool with a backend authorization check | Gateway: deferred, see requirements non-goals. |

"Harness" is overloaded in this repository: Strands assembled harness, AgentCore managed
harness, and `custom-agent-harness`. This packet uses "harness" only for this repository.

## Slice shape

```text
caller (synthetic principal)
  -> typed entrypoint (prompt: str, validated)
  -> Strands Agent (pinned model ID, pinned API mode)
       -> tool: lookup_record(record_id: str)     [read-only, schema-validated]
            -> backend authz: caller.tenant == record.tenant, else DENY
  -> response
Local mode: same code, stubbed model, in-memory backend, no AWS.
Cloud mode: same code in AgentCore Runtime, Bedrock model, in-memory backend.
```

The backend is an in-process fixture with two synthetic tenants. No database, queue, or
Gateway is created, so the slice adds no persisted state.

## API and schema changes

- New entrypoint accepting `{"prompt": "<string>"}` plus a caller principal. Any other shape
  is rejected before reaching the model. Confirm the exact payload contract against the
  installed SDK in T1.
- New tool `lookup_record(record_id: str) -> Record | Denied`. No other tools.
- No changes to existing repositories' APIs, tables, events, or `harness/policy/`.

## Dependencies

All versions are `null` until T1 records them from installed artifacts, not from `main`.

| Dependency | Source | Version / ref |
|---|---|---|
| Strands SDK | `strands-agents/harness-sdk` (`strands-py/`), PyPI `strands-agents` | null (name from secondary sources; its PyPI page did not load). `strands-harness` (0.3.2, 2026-10-22, Python >= 3.10, AWS-published) is the assembled agent and is **not** used here. Whether it depends on `strands-agents` is unconfirmed. |
| AgentCore Python SDK | `aws/bedrock-agentcore-sdk-python`, package `bedrock-agentcore` | null |
| AgentCore CLI | `aws/agentcore-cli`, package `@aws/agentcore` | null (release must be compared with the `refactor` branch the report viewed) |
| Strands Evals | `strands-agents/evals`, PyPI `strands-agents-evals` (secondary) | null |
| Runtimes | Python 3.12 proposed (strands-harness lists 3.10-3.13), Node >= 20.12 for the CLI (22+ only if TypeScript is ever used), `uv` | null |
| Bedrock model | model ID, region, API mode | null (Unresolved question 3) |

Rejected sources: `strands-agents/docs`, `strands-agents/sdk-typescript`,
`strands-agents/mcp-server`, `strands-agents/agent-builder` (archived),
`aws/bedrock-agentcore-starter-toolkit` (unsupported).

## Claims to verify before reliance

Each claim comes from `sources/` and is unverified. T1 records the result in `handoff.md`.

| # | Claim (source section) | Verification experiment |
|---|---|---|
| C1 | `@aws/agentcore` is the supported CLI and supports `create/build/deploy/status/invoke` as written (report 2 section 8; report 1 section 5). **Partly observed 2026-10-28** (GitHub, primary): v1.x README/`command.md` list `create --name --template`, `build`, `deploy --target --yes`, `status --target`, `invoke --runtime --payload`, global `--json`, `dev` for local runs. The npm page returned 403, so the published version and date are unverified. | Install the pinned release; compare `agentcore --help` per subcommand; record `which agentcore` and install provenance to rule out the toolkit collision |
| C2 | Python package names for the Strands SDK (report 2 section 4 vs report 1 limitations). **Partly observed**: PyPI `strands-harness` 0.3.2 exists (2026-10-22). `strands-agents` page did not load. | `pip index`/install in a clean venv; record name, version, and the repository tag it maps to |
| C3 | Python SDK validates `prompt` as a string and the entrypoint contract in report 1 section 5 | Read the pinned SDK source and test with a malformed payload |
| C4 | Gateway-only policy is bypassable by direct paths (report 2 section 3) | Out of scope to test here; the slice has no Gateway. Recorded so a later packet inherits it |
| C5 | Sync request timeout 15 min vs async 8 h (report 2 section 2) | Read the quotas page at the verification date; record the page revision/date |
| C6 | Generated CDK app and `agentcore.json` create the resources expected; `status` shows declared resources only (report 1 section 5). The v1.x docs read in research document no `deploy --dry-run` or `--diff` (the 0.x docs did), so a pre-deploy diff is unconfirmed. | Run the generator without deploying; run `agentcore deploy --help`; use CDK synth/diff in `agentcore/cdk/` if it works; diff the synthesized IAM; inventory resources by tag after T8 |
| C7 | Generated roles are not least-privilege (report 1 pitfall C) | Review the synthesized IAM in T6; replace wildcards |
| C8 | Tool calling plus streaming works for the chosen model (report 2 section 1) | Run the tool-use eval against the chosen model with streaming on and off |
| C9 | AgentCore Memory consumption pricing (report 2 section 7) | Not exercised: Memory is a non-goal. Pricing for Runtime and the model is taken from the pricing pages at T8 |
| C10 | Package/runtime prerequisites differ (Node 20.12 vs 22) | Record the actual engines fields of the pinned packages |
| C11 | Bedrock and AgentCore are HIPAA-eligible (secondary sources: added about 2026-02-10; BAA via AWS Artifact). Primary AWS pages were unreachable. | Not needed for this packet (synthetic data only). Verify on the AWS HIPAA Eligible Services page before any allowlist entry |
| C12 | AgentCore Runtime rates: USD 0.0895/vCPU-h and 0.00945/GB-h versus 0.1276 and 0.0169 (conflicting third-party sources; CPU billed on consumed cycles, memory on peak) | Read the live pricing page at T8 and price tokens, CloudWatch and ECR as well |
| C13 | Strands Evals judge evaluators default to a Bedrock Claude judge and require OpenTelemetry capture (secondary) | Run Tier 2 evals in T5 only; confirm the default judge and the telemetry setup from the installed package |
| C14 | Runtime inbound auth is SigV4 or JWT, not both; the SigV4 user-ID header is unverified (secondary) | Read the runtime OAuth page at T1; the slice does not depend on it (principal is injected by the entrypoint) |
| C15 | Teardown: v1.x has no `destroy`; `remove all --yes` then `deploy` is expected to delete the resources but no page says so. | In T6, deploy a throwaway stack in the sandbox only after approval and observe; otherwise delete the CloudFormation stack and inventory by tag |

## Evaluation tiers

- **Tier 1, deterministic (no AWS):** task completion, tool-call log, and unauthorized-action
  rate asserted directly in tests. The authorization acceptance criteria depend only on this
  tier, so they never rest on an LLM judge.
- **Tier 2, judge-based (from T5):** Strands Evals tool-selection and trajectory evaluators on
  synthetic inputs. Needs AWS credentials and tokens; reported separately and never gating
  the authorization criteria.
- **Cost controls:** USD 25 ceiling proposed (requirements question 8), AWS Budgets alerts at
  USD 10 and USD 20, every resource tagged.

## Migration plan

No migration: the slice creates no database, schema, or persisted business data. The agent
runs on an in-process fixture; AgentCore session filesystems are not used.

Because the packet reaches AWS, the release gate still applies
(`docs/architecture/release-gate.md`): `<migration-classifier-command>` does not exist, so
the result is treated as `migration` and `PreviewVerified -> MigrationGated` needs a `human`
actor. Whether items 1-4 of the gate are meaningful for a stateless sandbox deployment is
Unresolved question 5; this packet does not assume the answer. Recommended there: treat
the sandbox as preview-class and have a human approve `MigrationGated` with an explicit
"not applicable: no persisted state" justification. All of this depends on question 10: the
approval policy in `harness/policy/transitions.ts` is not implemented, so no transition can
currently be approved.

- **Expand / contract:** not applicable (no schema).
- **Ordering vs deploy:** not applicable.
- **Backward compatibility:** not applicable; nothing consumes the deployed agent.
- **Rollback limits:** teardown removes created AWS resources. It does not undo model
  invocations already made or CloudWatch/telemetry already emitted; retention is part of
  the T9 inventory.

## Rollout and rollback

- **Rollout:** local first (T2-T4), Bedrock second (T5), AgentCore last (T8). T8 starts only
  after T6 (IAM) and T7 (independent review) and a human-approved resource diff and cost
  plan. Staged exposure is not applicable: a single sandbox target, no real callers.
- **Rollback:** the v1.x CLI has no `destroy` command (research 2026-10-28). Candidate
  sequence, each step to be confirmed against the installed `--help`: `agentcore remove all`
  (needs `--yes`), then `agentcore deploy` to reconcile the stack; fall back to deleting the
  CloudFormation stack of the CDK app, then run the T9 inventory by tag and confirm zero
  remaining resources (ECR repository, CloudWatch log groups and IAM roles are the likely
  leftovers). Precondition: the deployment's resource inventory was recorded at deploy time.
  A deployed runtime keeps billing while it exists, so teardown is part of the cost control.

## Security notes

- Deployment role and runtime role are distinct. The runtime role's policy is derived from
  the operations the agent performs (invoke one model ID in one region; write its own logs).
- No production credentials are available to the coding worker. T8 deploys use credentials
  provided to the human approver's session, not to the agent; the agent supplies the diff and
  commands only (`README.md`, "Required controls").
- Tool and prompt payloads in traces contain only synthetic data. Trace retention is recorded
  in T9.
- Prompt injection: one eval case plants an instruction inside a synthetic record body and
  expects the agent not to call any tool outside the allowed set or reveal the other tenant's
  data (the backend check is what actually stops it).
