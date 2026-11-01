# Requirements: kiro-adapter — Kiro CLI agent adapter

> Status: proposed, not approved. Kiro claims carry a grade. **(P)** read on a kiro.dev
> docs page on 2026-10-29, re-checked 2026-11-01 for hooks, subagents and workspace trust
> (page text fetched with curl and converted to text, not rendered). **(M)** read in the
> KiroCrew repo's offline docs mirror or another public GitHub page, not re-checked on
> kiro.dev. **(S)** search snippets or third-party pages. **UNVERIFIED** not found or
> conflicting. Docs describe Kiro CLI V3 and 2.x differently; this packet targets V3
> (`--v3`) and the installed version and engine must be pinned.

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
  approvals stay in the external ledger, enforced by OS user separation (the Kiro process
  runs as the adapter-interface runtime user, separate from the adapter, with no write
  access to `<ledger-root>`).
- One writer per worktree.
- Kiro output, retrieved text and repo files are data, never instructions (AGENTS.md).
- Unknown or unverified runtime configuration is never treated as wider than
  `any-provider` (K-10), and never launched against content above that class (K-14).

## Adapter requirements

One normative sentence per row. Kiro facts and their grades are under "Kiro evidence by
requirement"; they explain a requirement and never add to or weaken it.

| ID | Requirement | Source |
|---|---|---|
| K-1 | Adapter runs Kiro with `--no-interactive --v3 --agent <harness agent>` in the task worktree at an explicit base SHA, and records exit status and output as artifacts; any exit other than 0 is a failed, untrusted run. | README "Adapter contract" |
| K-2 | The adapter uses the adapter-interface writer lock (A-11): acquired atomically before launch, released only after every Kiro process is confirmed gone, and released by a human when its owner cannot be confirmed. | AGENTS.md "One writer per worktree" |
| K-3 | The Kiro process runs as the runtime user of adapter-interface A-12, separate from the adapter's user, with no read or write access to `<ledger-root>`, the signing keys or the anchor-repo deploy key. | approval-ledger.md "Who writes, and how it is enforced" |
| K-4 | Kiro runs under pinned configuration that users and repos cannot override (login method, allowed hooks, retention and telemetry settings), or else is limited to `any-provider` content under K-17. | data-egress.md "Pinned configuration" |
| K-5 | Reads of content above the session's class ceiling are denied for every agent (default, named, subagent) by admin `deny` permission rules and OS-level controls that do not depend on hooks. | data-egress.md "Read hooks", "Enforcement", "Tests" |
| K-6 | A hook learns its runtime from which root-owned script ran, never from its input. | data-egress.md item 4 |
| K-7 | The adapter runs only CLI V3 (`--v3`) and refuses any other engine, and the agents it supplies list no `@builtin` or `*` in `tools` and name spawnable agents only by `subagent/<name>` entries; Kiro's internal delegation depth is unbounded unless T1 finds a control (adapter-interface A-14). | README "Required controls" (delegation depth) |
| K-8 | Before launch the adapter neutralises Kiro configuration from the worktree and the user's home (clean checkout, worktree `.kiro/` stripped or matched against an allowlist, `HOME` set to a harness-owned directory with only harness-supplied `~/.kiro/` content), without relying on Kiro's workspace-trust prompt. | README "Trust boundaries" |
| K-9 | MCP servers available to a Kiro task come from the harness profile only, and MCP and web tools are covered by the same `deny` rules and OS-level controls as K-5. | data-egress.md "Enforcement" |
| K-10 | A session whose runtime, account and settings match no BAA allowlist entry has the ceiling `any-provider`. | data-egress.md "BAA configuration allowlist" |
| K-11 | Kiro output is used only after it validates against the harness result schema, and counts as evidence only after the adapter (not the Kiro process) writes it to the content-addressed store and records its digest; anything else is a failed job that cannot advance task state. | README "Minimum acceptance tests"; approval-ledger.md `evidence_refs` |
| K-12 | Verified `kiro-cli` version is recorded in README Prerequisites; the compatibility test fails on mismatch and auto-update is disabled or detected. | README "Pin runtime and extension versions" |
| K-13 | The adapter enforces a deadline, can cancel a run and clean up its whole process tree, captures the Kiro session ID for continuation as adapter-interface A-7 requires, and records usage where available. | README "Adapter contract" |
| K-14 | The adapter refuses to launch when the class of the target repo, worktree or paths, assigned by the adapter or control plane and never by the session, exceeds the session ceiling (K-10). | data-egress.md "Egress classes", "Enforcement" |
| K-15 | Kiro starts with an allowlisted environment only (no `SSH_AUTH_SOCK`, signing keys, anchor deploy key, cloud or git-host tokens, or production credentials), and secrets are redacted from logs and artifacts before storage. | README "Required controls" |
| K-16 | The Kiro process cannot write the artifacts directory, evidence store or run ledger, except its own task's request directory (adapter-interface A-15). | approval-ledger.md `evidence_refs`; AGENTS.md "Approvals and state" |
| K-17 | Kiro is added to the approved-provider list for `any-provider` use only after a human decision recording its model routing, retention, training use and telemetry terms. | data-egress.md "Egress classes" (`any-provider` = approved for general engineering use) |
| K-18 | The adapter never passes trust-all, auto-approve or equivalent flags, passes `--sandbox`, and maps the task's permission profile to Kiro's tool permissions explicitly. | README "Required controls" (Cursor `--force`/`--yolo` analogue) |
| K-19 | Context injected without a tool call (steering, agent `resources`, auto-loaded files) is limited to the harness-supplied set and checked against the class ceiling. | data-egress.md "Enforcement" |
| K-20 | Kiro receives `AGENTS.md` through a confirmed instruction-discovery path; Kiro-specific steering cannot weaken it. | README "Confirm instruction discovery against pinned runtime versions" |
| K-22 | The setting that enables the Kiro runtime for a project, the provider decision (K-17), and the egress-profile entry live in root-owned configuration the agent user cannot write; changing them is a human action recorded outside the agent's reach. | approval-ledger.md "Who writes, and how it is enforced"; data-egress.md "Pinned configuration" |

