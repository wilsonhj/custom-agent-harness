# AWS Agent Stack: Bedrock, AgentCore, Gateway, Strands, and Kiro CLI

Research date: October 28, 2026  
Format: Editable implementation-review reference

## Scope and evidence

This report exports the preceding documentation-based research. It is not a hands-on deployment test, security audit, or benchmark reproduction. Source links are preserved as clickable Markdown references. Current documentation is mutable; recheck release status, regional support, API behavior, and pricing before implementation. Recommendations below are architectural judgments, not service guarantees.

## Executive summary

These are complementary layers, not five competing products:

| Component | Main role | Boundary |
|---|---|---|
| Amazon Bedrock | Managed model inference, Knowledge Bases, Guardrails, and AI application capabilities | Not application business logic |
| Amazon Bedrock AgentCore | Managed agent execution and supporting services; optional managed harness | Not automatic business correctness or transaction safety |
| AgentCore Gateway | Controlled access to MCP tools, HTTP services/agents, and inference targets | Not the agent reasoning loop |
| Strands Agents | Open-source, in-process agent orchestration and assembled harness | Not managed cloud infrastructure |
| Kiro CLI | Developer-facing coding agent for building and operating software | Not the production runtime for an application's agents |

Sources: [Bedrock overview](https://aws.amazon.com/bedrock/), [AgentCore managed harness announcement](https://aws.amazon.com/blogs/machine-learning/get-to-your-first-working-agent-in-minutes-announcing-new-features-in-amazon-bedrock-agentcore/), [Gateway targets](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/gateway-supported-targets.html), [Strands overview](https://strandsagents.com/), [Kiro CLI V3](https://kiro.dev/docs/cli/v3/).

The central distinction is development-time tooling versus application runtime. Kiro helps develop the system; Strands implements agent behavior; AgentCore can host and govern it; Gateway connects capabilities; Bedrock supplies inference. Adoption is modular, not all-or-nothing. [AgentCore architecture and harness](https://aws.amazon.com/blogs/machine-learning/get-to-your-first-working-agent-in-minutes-announcing-new-features-in-amazon-bedrock-agentcore/)

## 1. Amazon Bedrock

Bedrock combines managed foundation-model inference with services such as Knowledge Bases for retrieval-augmented generation and Guardrails for evaluating inputs and generated responses. Knowledge Bases manages retrieval-related plumbing; Guardrails applies configurable safeguards. Neither replaces application authorization. [Knowledge Bases and Guardrails](https://aws.amazon.com/blogs/machine-learning/protect-sensitive-data-in-rag-applications-with-amazon-bedrock/), [Guardrails behavior](https://docs.aws.amazon.com/bedrock/latest/userguide/guardrails-how.html)

### Inference and tool execution

For client-side tool use, the model requests a tool; application code executes it and returns the result. A framework such as Strands can own this orchestration. Current Bedrock documentation also describes server-side tool execution through the Responses API, where Bedrock invokes a registered Lambda function or AgentCore Gateway. Execution ownership therefore depends on the API and mode; the statement that Bedrock never executes tools is too broad. [Tool-use modes](https://docs.aws.amazon.com/bedrock/latest/userguide/tool-use.html)

Verify tool calling and streaming for the exact model. Support for response streaming does not necessarily imply support for streaming combined with tools. [Client-side tool use and compatibility](https://docs.aws.amazon.com/bedrock/latest/userguide/tool-use-client-side.html)

### Agents Classic versus AgentCore

Bedrock Agents Classic entered maintenance mode on July 30, 2026. New-customer access is restricted and its model catalog is frozen as of that date. Existing agents continue operating; AWS documents no migration deadline or planned end-of-life date. AWS recommends AgentCore for migration. This affects Classic orchestration, not Bedrock inference, Knowledge Bases, or Guardrails. [Maintenance-mode documentation](https://docs.aws.amazon.com/bedrock/latest/userguide/agents-classic-maintenance-mode.html)

Recommendation: do not base a new architecture on Agents Classic. Using Bedrock models through Strands is not equivalent to using Agents Classic.

## 2. Amazon Bedrock AgentCore

AgentCore is a composable platform supporting different frameworks and model providers. Its managed harness offers configuration-driven orchestration; custom behavior can remain code-defined. [Managed harness architecture](https://aws.amazon.com/blogs/machine-learning/get-to-your-first-working-agent-in-minutes-announcing-new-features-in-amazon-bedrock-agentcore/)

| Capability | Architectural purpose |
|---|---|
| Runtime | Hosts agents and tools in session-isolated execution environments |
| Managed harness | Assembles model, instructions, tools, and supporting infrastructure |
| Memory | Short-term interaction context and persistent long-term memories |
| Gateway | Access to tools, agents, and models |
| Identity | Workload identity and authenticated downstream access |
| Policy | Policy enforcement at the tool-access boundary |
| Observability | Execution traces, logs, and metrics |
| Evaluations | Agent-quality measurement |
| Browser and Code Interpreter | Managed browser interaction and code execution |

Sources: [Enterprise architecture](https://aws.amazon.com/blogs/machine-learning/ai-agents-in-enterprises-best-practices-with-amazon-bedrock-agentcore/), [Multi-tenant agents](https://aws.amazon.com/blogs/machine-learning/building-multi-tenant-agents-with-amazon-bedrock-agentcore/), [Policy and Evaluations announcement](https://aws.amazon.com/about-aws/whats-new/2025/12/amazon-bedrock-agentcore-policy-evaluations-preview/).

### Runtime versus managed harness

Runtime hosts the agent; the managed harness can also supply orchestration. AWS describes the harness as powered by Strands, with a path toward code-defined orchestration for custom routing or multi-agent coordination. The harness was introduced in preview in April 2026; that launch article does not establish its present release status. [Harness announcement](https://aws.amazon.com/blogs/machine-learning/get-to-your-first-working-agent-in-minutes-announcing-new-features-in-amazon-bedrock-agentcore/)

Verify availability per capability, not merely whether AgentCore is available in a region. [Regional feature matrix](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/agentcore-regions.html)

### Lifecycle and durability

Runtime documentation distinguishes a 15-minute synchronous request timeout from an eight-hour asynchronous job maximum. Session idle and lifetime settings are separate. A long-lived session does not permit a synchronous request of the same duration. [Runtime quotas](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/bedrock-agentcore-limits.html)

AWS documents persistent session filesystems. This does not establish exactly-once execution for external side effects. Recommendation: implement idempotency keys, operation records, and reconciliation in the business system; do not treat session recovery as a distributed transaction guarantee. [Session persistence architecture](https://aws.amazon.com/blogs/machine-learning/building-multi-tenant-agents-with-amazon-bedrock-agentcore/)

## 3. AgentCore Gateway

Gateway extends beyond converting Lambda functions into tools. It supports three categories. [Supported targets](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/gateway-supported-targets.html)

| Category | Behavior | Use |
|---|---|---|
| MCP | Aggregates capabilities into a unified virtual MCP server | Consolidated tool surface |
| HTTP | Direct forwarding without MCP aggregation or translation | Agents and HTTP services |
| Inference | Model-based routing across providers through a unified endpoint | Centralized inference access |

MCP integrations include Lambda, OpenAPI, Smithy, API Gateway, and existing MCP servers. Gateway also provides semantic tool discovery. [Gateway core concepts](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/gateway-core-concepts.html)

### Security boundaries

Resolve three separate questions:

1. Who may reach the gateway?
2. What may that principal invoke, with which parameters?
3. Which credentials does Gateway use downstream?

Inbound authorization, policy enforcement, and downstream credential injection are separate responsibilities. [Gateway security architecture](https://aws.amazon.com/blogs/machine-learning/ai-agents-in-enterprises-best-practices-with-amazon-bedrock-agentcore/), [Private gateway ingress](https://aws.amazon.com/blogs/machine-learning/secure-ingress-connectivity-to-amazon-bedrock-agentcore-gateway-using-interface-vpc-endpoints/)

Recommendation: preserve user and tenant identity; enforce tool policy; repeat business authorization in the backend. A broadly privileged runtime role must not silently become every user's authority. MicroVM isolation does not establish correct data-access authorization. [Multi-tenant architecture](https://aws.amazon.com/blogs/machine-learning/building-multi-tenant-agents-with-amazon-bedrock-agentcore/), [Resource-based policies](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/resource-based-policies.html)

Gateway policy is an enforcement boundary outside the prompt, but only for paths traversing Gateway. Recommendation: remove unnecessary direct credentials and backend connectivity or apply equivalent controls to those paths.

### Relationship to API Gateway

API Gateway remains useful for conventional API publishing; AgentCore Gateway adds agent-oriented integration. API Gateway REST API stages can be used as AgentCore Gateway targets. They can be composed rather than treated as alternatives. [Private connectivity and target patterns](https://aws.amazon.com/blogs/networking-and-content-delivery/private-connectivity-patterns-for-amazon-bedrock-agentcore-gateway-targets/)

### Protocol verification

Before implementation, verify the current MCP protocol revisions, operations, session semantics, authentication flows, and client compatibility. Avoid assuming older tool-only Gateway descriptions capture the complete current capability surface. [Gateway operations](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/gateway-using.html), [Gateway sessions](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/gateway-sessions.html)

## 4. Strands Agents

Strands supplies in-process orchestration in Python and TypeScript, with model providers, MCP, tools, context management, sessions, hooks, tracing, and multi-agent composition. Its repository documents Apache-2.0 licensing. [Strands overview](https://strandsagents.com/), [Consolidated repository](https://github.com/strands-agents/harness-sdk)

### Assembled harness versus SDK

| Entry point | Starting point | Selection guidance |
|---|---|---|
| Assembled harness | Defaults for tools, prompts, memory, sessions, context management | Rapid iteration when defaults fit |
| Harness SDK | Lower-level loop, providers, tools, hooks, sessions, and memory | Custom orchestration requirements |

The assembled APIs are `create_harness()` in Python and `createHarness()` in TypeScript. Providers include Bedrock, Anthropic, OpenAI, Google, and Ollama. Provider abstraction improves portability but does not ensure identical behavior or feature compatibility. [Strands toolkit](https://strandsagents.com/)

### Developer entry points

```bash
# Assembled Python harness
pip install strands-harness

# Python SDK and tools
pip install strands-agents strands-agents-tools

# Assembled TypeScript harness
npm install @strands-agents/harness

# TypeScript SDK
npm install @strands-agents/sdk
```

These commands are documentation examples, not commands executed during this research. Pin approved versions and verify current runtime prerequisites and package provenance. [Repository quickstarts](https://github.com/strands-agents/harness-sdk)

### Multi-agent patterns

- Graph: developer-defined topology and dependencies.
- Swarm: autonomous handoffs among specialized agents.
- Workflow: task chaining in application code; Python also offers a ready-made workflow tool.

Graph and Swarm are SDK orchestrators. Their state-sharing and execution behavior differ. [Pattern comparison](https://strandsagents.com/docs/user-guide/concepts/multi-agent/multi-agent-patterns/index.md), [Swarm](https://strandsagents.com/docs/user-guide/sdk/multi-agent/swarm/), [Graph API](https://strandsagents.com/docs/api/python/strands.multiagent.graph/)

Recommendation: start with one agent and narrowly scoped tools. Add graphs for explicit dependencies, or swarms when autonomous coordination has demonstrated value. Evaluate task quality, tool safety, token cost, and failure recovery before expanding agent count.

### Adjacent tooling

Strands Evals evaluates outputs and trajectories and supports simulation. Strands Shell offers an in-process virtual shell with declared access boundaries. Strands Box combines local OS containment with policy and was described as preview in the reviewed material. These are distinct from cloud runtime isolation and business authorization. [Toolkit overview](https://strandsagents.com/), [Project announcements](https://strandsagents.com/blog/)

## 5. Kiro CLI

Kiro is a developer-facing coding agent. CLI V3 shares a unified harness with Kiro IDE and Web while retaining surface-specific differences. The reviewed guide describes V3 as an opt-in early release. [V3 documentation](https://kiro.dev/docs/cli/v3/)

### V3 migration boundaries

| Area | Documented change |
|---|---|
| Entry point | `kiro-cli --v3` |
| Specs | Terminal workflow beginning with `/spec new <name>` |
| AWS access | V2 `use_aws` is unavailable; use MCP for AWS access |
| Persistent permissions | Capability-based `permissions.yaml` replaces legacy per-tool trust rules |
| Hooks | Formats and trigger names change |
| Sessions | V2 and V3 stores are not backward-compatible |
| Recovery | Conversation rewind is not file restoration |

Review `/upgrade-agent` diagnostics rather than assuming configuration conversion is complete. For ACP integrations the documented launch is `kiro-cli acp --agent-engine=v3 --auth-method=cli`. [Migration and feature-parity guide](https://kiro.dev/docs/cli/v3/)

Recommendation: use Kiro to author code, tests, specs, evaluation cases, and infrastructure. Scope deployment credentials and review MCP permissions as carefully as shell access. Supervised editing does not by itself restrict reads or all network access. [Security guidance](https://kiro.dev/docs/cli/privacy-and-security/)

### Privacy: do not conflate products

AWS states that Bedrock inputs and outputs are not used to train AWS or third-party models. Kiro has a different service-level policy: Free and individual paid subscriber content may be collected for service improvement, including training, unless users opt out. Enterprise content is excluded from service improvement. [Bedrock FAQ](https://aws.amazon.com/bedrock/faqs/), [Kiro data protection](https://kiro.dev/docs/cli/privacy-and-security/data-protection/)

Kiro documents separate telemetry and content-sharing controls, US East storage for individual users, cross-region inference, and potentially global routing for explicitly experimental features. Enterprise administrators may enable prompt logging and activity reporting. Opting out of service improvement is not the same as eliminating operational storage or abuse-detection retention. [Kiro data handling](https://kiro.dev/docs/cli/privacy-and-security/data-protection/), [Enterprise controls](https://kiro.dev/docs/enterprise/settings/)

## 6. Proposed reference architecture

This is a design recommendation, not a mandatory AWS topology.

```text
Development
  Kiro CLI
    -> code, specs, tests, evaluation cases, infrastructure

Application
  User / frontend
    -> authenticated application ingress
    -> AgentCore Runtime
        -> Strands agent or custom harness
        -> Bedrock inference
        -> AgentCore Memory
        -> AgentCore Gateway
            -> read-only retrieval tools
            -> business APIs / Lambda
            -> existing MCP servers
            -> other agents

Cross-cutting
  Identity, policy, backend authorization, tracing, evaluation
```

The component relationships are supported by [AgentCore architecture](https://aws.amazon.com/blogs/machine-learning/ai-agents-in-enterprises-best-practices-with-amazon-bedrock-agentcore/), [Gateway target categories](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/gateway-supported-targets.html), and [Strands providers and tools](https://strandsagents.com/).

### Separate three kinds of state

| State | Typical role | Design concern |
|---|---|---|
| Enterprise knowledge | Retrieved source content | Provenance, freshness, document authorization |
| Interaction memory | Conversation-derived context and long-term records | Tenant isolation, consent, retention, deletion |
| Execution artifacts | Session working files and intermediate results | Recovery, lifecycle, integrity |

Sources: [Knowledge Bases](https://aws.amazon.com/blogs/machine-learning/protect-sensitive-data-in-rag-applications-with-amazon-bedrock/), [AgentCore Memory](https://aws.amazon.com/about-aws/whats-new/2026/10/agentcore-memory-consumption-based-pricing/), [Session filesystems](https://aws.amazon.com/blogs/machine-learning/building-multi-tenant-agents-with-amazon-bedrock-agentcore/).

Guardrails should not be assumed to filter every retrieved source document. AWS documents that guardrails apply to input and generated responses, not automatically to references retrieved at runtime. [Knowledge Base retrieval caveat](https://docs.aws.amazon.com/bedrock/latest/userguide/kb-test-retrieve.html)

## 7. Cost model

Budget for model inference, agent compute and memory, Gateway operations, memory storage/retrieval, managed tools, telemetry, and downstream services separately. CPU waiting behavior does not imply the entire session is free: memory remains a billing dimension. [Harness cost controls](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/harness-operations.html), [AgentCore pricing](https://aws.amazon.com/bedrock/agentcore/pricing/)

### Dated pricing observations

- AgentCore short-term memory: AWS lists $1.00/GB ingestion, $0.20/GB retrieval, and $0.10/GB-month storage. Per-event billable size has a 12 KB minimum and 64 KB maximum for ingestion and retrieval; storage uses actual event size. Older flat-per-event examples may be stale. [Consumption-based memory pricing announcement](https://aws.amazon.com/about-aws/whats-new/2026/10/agentcore-memory-consumption-based-pricing/)
- Kiro monthly tiers in the reviewed page: Free $0; Pro $20; Pro+ $40; Pro Max $100; Power $200. Credits depend on work and model selection, not a fixed prompt count. Prices exclude applicable taxes; enterprise and regional conditions differ. [Kiro pricing](https://kiro.dev/pricing/)

Do not use these observations as a deployment quote. Recheck current region-specific pricing and estimate representative workloads.

## 8. Suggested implementation sequence

1. Validate one local agent with read-only tools and an explicit model/provider.
2. Create an evaluation set covering task completion, tool selection, unauthorized actions, and failure behavior.
3. Introduce Gateway when shared discovery, credentials, or policy justify centralization.
4. Deploy to Runtime and test lifecycle behavior, identity propagation, recovery, and cost.
5. Enable mutating tools only after backend authorization, idempotency, approval, and auditing are implemented.
6. Keep Kiro's developer permissions separate from production workload permissions.

This sequence is an architectural recommendation informed by [Strands tooling](https://strandsagents.com/), [AgentCore enterprise practices](https://aws.amazon.com/blogs/machine-learning/ai-agents-in-enterprises-best-practices-with-amazon-bedrock-agentcore/), and [Kiro security](https://kiro.dev/docs/cli/privacy-and-security/).

### Documented AgentCore CLI workflow

```bash
agentcore create --name MyProject --template agent-python-strands
# Enter the generated project directory before subsequent commands.
agentcore build
agentcore deploy
agentcore status
agentcore invoke --runtime agent --payload '{"prompt":"Hello, what can you do?"}'
```

This is a reference workflow, not a validated deployment script. Check credentials, permissions, installation instructions, generated resources, and costs first. Do not confuse AgentCore CLI with Kiro CLI or the Strands CLI. [Python Runtime quickstart](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/runtime-get-started-cli.html), [TypeScript Runtime quickstart](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/runtime-get-started-cli-typescript.html)

## 9. Claude review and implementation checklist

The following are review tasks, not claims that the preceding research validated these properties.

- [ ] Confirm each intended capability's GA/preview status and regional availability.
- [ ] Select assembled Strands harness, lower-level SDK, or AgentCore managed harness deliberately; avoid accidentally nesting redundant loops.
- [ ] Pin dependencies and validate current package/runtime requirements.
- [ ] Confirm exact model IDs, API modes, tool calling, streaming, and inference routing.
- [ ] Validate Gateway target types, MCP protocol/version compatibility, authentication, and credential flows.
- [ ] Trace user and tenant identity through ingress, Runtime, Gateway, memory, and backend data access.
- [ ] Remove unused direct credentials and alternative tool-access paths.
- [ ] Implement backend authorization and idempotency for every mutating operation.
- [ ] Test synchronous timeout, async execution, session expiry, interruption, resume, and repeated side effects.
- [ ] Separate knowledge, memory, and artifact retention/deletion policies.
- [ ] Configure trace redaction and retention; avoid collecting secrets in tool payloads.
- [ ] Test prompt injection and data exfiltration through retrieved content and tools.
- [ ] Measure task success and unsafe actions, not just answer quality.
- [ ] Review Kiro content sharing, enterprise prompt logging, and experimental model routing.
- [ ] Validate Kiro V2/V3 config, hooks, permissions, and session migration before team rollout.
- [ ] Run a representative cost experiment before increasing autonomy or agent count.

## 10. Developer reference index

| Topic | Official reference |
|---|---|
| Bedrock | [Product overview](https://aws.amazon.com/bedrock/) |
| Bedrock privacy | [FAQ](https://aws.amazon.com/bedrock/faqs/) |
| Tool execution modes | [Tool-use documentation](https://docs.aws.amazon.com/bedrock/latest/userguide/tool-use.html) |
| Classic migration | [Maintenance-mode guide](https://docs.aws.amazon.com/bedrock/latest/userguide/agents-classic-maintenance-mode.html) |
| Guardrails | [How Guardrails works](https://docs.aws.amazon.com/bedrock/latest/userguide/guardrails-how.html) |
| Retrieval caveats | [Knowledge Base retrieval](https://docs.aws.amazon.com/bedrock/latest/userguide/kb-test-retrieve.html) |
| AgentCore region support | [Feature matrix](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/agentcore-regions.html) |
| Runtime quotas | [Service limits](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/bedrock-agentcore-limits.html) |
| Python deployment | [Runtime CLI quickstart](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/runtime-get-started-cli.html) |
| TypeScript deployment | [Runtime CLI quickstart](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/runtime-get-started-cli-typescript.html) |
| Gateway architecture | [Core concepts](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/gateway-core-concepts.html) |
| Gateway targets | [Supported targets](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/gateway-supported-targets.html) |
| MCP operations | [Use a gateway](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/gateway-using.html) |
| Gateway lifecycle | [MCP sessions](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/gateway-sessions.html) |
| Access policies | [Resource-based policies](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/resource-based-policies.html) |
| AgentCore costs | [Pricing](https://aws.amazon.com/bedrock/agentcore/pricing/) |
| Strands toolkit | [Official site](https://strandsagents.com/) |
| Strands implementation | [Consolidated repository](https://github.com/strands-agents/harness-sdk) |
| Strands graph | [Python API](https://strandsagents.com/docs/api/python/strands.multiagent.graph/) |
| Kiro migration | [CLI V3 guide](https://kiro.dev/docs/cli/v3/) |
| Kiro security | [Privacy and security](https://kiro.dev/docs/cli/privacy-and-security/) |
| Kiro data policy | [Data protection](https://kiro.dev/docs/cli/privacy-and-security/data-protection/) |
| Kiro governance | [Enterprise settings](https://kiro.dev/docs/enterprise/settings/) |
| Kiro cost | [Pricing](https://kiro.dev/pricing/) |

## Open questions requiring implementation-time verification

- Current managed-harness release status and support for the intended region.
- Exact recovery guarantees for the chosen deployment mode; persistent files do not answer external side-effect semantics.
- Full Gateway MCP compatibility for the chosen client and protocol revision.
- Practical latency, cold-start behavior, concurrency, and workload-specific cost.
- End-to-end data residency, including model routing, logs, tool destinations, and developer-tool content policies.
- Whether provider portability preserves the specific tool, streaming, and structured-output behavior the application depends on.

No independent benchmark, hands-on integration test, or source-code security audit was completed in the preceding research.
