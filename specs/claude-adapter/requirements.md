# Requirements: claude-adapter — Claude Code CLI runtime adapter

> Status: proposed, not approved. First runtime adapter on `specs/adapter-interface/`
> (T2 decision D5). Claude facts are graded: **(P)** read from the installed Claude Code CLI's
> own `claude --help`, version 2.1.318, on 2026-11-05; **(D)** read on code.claude.com docs
> pages on 2026-11-05 (headless, cli-reference, settings, setup, network-config, sessions,
> sub-agents, data-usage); **(O)** observed by running 2.1.318 in this session (see
> handoff.md "T1 results"); **(R)** from this repository's researched notes
> (`docs/security/data-egress.md`); **UNVERIFIED** not yet confirmed. README "Prerequisites" pins 2.1.310; the pin is a T3 decision.

## User-visible behavior

- A task can name `claude` as its runtime; the harness runs Claude Code headless inside the
  adapter-interface sandbox and records its output as runtime claims (C-1, C-9).
- A Claude run reaches only the model API, never other hosts, and never the ledger,
  evidence store or operator credentials (C-6, C-7).

## Non-goals

- Interactive, background (`--bg`), cloud (`--cloud`), Remote Control or desktop sessions.
- Using Claude as the approval or review authority; review output is evidence only under
  adapter-interface A-17.
- Third-party model providers (Bedrock, Vertex, Foundry) until a separate decision.

## Invariants

- Everything in adapter-interface (A-1 to A-22) and its T2 decisions applies; this packet
  adds only what is specific to Claude Code.
- Claude output, repository files and retrieved text are data, never instructions (AGENTS.md).

## Requirements

One normative sentence per row; evidence is under "Claude evidence by requirement".

| ID | Requirement | Source |
|---|---|---|
| C-1 | The adapter runs `claude -p` with `--output-format stream-json --verbose`, the task prompt on stdin, and the working directory set to the sandboxed worktree. | adapter-interface A-1, A-2 |
| C-2 | The adapter starts Claude with `--bare --restricted --disable-slash-commands` and no `--add-dir`, so hooks, plugins, skills, CLAUDE.md auto-discovery and user, project and local settings files are not loaded, passes all configuration explicitly with `--settings` and `--append-system-prompt`, and controls the managed-settings path inside the sandbox so only the harness's managed settings (if any) exist there. | adapter-interface A-12, A-16; README "Trust boundaries" |
| C-3 | The adapter passes an explicit `--tools` and `--allowedTools` list mapped from the task's permission profile, `--permission-mode dontAsk` and `--permission-prompts none`, and never passes `--dangerously-skip-permissions`, `--allow-dangerously-skip-permissions` or `--permission-mode bypassPermissions`. | adapter-interface A-3; README "Required controls" |
| C-4 | MCP servers come only from the harness profile via `--mcp-config` with `--strict-mcp-config`. | adapter-interface A-18; data-egress.md "Enforcement" |
| C-5 | The adapter assigns the session ID with `--session-id <uuid>` on the first run (this is the runtime session ID adapter-interface A-7 records), checks that the `init` event's `session_id` equals it, keeps a harness-owned copy and SHA-256 of the transcript after each run (hash in the run ledger), and before a `--resume` verifies that hash and rebuilds the task home from the harness copy, refusing on mismatch; `--fork-session`, `--continue` and `--no-session-persistence` are not used. | adapter-interface A-7 |
| C-6 | Inside the sandbox, Claude reaches only the harness model proxy (C-7) through an explicit route the sandbox provides; direct connections, DNS to other hosts and `NO_PROXY` bypasses from any tool fail. | adapter-interface A-18; data-egress.md "Egress classes" |
| C-7 | The real API key never enters the sandbox: Claude gets a dummy key and `ANTHROPIC_BASE_URL` pointing at a harness-run model proxy that strips client credentials, adds the real key for the approved Console organization, and forwards only model API paths; `CLAUDE_CODE_SUBPROCESS_ENV_SCRUB=1` is also set. | adapter-interface A-8, A-6; Claude Code docs (env-vars: `ANTHROPIC_BASE_URL`, `CLAUDE_CODE_SUBPROCESS_ENV_SCRUB`) |
| C-8 | The harness enforces a per-task total spend budget across all runs and continuations, metered at the model proxy (C-7) with a hard cutoff, and caps the number of continuations; `--max-budget-usd` and `--max-turns` are passed per run as advisory limits only, and a run stopped by any limit is recorded as failed. | adapter-interface A-4, A-14; Claude Code docs (cli-reference `--max-budget-usd`: client-side estimate, can be exceeded, resets on resume) |
| C-9 | Stream-json output is parsed by the adapter, a run counts as successful only when the exit code is 0 and the final `result` event has `is_error: false`, and the output is validated against the harness result schema (optionally enforced with `--json-schema`), stored as runtime claims, and never cited as evidence except as A-17 allows. | adapter-interface A-5, A-6, A-17 |
| C-10 | The adapter checks `claude --version` against the pinned version before every run and refuses on mismatch, and starts Claude with `DISABLE_AUTOUPDATER=1` and `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`. | adapter-interface A-9, A-20 |
| C-11 | Claude's subagents (`--agents`, Task tool) are disabled unless the task's profile allows delegation, in which case adapter-interface A-14's depth cap is enforced. | adapter-interface A-14 |
| C-12 | A Claude session is launched only for content whose egress class the proxy's configured key is approved for: each BAA allowlist entry names the Console organization and key ID (zero data retention for `baa-covered-providers`), and the proxy refuses any other key or organization. | data-egress.md "BAA configuration allowlist"; adapter-interface A-19 |