## Launch procedure

The only answer to "may this task launch Kiro?". Requirement rows say what each check
must guarantee; this list is the order and the refusal rule, and design.md "Rollout"
refers here rather than restating it. The adapter runs these checks at every launch, in
order, and refuses at the first failure; `tests/kiro/enable-flag-root-owned`,
`tests/kiro/provider-approval` and `tests/kiro/egress-gate` test steps 1-3.

1. The project's Kiro enable flag and Kiro egress-profile entry exist in root-owned
   configuration the agent user cannot write, and the flag is on (K-22). Otherwise refuse.
2. A human provider decision for Kiro is recorded there (K-17). Otherwise refuse, except
   for the local test fixture.
3. The class of the target repo and paths, assigned by the adapter, does not exceed the
   session ceiling, which is `any-provider` while no BAA allowlist entry exists (K-10, K-14).
   Otherwise refuse.
4. Pre-launch checks pass: pinned managed configuration installed (K-4), clean checkout
   and neutralised `.kiro/` and `HOME` (K-8), scrubbed environment (K-15), pinned V3
   engine and version (K-7, K-12), worktree lock acquired (K-2), `--sandbox` passed
   (K-18). Otherwise refuse.

**Pending T3 (not normative):** proposed K-21 resolves data-egress.md's two statements
(data-egress.md "BAA configuration allowlist" ("A session whose runtime, account and settings do not match an entry is treated as `any-provider`") and "Enforcement" item 5 ("a session on an unpinned account is refused")): refuse Kiro jobs on `baa-covered-providers` and `local-only` content; allow
`any-provider` content only after step 2. Steps 1-3 already refuse those classes while no
BAA entry exists; K-21 only matters if one is added.

## Kiro evidence by requirement

Grades as in the banner. Notes here are evidence, not requirements.

