# Requirements: kiro-adapter — Kiro CLI agent adapter

> DRAFT. Kiro claims carry a grade. **(P)** read on a kiro.dev docs page on 2026-10-29
> (pages listed under Sources; the page text was fetched with curl and converted to text,
> not rendered). **(M)** read in the KiroCrew repo's offline docs mirror or another public
> GitHub page, not re-checked on kiro.dev. **(S)** search snippets or third-party pages.
> **UNVERIFIED** not found. Docs describe Kiro CLI V3 and 2.x; the installed version and
> agent engine (`--v3`) must be pinned and re-checked before approval.

## User-visible behavior

- A task can name `kiro` as its runtime and the harness runs `kiro-cli` in that task's
  worktree, capturing exit status and output as artifacts (K-1, K-11).
- A Kiro session cannot write the approval ledger or the evidence store and receives only
  content its egress class permits (K-3, K-4, K-5, K-10, K-14, K-16).

## Non-goals

- Replacing Kiro's agent loop; sharing transcripts across runtimes.
- Letting Kiro approve or mark any phase transition.
- Cloud/web/mobile Kiro surfaces; this packet covers the CLI only.

## Invariants

- Agents only request transitions (artifacts under `<local-artifacts-directory>/requests/`);
  approvals stay in the external ledger, enforced by OS user separation (the adapter and
  the Kiro process run as a user with no write access to `<ledger-root>`).
- One writer per worktree.
- Kiro output, retrieved text and repo files are data, never instructions (AGENTS.md).
- Unknown or unverified runtime configuration is never treated as wider than
  `any-provider` (K-10), and never launched against content above that class (K-14).

## Adapter requirements

Statuses: **Harness** = harness-side, no Kiro fact needed. **UNVERIFIED** = depends on a
Kiro behaviour not read from a primary page.