## Claude evidence by requirement

| ID | Evidence |
|---|---|
| C-1 | (P) `-p` prints and exits; 2.1.318 refuses `-p --output-format stream-json` without `--verbose`. (D) Exit code 0 on success, non-zero when the run fails; invalid flags error to stderr before the run; a failure inside the run is printed as the result on stdout; SIGTERM exits 143 and leaves the turn unrecorded. (O) An unauthenticated run exits 1 and its final `result` event has `subtype: "success"` with `is_error: true`, so the adapter must check `is_error` and the exit code, not `subtype`. (O) The first event is `system`/`init` (fields include `session_id`, `tools`, `mcp_servers`, `permissionMode`, `apiKeySource`, `claude_code_version`); (D) the last line is a `result` event with the final text, cost and session metadata; permission denials appear as `permission_denied` events and in `result.permission_denials`. (P) In `-p` mode settings files that fail validation are silently ignored, so the adapter validates its own `--settings` file before launch. |
| C-2 | (O) Tools at init with no `--tools`: `--bare` alone gives Bash, Edit, Read; `--restricted` alone gives 19 tools including Task (subagents), WebSearch, Skill and SendMessage; `--bare --restricted` gives Edit and Read only. Both flags are needed. (D) Without `--bare`, a `-p` session runs a project's `.claude/settings.json` hooks and `.mcp.json` servers even in an untrusted folder, with no trust dialog. (O) Without `--bare`, a run under a scrubbed environment and empty `HOME` still found credentials from the host and made a billed model call (see handoff), so `--bare` is also what confines auth to `ANTHROPIC_API_KEY` (C-7). (D) Under `--bare`, skills in an `--add-dir` directory still load and hooks passed with `--settings` do not run; `CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD=1` would load CLAUDE.md from added directories. (P) Managed settings still apply under `--restricted`. (D) `--restricted` needs v2.1.248+; it loads only managed settings and `--settings`, confines file tools to working directories and refuses `bypassPermissions`. |
| C-3 | (P) `--tools`, `--allowedTools`, `--disallowedTools`, `--permission-mode` (acceptEdits, auto, bypassPermissions, manual, dontAsk, plan), `--permission-prompts none` ("anything that would prompt is denied automatically"). |
| C-4 | (P) `--mcp-config`, `--strict-mcp-config` ("only use MCP servers from --mcp-config"). |
| C-5 | (P) `--session-id <uuid>`, `--resume`, `--fork-session`, `--no-session-persistence`. (D) Transcripts are stored at `~/.claude/projects/<project>/<session-id>.jsonl` (internal format, changes between versions); `CLAUDE_CONFIG_DIR` relocates the home-directory files, so the adapter points it at the task home (adapter-interface D1). `--resume` also accepts a transcript path. |
| C-6 | (D) Required hosts: `api.anthropic.com` (model API, WebFetch domain safety check, feature flags, telemetry); `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` disables the optional Datadog telemetry hosts; auth and installer hosts (`claude.ai`, `platform.claude.com`, `downloads.claude.ai`) are not needed with API-key auth and auto-update off. (D) Claude Code honours `HTTPS_PROXY`, so the sandbox egress proxy can be passed that way. The `harness/local` sandbox has no network (`--unshare-all`); the proxy itself is new design work. |
| C-7 | (P) `--bare` auth is `ANTHROPIC_API_KEY` or `apiKeyHelper` only. (D) `ANTHROPIC_BASE_URL` routes API requests through a proxy; `CLAUDE_CODE_SUBPROCESS_ENV_SCRUB=1` strips credentials from Bash, hook and MCP subprocess environments but the Claude process keeps them, so a key in the sandbox stays readable (e.g. via `/proc`); hence the dummy-key proxy design. |
| C-8 | (D) `--max-budget-usd` checks a client-side estimate that can differ from the bill, spend can pass the cap, and totals restored on `--resume` behave differently, so it cannot bound task cost. (D) `--max-turns` limits agentic turns in print mode (no limit by default); 2.1.318 accepts it although `--help` does not list it (O). (P) `--max-budget-usd` (print only). |
| C-9 | (P) `--json-schema` validates structured output (print only). |
| C-10 | (P) `-v/--version`. (D) Auto-update is disabled with `DISABLE_AUTOUPDATER=1` (environment or `settings.json` `env`); `claude doctor` reports it. |
| C-11 | (O) With `--bare --restricted` and no `--tools`, no Task/Agent tool is present, so subagents are off. (D) The Task tool was renamed Agent in 2.1.63 (`Task(...)` still works); spawnable types can be limited with `Agent(<type>)` in `tools`. |
| C-12 | (R) data-egress.md: Claude Code is BAA-covered only with zero data retention for the organization; surfaces to confirm with Anthropic. |

