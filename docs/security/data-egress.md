# Data egress policy

Status: Design (Phase 1). Applies to every agent runtime, the company brain, and any
indexing or embedding job.

## Scope

"Egress" is any content leaving this machine to a hosted model or embedding provider:
prompts, file contents read into a session, tool results, retrieved documents, and
**indexing or embedding**. Embedding a document discloses it to the embedding provider
just as pasting it into a prompt does.

At least one project served by this harness is HIPAA-eligible. Content that may
contain or reveal PHI may only go to providers covered by a Business Associate
Agreement (BAA), and only when it is needed.

## Egress classes

Every document and repository carries one class. Missing or unknown class means
`local-only`.

| Class | May be sent to | Typical content |
|---|---|---|
| `local-only` | No hosted provider. Local tools and local models only. | Incidents, customer-specific constraints, production logs, data exports, anything PHI-adjacent, anything unclassified. |
| `baa-covered-providers` | Only configurations listed in `<baa-configuration-allowlist>` (see below). | Source code and design docs of the HIPAA-eligible project, sanitized runbooks. |
| `any-provider` | Any provider approved for general engineering use. | Public docs, open-source code, generic engineering guidance with no customer data. |

Rules:

- **Default to most restrictive.** New documents start as `local-only`; widening a class
  is a reviewed change, narrowing is always allowed.
- A derived artifact (summary, lesson, embedding, excerpt) inherits the **most
  restrictive** class of its inputs.
- A session or job may only receive content whose class permits the provider that
  session runs on. The control plane checks this before retrieval returns content,
  not after.

## BAA configuration allowlist

A BAA covers specific products in specific configurations, not a vendor. Each entry in
`<baa-configuration-allowlist>` names all of:

| Field | Example |
|---|---|
| Runtime and surface | `claude-code-cli`, `codex-cli`, `cursor-desktop` |
| Account / auth path | `<org>` API console key, Enterprise SSO workspace |
| Required setting | Zero data retention applied to the org; HIPAA-eligible workspace; Privacy Mode locked org-wide |
| BAA version and date accepted | `<baa-version>`, `<YYYY-MM-DD>` |
| Verified by / on | `<owner>`, `<YYYY-MM-DD>` |

Vendor conditions at the research baseline (2026-10-26; recheck before relying on them):

- **Claude Code:** covered only with zero data retention enabled for the organization,
  which Anthropic applies per organization. Which surfaces qualify (CLI, desktop local
  mode) must be confirmed with Anthropic.
- **Codex:** covered when signed in to a HIPAA-eligible ChatGPT workspace. The BAA
  does not cover the local machine or third-party tools Codex calls.
- **Cursor:** Enterprise only, with Privacy Mode locked organization-wide, for listed
  eligible services.

A session whose runtime, account and settings do not match an entry is treated as
`any-provider`. An unverified or expired entry counts as no entry.

## Enforcement

The control plane's retrieval check covers only content it returns. Interactive sessions
read files directly, and no runtime tells a hook which account, zero-data-retention or
privacy setting it runs under (anything in the hook's environment can be set by the
session). So the session's configuration is **forced by managed configuration, not
detected**. Layers, strongest first:

1. **Placement (the boundary).** `local-only` content is owned by a separate macOS user
   (`<local-only-user>`, mode 600) and never stored in any checkout, worktree, or directory
   a hosted-agent session can reach.
2. **OS-level read deny.** Each runtime's sandbox denies reads of paths classed above the
   session's ceiling: Claude Code `sandbox.filesystem.denyRead`, Codex managed `deny_read`.
   Cursor offers only `sandbox.readBoundary`.
3. **Pinned configuration.** Root-owned managed config, which users and projects cannot
   override, pins each runtime to the account and workspace in its BAA allowlist entry and
   allows managed hooks only:
   - Claude Code (`/Library/Application Support/ClaudeCode/managed-settings.json` or MDM):
     `forceLoginMethod`, `forceLoginOrgUUID`, `allowedProviders`, `allowManagedHooksOnly`.
   - Codex (`/etc/codex/requirements.toml` or MDM): `allowed_login_methods`,
     `allowed_chatgpt_workspaces`, `allow_managed_hooks_only`.
   - Cursor (`/Library/Application Support/Cursor/hooks.json`): Enterprise only, with
     privacy mode enforced and MDM allowed team IDs.
