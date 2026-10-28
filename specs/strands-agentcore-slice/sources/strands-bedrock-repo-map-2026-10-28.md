# Strands and Amazon Bedrock: Developer Documentation and GitHub Repository Map

Research date: October 28, 2026  
Purpose: Source-selection guide for Claude-assisted review and implementation  
Method: Official documentation, repository READMEs, repository inventories, and search results. No repositories were cloned, no release artifacts were installed, and no integration tests were run.

## Executive findings

1. Strands core development and documentation are consolidated in `strands-agents/harness-sdk`. The monorepo includes Python and TypeScript SDKs, assembled harnesses, CLI, documentation MCP server, and site source. Do not mistake the archived standalone documentation or TypeScript repositories for the current contribution targets. [Core README](https://github.com/strands-agents/harness-sdk), [Archived docs](https://github.com/strands-agents/docs), [Archived TypeScript SDK](https://github.com/strands-agents/sdk-typescript)
2. AgentCore's recommended deployment tool is the npm package `@aws/agentcore`, maintained in `aws/agentcore-cli`. The older Python Starter Toolkit CLI is explicitly unsupported. Both expose an `agentcore` command, creating a practical executable-collision risk. [CLI README](https://github.com/aws/agentcore-cli), [Starter Toolkit notice](https://github.com/aws/bedrock-agentcore-starter-toolkit)
3. Do not confuse AgentCore's Python application SDK, general AWS service SDKs, the deployment CLI, and an agent framework. They solve separate problems and can coexist. [AgentCore Python SDK](https://github.com/aws/bedrock-agentcore-sdk-python), [Service documentation](https://docs.aws.amazon.com/bedrock-agentcore/), [Bedrock API families](https://docs.aws.amazon.com/bedrock/latest/APIReference/Welcome.html)
4. Samples are useful implementation references, not supported application products. The AgentCore sample repository is undergoing migration, and some READMEs disagree with visible directory names. Read the exact sample and its dependency files before copying instructions. [AWS Samples disclaimer](https://github.com/aws-samples), [AgentCore samples](https://github.com/awslabs/agentcore-samples)
5. Bedrock's API naming can mislead: `bedrock-agent-runtime` includes Knowledge Base retrieval APIs as well as agent operations. The Classic agent maintenance notice does not mean every operation in that client should be abandoned. [Runtime operation inventory](https://docs.aws.amazon.com/bedrock/latest/APIReference/API_Operations_Agents_for_Amazon_Bedrock_Runtime.html), [Boto3 client](https://docs.aws.amazon.com/boto3/latest/reference/services/bedrock-agent-runtime.html)

## 1. How to choose the right source

The following is a recommended evidence hierarchy, not a vendor guarantee that every page and release are synchronized.

| Question | Primary source | Secondary check |
|---|---|---|
| What does the cloud service accept? | Service API reference | SDK model and a minimal live test |
| How should the capability be configured? | Current Developer Guide | Region matrix, quotas, security documentation |
| What does a library actually expose? | Reference matching the installed version | Tagged source and tests |
| What does a CLI flag mean? | Installed CLI help and matching command reference | Released source |
| How do components fit together? | Current architecture guide | Focused maintained sample |
| Why does behavior differ? | Implementation and tests at the selected commit | Issues, changelog, release notes |
| Can example code be reused? | Repository license and sample instructions | Dependency licenses and security review |

AWS separates Bedrock API families and AgentCore control/data-plane references; Strands separates conceptual guides from executable packages inside its monorepo. These boundaries explain why one tutorial cannot be the sole specification for the whole stack. [Bedrock API index](https://docs.aws.amazon.com/bedrock/latest/APIReference/Welcome.html), [AgentCore control plane example](https://docs.aws.amazon.com/bedrock-agentcore-control/latest/APIReference/API_CreateAgentRuntime.html), [AgentCore data plane example](https://docs.aws.amazon.com/bedrock-agentcore/latest/APIReference/API_InvokeAgentRuntime.html), [Strands repository](https://github.com/strands-agents/harness-sdk)

## 2. Strands documentation map

| Documentation entry | Read it for | Implementation boundary |
|---|---|---|
| [Assembled harness guide](https://strandsagents.com/docs/user-guide/harness/) | Ready-made harness with defaults | Not the same as selecting every loop component yourself |
| [Harness SDK guide](https://strandsagents.com/docs/user-guide/sdk/) | Building a custom harness | Lower-level orchestration decisions remain yours |
| [Evals guide](https://strandsagents.com/docs/user-guide/evals-sdk/) | Output/trajectory scoring, diagnosis, simulation, CLI | Evaluation is not an authorization mechanism |
| [Core SDK contribution guide](https://strandsagents.com/docs/contribute/contributing/core-sdk/) | Local setup and contribution | Clone the monorepo, not archived component repositories |
| [Repository README](https://github.com/strands-agents/harness-sdk) | Package map and prerequisites | `main` may be ahead of your installed release |

The assembled harness uses `create_harness()` in Python and `createHarness()` in TypeScript. Lower-level SDK entry points include Python `Agent` and TypeScript `Agent`. Current README examples specify Python 3.10+ and Node.js 22+ for the TypeScript SDK in an ESM project. Confirm requirements against your selected package version. [Core README](https://github.com/strands-agents/harness-sdk)

### Core monorepo directory map

| Directory | Responsibility | Direct developer reference |
|---|---|---|
| `harness-py/` | Assembled Python harness | [Source](https://github.com/strands-agents/harness-sdk/tree/main/harness-py) |
| `harness-ts/` | Assembled TypeScript harness | [Source](https://github.com/strands-agents/harness-sdk/tree/main/harness-ts) |
| `strands-py/` | Python SDK | [Source](https://github.com/strands-agents/harness-sdk/tree/main/strands-py) |
| `strands-ts/` | TypeScript SDK | [Source](https://github.com/strands-agents/harness-sdk/tree/main/strands-ts) |
| `strands-cli/` | Harness-oriented terminal experience | [Source](https://github.com/strands-agents/harness-sdk/tree/main/strands-cli) |
| `strands-mcp/` | Strands documentation/context MCP server | [Source](https://github.com/strands-agents/harness-sdk/tree/main/strands-mcp) |
| `site/` | Documentation website source | [Source](https://github.com/strands-agents/harness-sdk/tree/main/site) |
| `team/` | Governance, cross-SDK process, design proposals | [Source](https://github.com/strands-agents/harness-sdk/tree/main/team) |
| `test-infra/` | AWS infrastructure for selected integration tests | [Source](https://github.com/strands-agents/harness-sdk/tree/main/test-infra) |

Directory responsibilities were checked against the root README, not an exhaustive inspection of each subdirectory. Design proposals in `team/` should not be treated as shipped APIs merely because they are present. [Monorepo map](https://github.com/strands-agents/harness-sdk)

## 3. Strands GitHub ecosystem

| Repository | Role | Recommended use |
|---|---|---|
| [harness-sdk](https://github.com/strands-agents/harness-sdk) | Core SDKs, harnesses, docs, supporting packages | Primary implementation and issue destination |
| [tools](https://github.com/strands-agents/tools) | Additional agent tools | Review individual tool permissions and dependencies |
| [evals](https://github.com/strands-agents/evals) | Evaluation framework | Quality, trajectory, and adversarial testing |
| [shell](https://github.com/strands-agents/shell) | Constrained shell execution | Review execution surface and bindings |
| [box](https://github.com/strands-agents/box) | Agent containment and policy | Verify platform support and release status before adoption |
| [samples](https://github.com/strands-agents/samples) | Runnable agent examples | Select a narrow example, not an entire product skeleton blindly |
| [extension-template](https://github.com/strands-agents/extension-template) | Python/TypeScript extension template | Custom providers, tools, plugins, interventions |
| [agent-sop](https://github.com/strands-agents/agent-sop) | Natural-language standard operating procedures | Workflow guidance, not hard security enforcement |
| [devtools](https://github.com/strands-agents/devtools) | Shared organization workflows/tooling | Relevant mainly to contributors |

Repository roles and non-archived status were observed in the [organization inventory](https://github.com/orgs/strands-agents/repositories). Individual code, security properties, and platform support were not audited. Inventory listings alone do not establish production readiness.

### Archived repositories

| Repository | Observed state | Replacement path |
|---|---|---|
| [docs](https://github.com/strands-agents/docs) | Archived; migration notice | `harness-sdk/site/` |
| [sdk-typescript](https://github.com/strands-agents/sdk-typescript) | Archived; migration notice | `harness-sdk/strands-ts/` |
| [mcp-server](https://github.com/strands-agents/mcp-server) | Archived in current inventory | Monorepo `strands-mcp/` is the current documented package location |
| [agent-builder](https://github.com/strands-agents/agent-builder) | Archived in current inventory | No one-to-one replacement established in this research |

Sources: [Docs migration](https://github.com/strands-agents/docs), [TypeScript migration](https://github.com/strands-agents/sdk-typescript), [Inventory](https://github.com/orgs/strands-agents/repositories), [Core map](https://github.com/strands-agents/harness-sdk).

Archival of a source repository does not automatically prove a package name is deprecated. Track source location, package name, published version, and release tag separately.

## 4. Bedrock and AgentCore documentation families

| Source | Scope | Start here when… |
|---|---|---|
| [Bedrock User Guide](https://docs.aws.amazon.com/bedrock/latest/userguide/) | Service features and configuration | Choosing inference, retrieval, and safety capabilities |
| [Bedrock API reference](https://docs.aws.amazon.com/bedrock/latest/APIReference/Welcome.html) | Multiple Bedrock API families | Confirming exact service operation ownership |
| [AWS SDK example index](https://docs.aws.amazon.com/code-library/latest/ug/bedrock_code_examples.html) | Language-specific examples | Implementing direct SDK calls |
| [AgentCore documentation hub](https://docs.aws.amazon.com/bedrock-agentcore/) | Platform guides and SDK references | Navigating runtime, memory, identity, gateway |
| [AgentCore overview](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/what-is-bedrock-agentcore.html) | Service composition | Designing the managed platform layer |
| [AgentCore control API](https://docs.aws.amazon.com/bedrock-agentcore-control/latest/APIReference/API_CreateAgentRuntime.html) | Resource creation and configuration | Provisioning runtimes |
| [AgentCore data API](https://docs.aws.amazon.com/bedrock-agentcore/latest/APIReference/API_InvokeAgentRuntime.html) | Runtime invocation | Building application callers |
| [AgentCore Python SDK reference](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/agentcore-python-sdk-reference.html) | Public convenience SDK API | Integrating the Python application adapter |

### Naming trap: “agent runtime” is not always AgentCore Runtime

`bedrock-agent-runtime` contains APIs such as `Retrieve`, `RetrieveAndGenerate`, flows, sessions, and `InvokeAgent`. AgentCore invocation is a separate API family represented by `InvokeAgentRuntime`. Choose by operation, not by a vague similarity in client names. [Bedrock agent-runtime operations](https://docs.aws.amazon.com/bedrock/latest/APIReference/API_Operations_Agents_for_Amazon_Bedrock_Runtime.html), [AgentCore invocation](https://docs.aws.amazon.com/bedrock-agentcore/latest/APIReference/API_InvokeAgentRuntime.html)

## 5. Bedrock and AgentCore repository map

| Repository | Type | Why it matters |
|---|---|---|
| [aws/agentcore-cli](https://github.com/aws/agentcore-cli) | Deployment/developer CLI | Current recommended terminal workflow; CLI and TUI |
| [aws/bedrock-agentcore-sdk-python](https://github.com/aws/bedrock-agentcore-sdk-python) | Application SDK | Framework-agnostic runtime entrypoint, service integration, protocol adapters |
| [awslabs/agentcore-samples](https://github.com/awslabs/agentcore-samples) | Integration and deployment samples | Feature examples, use cases, IaC, blueprints |
| [aws-samples/amazon-bedrock-samples](https://github.com/aws-samples/amazon-bedrock-samples) | Bedrock notebooks and recipes | Inference, RAG, embeddings, safety, evaluation |
| [aws/bedrock-agentcore-starter-toolkit](https://github.com/aws/bedrock-agentcore-starter-toolkit) | Deprecated CLI/toolkit | Migration reference only, not the recommended new-project path |
| [aws-samples/sample-building-with-strands-course](https://github.com/aws-samples/sample-building-with-strands-course) | Structured teaching examples | Learning the agent loop through evaluation and deployment |

### Distinguish the four implementation layers

- Framework: Strands owns the agent loop and framework-level state/tool behavior.
- Application adapter: `bedrock-agentcore` wraps or serves agent code for AgentCore and exposes integration helpers.
- Service SDK: general AWS SDK clients call modeled service operations.
- Deployment CLI: `@aws/agentcore` scaffolds, packages, provisions, invokes, and inspects resources.

These responsibilities are visible in the [Strands README](https://github.com/strands-agents/harness-sdk), [AgentCore Python SDK README](https://github.com/aws/bedrock-agentcore-sdk-python), [CLI README](https://github.com/aws/agentcore-cli), and [API references](https://docs.aws.amazon.com/bedrock/latest/APIReference/Welcome.html).

### AgentCore CLI details relevant to Claude

The fetched CLI README specifies Node.js 20.12+; Python templates additionally require `uv`. It provides scriptable JSON output and an interactive TUI. Example commands include `agentcore status --json` and template scaffolding with `agentcore create --name MyAgent --template agent-python-strands`. These are reference examples, not commands executed during research. [CLI README](https://github.com/aws/agentcore-cli)

The generated project includes `agentcore/agentcore.json`, `agentcore/aws-targets.json`, and a CDK app under `agentcore/cdk/`. The README explicitly warns that `agentcore status` reports declared project resources, not necessarily additional resources added directly to the stack. Existing execution roles require their permissions to be updated separately. [Project and CDK details](https://github.com/aws/agentcore-cli)

Important version caveat: the fetched GitHub view displayed branch `refactor`. Do not assume everything in that branch is already available in the published npm release. Establish a released version and compare its command help before copying commands or schemas.

### Application SDK example

The current Python SDK README shows `BedrockAgentCoreApp`, an entrypoint handler, and framework streaming integration. It also emphasizes validating prompt input as a string rather than forwarding arbitrary request structures into the agent. Protocol extras include AG-UI and separate A2A variants; choose versions deliberately rather than installing every extra. [SDK README](https://github.com/aws/bedrock-agentcore-sdk-python)

## 6. Documentation drift and concrete pitfalls

### A. Starter Toolkit executable collision

The old Python toolkit and the new npm CLI both supply `agentcore`. The new CLI README advises removing the old toolkit, and the toolkit itself says its CLI is unsupported. Recommendation: record the executable path, CLI version, package manager provenance, and selected package version before debugging apparent command mismatches. [CLI installation](https://github.com/aws/agentcore-cli), [Deprecation](https://github.com/aws/bedrock-agentcore-starter-toolkit)

### B. Sample repository directory mismatch

The AgentCore sample tree displayed numbered folders such as `00-getting-started`, `01-features`, and `02-use-cases`, while its README also described unnumbered categories such as `getting-started/`, `features/`, and `end-to-end/`. Its migration notice says toolkit-dependent examples are being relocated to `legacy/`. Recommendation: use the selected commit's actual tree and `MIGRATION.md`, not a guessed path from prose. [Repository](https://github.com/awslabs/agentcore-samples), [Migration map](https://github.com/awslabs/agentcore-samples/blob/main/MIGRATION.md)

### C. Overprivileged educational IAM

The Bedrock samples README contains an example allowing `bedrock:*` on `*`. The AgentCore sample quickstart also suggests broad managed access policies. These simplify learning but are not evidence of least privilege. Recommendation: derive production policy from the actual operations and resource scope, and separate deployment from runtime roles. [Bedrock sample IAM](https://github.com/aws-samples/amazon-bedrock-samples), [AgentCore quickstart](https://github.com/awslabs/agentcore-samples)

### D. “Production-ready” labels are not independent certification

AWS Samples describes its repositories as educational examples rather than supported products. Treat a sample's production-ready language as an implementation starting point requiring your own security, reliability, cost, and operations review. [AWS Samples statement](https://github.com/aws-samples)

### E. Framework tools can have multiple entry points

The current Strands core README demonstrates vended tools imported from the SDK, while the separate `tools` repository also exists. Recommendation: inspect the exact tool and version; do not automatically rewrite every older `strands_tools` example or assume the separate package has disappeared. [Current imports](https://github.com/strands-agents/harness-sdk), [Tools inventory](https://github.com/orgs/strands-agents/repositories)

## 7. Recommended reading and implementation routes

These are engineering recommendations, not mandatory vendor workflows.

### Route A: Minimal Bedrock inference or retrieval

Read the service guide, identify the operation in the API index, choose a language-specific SDK example, then use the Bedrock recipes for a focused extension. Avoid importing a full agent platform for a simple request/response requirement. [API index](https://docs.aws.amazon.com/bedrock/latest/APIReference/Welcome.html), [SDK examples](https://docs.aws.amazon.com/code-library/latest/ug/bedrock_code_examples.html), [Recipes](https://aws-samples.github.io/amazon-bedrock-samples/)

### Route B: Custom Strands agent

Read assembled-harness versus SDK guides, select Python or TypeScript explicitly, inspect the matching package source/tests, implement one tool, and add an evaluation before deployment. [Harness](https://strandsagents.com/docs/user-guide/harness/), [SDK](https://strandsagents.com/docs/user-guide/sdk/), [Evals](https://strandsagents.com/docs/user-guide/evals-sdk/)

### Route C: Deploy on AgentCore

Read the Runtime guide, verify current CLI installation, scaffold with an approved release, inspect generated IAM and CDK, and use a narrow feature sample for Gateway or Memory. [Runtime CLI quickstart](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/runtime-get-started-cli.html), [CLI](https://github.com/aws/agentcore-cli), [Samples](https://github.com/awslabs/agentcore-samples)

### Route D: Contribute upstream

For Strands docs or SDK bugs, start in the monorepo and its contribution guide. For adapter behavior use the AgentCore Python SDK; for scaffold/deployment behavior use AgentCore CLI. A cloud service failure is not automatically a framework bug. [Strands contribution guide](https://strandsagents.com/docs/contribute/contributing/core-sdk/), [Adapter](https://github.com/aws/bedrock-agentcore-sdk-python), [CLI](https://github.com/aws/agentcore-cli)

## 8. Claude handoff instructions

The following block is a proposed review prompt, not an assertion that its checks have already passed.

```text
Review this report before implementation.

1. Choose one target deliverable and name each layer:
   model/service API, agent framework, application adapter, deployment tooling.
2. Confirm canonical repositories and reject archived sources for new code.
3. Record package versions, source commits/tags, region, model ID, and API mode.
4. Compare installed CLI --help and matching released code with mutable docs.
5. Do not assume GitHub main or refactor matches the published package.
6. Read only the focused sample, its dependency manifests, tests, IaC, and cleanup.
7. Remove tutorial wildcard permissions; separate deployment and runtime roles.
8. Validate prompts and tool arguments at typed boundaries.
9. Build a local smoke test, negative authorization tests, and an evaluation case.
10. Present a diff and deployment/cost plan for approval before changing AWS state.

Deliver:
- architecture and dependency decision record;
- exact version/source manifest;
- minimal implementation;
- tests and failure cases;
- scoped IAM and resource inventory;
- cleanup/rollback instructions;
- remaining uncertainties, each tied to a source or experiment.
```

### Proposed provenance manifest

```yaml
research_date: 2026-10-28
implementation_status: not_started
language: null
region: null
model_id: null
api_mode: null
sources:
  strands:
    repository: https://github.com/strands-agents/harness-sdk
    commit: null
    package: null
    version: null
  agentcore_cli:
    repository: https://github.com/aws/agentcore-cli
    package: '@aws/agentcore'
    version: null
    source_ref: null
  agentcore_python_sdk:
    repository: https://github.com/aws/bedrock-agentcore-sdk-python
    package: bedrock-agentcore
    version: null
samples:
  repository: null
  commit: null
  path: null
verification:
  installed_help_checked: false
  local_smoke_test_passed: false
  iam_reviewed: false
  deployment_approved: false
```

## 9. Limitations and next verification steps

- This is a repository/documentation map and migration analysis, not a source-code audit.
- Core READMEs and inventories were read; individual implementations and all tests were not inspected.
- No current npm/PyPI release-to-commit mapping was established.
- No SDK parity claim is made between Python and TypeScript.
- Box platform support and exact sandbox guarantees require dedicated version-specific review.
- No sample deployment, performance measurement, or IAM effectiveness test was performed.
- Source URLs are mutable. Replace `main` links with commit permalinks in the eventual implementation decision record.

The highest-value follow-up is a pinned, minimal vertical slice: one typed Strands tool, one Bedrock invocation path, one AgentCore deployment target, one negative authorization test, and one evaluation. This converts documentation confidence into executable evidence without prematurely adopting an entire sample application.
