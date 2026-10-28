# Requirements: kiro-adapter — Kiro CLI agent adapter

> DRAFT. kiro.dev was unreachable from the authoring session (proxy 403 on CONNECT), so
> no kiro.dev page was read directly. Kiro claims carry a grade:
> **(M)** read in the KiroCrew repo's offline mirror of the kiro-cli docs (a GitHub
> mirror, not the primary docs, summarised by a research agent) or another public GitHub
> page; **(S)** seen only in search snippets or third-party pages; **UNVERIFIED** not
> found. Re-check every (M) and (S) item against https://kiro.dev/docs/cli/ before
> approval. Sources are listed at the end.

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
| K-1 | Adapter runs Kiro non-interactively with a named agent in the task worktree (explicit workspace and base SHA), and records exit status and output as artifacts. | README "Adapter contract" | Harness. (M) headless is `kiro-cli chat --no-interactive "<prompt>"` with `KIRO_API_KEY` (a browser login takes precedence; API keys need a paid plan); `--agent` exists. No documented exit-code table and no output schema found; JSON output is only an open feature request (Kiro issue 5423); `--output-format stream-json` is (S). |
| K-2 | Adapter acquires an atomic per-worktree writer lock (no check-then-start race) before launching, and releases it on exit, crash or cancel. | AGENTS.md "One writer per worktree" | Harness |
| K-3 | The Kiro process and adapter run as an OS user with no read or write access to `<ledger-root>`, the signing keys or the anchor-repo deploy key. Kiro config also denies those paths and a managed hook rejects shell commands referencing them (defence in depth only). | approval-ledger.md "Who writes, and how it is enforced" | Harness (OS boundary). (M) agent config is JSON or Markdown with `permissions.rules` (allow/ask/deny; deny wins) and `toolsSettings`. (S) writes to `.kiro/settings/` are always denied. |
| K-4 | Kiro runs under pinned configuration that users and repos cannot override: login method, allowed hooks only, and retention/telemetry settings. If Kiro has no such mechanism, it is limited to `any-provider` content and only if K-17 approves it. | data-egress.md "Pinned configuration" | (S) `managed-settings.json` in a system path (CLI 2.25.0+) is the top layer but limited to deny/ask rules, sign-in methods, MCP registry and model lists; Kiro's own MCP docs say controls are client-side and bypassable with local admin. Whether agents, hooks or permissions can be force-pinned: UNVERIFIED. Retention/telemetry pinning: UNVERIFIED. |
| K-5 | Read interception covers file read, grep, glob, shell, MCP and retrieval, fails closed, and applies to **every** agent (default, named, subagents), not only the agent the adapter selected. | data-egress.md "Read hooks", "Tests" | (M) hooks: AgentSpawn, UserPromptSubmit, PreToolUse, PostToolUse, Stop; exit 2 blocks only on PreToolUse/UserPromptSubmit; matchers cover built-in and MCP tools. (M) the mirror says fail-closed behaviour is Kiro Crew's, with no upstream counterpart; (S) upstream treats other non-zero exits as a warning and proceeds, so upstream PreToolUse is fail-open on crash/timeout. The harness hook must therefore be a wrapper that always exits 2 on error. (S) the sandbox covers shell commands and children, not built-in file tools. Whether hooks apply to every agent: UNVERIFIED. |
| K-6 | A hook learns its runtime from which root-owned script ran, never from its input. | data-egress.md item 4 | Harness |
| K-7 | Subagents inherit no more than the task's permission profile, run under the same hooks as the parent (K-5), and respect a delegation-depth limit; `availableAgents`-style allowlisting is used if Kiro supports it. | README "Required controls" (delegation depth) | (M) subagents share the parent's steering, MCP servers, file access and permissions config, but **hook triggers are isolated**, so parent hooks may not fire inside a subagent. `toolsSettings.subagent.availableAgents` is a glob allow-list (omitted = all agents allowed); `trustedAgents` run without prompts. A custom subagent uses its own tools and permissions. |
| K-8 | Kiro configuration inside the worktree (`.kiro/` agents, steering, hooks, MCP) and in the user's home is untrusted: it cannot override or weaken managed configuration or the harness-supplied agent, and the adapter ignores or overwrites it. | README "Trust boundaries" | (M) workspace agents (`.kiro/agents/`) win over global on a name clash, and load only if the workspace is trusted. (S) workspace permission rules are stored per user outside the repo. |
| K-9 | MCP servers available to a Kiro task come from the harness profile only. MCP server processes and web/fetch tools are treated as egress channels and covered by the same class check and OS-level read denies as K-5. | data-egress.md "Enforcement" | (M) stdio MCP servers run outside the tool-execution sandbox with the agent's full privileges, so they are a read/exfiltration path the hook does not see. |
| K-10 | A session whose runtime, account and settings match no BAA allowlist entry has the ceiling `any-provider`. | data-egress.md "BAA configuration allowlist" | Harness. (S) Kiro's compliance page says IDE and CLI are HIPAA eligible (Web excluded); no Kiro-specific BAA process was found (UNVERIFIED), so no allowlist entry exists. |
| K-11 | Output is validated against the harness result schema before use; invalid or unparseable output is a failed job and cannot advance task state. Output counts as evidence only after the **adapter** (not the Kiro process) writes it to the content-addressed store and records its digest. | README "Minimum acceptance tests"; approval-ledger.md `evidence_refs` | Harness; output format UNVERIFIED |
| K-12 | Verified `kiro-cli` version is recorded in README Prerequisites; the compatibility test fails on mismatch and auto-update is disabled or detected. | README "Pin runtime and extension versions" | Harness. (M) Kiro auto-updates and only the latest build is distributed, so a version cannot be installed on demand; hold it with `kiro-cli settings app.disableAutoupdates true` and record the installed version. |
| K-13 | Adapter enforces a deadline, can cancel a run and clean up its whole process tree, and records session identity and usage where available. | README "Adapter contract" | Harness; usage data UNVERIFIED |
| K-14 | **Egress gate at start:** the adapter refuses to launch when the target repo/worktree/path class exceeds the session ceiling (K-10). The class is assigned by the adapter/control plane, never by the session. | data-egress.md "Egress classes", "Enforcement" | Harness |
| K-15 | **Scrubbed environment:** Kiro starts with an allowlisted environment only. No `SSH_AUTH_SOCK`, signing keys, anchor deploy key, cloud or git-host tokens, or production credentials. Secrets are redacted from logs and artifacts before storage. | README "Required controls" | Harness |
| K-16 | The Kiro process cannot write the evidence store, `<local-artifacts-directory>/requests/` excepted for transition requests. | approval-ledger.md `evidence_refs`; AGENTS.md "Approvals and state" | Harness |
| K-17 | Kiro is added to the approved-provider list for `any-provider` use only after a human decision recording its model routing, retention, training use and telemetry terms. Until then the adapter refuses all Kiro jobs outside a local test fixture. | data-egress.md "Egress classes" (`any-provider` = approved for general engineering use) | Harness. (S) individual/free content may be collected for service improvement (opt-out); inputs may be kept up to 60 days for abuse detection regardless of opt-out; enterprise accounts are opted out automatically. Kiro CLI is closed source (M). Model routing and training terms: UNVERIFIED. |
| K-18 | Forbidden-flag list: the adapter never passes trust-all, auto-approve or equivalent flags, and maps the task's permission profile to Kiro's tool permissions explicitly. | README "Required controls" (Cursor `--force`/`--yolo` analogue) | Harness. (M) `--trust-all-tools` / `/tools trust-all` exist (kiro-action defaults to `--trust-all-tools`) and must be forbidden; (S) headless guidance is `--trust-tools <exact tool IDs>` with `--no-interactive`, and `--sandbox` makes the run exit if the sandbox cannot start (without it, it continues unsandboxed). |
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

