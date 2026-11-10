# Custom Agent Harness

> A company-aware engineering harness for Claude Code, Codex CLI, and Cursor.

<!-- Replace <placeholders>, remove unused sections, and update status before publishing. -->
<!-- This README is a design scaffold, not a claim that the system is implemented. -->

| Field | Value |
|---|---|
| Status | Design stage. Node pilot on `main` (Linux sandbox); Rust core in review in [PR #13](https://github.com/wilsonhj/custom-agent-harness/pull/13) |
| Owner | <Team or maintainer> |
| Primary repository | <Repository URL> |
| Knowledge repository | <Company-brain repository URL> |
| Support | <Issue tracker or support channel> |
| Last verified | <YYYY-MM-DD> |

## Table of contents

- [Overview](#overview)
- [Goals and non-goals](#goals-and-non-goals)
- [Capabilities and implementation status](#capabilities-and-implementation-status)
- [Architecture](#architecture)
- [Repository layout](#repository-layout)
- [Prerequisites](#prerequisites)
- [Quick start](#quick-start)
- [Configuration](#configuration)
- [Agent integrations](#agent-integrations)
- [Company brain](#company-brain)
- [Feature-development workflow](#feature-development-workflow)
- [Deployment and monitoring](#deployment-and-monitoring)
- [Security and approvals](#security-and-approvals)
- [Evaluation and observability](#evaluation-and-observability)
- [Testing](#testing)
- [Troubleshooting](#troubleshooting)
- [Roadmap](#roadmap)
- [Contributing](#contributing)
- [Documentation and sources](#documentation-and-sources)
- [License](#license)

## Overview

### Runnable local pilot

The first local evidence workflow is implemented under `harness/local/` (Node 24,
Linux bubblewrap). It prepares an isolated worktree, runs operator-configured worker,
verifier and reviewer processes, verifies an out-of-band human review decision, and
writes a durable handoff. Start with [the local workflow guide](docs/local-workflow.md)
and `npm test`. The remaining sections describe the broader design; this pilot does
not implement task-phase approval, hosted adapters or production release authority.

<Project Name> connects coding agents to a shared engineering workflow and an evolving company knowledge base.

The system keeps agent sessions replaceable while preserving specifications, architectural decisions, operational procedures, reusable skills, and verification evidence.

### Intended users

- <Primary user: e.g., a Claude Code power user developing a full-stack product.>
- <Additional users or teams.>

### Default operating model

| Component | Responsibility |
|---|---|
| Claude Code | Primary interactive interface, planning, implementation, integration |
| Codex | Independent plan challenge, scoped implementation, code review |
| Cursor | Interactive editing and optional bounded agent jobs |
| Control plane | Task state, job execution, isolation, budgets, approval gates |
| Company brain | Curated knowledge, evidence retrieval, proposed updates |
| CI and deployment tooling | Deterministic validation, releases, operational evidence |

These are configurable workflow assignments, not model-performance rankings.

## Goals and non-goals

### Goals

- [ ] Share engineering context across agent runtimes.
- [ ] Make planning, implementation, and review reproducible.
- [ ] Give each job bounded permissions and clear ownership.
- [ ] Preserve evidence for completion and release decisions.
- [ ] Grow company knowledge through reviewed updates.
- [ ] Improve workflows through evaluations and real failure cases.

### Non-goals

- Replacing vendor-provided agent loops.
- Sharing one conversation transcript across every agent.
- Treating instruction files as security enforcement.
- Granting unattended agents unrestricted production access.
- Promoting every session summary into accepted company knowledge.
- <Project-specific exclusions.>

## Capabilities and implementation status

<!-- Status values: Planned, In progress, Partially covered, Implemented, Implemented (external), Verified, Deferred. -->
<!-- "Implemented (external)" means provided by an installed third-party component, not by this repository. -->

| Capability | Status | Implementation / evidence |
|---|---|---|
| Local evidence workflow | Implemented (pilot) | `harness/local/`, `tests/local-workflow.test.mjs`, [operator guide](docs/local-workflow.md). Offline Linux process interface; task-phase/release approval remains unimplemented. |
| Claude adapter | Planned | <Path or issue> |
| Codex adapter | Implemented (external) | Installed `codex` Claude Code plugin v1.0.6 (`openai-codex` marketplace), which talks to Codex through its app-server broker. Commands: `/codex:review`, `/codex:adversarial-review`, `/codex:status`, `/codex:result`, `/codex:cancel`. |
| Cursor adapter | Planned | Cursor CLI (`agent`) installed; no harness wrapper yet. |
| Isolated worktree management | Partially covered | Claude Code native worktree isolation for subagents and sessions. Harness-level worktree ownership is not implemented. |
| Structured review results | Implemented (external) | Plugin schema `review-output.schema.json`: `verdict`, `summary`, `findings` (`severity`, `file`, `line_start`, `line_end`, `confidence`, `recommendation`), `next_steps`. Reviews run with sandbox `read-only`. |
| Company-brain search | Planned | <Service and evaluations> |
| Knowledge-update review | Planned | <Workflow> |
| Preview verification | Planned | <Pipeline> |
| Vercel integration | Planned | <Integration tests> |
| Cloudflare integration | Planned | <Integration tests> |
| Production approval gate | Planned | Design: `docs/architecture/approval-ledger.md`. Policy stub: `harness/policy/transitions.ts`. |
| Run ledger and budgets | Planned | Design: `docs/architecture/approval-ledger.md`. Policy stub: `harness/policy/transitions.ts`. |

External components must be version-pinned. The `codex` plugin depends on Codex's experimental app-server, so a Codex or plugin upgrade can break the adapter without any change in this repository.

## Architecture

```text
Developer: Claude Code / Cursor / terminal
                       |
                Skills and commands
                       |
              Engineering control plane
              +-- Task state machine
              +-- Agent adapters
              |   +-- Claude
              |   +-- Codex
              |   +-- Cursor
              +-- Worktree / runner management
              +-- Permission and approval policy
              +-- Validation and artifact storage
              +-- Run ledger and evaluation
                       |
          +------------+-------------+
          Git host   Company brain   Deployment platforms
```

### Design boundaries

| Layer | Owns | Does not own |
|---|---|---|
| Instructions | Standing expectations | Permission enforcement |
| Skills | Reusable procedures | Approval authority |
| Agent adapters | Runtime invocation, events, sessions | Workflow approval |
| Control plane | State, ownership, budgets, policy | Accepted knowledge authorship |
| Company brain | Retrieval, provenance, proposed updates | Automatic promotion of guesses |
| Release pipeline | Approved deployment and evidence | Unrestricted agent actions |

### Key decisions

| Decision | Rationale | ADR |
|---|---|---|
| Shared artifacts instead of shared conversations | Durable, inspectable handoffs | <ADR link> |
| One writer per task/worktree by default | Clear ownership | <ADR link> |
| Knowledge updates require review | Prevent memory contamination | <ADR link> |
| Separate execution and release credentials | Limit privilege exposure | <ADR link> |
| <Decision> | <Rationale> | <ADR link> |

## Repository layout

<!-- Proposed layout. Update paths to match the implementation. -->

```text
.
+-- README.md
+-- AGENTS.md
+-- CLAUDE.md
+-- .agents/
|   +-- skills/
+-- harness/
|   +-- adapters/
|   +-- orchestration/
|   +-- policy/
|   |   +-- transitions.ts
|   +-- schemas/
|   +-- integrations/
+-- config/
|   +-- harness.example.yaml
+-- specs/
|   +-- _template/
|   +-- <feature-id>/
|       +-- requirements.md
|       +-- design.md
|       +-- tasks.json
|       +-- acceptance-tests.md
|       +-- handoff.md
+-- docs/
|   +-- architecture/
|   |   +-- approval-ledger.md
|   |   +-- release-gate.md
|   +-- runbooks/
|   +-- security/
|   |   +-- data-egress.md
|   |   +-- threat-model.md
+-- evaluations/
+-- tests/
+-- scripts/
+-- <local-artifacts-directory>/
```

Company knowledge may live in a separate private repository. Keep secrets, sensitive artifacts, and generated runtime state out of source control according to policy.

## Prerequisites

| Dependency | Supported / pinned version | Verification |
|---|---|---|
| Runtime and package manager | <Versions> | <Command> |
| Git | <Version> | <Command> |
| Claude Code | 2.1.310 (verified locally 2026-10-26) | `claude --version` |
| Codex CLI | 0.163.0 (verified locally 2026-10-26) | `codex --version` |
| Cursor CLI (`agent`) | 2026.06.02-8c11d9f (verified locally 2026-10-26) | `agent --version` |
| Container or sandbox runtime | <Version or optional> | <Command> |
| Cloud accounts | <Required platforms> | <Access check> |

Versions marked "verified locally" are the versions observed on one development machine on 2026-10-26. They are not a support policy; define supported ranges once adapter compatibility tests exist.

### Required access

- <Repository read/write scope.>
- <Knowledge-base access.>
- <Model authentication method.>
- <Preview deployment permissions.>
- <Production access, if separately approved.>

Document authentication and billing separately for each runtime. Do not assume one subscription or login covers every SDK or unattended execution mode.

## Quick start

> Setup is incomplete until the project-specific commands below are supplied and tested. Angle-bracket commands are placeholders, not runnable instructions.

### 1. Install

```bash
git clone <repository-url>
cd <repository-directory>
<install-dependencies-command>
```

### 2. Configure

```bash
cp config/harness.example.yaml config/harness.local.yaml
<configure-authentication-command>
```

Keep configuration examples secret-free. Store credentials using <approved secret mechanism>.

### 3. Validate the environment

```bash
<environment-validation-command>
<adapter-compatibility-test-command>
```

Expected result: <Describe successful validation, required services, and output artifacts.>

### 4. Run a safe first task

```bash
<read-only-planning-command>
<independent-review-command>
<inspect-artifacts-command>
```

Expected result: a recorded job, validated output artifacts, and no workspace or production changes.

### Proposed CLI surface

These names describe the intended custom CLI. They are not vendor commands and are unavailable until implemented.

```bash
eng plan FEATURE-123
eng implement FEATURE-123 --agent claude
eng review FEATURE-123 --agent codex
eng verify FEATURE-123 --target preview
eng deploy-check FEATURE-123
eng learn FEATURE-123
```

## Configuration

### Example configuration contract

This is a proposed project-owned schema, not configuration accepted directly by Claude, Codex, or Cursor. Implement and validate it before use.

```yaml
project:
  id: "<project-id>"
  repository: "<repository-url>"

agents:
  default_implementer: claude
  default_reviewer: codex
  enabled: [claude, codex, cursor]

execution:
  one_writer_per_worktree: true
  deadline_seconds: 1800
  max_repair_rounds: 2
  max_delegation_depth: 1
  max_parallel_jobs: 2

knowledge:
  repository: "<private-knowledge-repository>"
  retrieval_endpoint: "<company-brain-endpoint>"
  require_source_provenance: true
  require_review_before_publish: true

policy:
  default_profile: analysis
  production_changes_require_approval: true
  allow_production_credentials_in_workers: false

artifacts:
  directory: "<local-artifacts-directory>"
  retention_days: 30
```

All sample budgets are starting points, not validated sizing recommendations. Configuration expresses intent; implementation and tests must enforce it.

### Permission profiles

| Profile | Workspace | Network | Secrets | External writes |
|---|---|---|---|---|
| `analysis` | Read-only | <Allowlist> | None by default | Denied |
| `workspace` | Assigned worktree | <Allowlist> | Task-scoped only | Denied unless approved |
| `preview-ops` | <Scope> | Deployment allowlist | Preview-scoped | Approved preview actions |
| Privileged release job | No arbitrary worker execution | Provider allowlist | Release-scoped | Exact approved release |

## Agent integrations

### Shared instructions

Use `AGENTS.md` for common agreements. Keep runtime-specific additions small.

Example `CLAUDE.md`:

```markdown
@AGENTS.md

# Claude-specific guidance

Use the feature-planning skill before substantial changes.
Use company-brain tools for cross-repository decisions.
```

Confirm instruction discovery against pinned runtime versions.

### Adapter contract

Each adapter must support:

- [ ] Bounded task execution.
- [ ] Explicit workspace and base SHA.
- [ ] Permission-profile mapping.
- [ ] Structured result validation.
- [ ] Explicit session identity.
- [ ] Deadline and cancellation.
- [ ] Process-tree cleanup.
- [ ] Artifact and event capture.
- [ ] Usage accounting where available.

| Runtime | Integration approach | Configuration | Compatibility tests |
|---|---|---|---|
| Claude | <CLI / Agent SDK> | <Path> | <Path> |
| Codex | <exec / SDK / app server> | <Path> | <Path> |
| Cursor | <Headless CLI> | <Path> | <Path> |

#### Verified CLI surface

Observed locally on 2026-10-26 against the versions in [Prerequisites](#prerequisites). Recheck after any upgrade.

- **Codex CLI 0.163.0**
  - `codex exec` supports `--json`, `--output-schema`, `-s`/`--sandbox`, and `-o`/`--output-last-message`.
  - A native `codex review` subcommand exists.
  - `codex mcp` manages *external* MCP servers only; it does not expose Codex as a server.
  - `codex app-server` is experimental.
  - There is no `mcp-server` subcommand.
- **Cursor CLI (`agent`) 2026.06.02-8c11d9f**
  - `-p`/`--print` for non-interactive runs; `--output-format` applies with `--print`.
  - `-f`/`--force` (alias `--yolo`) and `--approve-mcps` relax approval prompts.
  - `--sandbox <mode>` selects the sandbox mode.

### Skills

| Skill | Inputs | Output | Approval boundary |
|---|---|---|---|
| Feature planning | Request, company constraints | Feature packet | Plan acceptance |
| Implementation | Approved packet, worktree | Patch and test evidence | Scope changes |
| Independent review | Requirements, diff, evidence | Structured findings | No write authority |
| Deployment check | Deployment ID, release criteria | Evidence packet | Release approval |
| Knowledge capture | Completed work and sources | Proposed update | Knowledge publication |

### Cross-agent jobs

Proposed MCP tools:

```text
engineering.submit_job(...)
engineering.get_job(job_id)
engineering.cancel_job(job_id)
engineering.get_artifact(job_id, artifact_id)
```

Resolve registered task and workspace IDs server-side. Do not expose unrestricted shell execution through a generic agent-spawning endpoint.

## Company brain

### Knowledge layers

| Layer | Contents | Authority / freshness |
|---|---|---|
| Engineering constitution | Stable standing agreements | Reviewed, concise |
| Curated knowledge | ADRs, policies, ownership, runbooks | Accepted status and owner |
| Searchable evidence | Documents, source records, investigations | Provenance and access control |
| Operational state | Deployments, incidents, rollout status | Live or time-bounded |

### Suggested knowledge layout

```text
company-brain/
+-- index.md
+-- product/
+-- architecture/decisions/
+-- engineering/
+-- services/<service-name>/
+-- runbooks/
+-- incidents/
+-- evaluations/
```

### Retrieval interface

Proposed tools:

```text
brain.search(query, repo?, service?, knowledge_type?, as_of?)
brain.read(document_id, revision?)
brain.related(entity_id)
brain.propose_update(document_id?, patch, evidence_refs)
```

### Required metadata

- Document ID and revision.
- Status: <draft / accepted / superseded / unverified>.
- Owner and access scope.
- Source references.
- Last verified timestamp.
- Superseded-document relationships.

### Retrieval strategy

Start with Git-backed documents and file search. Add full-text indexing, then evaluate hybrid lexical/semantic retrieval when corpus size and terminology variation justify it.

| Stage | Implementation | Acceptance criteria |
|---|---|---|
| File search | <Implementation> | <Evaluation target> |
| Full-text | <Implementation> | <Evaluation target> |
| Hybrid retrieval | <Implementation or deferred> | <Evaluation target> |

### Learning workflow

```text
Completed work -> Candidate lesson -> Evidence and contradiction checks
              -> Reviewed proposal -> Published knowledge -> Evaluation
```

Provenance requirement for `brain.propose_update`:

- Every proposal must cite source evidence in `evidence_refs`; proposals without it are rejected.
- Lessons derived from logs, issues, or retrieved text are flagged `untrusted_origin` and require human review before publication.

This prevents prompt-injected content from becoming accepted knowledge.

Promote lessons to the right destination:

- Repeated procedure -> skill or script.
- Architectural constraint -> ADR or policy.
- Missed defect -> regression test and evaluation.
- Operational failure -> incident record and runbook.
- Unconfirmed hypothesis -> unverified investigation note.
- Personal preference -> personal memory.

### Data governance

- [ ] Enforce identity and access checks before retrieval.
- [ ] Preserve repository and customer boundaries.
- [ ] Propagate deletions and revocations.
- [ ] Handle stale and superseded sources.
- [ ] Define redaction and retention policy.
- [ ] Document which excerpts may reach hosted model/embedding providers (see [Provider egress classes](#provider-egress-classes)).
- [ ] Audit reads and proposed updates.

#### Provider egress classes

Each document carries an egress class that limits which providers may receive its content:

| Class | May be sent to |
|---|---|
| `local-only` | No hosted provider |
| `baa-covered-providers` | Only allowlisted configurations (runtime, account, required setting, BAA version) covered by a business associate agreement |
| `any-provider` | Any configured provider |

- Documents without an explicit class default to the most restrictive class (`local-only`).
- Indexing and embedding count as disclosure; they are subject to the same class as prompting.
- PHI-adjacent content is excluded from indexing by default.
- Enforcement covers every read path, not only retrieval: `local-only` content is kept out of
  agent-reachable directories, with read hooks as defense in depth.

Details: `docs/security/data-egress.md`.

## Feature-development workflow

### State machine

Proposed successful path:

```text
Draft -> Planned -> Approved -> Implementing -> Reviewing
      -> Verified -> PreviewVerified -> MigrationGated -> Released
      -> Observed -> Closed
```

Names above are the `TaskState` values. The `Blocked`, `Failed`, `Cancelled`, `Repair`, and `Superseded` transitions are defined in `harness/policy/transitions.ts`. Agents cannot approve their own transitions.

The migration gate exists because some platforms (for example, Vercel auto-deploy) do not run migrations, and application rollback does not reverse schema changes. See `docs/architecture/release-gate.md`.

### Required artifacts

| Phase | Required evidence | Gate |
|---|---|---|
| Planning | Requirements, design, tasks, acceptance criteria | Human or policy approval |
| Implementation | Patch, base/head SHAs, test results | Scope and validation checks |
| Review | Actionable findings and resolutions | Independent verification |
| Preview | Build, smoke/browser tests, bounded logs | Preview acceptance |
| Migration | Reviewed migration plan, migration applied and verified in preview, old-code-on-new-schema compatibility check, recovery procedure | Release authorization |
| Release | Approved target and recovery plan | Release authorization |
| Observation | Deployment-specific health evidence | Close, or escalate (`Blocked`; `Failed` if the release is unhealthy) |
| Learning | Candidate updates and source evidence | Knowledge review |

### Handoff checklist

- [ ] Base and head SHA recorded.
- [ ] Changed files listed.
- [ ] Implemented requirements identified.
- [ ] Commands and test results captured.
- [ ] Limitations and unresolved decisions documented.
- [ ] Exact next action identified.

### Review and repair policy

- Reviewer receives requirements, constraints, diff, and relevant evidence.
- Findings include severity, location, failure scenario, and reproducible evidence.
- Implementer addresses findings; reviewer verifies resolutions.
- Automated repair limit: <number>.
- Escalation conditions: <repeated findings / scope change / budget / disagreement>.
- Completion requires validated evidence, not a model's assertion.

## Deployment and monitoring

### Provider integration

| Provider | Integration | Target identity | Permission scope |
|---|---|---|---|
| Vercel | <Official MCP / API / CLI> | <Team/project/environment> | <Scopes> |
| Cloudflare | <Official MCP / API / CLI> | <Account/service/environment> | <Scopes> |

Verify current endpoints and supported operations in official documentation. Do not assume a connected personal account is read-only.

### Release evidence

- Repository and commit SHA.
- Provider, account, project, and environment.
- Deployment/version ID and URL.
- Build and smoke-test results.
- Browser-test evidence where applicable.
- Time-bounded logs and metrics, with sample counts.
- Recovery procedure and known limitations.

### Event-driven operations

```text
Deployment event / alert -> Deterministic filtering -> Bounded evidence
                         -> Agent diagnosis -> Incident or proposed patch
```

| Action | Automatic? | Approval / conditions |
|---|---|---|
| Read status and bounded logs | <Yes/No> | <Conditions> |
| Run safe smoke tests | <Yes/No> | <Conditions> |
| Draft incident | <Yes/No> | <Conditions> |
| Publish patch / PR | <Yes/No> | <Conditions> |
| Change production configuration | No by default | Exact target and change |
| Change DNS or secrets | No by default | Exact target and change |
| Roll back production | No by default | Tested recovery policy or explicit approval |
| Destructive database action | No by default | Separate operational procedure |

Application rollback does not automatically reverse schema changes or external side effects.

## Security and approvals

### Trust boundaries

Treat repository files, hooks, MCP configuration, retrieved documents, issue text, logs, and dependency scripts as potentially untrusted inputs.

Worktrees are checkout isolation, not a complete execution sandbox.

### Required controls

- [ ] Keep production credentials outside coding workers.
- [ ] Separate patch generation from privileged publication and release.
- [ ] Use exact-target, exact-content approval records.
- [ ] Enforce filesystem and network restrictions externally.
- [ ] Apply deadlines, retry limits, concurrency limits, and delegation depth.
- [ ] Use controlled configuration for untrusted repositories.
- [ ] Set Cursor `--sandbox` explicitly for every job; never pass `--approve-mcps`, `--force`, or `--yolo` for untrusted repositories.
- [ ] Redact secrets from logs and artifacts.
- [ ] Pin runtime and extension versions.
- [ ] Test denied actions, not only successful paths.

Security policy: <Link to threat model, incident reporting, and credential procedures>.

## Evaluation and observability

### Evaluation set

Use real historical failures and representative tasks.

| Area | Metric | Target | Evaluation location |
|---|---|---|---|
| Retrieval | Correct source / stale-source rate | <Target> | <Path> |
| Access control | Unauthorized content returned | Zero | <Path> |
| Planning | Missing requirements caught early | <Target> | <Path> |
| Implementation | Acceptance success / human rework | <Target> | <Path> |
| Review | Verified findings / false positives | <Target> | <Path> |
| Deployment | Correct attribution / unsafe actions | <Target> | <Path> |
| Efficiency | Time, spend, retries, interventions | <Budget> | <Dashboard> |

### Run ledger

Record task/job IDs, runtime and skill versions, permission profile, knowledge revision, explicit session IDs, commit SHAs, evidence, approvals, timing, and available usage data.

Retention: <Policy>. Access: <Roles>. Redaction: <Policy>.

## Testing

Replace placeholders with implemented commands:

```bash
node --experimental-strip-types --test tests/*.test.ts   # state machine (Node >= 22.6)
<adapter-compatibility-test-command>
<permission-policy-test-command>
<retrieval-evaluation-command>
<end-to-end-preview-test-command>
```

### Minimum acceptance tests

- [ ] Analysis jobs cannot modify the workspace.
- [ ] Writers cannot reach unrelated paths.
- [ ] Required integration failures block the workflow.
- [ ] Invalid output cannot advance task state.
- [ ] Cancellation terminates job descendants.
- [ ] Missing or mismatched approval blocks external writes.
- [ ] Session continuation uses the recorded session ID.
- [ ] Retrieval does not leak restricted documents.
- [ ] Deployment evidence matches the intended commit and environment.

## Troubleshooting

| Symptom | Diagnostic evidence | Resolution / runbook |
|---|---|---|
| Runtime authentication fails | Adapter error and auth profile, without secrets | <Runbook> |
| Model output fails schema validation | Raw artifact and validator errors | <Runbook> |
| Required MCP service unavailable | Integration health and job events | <Runbook> |
| Agent changes an unexpected file | Workspace diff and policy trace | <Runbook> |
| Knowledge answer cites stale material | Source revision and verification date | <Runbook> |
| Deployment check targets wrong release | Commit/deployment mapping | <Runbook> |
| Job survives cancellation | Process tree and runner events | <Runbook> |

## Roadmap

### Phase 1 — Reliable Claude-first workflow

- [ ] Shared instructions.
- [ ] Feature packets and handoffs.
- [ ] One-writer worktrees.
- [ ] Independent Codex review. The external `codex` Claude Code plugin v1.0.6 provides the review commands; nothing is implemented in this repository. Not done for `baa-covered-providers` content: the BAA configuration allowlist and the managed configuration are still placeholders (`docs/security/data-egress.md`, "BAA configuration allowlist" and "Open questions"), so until they exist a Codex session counts as `any-provider`.
- [ ] Deterministic validation.
- [ ] Git-backed company knowledge.
- [ ] Migration-aware release gate with a fail-closed migration classifier.
- [ ] Local approval ledger (agents cannot self-approve).
  Done when: signed, serialized, anchored records on one machine; `canTransition` enforces
  them; `MigrationGated -> Released` cannot succeed without a valid human record for the
  exact commit and deployment target.

From Phase 1 the harness's control plane records and checks the release *decision*
(`canTransition` against the ledger; today `canTransition` denies every transition except
entering `Blocked`, pending the actor policy). That does not stop a release made outside the
harness: only Phase 3 ("Provider-enforced production releases") confines production deploy
credentials to the privileged release job. Later phases change where the ledger lives and who
can execute a release, not whether approval is required.

### Phase 2 — Unified knowledge and orchestration

- [ ] Company-brain MCP.
- [ ] Provenance and reviewed updates.
- [ ] Versioned skills/plugin distribution.
- [ ] Shared ledger service and job state.
  Done when: the Phase 1 record format and rules run as a service usable from more than one
  machine, and jobs submitted through `engineering.submit_job` record and read their state there.
- [ ] Adapter compatibility suite.

### Phase 3 — Delivery and operations

- [ ] Vercel / Cloudflare integrations.
- [ ] Preview verification and evidence packets.
- [ ] Event-driven incident triage.
- [ ] Provider-enforced production releases.
  Done when: production deploy credentials are held only by the privileged release job, and
  that job refuses to deploy without a matching ledger record, so a release cannot happen
  outside the harness.

### Phase 4 — Evaluated autonomy

- [ ] Durable scheduling and isolated runners.
- [ ] Selective parallel implementation.
- [ ] Bounded repair workflows.
- [ ] Tested, narrow recovery policies.

## Contributing

1. Open an issue describing the workflow or failure case.
2. Identify affected policies, skills, adapters, and knowledge sources.
3. Add a regression case where possible.
4. Implement the smallest scoped change.
5. Run relevant tests and evaluations.
6. Request review; do not self-approve privileged policy changes.
7. Document compatibility, migration, and operational effects.

Contribution guide: <Link>. Ownership: <Link>. Decision process: <Link>.

## Documentation and sources

Research baseline: October 26, 2026. Recheck official documentation against pinned versions; vendor interfaces and endpoint support may change.

| Topic | Official documentation |
|---|---|
| Claude Agent SDK | https://code.claude.com/docs/en/agent-sdk/overview |
| Claude headless execution | https://code.claude.com/docs/en/headless |
| Claude hooks | https://code.claude.com/docs/en/hooks |
| Claude memory and instructions | https://code.claude.com/docs/en/memory |
| Codex noninteractive execution | https://developers.openai.com/codex/noninteractive/ |
| Codex SDK | https://developers.openai.com/codex/sdk/ |
| Codex app server | https://developers.openai.com/codex/app-server/ |
| Codex instructions | https://developers.openai.com/codex/guides/agents-md/ |
| Cursor headless CLI | https://cursor.com/docs/cli/headless |
| Cursor CLI MCP | https://cursor.com/docs/cli/mcp |
| Cursor hooks | https://cursor.com/docs/hooks |
| Cursor skills | https://cursor.com/docs/context/skills |
| Vercel MCP | https://vercel.com/docs/mcp/vercel-mcp |
| Cloudflare MCP | https://github.com/cloudflare/mcp-server-cloudflare |
| Long-running harness design | https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents |
| Contextual retrieval | https://www.anthropic.com/engineering/contextual-retrieval |

### Compatibility notes to verify

- Prefer Codex `exec`, SDK, or app-server integration over copying older `mcp-server` examples without checking current support. Verified for Codex 0.163.0: there is no `mcp-server` subcommand (see [Verified CLI surface](#verified-cli-surface)).
- Confirm Cloudflare endpoint deprecations before adopting product-specific or older transport URLs.
- Verify Claude instruction loading and repository-configuration behavior for the installed version.
- Validate portable skills across runtimes instead of assuming identical behavior.

## License

<License name and link, or internal-use statement.>
