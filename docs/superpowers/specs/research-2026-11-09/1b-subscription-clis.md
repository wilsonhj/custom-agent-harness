# Track 1b: Claude Code, Codex CLI and Cursor CLI under subscription logins

Date: 2026-11-09. Author: research agent (read-only).

Grades: **(P)** probed locally on this Mac today (`--help`/`--version` only, plus throwaway
`sandbox-exec` probes in the scratchpad); **(D)** read on an official docs page today (URL
given); **(S)** secondary source; **UNVERIFIED** not confirmed. No model calls were made, no
login state or credential was read.

Installed versions (P): `claude` 2.1.318 (`~/.local/bin/claude`), `codex-cli 0.166.0`
(nvm global), `agent` / `cursor-agent` 2026.11.05-86047df (`~/.local/bin`).

Raw help captures: `<scratchpad>/probes/`.

---

## 0. Headline findings

1. **The existing Claude adapter design (`--bare --restricted`, API key behind a proxy) cannot
   run on a subscription.** `--bare` never reads OAuth or the keychain and ignores
   `CLAUDE_CODE_OAUTH_TOKEN` (P: `claude --help`; D: authentication, headless). The
   replacement for `--bare`'s isolation is a combination of `--restricted`, `--safe-mode` or
   `--setting-sources`, `--strict-mcp-config`, `--disable-slash-commands`,
   `CLAUDE_CODE_DISABLE_CLAUDE_MDS=1`, and a harness-owned `CLAUDE_CONFIG_DIR`, plus a
   harness sandbox layer that hides repo agent-config paths. Several of these combinations
   need a live test.
2. **Terms are the go/no-go issue, and they differ by vendor.**
   - Anthropic Consumer Terms (Free/Pro/Max/Ultra) prohibit automated access "through a bot,
     script, or otherwise" except via an API key or where Anthropic "explicitly permit[s] it".
     Claude Code's own docs explicitly document `claude -p` and `claude setup-token` for
     "CI pipelines and scripts" with a subscription, and the legal page permits "an end user
     signing in to the unmodified Claude Code binary with their own Claude subscription".
     They also say plan limits "assume ordinary, individual usage". A personal harness that
     launches the official binary under the user's own login fits the documented use. A
     **published** open-source tool is fine only if each user signs in through Anthropic's
     own flow and the tool never collects, stores or forwards the credential.
   - OpenAI documents `codex exec` reusing the saved ChatGPT login, but recommends API keys
     for automation and warns against public repos. The app-server docs say local and
     open-source apps using app-server auth "can continue using it", and that commercial or
     hosted use was never permitted.
   - Cursor's terms are silent on scripted CLI use. Its docs recommend API keys for scripts.
3. **Credential isolation from the agent's own tools is the hard part on macOS.** All three
   CLIs keep the login in the user keychain or a home-directory file that their own
   sandboxed shell children can probably reach. Each vendor's sandbox covers shell commands
   only, not the CLI process (D for Claude and Codex). The safest pattern is a **read-only
   reviewer with no shell tool at all**. For implementers, the harness relies on the
   vendor's inner sandbox, so a live test that tries to read the credential from inside it
   is a precondition for go-live.
4. **First Codex integration: `codex exec --json`** for both roles. Use `codex app-server`
   (marked "[experimental]" in 0.166.0, P) only when the harness wants to answer approvals
   itself, or wants live rate-limit and usage notifications.
5. **Cursor is the weakest runtime to automate safely today.** It has no structured-output
   flag and no usage data in stream-json. MCP config only comes from fixed paths, and
   approval is all-or-nothing (`--approve-mcps`). Project hooks run once a workspace is
   trusted, and `--trust` is the headless way to trust. Recommend Cursor as a reviewer only
   after a live test, behind harness-side shadowing of `.cursor/`.

---

## 1. Headless invocation, streaming output, success/failure, sessions

### Claude Code 2.1.318

- Invocation: `claude -p --output-format stream-json --verbose` with the prompt on stdin
  (P). `--input-format stream-json` is available for multi-turn input (P).