## Sources for Kiro claims (none read on kiro.dev itself)

(M), public GitHub, researched 2026-10-28:
- https://github.com/kirodotdev/KiroCrew (`docs/reference/kiro-cli/`: installation, authentication, hooks, custom-agents/configuration-reference, chat/subagents, mcp/security; upstream fetch dated 2026-09-06)
- https://github.com/kirodotdev-labs/kiro-action , https://github.com/kirodotdev/Kiro/issues/5423 , https://github.com/kirodotdev/KiroCrew/issues/7744
- https://github.com/aws/amazon-q-developer-cli (README: Kiro CLI is closed source)

(S), search snippets or third-party pages:
- https://kiro.dev/docs/cli/headless/ , https://kiro.dev/docs/sandbox/ , https://kiro.dev/docs/permissions/
- https://kiro.dev/docs/enterprise/governance/ , https://kiro.dev/docs/enterprise/governance/mcp/
- https://kiro.dev/docs/privacy-and-security/data-protection/ , https://kiro.dev/docs/privacy-and-security/compliance-validation/
- https://kiro.dev/changelog/cli/ (latest official entry seen: 2.34.0, 2026-10-23); a third-party tracker lists v3.0.0, unconfirmed.
- Pricing (third-party only, re-check): Free 50 credits; Pro $20/1,000; Pro+ $40/2,000; Pro Max $100/5,000; Power $200/10,000; overage $0.04/credit.

Not found: exit-code table, `stream-json` schema, whether user rules can override managed settings, Kiro-specific BAA or zero-retention statement, binary licence text.

## Unresolved questions

- [ ] Can `managed-settings.json` pin agents, hooks and permissions, and can user rules override it? (S: deny/ask only, bypassable with local admin.) — owner: TBD, blocks: T3
- [ ] Do hooks apply inside subagents and to every agent? (M: subagent hook triggers are isolated.) Can the wrapper hook guarantee exit 2 on crash/timeout? — owner: TBD, blocks: T3
- [ ] Output format and exit codes for `--no-interactive` runs: nothing documented; is adapter-side parsing of text output acceptable (K-11)? — owner: TBD, blocks: T4
- [ ] Can a single account/API key meet K-4 (pinned login) for an unattended runner, given a browser login overrides `KIRO_API_KEY`? — owner: TBD, blocks: T3
- [ ] Is Kiro an approved provider; what are its routing, retention, training and telemetry terms (K-17)? — owner: TBD, blocks: T3, T4
- [ ] Which auth paths exist; is any covered by a BAA or zero-retention terms? — owner: TBD, blocks: T3
- [ ] Is Kiro used at all for `baa-covered-providers` work? Default per K-21: no. — owner: TBD, blocks: T3
- [ ] Confirm the K-21 rule (refuse BAA/local-only tasks; `any-provider` only under K-17), resolving the "refused" vs "treated as any-provider" wording in data-egress.md. — owner: TBD, blocks: T3