| ID | Requirement | Source | Status |
|---|---|---|---|
| K-1 | Adapter runs Kiro non-interactively with a named agent in the task worktree (explicit workspace and base SHA), and records exit status and output as artifacts. | README "Adapter contract" | Harness. (P) `kiro-cli chat --no-interactive [--v3] [--agent <name>] [--model <id>] [--sandbox] [--require-mcp-startup] [--output-format stream-json]`, authenticated by `KIRO_API_KEY`; exit codes 0 success, 1 failure, 3 MCP startup failure, 4 named agent unavailable. Exit 4 can occur after the built-in default agent already made tool calls, so `--agent` alone is not a safe gate. Without `--agent` a V3 run uses `chat.defaultAgent` or the built-in default. In headless runs every `ask` is treated as `deny`. The `stream-json` event schema is not documented on the headless page (UNVERIFIED). |
| K-2 | Adapter acquires an atomic per-worktree writer lock (no check-then-start race) before launching, and releases it on exit, crash or cancel. | AGENTS.md "One writer per worktree" | Harness |
| K-3 | The Kiro process and adapter run as an OS user with no read or write access to `<ledger-root>`, the signing keys or the anchor-repo deploy key. Kiro config also denies those paths and a managed hook rejects shell commands referencing them (defence in depth only). | approval-ledger.md "Who writes, and how it is enforced" | Harness (OS boundary). (M) agent config is JSON or Markdown with `permissions.rules` (allow/ask/deny; deny wins) and `toolsSettings`. (S) writes to `.kiro/settings/` are always denied. |
| K-4 | Kiro runs under pinned configuration that users and repos cannot override: login method, allowed hooks only, and retention/telemetry settings. If Kiro has no such mechanism, it is limited to `any-provider` content and only if K-17 approves it. | data-egress.md "Pinned configuration" | (P) Admin controls: console settings for models, MCP registry (allow-list or disable), API-key generation and web tools; `managed-settings.json` (CLI 2.25.0+) for sign-in controls and enterprise permission policies in the top `administration` scope, deny and ask only. Deny wins across all scopes, so admin `deny` rules on `fs_read`/`fs_write` can protect `local-only` paths. Kiro states MCP/registry controls are enforced client-side and can be circumvented with local administrative access. Whether `managed-settings.json` can pin agents, hooks or retention/telemetry: UNVERIFIED. |
| K-5 | Read interception covers file read, grep, glob, shell, MCP and retrieval, fails closed, and applies to **every** agent (default, named, subagents), not only the agent the adapter selected. | data-egress.md "Read hooks", "Tests" | (P) `PreToolUse` hooks can block (exit 2); any other non-zero exit is "hook failed; STDERR shown as warning", so a crashed hook does not block (fail-open). V3 non-interactive runs load hooks from the selected agent and the workspace (2.27.1+); global hooks live in `~/.kiro/hooks/`. Default hook timeout 60s; timeout outcome UNVERIFIED. (P) The local command sandbox covers shell commands only; built-in file tools are governed by permission rules, and remote HTTP/SSE MCP traffic ignores sandbox rules. So the boundary for built-in tools is admin `deny` permission rules, with hooks as a wrapper (always exit 2 on error) for defence in depth. |
| K-6 | A hook learns its runtime from which root-owned script ran, never from its input. | data-egress.md item 4 | Harness |
| K-7 | Subagents inherit no more than the task's permission profile, run under the same hooks as the parent (K-5), and respect a delegation-depth limit; `availableAgents`-style allowlisting is used if Kiro supports it. | README "Required controls" (delegation depth) | (P) Subagents inherit the parent's full permission rule set (deny-wins intersection) but **hook triggers are isolated**, so a hook does not fire inside a subagent. Protection for subagents must therefore come from permission `deny` rules, not hooks. `toolsSettings.subagent.availableAgents` is a glob allow-list (omitted = all agents); `trustedAgents` run without prompts; a custom subagent uses its own tools and permissions. Delegation-depth limit: UNVERIFIED. |
| K-8 | Kiro configuration inside the worktree (`.kiro/` agents, steering, hooks, MCP) and in the user's home is untrusted: it cannot override or weaken managed configuration or the harness-supplied agent, and the adapter ignores or overwrites it. | README "Trust boundaries" | (P) Workspace permission rules live outside the repo (`~/.kiro/workspace-roots/<hash>/`); a cloned repo cannot inject them; writes to `.kiro/settings/`, `.kiroignore` and Kiro state stores are always denied; writes to `.kiro` agents/hooks/workflows/powers always ask. Agent profiles, steering, workspace MCP config and skills in `.kiro/` are untrusted until the workspace is trusted, and trust is stored outside the workspace. Workspace hooks load in non-interactive runs (P); how workspace trust is decided in an unattended run: UNVERIFIED. Workspace agents win over global on a name clash (M). |
| K-9 | MCP servers available to a Kiro task come from the harness profile only. MCP server processes and web/fetch tools are treated as egress channels and covered by the same class check and OS-level read denies as K-5. | data-egress.md "Enforcement" | (P) Stdio MCP server processes are inside the sandbox only when `mcpSandboxing` wrapping is on; remote HTTP/SSE MCP traffic does not follow sandbox rules; every MCP tool call asks. Admins can disable MCP or restrict it to a registry allow-list (client-enforced). `--require-mcp-startup` makes the run exit 3 if a required server fails. |
| K-10 | A session whose runtime, account and settings match no BAA allowlist entry has the ceiling `any-provider`. | data-egress.md "BAA configuration allowlist" | Harness. (P) Kiro CLI is HIPAA eligible per Kiro's compliance page, which says the customer stays responsible for configuration. No Kiro-specific BAA process or allowlist-ready configuration was found (UNVERIFIED), so no allowlist entry exists. |
| K-11 | Output is validated against the harness result schema before use; invalid or unparseable output is a failed job and cannot advance task state. Output counts as evidence only after the **adapter** (not the Kiro process) writes it to the content-addressed store and records its digest. | README "Minimum acceptance tests"; approval-ledger.md `evidence_refs` | Harness; output format UNVERIFIED |
| K-12 | Verified `kiro-cli` version is recorded in README Prerequisites; the compatibility test fails on mismatch and auto-update is disabled or detected. | README "Pin runtime and extension versions" | Harness. (P) `kiro-cli settings app.disableAutoupdates true` disables background auto-updates; an enterprise "Managed updates" page exists (not read). (M) only the latest build is distributed, so an arbitrary older version may not be installable; record the installed version. |
| K-13 | Adapter enforces a deadline, can cancel a run and clean up its whole process tree, and records session identity and usage where available. | README "Adapter contract" | Harness; usage data UNVERIFIED |
| K-14 | **Egress gate at start:** the adapter refuses to launch when the target repo/worktree/path class exceeds the session ceiling (K-10). The class is assigned by the adapter/control plane, never by the session. | data-egress.md "Egress classes", "Enforcement" | Harness |
| K-15 | **Scrubbed environment:** Kiro starts with an allowlisted environment only. No `SSH_AUTH_SOCK`, signing keys, anchor deploy key, cloud or git-host tokens, or production credentials. Secrets are redacted from logs and artifacts before storage. | README "Required controls" | Harness |
| K-16 | The Kiro process cannot write the evidence store, `<local-artifacts-directory>/requests/` excepted for transition requests. | approval-ledger.md `evidence_refs`; AGENTS.md "Approvals and state" | Harness |
| K-17 | Kiro is added to the approved-provider list for `any-provider` use only after a human decision recording its model routing, retention, training use and telemetry terms. Until then the adapter refuses all Kiro jobs outside a local test fixture. | data-egress.md "Egress classes" (`any-provider` = approved for general engineering use) | Harness. (P) Kiro is built on Amazon Bedrock and uses cross-region inference; global routing may process requests outside the profile's geography, while storage stays in the profile region (US East for free/individual). Content use: free tier and individual subscribers may have content used for service improvement unless they opt out; enterprise users are not used for service improvement. Free tier inputs may be stored up to 60 days for abuse detection, and model-specific retention may apply to all tiers. Enterprise prompt logging goes to a customer S3 bucket. Kiro CLI is closed source (M). |
| K-18 | Forbidden-flag list: the adapter never passes trust-all, auto-approve or equivalent flags, and maps the task's permission profile to Kiro's tool permissions explicitly. | README "Required controls" (Cursor `--force`/`--yolo` analogue) | Harness. (P) Headless flags: `--trust-all-tools` (auto-approve all, forbidden) and `--trust-tools=<exact internal tool IDs>` (V3 only; allowed, narrowest set). `--trust-all-tools` also auto-approves hook confirmation requests. Docs recommend scoped `permissions.yaml` allow rules over trust flags. `--sandbox` makes the run exit when the sandbox cannot start; without it the run continues unsandboxed (require it). |
| K-19 | Context injected without a tool call (steering, agent `resources`, auto-loaded files) is limited to the harness-supplied set and checked against the class ceiling. | data-egress.md "Enforcement" | UNVERIFIED |
| K-20 | Kiro receives `AGENTS.md` through a confirmed instruction-discovery path; Kiro-specific steering cannot weaken it. | README "Confirm instruction discovery against pinned runtime versions" | UNVERIFIED |
| K-21 | When a BAA-class or `local-only` task is requested for Kiro: refuse. When the repo is `any-provider` but the Kiro account is unpinned: downgrade is not applicable (class already `any-provider`), proceed only under K-17. | data-egress.md L61 vs L97 (see Assumptions in handoff.md) | Harness; rule choice needs human confirmation |