4. **Read hooks (defense in depth).** Each runtime's managed config points at its **own**
   root-owned hook script (directory mode 755, scripts 555), so a hook knows its runtime
   from which script ran, never from its input. The hook reads the root-owned profile file
   `<egress-profile-file>` (root:wheel, mode 644), which maps each runtime to the highest
   class it may read; an administrator writes it from the verified allowlist. Unknown
   runtime, a missing entry, or a read error means deny. Hooks are set to fail closed where
   the runtime allows it (Claude Code `onFailure: "block"`, Cursor `failClosed: true`); the
   Codex documentation calls its hooks a guardrail, not a boundary.
5. **Tests.** The acceptance suite places a canary `local-only` file and checks that each
   read path (file read, grep, shell `cat`, MCP, retrieval) is denied for each hosted
   runtime, and that a session on an unpinned account is refused.

Residual risk: a local administrator can change all of the above. Deploying the managed
files through MDM narrows it.

### Runtime capabilities (researched 2026-10-26)

| | Claude Code 2.1.310 | Codex CLI 0.163.0 | Cursor `agent` 2026.06.02 |
|---|---|---|---|
| Read interception | `PreToolUse` on Read, Grep, Glob, Bash, `mcp__*` | `PreToolUse` on shell, patch, MCP and local tools | `beforeReadFile`, `beforeShellExecution`, `beforeMCPExecution`; CLI coverage of non-shell hooks reported incomplete (unverified) |
| Identity fields in hook input | None (session, cwd, tool) | `model` only | `model`, `user_email`; no privacy-mode or plan field |
| Failure default | Fails open unless `onFailure: "block"` | Invalid output fails open | Fails open unless `failClosed: true` |

The Claude Code and Codex managed keys were confirmed present in the installed binaries;
the rest is from vendor documentation: https://code.claude.com/docs/en/hooks,
https://code.claude.com/docs/en/managed-settings, https://code.claude.com/docs/en/permissions,
https://learn.chatgpt.com/docs/hooks, https://learn.chatgpt.com/docs/enterprise/managed-configuration.md,
https://cursor.com/docs/hooks, https://cursor.com/docs/cli/reference/configuration.

## Indexing and embedding

- Indexing jobs read the class first and skip anything not permitted for the configured
  embedding provider.
- **PHI-adjacent content is excluded from indexing by default**: `incidents/`, customer
  constraints, operational logs, support tickets, and data samples. Including them
  requires an explicit, reviewed exception naming the provider and its BAA.
- Prefer local full-text search for `local-only` content (README "Retrieval strategy":
  file search and full-text before hybrid retrieval).

## Recording the class

- In company-brain documents: an `egress_class` field alongside the README "Required
  metadata" (document ID, status, owner, sources, last verified).
- For repositories: a default in `config/harness.local.yaml` (`<egress-class-key>`),
  overridable per path.

## Provenance check for `brain.propose_update`

`brain.propose_update(document_id?, patch, evidence_refs)` is rejected unless:

1. `evidence_refs` is non-empty and every ref resolves to a source the proposer could
   read (document ID and revision, commit SHA, or artifact path).
2. The proposal's egress class is at least as restrictive as every cited source.
3. If any cited source is a log, an issue or ticket body, a review comment, or other
   retrieved text, the proposal is flagged **`untrusted_origin: true`**. Proposals flagged
   `untrusted_origin` require **human** review; a policy actor cannot accept them. Such text may
   contain instructions written by someone outside the team and is treated as data.

A proposal with no traceable sources is stored, if at all, as an unverified
investigation note (README "Learning workflow"), never as accepted knowledge.

## Open questions

- Which configurations this organization actually holds (`<baa-configuration-allowlist>`),
  who maintains the list, and how often it is re-verified?
- Whether to use MDM (or root-installed managed files) on the development Mac, and
  whether Cursor is used at all for `baa-covered-providers` work, given its CLI hook
  coverage is unconfirmed and BAA coverage needs Enterprise.
- Redaction: is sanitizing a `local-only` document into a wider class allowed, and who
  reviews the sanitized result?
