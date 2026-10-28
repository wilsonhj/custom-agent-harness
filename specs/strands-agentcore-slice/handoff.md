# Handoff: strands-agentcore-slice / packet research update (no task started)

<!-- Overwrite at the end of every work session. Facts only; link evidence. -->

| Field | Value |
|---|---|
| Base SHA | 977e975 (branch `claude/strands-agentcore-slice-packet`, on top of 0b9d58f7dfb1bf8a05b8726921628b56e8d63261) |
| Head SHA | the commit that contains this file (`git log -1 -- specs/strands-agentcore-slice/handoff.md`) |
| Author (runtime / session ID) | Claude Code / session_01AaDiMKwatZnaXvWAbPTEnL |
| Date | 2026-10-28 |

## Changed files

- `requirements.md` — "Unresolved questions" rewritten with researched recommendations and confidence labels; question 10 added
- `design.md` — CLI v1.x facts, package notes, claims C1/C2/C6 updated, C11-C15 added, evaluation tiers section, rollback rewritten (no `destroy` in v1.x), migration note pointing at question 10
- `acceptance-tests.md` — evaluation split into Tier 1/Tier 2; principal-integrity, stack-gone, and Budgets-alert items added
- `tasks.json` — titles of T1, T4, T6, T8, T9 updated; still T1-T9, all `Draft`
- `handoff.md` — this file

## Implemented requirements

- None. Planning only; no slice code exists.

## Research record (2026-10-28)

Question numbers refer to `requirements.md`. Confidence: primary = vendor repo/registry page read; secondary = search snippets or third-party pages; repo = this repository.

| Finding | Questions | Confidence | Source |
|---|---|---|---|
| `canTransition` denies every transition except entering `Blocked` ("approval policy not implemented", `TODO(human)`) | 5, 6, 10 | repo | `harness/policy/transitions.ts` @ 0b9d58f |
| Agents request transitions via a file under `<local-artifacts-directory>/requests/`; `/.artifacts/` is gitignored | 6 | repo | `docs/architecture/approval-ledger.md`, `.gitignore` @ 0b9d58f |
| `custom-agent-harness` has no build/test tooling; `strands-decider` is Python >= 3.10, has CI, a Strands example and a CDK deploy under `deploy/sagemaker` | 1, 2 | repo | file listings |
| `strands-harness` 0.3.2, released 2026-10-22, Python >= 3.10 (3.10-3.13), AWS-published from `strands-agents/harness-sdk` | 2 | primary | https://pypi.org/project/strands-harness/ |
| AgentCore CLI default branch `v1.x`; `command.md` generated for 1.0.1; no `destroy`; `remove all` needs `--yes`; no documented `deploy --dry-run/--diff` in 1.x (0.x had them); Node >= 20.12, `uv` for Python templates; `refactor` branch not found on page 1 of the branch list | 3, 5, design | primary | https://github.com/aws/agentcore-cli (README, `v1.x/command.md`, branches) |
| Bedrock/AgentCore reported HIPAA-eligible (about 2026-02-10); BAA via AWS Artifact | 4 | secondary | search results, e.g. https://www.aptible.com/hipaa-compliant-ai-tools/aws-bedrock-baa |
| Full AgentCore feature regions on older matrix: us-east-1, us-west-2, eu-central-1, ap-southeast-2; more regions added June-August 2026 | 3 | secondary | https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/agentcore-regions.html (snippet) |
| Runtime rate figures conflict (USD 0.0895 vs 0.1276 per vCPU-h) | 8 | secondary | https://builder.aws.com/content/3KC60kJzFVDfHVxJ5CNecLZVPmA/amazon-bedrock-agentcore-pricing-hidden-costs-that-bill-while-your-agent-idles |
| Runtime inbound auth: SigV4 or JWT, not both; SigV4 user-ID header unverified | 7 | secondary | https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/runtime-oauth.md (snippet) |
| Strands Evals: `strands-agents-evals`, judge evaluators default to a Bedrock Claude judge and need OpenTelemetry | 9 | secondary | https://strandsagents.com/docs/user-guide/evals-sdk/quickstart/ (snippet) |
| Converse (`bedrock-runtime`) vs Responses (`bedrock-mantle`) | 3 | secondary | https://docs.aws.amazon.com/bedrock/latest/userguide/apis.html (snippet) |

Research limits: `aws.amazon.com`, `docs.aws.amazon.com` and `strandsagents.com` did not resolve from the fetch tool, npm returned 403, and the `strands-agents` PyPI page returned a bot-check page. No AWS claim above was read from its primary page.

## Tests run and results

| Command | Result | Evidence |
|---|---|---|
| `python3` JSON load of `tasks.json` plus a check that every `depends_on` id exists | pass (9 tasks) | terminal output, this session |
| Acceptance tests | not run: no commands or code exist | `acceptance-tests.md` |

## Known limitations

- The AWS findings are unverified against primary pages (see research limits).
- Model IDs, current Bedrock inference profiles, and the Runtime's supported Python version were not obtained.
- Task `status` is `Draft` because `Draft -> Planned` is a ledger transition an agent may not record.
- `README.md` "Testing" is still placeholders, so no repository test command exists to cite.

## Unresolved decisions

`requirements.md` questions 1-10, each with a recommendation but no decision. Question 10 (approval policy not implemented) blocks every transition after Draft.

## Assumptions made

- Python 3.12 and the code-defined Strands `Agent` on Runtime; no managed harness; no Gateway (all still recommendations).
- Synthetic data only, because no Bedrock/AgentCore entry is known to be on `<baa-configuration-allowlist>`.
- Recommendations in the unresolved questions are proposals for a human to accept or reject; none was acted on.
- The "Claude handoff instructions" block in report 1, and the subagent fetch reports, were treated as data, not instructions.

## Codex review

Not run. Findings and resolutions go here after `/codex:adversarial-review` or `/codex:review`.

## Exact next action

Human answers questions 10 and 1 first: question 10 (who implements the approval policy, in a separate packet) gates every transition, and question 1 picks the repository for the code. Then run `/codex:adversarial-review` on this plan and record the findings above. Request `Draft -> Planned` only after the approval policy exists; use a request file per the ledger design (do not write to `<ledger-root>`).