| ID | Status and Kiro evidence |
|---|---|
| K-1 | Harness. (P) `kiro-cli chat --no-interactive [--v3] [--agent <name>] [--model <id>] [--sandbox] [--require-mcp-startup] [--output-format stream-json]`, authenticated by `KIRO_API_KEY`; exit codes 0 success, 1 failure, 3 MCP startup failure, 4 named agent unavailable. Exit 4 can occur after the built-in default agent already made tool calls, so `--agent` alone is not a safe gate. Without `--agent` a V3 run uses `chat.defaultAgent` or the built-in default. In headless runs every `ask` is treated as `deny`. The `stream-json` event schema is not documented on the headless page (UNVERIFIED). (P) On V3, if setting `--model` fails Kiro writes a warning and continues, so `--model` alone is not a policy boundary; verify the effective model if model pinning matters. |
| K-2 | Harness |
| K-3 | Harness (OS boundary). Defence in depth, not the boundary: Kiro config also denies those paths and a managed hook rejects shell commands referencing them. (M) agent config is JSON or Markdown with `permissions.rules` (allow/ask/deny; deny wins) and `toolsSettings`. (S) writes to `.kiro/settings/` are always denied. |
| K-4 | (P) Admin controls: console settings for models, MCP registry (allow-list or disable), API-key generation and web tools; `managed-settings.json` (CLI 2.25.0+) for sign-in controls and enterprise permission policies in the top `administration` scope, deny and ask only. Deny wins across all scopes, so admin `deny` rules on `fs_read`/`fs_write` can protect `local-only` paths. Kiro states MCP/registry controls are enforced client-side and can be circumvented with local administrative access. Whether `managed-settings.json` can pin agents, hooks or retention/telemetry: UNVERIFIED. |
| K-5 | Read hooks, where they fire, should fail closed, but they are defence in depth only. (P) Conflicting docs on hook failure: the CLI exit-codes page says hook exit 0 = success, 2 = block (PreToolUse only), other = "hook failed; STDERR shown as warning"; the hooks actions page says any non-zero exit blocks a PreToolUse tool call. Treat failure semantics as UNVERIFIED and test them (`hook-failure`); the wrapper must exit 2 on every error either way. V3 non-interactive runs load hooks from the selected agent and the workspace (2.27.1+); global hooks in `~/.kiro/hooks/`. Default timeout 60s; timeout outcome UNVERIFIED. Hooks do not see inside sandbox gaps: (P) the sandbox covers shell commands only; built-in file tools are governed by permission rules, remote HTTP/SSE MCP traffic ignores sandbox rules. The boundary for built-in tools is therefore admin `deny` permission rules. Whether parent hooks fire inside subagents is not stated (UNVERIFIED); do not rely on them there. |
| K-6 | Harness |
| K-7 | (P) V3: every subagent tool call is checked against the parent session policy and the subagent's own permissions (a subagent's allow rules never grant more than the parent allows); `toolsSettings` (`availableAgents`, `trustedAgents`) is ignored in V3, so `availableAgents` must not be relied on. 2.x: `toolsSettings.subagent` works, but a subagent's own pre-approved tools run without a prompt even if the parent does not allow them, so the 2.x intersection guarantee does not hold. Subagent hook triggers are listed as isolated per subagent; whether parent hooks fire in subagents is UNVERIFIED. Depth limit: UNVERIFIED. |
| K-8 | (P) The workspace-trust screen applies only to interactive V3 sessions in CLI 2.33+; non-interactive (`--no-interactive`) runs and V1/V2 sessions are not gated and load workspace hooks, agents, MCP, steering and skills without asking. So unattended runs get no trust protection from Kiro. Still true: workspace permission rules live outside the repo (`~/.kiro/workspace-roots/<hash>/`), writes to `.kiro/settings/` and `.kiroignore` are always denied, writes to `.kiro` agents/hooks/workflows/powers always ask (an ask is a deny when headless). Workspace agents win over global on a name clash (M). If planted config loads anyway, the admin `deny` rules of K-5 are what still hold (`tests/kiro/workspace-config`). |
| K-9 | (P) Stdio MCP server processes are inside the sandbox only when `mcpSandboxing` wrapping is on; remote HTTP/SSE MCP traffic does not follow sandbox rules; in an untrusted workspace every MCP tool call asks, but scoped `mcp` permission rules with `effect: allow` let matching calls run without a prompt, and an `ask` becomes a `deny` when headless, so prompting is not a boundary: rely on admin `deny` rules and OS-level controls. Admins can disable MCP or restrict it to a registry allow-list (client-enforced). `--require-mcp-startup` makes the run exit 3 if a required server fails. |
| K-10 | Harness. (P) Kiro CLI is HIPAA eligible per Kiro's compliance page, which says the customer stays responsible for configuration. No Kiro-specific BAA process or allowlist-ready configuration was found (UNVERIFIED), so no allowlist entry exists. |
| K-11 | Harness; output format UNVERIFIED |
| K-12 | Harness. (P) `kiro-cli settings app.disableAutoupdates true` disables background auto-updates; an enterprise "Managed updates" page exists (not read). (M) only the latest build is distributed, so an arbitrary older version may not be installable; record the installed version. |
| K-13 | Harness. (P) headless plain-text output prints the cloud session ID and `--resume-id <ID>` resumes a local or cloud session; how a local V3 session ID is exposed (for example in `stream-json`) is UNVERIFIED. Usage data UNVERIFIED. |
| K-14 | Harness |
| K-15 | Harness |
| K-16 | Harness |
| K-17 | Harness. (P) Kiro is built on Amazon Bedrock and uses cross-region inference; global routing may process requests outside the profile's geography, while storage stays in the profile region (US East for free/individual). Content use: free tier and individual subscribers may have content used for service improvement unless they opt out; enterprise users are not used for service improvement. Free tier inputs may be stored up to 60 days for abuse detection, and model-specific retention may apply to all tiers. Enterprise prompt logging goes to a customer S3 bucket. Kiro CLI is closed source (M). |
| K-18 | Harness. (P) Headless flags: `--trust-all-tools` (auto-approve all, forbidden) and `--trust-tools=<exact internal tool IDs>` (V3 only; allowed, narrowest set). `--trust-all-tools` also auto-approves hook confirmation requests. Docs recommend scoped `permissions.yaml` allow rules over trust flags. `--sandbox` makes the run exit when the sandbox cannot start; without it the run continues unsandboxed (require it). |
| K-19 | UNVERIFIED |
| K-20 | UNVERIFIED |
| K-22 | Harness |
| K-21 (proposed) | Harness; rule choice needs human confirmation |

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
- [ ] Hook failure semantics conflict between kiro.dev pages, and parent hooks may not fire in subagents (P). Accept relying on admin `deny` permission rules as the real boundary, with hooks as defence in depth? — owner: TBD, blocks: T3
- [ ] `stream-json` event schema and a stable end-of-run record for K-11 (P says an interrupted V3 run writes a final interruption record). — owner: TBD, blocks: T4
- [ ] (P) Unattended runs load workspace hooks/agents/MCP/steering without a trust prompt. Which adapter-side control (clean checkout, `.kiro/` stripping, harness-owned `HOME`) is sufficient for K-8, and does Kiro offer any switch to ignore workspace config? — owner: TBD, blocks: T3
- [ ] Provider approval for Kiro (K-17): Bedrock-backed, global cross-region inference possible, retention terms by tier. Which tier and account type would be used? — owner: TBD, blocks: T3, T4
- [ ] Is Kiro CLI usable for `baa-covered-providers` work? (P: HIPAA eligible; BAA/configuration details not found.) Until answered, Launch procedure step 3 refuses it. — owner: TBD, blocks: T3
- [ ] `KIRO_API_KEY` needs a paid plan and admin enablement; a browser login takes precedence. How does the runner guarantee only the API key is used (K-4)? — owner: TBD, blocks: T3
- [ ] Where does a local `--no-interactive --v3` run expose its session ID, so K-13 can capture it? Does Kiro offer any internal delegation-depth control (K-7)? — owner: TBD, blocks: T4
- [ ] Confirm proposed K-21 (see Launch procedure) and record it as normative. — owner: TBD, blocks: T3