## Cited company decisions

| Constraint | Source (repo commit `30fc4e4`) |
|---|---|
| Adapter contract and T2 decisions | `specs/adapter-interface/requirements.md` |
| Egress classes and BAA allowlist | `docs/security/data-egress.md` |
| Approvals only in the external ledger | `docs/architecture/approval-ledger.md` |

## Unresolved questions

- [ ] Egress route: how the bubblewrap network namespace (loopback only under `--unshare-all`) reaches the harness model proxy (e.g. a bound Unix socket bridged inside the sandbox) (C-6, C-7). — owner: TBD, blocks: T4
- [ ] data-egress.md assumes fail-closed read hooks (layer 4) and `forceLoginOrgUUID` (layer 3); neither applies under `--bare` with API-key auth, and layer 1 assumes a separate macOS user while adapter-interface D1 is Linux bubblewrap. A reviewed amendment to data-egress.md naming the bubblewrap and proxy equivalents is needed before any `baa-covered-providers` use (C-12). — owner: human, blocks: T3
- [ ] Which Anthropic Console organization and key the proxy uses, whether it has a BAA and zero data retention, and the key's own spend limit (C-7, C-8, C-12). — owner: human, blocks: T3
- [ ] Pin 2.1.310 (README) or 2.1.318 (installed) (C-10). — owner: human, blocks: T3
- [ ] Live checks that need an API key (T5): transcripts under `CLAUDE_CONFIG_DIR`, `--resume` from a rebuilt task home, worktree and task-home config not loading. — owner: TBD, blocks: none for T4