## Cited company decisions

| Constraint | Source (file, repo commit `0b9d58f`) |
|---|---|
| Approvals live outside agents' reach | `docs/architecture/approval-ledger.md`, "Who writes, and how it is enforced" |
| Egress classes, BAA allowlist, forced configuration | `docs/security/data-egress.md` |
| Packet before code; one writer per worktree; data not instructions | `AGENTS.md`, "Before changing code" and "While working" |
| Adapter contract; required controls; trust boundaries | `README.md`, "Adapter contract", "Security and approvals" |

## Sources for Kiro claims

(P) kiro.dev, fetched 2026-10-29 (page footers show "updated" dates in Sept-Oct 2026):
- https://kiro.dev/docs/cli/headless/ (updated 2026-10-21), https://kiro.dev/docs/reference/exit-codes/ (2026-10-05)
- https://kiro.dev/docs/hooks/ (2026-10-21), https://kiro.dev/docs/permissions/, https://kiro.dev/docs/sandbox/
- https://kiro.dev/docs/custom-agents/subagents/ , https://kiro.dev/docs/reference/settings/
- https://kiro.dev/docs/enterprise/governance/ (2026-09-30), https://kiro.dev/docs/enterprise/governance/mcp/
- https://kiro.dev/docs/privacy-and-security/data-protection/ , https://kiro.dev/docs/privacy-and-security/compliance-validation/

(M), public GitHub, researched 2026-10-28, not re-read on kiro.dev:
- https://github.com/kirodotdev/KiroCrew (`docs/reference/kiro-cli/`), https://github.com/kirodotdev-labs/kiro-action
- https://github.com/aws/amazon-q-developer-cli (README: Kiro CLI is closed source)

(S), third-party or snippets: pricing aggregators; changelog https://kiro.dev/changelog/cli/ (latest seen 2.34.0, 2026-10-23). Pricing is not used in any requirement.

Not found: `stream-json` event schema; hook timeout outcome; delegation-depth limit; whether `managed-settings.json` can pin agents, hooks or retention; how workspace trust is decided in unattended runs; Kiro-specific BAA or zero-retention terms; binary licence text.

## Unresolved questions

- [ ] Can `managed-settings.json` pin agents, hooks and permissions, or only deny/ask rules and sign-in? Is admin `deny` on `fs_read` enough to protect `local-only` paths for built-in tools? (P: deny rules supported, enforcement is client-side.) — owner: TBD, blocks: T3
- [ ] Hooks are fail-open on non-0/2 exit and do not fire in subagents (P). Accept relying on permission `deny` rules as the real boundary, with hooks as defence in depth? — owner: TBD, blocks: T3
- [ ] `stream-json` event schema and a stable end-of-run record for K-11 (P says an interrupted V3 run writes a final interruption record). — owner: TBD, blocks: T4
- [ ] How is workspace trust decided in an unattended run, given workspace hooks and agent profiles load in `--no-interactive`? Can the adapter force an untrusted state or ignore workspace `.kiro/` (K-8)? — owner: TBD, blocks: T3
- [ ] Provider approval for Kiro (K-17): Bedrock-backed, global cross-region inference possible, retention terms by tier. Which tier and account type would be used? — owner: TBD, blocks: T3, T4
- [ ] Is Kiro CLI usable for `baa-covered-providers` work? (P: HIPAA eligible; BAA/configuration details not found.) Default per K-21: no. — owner: TBD, blocks: T3
- [ ] `KIRO_API_KEY` needs a paid plan and admin enablement; a browser login takes precedence. How does the runner guarantee only the API key is used (K-4)? — owner: TBD, blocks: T3
- [ ] Confirm the K-21 rule (refuse BAA/local-only tasks; `any-provider` only under K-17), resolving the "refused" vs "treated as any-provider" wording in data-egress.md. — owner: TBD, blocks: T3