- Events: the first event is `system/init` (session_id, tools, mcp_servers,
  `mcp_server_errors`, plugins, `plugin_errors`, permissionMode, apiKeySource, capabilities).
  The last line is `result`. Retries are reported as `system/api_retry` with
  `error ∈ {authentication_failed, rate_limit, overloaded, billing_error, …}`. Denials are
  reported as `permission_denied` (D: https://code.claude.com/docs/en/headless). The SDK
  message union includes `SDKRateLimitEvent`; its field definitions were not retrieved (D:
  https://code.claude.com/docs/en/agent-sdk/typescript, fields UNVERIFIED).
- Success: exit 0 and `result.is_error == false`. A failure inside the run prints the error
  as the result on stdout. SIGTERM gives exit 143 (D: headless). Repo evidence (O, 2.1.318):
  an unauthenticated run exits 1 with `subtype:"success"` and `is_error:true`, so check
  `is_error` and the exit code, never `subtype`. The error arm subtypes are
  `error_max_turns | error_during_execution | error_max_budget_usd |
  error_max_structured_output_retries`, with `errors[]` and `terminal_reason` (D: agent-sdk
  typescript).
- Sessions: the harness assigns the ID with `--session-id <uuid>`, continues with
  `--resume <uuid>` or a `.jsonl` path, and can turn persistence off with
  `--no-session-persistence` (P; D: headless). Transcripts live under
  `$CLAUDE_CONFIG_DIR/projects/…` (D: env-vars `CLAUDE_CONFIG_DIR` "All settings, session
  history, and plugins are stored under this path").
- Structured output: `--json-schema` gives `structured_output`; the field is absent if the
  run declined (D: headless).

### Codex CLI 0.166.0

- Invocation: `codex exec --json [PROMPT|-]`; the prompt can come from stdin (P).
  `-o/--output-last-message <file>` and `--output-schema <file>` are available (P).
- Events (JSONL): `thread.started{thread_id}`, `turn.started`, `turn.completed{usage}`,
  `turn.failed{error.message}`, `item.started|updated|completed{item}` and `error{message}`.
  Item types are agent_message, reasoning, command_execution (exit_code, status incl.
  `declined`), file_change, mcp_tool_call, collab_tool_call, web_search, todo_list and error.
  Usage fields are `input_tokens`, `cached_input_tokens`, `cache_write_input_tokens`,
  `output_tokens` and `reasoning_output_tokens` (D: https://learn.chatgpt.com/docs/non-interactive-mode;
  source `codex-rs/exec/src/exec_events.rs` on GitHub main, which may differ slightly from
  0.166.0).
- Success: exit codes are **not documented** (D). A required MCP server that fails to start
  makes exec "exit with an error" (D). Treat success as exit 0, a `turn.completed` event and
  no `turn.failed`/`error` event (UNVERIFIED: the exit code on `turn.failed`).
- Sessions: the ID comes from `thread.started.thread_id`. Continue with
  `codex exec resume <SESSION_ID> [PROMPT|-]` (P). Note (P): `exec resume` and `exec review`
  have **no `--sandbox` or `-C` flag**, so a resumed run must set
  `-c sandbox_mode="…"` explicitly and run with the right cwd. `--ephemeral` turns
  persistence off (P). The harness cannot pre-assign the ID.
- Default exec sandbox is read-only (D: non-interactive-mode). `--full-auto` is gone; use
  `--sandbox workspace-write` (D).

### Cursor CLI 2026.11.05

- Invocation: `agent -p --output-format stream-json [--stream-partial-output]` (P).
- Events: `system/init{apiKeySource,cwd,session_id,model,permissionMode}`, `user`,
  `assistant`, `tool_call{subtype: started|completed, call_id, tool_call}`, and
  `result{subtype:"success", duration_ms, duration_api_ms, is_error:false, result,
  session_id, request_id?}` (D: https://cursor.com/docs/cli/reference/output-format).
- Failure: "exits with a non-zero code and writes an error message to stderr"; "the stream
  may end early without a terminal event". There is no error event type (D). So success
  means exit 0 and a `result` event.
- Sessions: `agent create-chat` "Create a new empty chat and return its ID", then
  `--resume <chatId>` (P). The harness can therefore pre-create the ID.
- File writes: in `-p` mode, "Without --force, changes are only proposed, not applied"
  (D: https://cursor.com/docs/cli/headless). `--help` says `-p` "Has access to all tools,
  including write and shell" (P). The two statements conflict, so whether a shell command
  without `--force` runs is UNVERIFIED.
- No structured-output or schema flag (P).

---

## 2. Subscription auth in headless mode, credential location, keeping it away from tools

### Claude Code

- Supported and documented. `claude setup-token` makes a "one-year OAuth token … for CI
  pipelines, scripts"; it is set as `CLAUDE_CODE_OAUTH_TOKEN`, needs a Pro, Max, Ultra, Team
  or Enterprise plan, "can only make model requests", and is not saved by the command
  (D: https://code.claude.com/docs/en/authentication). In `-p` mode the default login
  (`/login`) also works (D: precedence list item 7).
- Precedence: cloud provider → `ANTHROPIC_AUTH_TOKEN` → `ANTHROPIC_API_KEY` (always used in
  `-p` when present) → `apiKeyHelper` → `CLAUDE_CODE_OAUTH_TOKEN` → Anthropic profile →
  `/login` (D). **The harness must unset `ANTHROPIC_API_KEY` and `ANTHROPIC_AUTH_TOKEN`** or
  the run silently switches to API billing.
- Storage on macOS: the encrypted Keychain. If the Keychain write fails it falls back to
  `~/.claude/.credentials.json` (0600). With `CLAUDE_CONFIG_DIR` set, the Keychain entry is
  **keyed to that directory**, so a different `CLAUDE_CONFIG_DIR` reads a different entry
  (D: authentication). Consequence: a harness-owned `CLAUDE_CONFIG_DIR` will not find the
  user's `/login`; it needs either `CLAUDE_CODE_OAUTH_TOKEN` or a one-time `/login` done by
  the user inside that directory (that login is then stored in its own Keychain entry and
  the harness never sees it).
- `--bare` reads neither OAuth nor the Keychain, and does not read `CLAUDE_CODE_OAUTH_TOKEN`
  (P, D).
- Keeping it away from tools:
  - `CLAUDE_CODE_SUBPROCESS_ENV_SCRUB=1` strips credentials (matched by name or value shape)
    from Bash, hook and stdio-MCP environments; "The Claude Code process keeps the
    credentials". It also removes `CLAUDE_CONFIG_DIR` from children (D: env-vars). Whether
    it removes `CLAUDE_CODE_OAUTH_TOKEN` is UNVERIFIED (likely, because the name contains
    TOKEN).
  - Claude's Bash sandbox (Seatbelt on macOS) "covers shell commands only. Claude's file
    tools, MCP servers, and hooks run outside it". The default read policy covers "most of
    the machine", and "There is no built-in credential deny list" (D:
    https://code.claude.com/docs/en/sandboxing). Use `sandbox.credentials` deny entries,
    `sandbox.failIfUnavailable: true` and `allowUnsandboxedCommands: false`.
  - Keychain risk: if Claude Code writes its Keychain item with `/usr/bin/security`, that
    binary is on the item's ACL, so any same-user child could read it with
    `security find-generic-password`. This is UNVERIFIED and is the most important live
    test for a Bash-enabled role.
  - Read tool: confine it with `--restricted` (file tools limited to the working directories)
    and add `Read(~/.claude/**)` deny rules.
- Nested Seatbelt (P, partial): `sandbox-exec … sandbox-exec … /usr/bin/true` succeeded
  (exit 0) under a permissive outer profile. So a harness Seatbelt around `claude` does not,
  by itself, stop Claude's inner Bash sandbox from starting. The probe ran inside this
  session's own environment, which may itself be sandboxed, so re-test with the real
  profile. A `ps -E` probe for reading a sibling's environment was inconclusive (UNVERIFIED).

### Codex CLI

- Supported and documented with caveats. `codex exec` "reuses saved CLI authentication"
  (D: non-interactive-mode). ChatGPT-managed auth for CI is labelled "(advanced)", and API
  keys are "the right default for automation" (D). The docs say: "Don't expose Codex
  execution in untrusted or public environments"; "Treat `~/.codex/auth.json` like a
  password" (D: https://learn.chatgpt.com/docs/auth).
- Storage: `cli_auth_credentials_store = file | keyring | auto | ephemeral`. `file` means
  `$CODEX_HOME/auth.json` (default `~/.codex`); `keyring` means the OS credential store
  (D: auth). Tokens refresh automatically during use (D). The local default value of the
  setting was not checked (UNVERIFIED).
- Long-lived token: `codex login --with-access-token` / `CODEX_ACCESS_TOKEN`. Access tokens
  are "intended for trusted scripts, schedulers, and private CI runners", but in ChatGPT
  Enterprise workspaces an admin must grant permission first; availability on Plus or Pro
  is UNVERIFIED (D: auth; P: `codex login --help`). Device auth: `--device-auth` (P).
- `--ignore-user-config` skips `$CODEX_HOME/config.toml` while "auth still uses
  `CODEX_HOME`" (P). This is the key flag: use the real login without the user's config.
- Keeping it away from tools: the sandbox "applies to spawned commands" (D:
  https://learn.chatgpt.com/docs/sandboxing). The docs do not say whether a read-only or
  workspace-write command can read `~/.codex/auth.json` or the keyring (D:
  agent-approvals-security, "not covered"). Beta permission profiles support
  `deny` path entries (D: https://learn.chatgpt.com/docs/permissions). A
  `deny ~/.codex/auth.json` profile is a candidate, but whether profiles apply to `exec`
  is UNVERIFIED. Set `shell_environment_policy.inherit = "core"`; by default variables
  containing KEY, SECRET or TOKEN are excluded (D: config-reference).

### Cursor CLI

- Browser login is "recommended"; for "automation, scripts, or CI environments, use API key
  authentication" (`CURSOR_API_KEY` / `--api-key`, a "user API key" from the dashboard)
  (D: https://cursor.com/docs/cli/reference/authentication). Whether headless `-p` works
  with the browser login alone is not stated (UNVERIFIED, likely yes). Whether user API
  keys bill to the plan is not stated (UNVERIFIED).
- Storage: "securely stored locally"; the location is not documented (D). UNVERIFIED.
- Keeping it away from tools: the macOS sandbox is Seatbelt via `sandbox-exec` for "the full
  subprocess tree". `sandbox.readBoundary: workspace` limits reads to the workspace, a Read
  Allowlist and "common system paths" (Cursor 3.23+) (D:
  https://cursor.com/docs/agent/security/run-modes; https://cursor.com/docs/cli/reference/configuration).
  Whether `~/.cursor` or the Keychain is reachable under `readBoundary: workspace` is
  UNVERIFIED.

---

## 3. Terms: automated use of a consumer subscription through the official CLI

| Vendor | What the text says (each quote under 15 words) | Reading |
|---|---|---|
| Anthropic, Consumer Terms §3(7), effective Oct 8 2025 (D: https://www.anthropic.com/legal/consumer-terms) | Prohibits automated access "through a bot, script, or otherwise", except via an API key or "where we otherwise explicitly permit it". | Restricted by default. The exception turns on Anthropic's explicit permission. |
| Claude Code docs (D: https://code.claude.com/docs/en/authentication) | `setup-token` is for "CI pipelines, scripts, or other environments" and needs a subscription. | Anthropic's own docs explicitly document scripted subscription use of the official CLI. Most likely this is the "explicitly permit" case (interpretation, not legal advice). |
| Claude Code legal page (D: https://code.claude.com/docs/en/legal-and-compliance) | OAuth "is designed to support ordinary use of Claude Code". Limits "assume ordinary, individual usage". Developers "may not collect, store, or intermediate Claude.ai credentials". It also permits "an end user signing in to the unmodified Claude Code binary". | **Allowed:** one person driving the unmodified binary with their own login. **Restricted:** a tool that offers claude.ai login, routes others' requests, or stores or forwards tokens. **Volume:** heavy parallel automation may exceed "ordinary, individual" use; nothing defines the line. |
| OpenAI, Codex docs (D: learn.chatgpt.com auth, non-interactive-mode, app-server) | exec "reuses saved CLI authentication"; API keys are the "recommended default for automation"; "Don't expose Codex execution in untrusted or public environments"; local or open-source app-server apps "can continue using it". | Allowed for local personal use of the official CLI. API keys are preferred for automation. Commercial or hosted use is not allowed. |
| OpenAI, consumer Terms of Use | The page **failed to load (HTTP 403)**. Secondary sources (S: conductatlas.com, manifest.build) cite a prohibition on using automated or programmatic means to extract data or Output. An OpenAI executive's X post says ChatGPT accounts work "in a flourishing set of other tools" (S; not contractual). | Unclear for third-party harnesses. Driving the official CLI is the documented path. |
| Cursor ToS, updated Sep 3 2026 (D: https://cursor.com/terms-of-service) | No clause on scripts or CLI. It prohibits harvesting, scraping or extracting data from the Service (§1.5(viii)) and renting, leasing, lending or selling it (§1.5(iii)). The incorporated AUP was not read. | Not addressed. API keys are the documented path for scripts. |

Go/no-go input: for **personal, local use by the subscriber**, all three vendors document or
tolerate driving their official CLI from scripts, with Claude's and OpenAI's own docs saying
so. For the **open-source release**, the harness must keep to these rules:
(a) never prompt for, read, copy or proxy vendor credentials;
(b) launch only the unmodified vendor binaries;
(c) leave login to each user via the vendor's own `login` / `setup-token` flow;
(d) ship guidance that subscription limits assume individual use.
The human should accept or reject the "ordinary, individual usage" ambiguity for Claude.

---

## 4. Auto-loaded project and user files, and how to suppress each

### Claude Code without `--bare`

Under `-p` "folder never trusted", per D (https://code.claude.com/docs/en/permissions,
"What runs before you trust a folder"):

- Project settings-file hooks, the `env` block, `apiKeyHelper`, and a project skill's hooks
  and `allowed-tools`: **Used**.
- `.mcp.json` servers: "Connected without asking, approved or not."
- Project `permissions.allow` and `additionalDirectories`: not used.
- Subagent frontmatter hooks and inline MCP: not used.

CLAUDE.md, skills, commands, agents and plugins are discovered "as an interactive session
would", including `~/.claude` (D: headless).

| Vector | Suppression | Grade |
|---|---|---|
| Project/local settings (hooks, env, helpers) | `--setting-sources user` (project and local not read, `.mcp.json` not read) or `--restricted` (loads only managed + `--settings`) | D (both); interaction UNVERIFIED |
| User settings `~/.claude` (user hooks, plugins, CLAUDE.md) | `CLAUDE_CONFIG_DIR=<harness task home>` so "user" scope is harness-owned; or `--safe-mode` | D |
| `.mcp.json` / other MCP | `--strict-mcp-config --mcp-config <harness file>`; `ENABLE_CLAUDEAI_MCP_SERVERS=false` to stop claude.ai connectors | D |
| Hooks generally | `--settings '{"disableAllHooks":true}'` (user settings alone are not enough because project settings outrank them) | D |
| CLAUDE.md (all scopes) | `CLAUDE_CODE_DISABLE_CLAUDE_MDS=1`; pass harness context with `--append-system-prompt[-file]` | D |
| Skills (project `.claude/skills`, including their hooks) | `--disable-slash-commands` ("Disable all skills and commands"); `--safe-mode` | P/D; whether `--disable-slash-commands` also stops a project skill's hooks is UNVERIFIED |
| Plugins, auto memory, workflows, output styles, LSP | `--safe-mode` (also sets `CLAUDE_CODE_SAFE_MODE=1`, inherited by children); `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1`; `CLAUDE_CODE_DISABLE_WORKFLOWS=1`; `CLAUDE_CODE_DISABLE_POLICY_SKILLS=1`; `CLAUDE_CODE_DISABLE_BUNDLED_SKILLS=1` | D |
| Managed settings | Still apply under `--restricted` and `--safe-mode`. The harness checks that none exist on the machine (or owns them) | D |
| Telemetry / auto-update | `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` (any value, even 0, disables), `DISABLE_AUTOUPDATER=1` | D |

`--safe-mode` keeps "Authentication, model selection, built-in tools, and permissions" working
"normally, which differs from `--bare`" (D: cli-reference). It is the closest
subscription-compatible equivalent of `--bare`. **Open questions (live test):** does
`--safe-mode` still honour `--mcp-config`, `--plugin-dir` (harness skills), `--settings`
hooks and `--agents`? The docs say MCP servers and plugins "do not load" and do not mention
these flags.

**Defence in depth, independent of vendor flags (recommended):** the harness sandbox profile
denies the agent process tree **read access** to repo agent-config paths in the worktree:
`.claude/`, `.mcp.json`, `CLAUDE.md`, `CLAUDE.local.md`, `.codex/`, `.agents/`, `AGENTS.md`,
`AGENTS.override.md`, `.cursor/`, `.cursorrules`. A file the CLI cannot read cannot be loaded.
Trade-off: the agent also cannot read or edit those files. A reviewer of changes to them gets
the diff as prompt text instead. Whether each CLI fails open (skips the unreadable file) or
crashes is UNVERIFIED.

### Codex CLI

- Config layers (highest first): CLI `-c` → project `.codex/config.toml` (**trusted projects
  only**) → `--profile` → user `~/.codex/config.toml` → cloud-managed → `/etc/codex` →
  defaults. Untrusted projects skip "project-local config, hooks, and rules" (D:
  https://learn.chatgpt.com/docs/config-file/config-basic). Trust is
  `projects.<path>.trust_level = "trusted"|"untrusted"` (D: config-reference).
- Suppress: `--ignore-user-config` (P), `--ignore-rules` (P: skips user and project
  execpolicy `.rules`), `-c 'projects."<worktree>".trust_level="untrusted"'`, `--disable hooks`
  (= `features.hooks=false`; D: config-reference), and `--strict-config` so unknown keys
  fail (P).
- `--dangerously-bypass-hook-trust` must never be passed (P: "DANGEROUS").
- AGENTS.md: global `$CODEX_HOME/AGENTS[.override].md`, then repo root down to cwd (D:
  agents-md). It is **not trust-gated** (it is instructions, not code). No documented off
  switch; `project_doc_max_bytes = 0` is UNVERIFIED. Use the sandbox shadowing above if the
  repo's AGENTS.md must be kept out.
- Skills: `.agents/skills` from cwd up to the repo root, `$HOME/.agents/skills`,
  `/etc/codex/skills`, and bundled skills (D: build-skills). Disable with `[[skills.config]]`
  entries (`enabled=false`) and `skills.bundled.enabled=false`. Whether repo skills are
  trust-gated is UNVERIFIED.
- Analytics: `analytics.enabled=false`, `feedback.enabled=false`,
  `otel.metrics_exporter="none"`, `check_for_update_on_startup=false` (D).
- Under workspace-write, `.git`, `.codex` and `.agents` stay read-only (recursive) (D:
  https://learn.chatgpt.com/docs/agent-approvals-security). So the agent cannot plant new
  repo config for a later run, but it also cannot edit those paths when a task needs it.

### Cursor CLI

- Rules: `.cursor/rules/*.mdc` and `AGENTS.md` (root and nested), plus User and Team rules
  (D: https://cursor.com/docs/rules). Whether the CLI loads them and how to disable them
  are not documented.
- Hooks: `<project>/.cursor/hooks.json`, `~/.cursor/hooks.json`, and enterprise and team
  hooks. Project hooks "Require the workspace to be trusted" (D: https://cursor.com/docs/hooks).
  `--trust` "Trust the workspace without prompting (headless mode only)" (D:
  https://cursor.com/docs/cli/reference/parameters). **So `--trust` on an untrusted repo
  runs its hooks.** Without `--trust`, whether headless runs at all is UNVERIFIED.
  No global hook-off switch is documented.
- MCP: `.cursor/mcp.json` and `~/.cursor/mcp.json`, approved via `agent mcp enable <id>` or
  `--approve-mcps` ("Automatically approve all MCP servers") (P; D:
  https://cursor.com/docs/mcp). `--approve-mcps` would approve repo-supplied servers too.
- Skills: `.agents/skills`, `.cursor/skills`, `~/.agents/skills`, `~/.cursor/skills`, and
  for compatibility `.claude/skills`, `.codex/skills`, `~/.claude/skills`, `~/.codex/skills`
  (D: https://cursor.com/docs/skills). Whether the CLI loads them is UNVERIFIED.
- Config: `~/.cursor/cli-config.json` (relocatable with `CURSOR_CONFIG_DIR`). The project
  `.cursor/cli.json` can set only permissions (D: configuration). Whether
  `CURSOR_CONFIG_DIR` also relocates `mcp.json`, `hooks.json`, skills and auth is
  UNVERIFIED.
- `--plugin-dir` exists (P). `--skip-worktree-setup` skips `.cursor/worktrees.json` scripts
  (P). Always pass it if `-w` is ever used.
- Conclusion: for Cursor, harness-side shadowing of `.cursor/`, `AGENTS.md` and
  `.agents/` is **required**, not optional, before `--trust` or `--approve-mcps`.

---

## 5. Tool and permission restriction per role

All roles start from these common env settings. Unset `ANTHROPIC_API_KEY`,
`ANTHROPIC_AUTH_TOKEN`, `OPENAI_API_KEY`, `CODEX_API_KEY` and `CURSOR_API_KEY` (unless the
user chose API-key mode). Pass an allowlisted env (`PATH`, `HOME`, `LANG`, `TMPDIR`, plus
per-CLI settings), run with cwd = the worktree, and run under the harness sandbox (network
limited to the vendor's model and auth hosts; writes limited to the worktree and the
task home).

### Read-only adversarial reviewer

| CLI | Flags | Notes |
|---|---|---|
| Claude | `-p --output-format stream-json --verbose --restricted --tools "Read,Grep,Glob" --permission-mode dontAsk --permission-prompts none --strict-mcp-config --mcp-config <h.json> --disable-slash-commands --setting-sources user --settings <h-settings.json> --json-schema <findings.schema.json> --session-id <uuid> --max-turns N --no-session-persistence` (plus `--safe-mode` if live-tested compatible) | No Bash means no child processes and no shell route to the Keychain. `--restricted` confines Read to the working directories (D). `--tools` doesn't affect MCP tools, so add `--disallowedTools "mcp__*"` except allowlisted read-only MCP tools (D). Note `--tools` names: Glob and Grep are left out of "default" on macOS, so list them explicitly (D). |
| Codex | `exec --json --sandbox read-only -c approval_policy="never" --ignore-user-config --ignore-rules --strict-config --ephemeral --output-schema <findings.schema.json> -o <last.json> -C <worktree> -c 'projects."<wt>".trust_level="untrusted"' -c web_search="disabled" -c shell_environment_policy.inherit="core" --disable hooks -` | `-a/--ask-for-approval` is listed on top-level `codex`, but not in `codex exec --help` (P). Pass `-c approval_policy="never"` instead. The read-only sandbox can still run read-only commands (D). Alternative: `codex exec review --base <sha> --json --output-schema … -c sandbox_mode="read-only"` (P: review has no `--sandbox`) to use OpenAI's tuned review prompt. |
| Cursor | `-p --output-format stream-json --mode ask --sandbox enabled --workspace <wt> [--trust only with shadowing] --model <m>`; no `--force`, no `--approve-mcps`; harness `cli-config.json` with `permissions.deny: ["Write(**)","Shell(*)"]`, `approvalMode: "allowlist"`, `sandbox.readBoundary: "workspace"`, `sandbox.networkAccess` off | `--mode ask` is "read-only" (P). No schema flag, so the harness prompts for a fenced JSON block and validates it itself. A non-schema-valid result means no findings were produced (fail closed). |

### Workspace-write implementer, no network beyond the model

| CLI | Flags | Notes |
|---|---|---|
| Claude | `-p --output-format stream-json --verbose --tools "Read,Edit,Write,Glob,Grep,Bash" --allowedTools "<profile-mapped>" --permission-mode dontAsk --permission-prompts none --strict-mcp-config --mcp-config <h.json> --setting-sources user --settings <h-settings.json> --session-id <uuid> --max-turns N`; settings: `{"disableAllHooks":true,"sandbox":{"enabled":true,"failIfUnavailable":true,"allowUnsandboxedCommands":false,"network":{"allowedDomains":[]},"credentials":{…deny ~/.claude, ~/.codex, ~/.cursor, ~/.ssh…}},"permissions":{"deny":["Read(~/.claude/**)","WebFetch","WebSearch"]}}`; env `CLAUDE_CODE_SUBPROCESS_ENV_SCRUB=1`, `CLAUDE_CODE_DISABLE_CLAUDE_MDS=1`, `DISABLE_AUTOUPDATER=1`, `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`, `ENABLE_CLAUDEAI_MCP_SERVERS=false` | Not `--restricted` (it removes Bash unless named; it is compatible if Bash is named in `--tools`, so prefer **including `--restricted` with `--tools` naming Bash**, which also refuses bypassPermissions and confines file tools; D). Never pass `--dangerously-skip-permissions` or `--allow-dangerously-skip-permissions`. |
| Codex | `exec --json --sandbox workspace-write -c approval_policy="never" -c sandbox_workspace_write.network_access=false --ignore-user-config --ignore-rules --strict-config -C <wt> -c 'projects."<wt>".trust_level="untrusted"' -c web_search="disabled" -c shell_environment_policy.inherit="core" --disable hooks -` | Network is off by default in workspace-write (D). Never pass `--dangerously-bypass-approvals-and-sandbox`, `--approve-for-me` (automatic approval review) or `--add-dir`. |
| Cursor | `-p --force --sandbox enabled --output-format stream-json --workspace <wt> --skip-worktree-setup` with harness `cli-config.json` `approvalMode:"allowlist"`, explicit `permissions.allow` / `permissions.deny`, `sandbox.readBoundary:"workspace"`, network off | `--force` = "Force allow commands unless explicitly denied" (P), so the deny list carries the whole policy. **Not recommended** as an implementer until hooks, trust and credential reachability are live-tested. |

---

## 6. One harness-owned MCP + skills definition for all three

Canonical harness record (per server): `id`, `transport: stdio|http`, `command`, `args`,
`env` (names only; values resolved by the harness at launch), `url`, `headers`,
`tools_allow[]`, `required: bool`.

| CLI | MCP rendering | Per-run, harness-owned path? | Grade |
|---|---|---|---|
| Claude | JSON `{"mcpServers":{id:{command,args,env}│{type:"http",url,headers}}}` → `--mcp-config <file> --strict-mcp-config`; tools gated with `--allowedTools mcp__<id>__<tool>`; check `init.mcp_server_errors` (fail if non-empty) | Yes | D |
| Codex | `-c mcp_servers.<id>.command="…" -c mcp_servers.<id>.args=[…] -c mcp_servers.<id>.env={…} -c mcp_servers.<id>.enabled_tools=[…] -c mcp_servers.<id>.required=true` (or `.url`, `bearer_token_env_var`) | Yes via `-c` (no file needed); `required` makes exec fail on startup error | P (`-c`), D (keys) |
| Cursor | `mcpServers` JSON in `.cursor/mcp.json` or `~/.cursor/mcp.json` only; `${env:NAME}` interpolation; approval via `agent mcp enable <id>` | Only by relocating home (`HOME` or perhaps `CURSOR_CONFIG_DIR`) to a harness task home; UNVERIFIED | D/UNVERIFIED |

Skills: all three use the agentskills.io `SKILL.md` format (name + description frontmatter)
(D: Codex build-skills and Cursor skills both cite it; Claude skills docs not re-read today).
Delivery from one harness directory `skills/<name>/SKILL.md`:

- Claude: package it as a plugin directory with `skills/` and pass `--plugin-dir <dir>` (P).
  This conflicts with `--disable-slash-commands`, which disables all skills, and possibly
  with `--safe-mode`. Live test: does `--plugin-dir` still load skills with
  `--setting-sources user` while project `.claude/skills` is shadowed? Fallback: inline the
  skill text with `--append-system-prompt-file`.
- Codex: no `--skills-dir` flag. Options: set `HOME` for the codex process to a harness task
  home containing `.agents/skills` (with `CODEX_HOME` still pointing at the real
  `~/.codex`), or `[[skills.config]] path=…` (whether it adds undiscovered skills is
  UNVERIFIED), or `codex plugin` marketplaces. Relocating `HOME` may break Keychain lookup
  (UNVERIFIED).
- Cursor: `~/.agents/skills` under a relocated home, or `--plugin-dir <dir>` (P; plugin
  layout UNVERIFIED).

Recommendation: the harness keeps one canonical `mcp.toml` and `skills/` and renders per-CLI
artifacts into the per-task home at launch. It hashes the rendered artifacts into the run
record. MCP servers are always started by the CLI as its children, so they must also
run in the harness sandbox with a scrubbed env.

---

## 7. Usage and limit exhaustion

| CLI | Per-run usage | Limit exhaustion signal | Grade |
|---|---|---|---|
| Claude | `result.usage`, `result.modelUsage` (per-model tokens), `total_cost_usd` (an estimate; notional under a subscription), `num_turns`, `duration_ms` | `system/api_retry{error:"rate_limit", error_status:429, retry_delay_ms}`; `SDKRateLimitEvent` exists (fields unread); the final `result.is_error` with `terminal_reason:"api_error"`; exit non-zero. Without `CLAUDE_CODE_RETRY_WATCHDOG` it gives up after `CLAUDE_CODE_MAX_RETRIES` (default 10). With the watchdog it waits out "session or weekly limit" windows. **For a harness that wants to fail over, do NOT set the watchdog; set `CLAUDE_CODE_MAX_RETRIES` low and treat `api_retry.error=="rate_limit"` as a pause/failover signal.** | D (headless, env-vars, agent-sdk) |
| Codex (exec) | `turn.completed.usage` tokens only | `turn.failed{error.message}` or `error{message}`, message text only, no structured limit fields (D: source). Classify by message text (fragile) | D |
| Codex (app-server) | `thread/tokenUsage/updated` | `account/rateLimits/updated` notification and `account/rateLimits/read` (ChatGPT rate limits); payload not documented, so read it from the generated schema | D (fields UNVERIFIED) |
| Cursor | `duration_ms`, `duration_api_ms` only; no tokens | Non-zero exit + stderr; the stream may end without `result` | D |

Harness rule: the adapter maps each signal to one `RunOutcome::LimitExhausted{reset_at:
Option<Time>}`. Any unclassified non-zero exit is `Failed`. The router fails over only on
`LimitExhausted`, never on `Failed`.

---

## 8. Codex: `exec --json` versus `app-server`

Installed 0.166.0 (P):

- `codex exec` has `--json`, `--output-schema`, `-o`, `--ephemeral`, `--ignore-user-config`,
  `--ignore-rules` and `--strict-config`, plus `resume`, `fork` and `review` subcommands.
  It is not marked experimental.
- `codex app-server` is marked "[experimental]". It offers `--listen stdio://` (the
  default), `unix://` and `ws://`, `generate-json-schema --out <dir> [--experimental]`,
  `generate-ts`, and a `daemon` subcommand. The schema was **not generated** in this track
  (help-only rule); generate it in the next step and pin it per version.
- app-server protocol (D: https://learn.chatgpt.com/docs/app-server): JSON-RPC 2.0 without
  the `jsonrpc` field, JSONL on stdio. `initialize` / `initialized`; `thread/start{model,
  cwd, approvalPolicy, sandbox}`; `turn/start{threadId, input, sandboxPolicy,
  outputSchema,…}`; `thread/resume`; `review/start{target: uncommittedChanges|baseBranch|
  commit|custom}`. Server→client approval requests are
  `item/commandExecution/requestApproval` and `item/fileChange/requestApproval`, with
  decisions `accept|acceptForSession|decline|cancel`. Notifications include `turn/completed`,
  `item/*`, `thread/tokenUsage/updated` and `account/rateLimits/updated`. The
  `chatgptAuthTokens` login is experimental and would make the harness handle tokens, so do
  not use it.

Recommendation:

- **Reviewer:** `codex exec --json --sandbox read-only --output-schema` now. It needs no
  approvals, is stable, and gives schema-valid findings.
- **Implementer:** start with `codex exec --sandbox workspace-write` and
  `approval_policy=never`. The sandbox is the boundary; denied commands fail back to the
  model. Move to `app-server` (behind the same `RuntimeAdapter`) when the harness wants to
  (a) answer approval requests from its own policy engine, (b) get live
  `account/rateLimits` data for routing, or (c) keep one long-lived process for the repair
  loop. Pin the generated JSON Schema per Codex version and fail on drift, because the
  surface is experimental.

---

## 9. Recommended Rust adapter surface

```rust
/// One per CLI. Builds a launch plan; the core spawns it inside the Sandbox and feeds
/// stdout lines back to `parse_line`. Adapters never spawn processes or touch credentials.
pub trait RuntimeAdapter: Send + Sync {
    fn id(&self) -> RuntimeId;                       // Claude | Codex | Cursor
    fn capabilities(&self) -> &'static Capabilities;
    /// Validate installed version against pin; returns parsed version or refusal.
    fn probe(&self, version_output: &str) -> Result<RuntimeVersion, AdapterError>;
    /// Pure: role + task + rendered harness artifacts -> argv/env/stdin/sandbox needs.
    fn plan(&self, req: &RunRequest) -> Result<LaunchPlan, AdapterError>;
    /// Pure, incremental: one stdout line -> zero or more normalized events.
    fn parse_line(&mut self, line: &str) -> Vec<RuntimeEvent>;
    /// After exit: combine exit status + seen events into the run's claim.
    fn finish(&mut self, exit: ExitStatus, stderr_tail: &str) -> RunOutcome;
}

pub struct RunRequest {
    pub role: Role,                    // Reviewer | Implementer
    pub workdir: PathBuf,              // worktree (implementer) or read-only checkout
    pub task_home: PathBuf,            // harness-owned, per task; config dir lives here
    pub prompt: PromptSource,          // stdin text, or file
    pub session: SessionMode,          // New{assigned: Option<Uuid>} | Resume{id: String}
    pub mcp: Vec<RenderedMcp>,         // from the one harness definition
    pub skills_dir: Option<PathBuf>,
    pub output_schema: Option<PathBuf>,// findings schema for reviewers
    pub model: Option<String>,
    pub limits: RunLimits,             // max_turns, wall_clock
}

pub struct LaunchPlan {
    pub program: PathBuf,              // resolved absolute path, version-pinned
    pub argv: Vec<OsString>,
    pub env: BTreeMap<OsString, OsString>,   // complete env; core starts from empty
    pub stdin: Option<Vec<u8>>,
    pub sandbox: SandboxNeeds,         // read/write roots, denied paths, net hosts
    pub files_to_render: Vec<(PathBuf, Vec<u8>)>, // into task_home only
}

pub enum RuntimeEvent { Init{session_id, tools, mcp_ok, mcp_errors}, Text(String),
    ToolCall{..}, ToolResult{..}, PermissionDenied{..}, Retry{kind: RetryKind, delay_ms},
    Usage(UsageDelta), Final{text, structured: Option<serde_json::Value>}, Error(String) }

pub enum RunOutcome { Succeeded{session_id, final_: FinalOutput, usage: Usage},
    LimitExhausted{reset_at: Option<Timestamp>}, Failed{reason}, Cancelled }
```

Static capability description (one `const` per adapter, serialisable):

| Field | Claude | Codex (exec) | Cursor |
|---|---|---|---|
| `assigns_session_id` | yes (`--session-id`) | no (read `thread.started`) | yes (`create-chat`) |
| `structured_output` | `--json-schema` | `--output-schema` | none (harness parses) |
| `read_only_mode` | tools allowlist (no Bash) | `--sandbox read-only` | `--mode ask` |
| `inner_sandbox` | Seatbelt, Bash only | Seatbelt, spawned commands | Seatbelt, subprocess tree |
| `per_run_mcp` | file flag + strict | `-c` overrides | fixed paths (home relocation) |
| `per_run_skills` | `--plugin-dir` | none (home relocation) | `--plugin-dir`/home (UNVERIFIED) |
| `usage_tokens` | yes | yes | no |
| `limit_signal` | `api_retry{rate_limit}` | message text only | exit code only |
| `repo_config_trust_gate` | none in `-p` (flags needed) | project `.codex` trust-gated | `--trust` enables hooks |
| `subscription_headless` | documented | documented, API key preferred | API key recommended |
| `stability` | stable | stable (exec) / experimental (app-server) | stable |

### Exact launch plans (one task; `$WT` = worktree, `$TH` = task home)

**Claude reviewer**

- argv: `claude -p --output-format stream-json --verbose --restricted --tools Read,Grep,Glob
  --disallowedTools 'mcp__*' --allowedTools <mcp read tools> --permission-mode dontAsk
  --permission-prompts none --setting-sources user --settings $TH/claude/settings.json
  --strict-mcp-config --mcp-config $TH/claude/mcp.json --disable-slash-commands
  --json-schema "$(cat findings.schema.json)" --session-id <uuid> --max-turns 40
  --append-system-prompt-file $TH/prompt/system.md`
- stdin: the prompt.
- env: `HOME`, `PATH`, `LANG`, `CLAUDE_CODE_DISABLE_CLAUDE_MDS=1`,
  `CLAUDE_CODE_SUBPROCESS_ENV_SCRUB=1`, `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1`,
  `DISABLE_AUTOUPDATER=1`, `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`,
  `ENABLE_CLAUDEAI_MCP_SERVERS=false`, `CLAUDE_CODE_MAX_RETRIES=3`.
- Credential option K: keep the default `CLAUDE_CONFIG_DIR` and Keychain login (the harness
  never sees the token; user-scope settings are the user's own, which is why
  `--setting-sources user` is not enough here and `--safe-mode` must be live-tested).
- Credential option T: `CLAUDE_CONFIG_DIR=$TH/claude-home` plus `CLAUDE_CODE_OAUTH_TOKEN`
  from a token the user minted with `setup-token` (the harness then holds the token, which
  contradicts the brief decision).
- Credential option L (recommended to test first): `CLAUDE_CONFIG_DIR=<harness-owned
  persistent dir>` where the **user runs `claude /login` once themselves**. The login is
  stored in a Keychain entry keyed to that dir (D), the harness never reads it, and
  user-scope config is harness-owned and deterministic.

**Claude implementer**: same base, minus `--json-schema` and `--disable-slash-commands`
(only if harness skills via `--plugin-dir` are wanted), with
`--tools Read,Edit,Write,Glob,Grep,Bash`, profile-mapped `--allowedTools`, and the
sandboxed settings in §5.

**Codex reviewer**: `codex exec --json --sandbox read-only -c approval_policy="never"
--ignore-user-config --ignore-rules --strict-config --ephemeral -C $WT
-c 'projects."$WT".trust_level="untrusted"' -c web_search="disabled"
-c shell_environment_policy.inherit="core" -c analytics.enabled=false
-c check_for_update_on_startup=false --disable hooks -c mcp_servers.<id>.…
--output-schema findings.schema.json -o $TH/codex/last.json -`, with env `HOME`, `PATH`,
`LANG`, `CODEX_HOME` unset (default `~/.codex`, auth only, because of
`--ignore-user-config`).

**Codex implementer**: as above with `--sandbox workspace-write
-c sandbox_workspace_write.network_access=false`, without `--ephemeral` (resume needs
the rollout). Resume: `codex exec resume <thread_id> --json -c sandbox_mode="workspace-write"
<same -c set> -` with cwd = `$WT`.

**Cursor reviewer**: `agent -p --output-format stream-json --mode ask --sandbox enabled
--workspace $WT --resume <chatId from create-chat> --model <m>`, with env
`CURSOR_CONFIG_DIR=$TH/cursor` (cli-config with deny-all-writes, `readBoundary:
workspace`), `HOME`, `PATH`. No `--force`, `--approve-mcps` or `--trust` until shadowing is
verified.

**Cursor implementer**: deferred (see §5).

### Facts that still need a live, authenticated test

1. Claude: `--safe-mode` with `--mcp-config`, `--plugin-dir`, `--settings` and `--agents`.
   Do they load? Does auth work in `-p` under a subscription?
2. Claude: `--restricted` plus `--setting-sources user` plus `--disable-slash-commands`.
   Do project `.claude/skills` hooks and `allowed-tools`, project `.claude/agents` and
   `CLAUDE.md` all stay out? Plant each in a scratch repo and check `init` plus the
   transcript.
3. Claude: option L. Does `claude /login` under a custom `CLAUDE_CONFIG_DIR` create a
   separate Keychain entry that `-p` then uses (check `init.apiKeySource`)?
4. Claude: from inside Claude's Bash sandbox (implementer settings), can a command (a) see
   `CLAUDE_CODE_OAUTH_TOKEN`, (b) read the parent's env (`ps -wwE -p $PPID`), (c) query the
   Keychain item's **existence** (`security find-generic-password -s …` without `-w`)?
   Any "yes" blocks option T or K for implementers.
5. Claude: the exact `SDKRateLimitEvent` / `rate_limit_event` JSON, and `api_retry` and the
   exit code when the subscription usage limit is hit.
6. Claude: does the 2.1.318 binary run inside a harness Seatbelt profile that denies reads of
   `$WT/.claude`, `$WT/.mcp.json` and `$WT/CLAUDE.md` (skip or crash)?
7. Codex: exit code on `turn.failed`, the usage-limit error message text, and whether
   `exec` refuses or prompts in an untrusted dir.
8. Codex: can a `read-only` or `workspace-write` command read `~/.codex/auth.json` or the
   keyring? Does a beta permission profile with `deny` work under `exec`
   (`-c default_permissions=…`)?
9. Codex: `cli_auth_credentials_store` effective value on this Mac (via `codex doctor --json`,
   redacted); whether `HOME` relocation breaks keyring auth; whether repo `.agents/skills`
   loads in an untrusted project.
10. Codex: run `codex app-server generate-json-schema --out …` and record the approval
    response and rate-limit payload shapes for 0.166.0.
11. Cursor: does `-p` work on the browser login without `CURSOR_API_KEY`? Where is the
    credential stored? Does `-p` without `--trust` run in an untrusted repo? Do project
    hooks, rules, skills and `.cursor/mcp.json` load in CLI headless with and without
    `--trust`? Does `CURSOR_CONFIG_DIR` relocate `mcp.json`, auth and skills? Is a shell
    command without `--force` executed or only proposed?
12. All: which network hosts each CLI needs under a subscription (model, OAuth refresh,
    feature flags), so the harness egress allowlist can be exact.

---

## 10. Questions only the human can answer

1. **Credential handling:** accept option L (the user logs each CLI into a harness-owned
   config dir; the harness never reads tokens), or allow option T (the harness injects a
   `setup-token` / access token, which breaks the "never handles tokens" decision but gives
   cleaner isolation)?
2. **Terms risk:** proceed on the reading that personal scripted use of the official
   binaries is permitted (Claude docs, Codex docs), given Anthropic's "ordinary, individual
   usage" caveat on plan limits? For the open-source release, is "each user logs in
   themselves via the vendor's flow; the harness never touches credentials" an acceptable
   policy?
3. **Cursor:** is a Cursor reviewer required in v1, given the hook, trust and MCP-approval
   gaps? Or Codex-only reviews first?
4. **Repo config visibility:** accept that agents cannot read or edit `.claude/`, `.cursor/`,
   `.codex/`, `.agents/`, `AGENTS.md` and `CLAUDE.md` in the worktree (shadowed), with
   changes to those files done only by the human?
5. **Failover policy:** when a subscription limit is hit, pause until reset, or fail over to
   another runtime (different model family, which may weaken the "reviewer is a different
   family" rule)?
