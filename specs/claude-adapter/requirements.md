# Requirements: claude-adapter — Claude Code CLI runtime adapter

> Status: proposed, not approved. First runtime adapter on `specs/adapter-interface/`
> (T2 decision D5). Claude facts are graded: **(P)** read from the installed Claude Code CLI's
> own `claude --help`, version 2.1.318, on 2026-11-05; **(R)** from this repository's
> researched notes (`docs/security/data-egress.md`, Claude Code 2.1.310); **UNVERIFIED** not
> yet confirmed. README "Prerequisites" pins 2.1.310; the pin is a T3 decision.

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
| C-1 | The adapter runs `claude -p` with `--output-format stream-json`, the task prompt on stdin, and the working directory set to the sandboxed worktree. | adapter-interface A-1, A-2 |
| C-2 | The adapter starts Claude with `--bare` and `--restricted`, so hooks, plugins, CLAUDE.md auto-discovery and user, project and local settings files are not loaded, and passes all configuration explicitly with `--settings`, `--append-system-prompt-file` or `--add-dir`. | adapter-interface A-12, A-16; README "Trust boundaries" |
| C-3 | The adapter passes an explicit `--tools` and `--allowedTools` list mapped from the task's permission profile, `--permission-mode dontAsk` and `--permission-prompts none`, and never passes `--dangerously-skip-permissions`, `--allow-dangerously-skip-permissions` or `--permission-mode bypassPermissions`. | adapter-interface A-3; README "Required controls" |
| C-4 | MCP servers come only from the harness profile via `--mcp-config` with `--strict-mcp-config`. | adapter-interface A-18; data-egress.md "Enforcement" |
| C-5 | The adapter assigns the session ID with `--session-id <uuid>` on the first run, records it as adapter-interface A-7 requires, and continues only with `--resume <recorded id>`; `--fork-session`, `--continue` and `--no-session-persistence` are not used. | adapter-interface A-7 |
| C-6 | Inside the sandbox, Claude can reach only the configured model API host, through a harness-run egress proxy; every other network destination is denied. | adapter-interface A-18; data-egress.md "Egress classes" |
| C-7 | The API key reaches Claude only as `ANTHROPIC_API_KEY` in the allowlisted sandbox environment (or `apiKeyHelper` in `--settings`), never written into the worktree, task home, artifacts or logs. | adapter-interface A-8, A-6 |
| C-8 | The adapter passes `--max-budget-usd` and `--max-turns`-equivalent limits from the task's budget, and a run that hits a limit is recorded as failed. | README "Required controls" (deadlines, retry limits); adapter-interface A-4 |
| C-9 | Stream-json output is parsed by the adapter, validated against the harness result schema (optionally enforced with `--json-schema`), stored as runtime claims, and never cited as evidence except as A-17 allows. | adapter-interface A-5, A-6, A-17 |
| C-10 | The adapter checks `claude --version` against the pinned version before every run and refuses on mismatch, and Claude's auto-update is disabled in the sandbox. | adapter-interface A-9, A-20 |
| C-11 | Claude's subagents (`--agents`, Task tool) are disabled unless the task's profile allows delegation, in which case adapter-interface A-14's depth cap is enforced. | adapter-interface A-14 |
| C-12 | A Claude session is launched only for content whose egress class the Claude configuration in use is approved for (BAA allowlist entry with zero data retention for `baa-covered-providers`). | data-egress.md "BAA configuration allowlist"; adapter-interface A-19 |

## Claude evidence by requirement

| ID | Evidence |
|---|---|
| C-1 | (P) `-p/--print` prints and exits; `--output-format text|json|stream-json` (print only); `--input-format stream-json` exists. (P) In `-p` mode "the workspace trust dialog is skipped" and "settings files that fail validation are silently ignored": the adapter must validate its own `--settings` file before launch. Exit codes and the stream-json event schema: UNVERIFIED. |
| C-2 | (P) `--bare`: skips hooks, LSP, plugin sync, auto-memory, keychain reads and CLAUDE.md auto-discovery; auth is strictly `ANTHROPIC_API_KEY` or `apiKeyHelper` via `--settings`. (P) `--restricted`: removes Bash and other code-running tools and WebFetch unless `--tools` names them, ignores user/project/local settings (managed settings and `--settings` still apply), confines file tools to working directories, refuses `bypassPermissions`. How `--bare` and `--restricted` combine: UNVERIFIED. |
| C-3 | (P) `--tools`, `--allowedTools`, `--disallowedTools`, `--permission-mode` (acceptEdits, auto, bypassPermissions, manual, dontAsk, plan), `--permission-prompts none` ("anything that would prompt is denied automatically"). |
| C-4 | (P) `--mcp-config`, `--strict-mcp-config` ("only use MCP servers from --mcp-config"). |
| C-5 | (P) `--session-id <uuid>`, `-r/--resume <id>`, `--fork-session`, `--no-session-persistence`. Where session state is stored (expected under the sandbox home, `~/.claude/projects`): UNVERIFIED. |
| C-6 | Claude needs network for the model API, while the `harness/local` sandbox uses `--unshare-all` (no network). The egress proxy design is new to this packet; the API host list is UNVERIFIED. |
| C-7 | (P) `--bare` auth is `ANTHROPIC_API_KEY` or `apiKeyHelper` only. |
| C-8 | (P) `--max-budget-usd` (print only). A max-turns flag is not listed in 2.1.318 `--help`: UNVERIFIED. |
| C-9 | (P) `--json-schema` validates structured output (print only). |
| C-10 | (P) `-v/--version`; `claude install <version>` and `update`. Auto-update disable setting: UNVERIFIED. |
| C-11 | (P) `--agents`, `--agent`, `--forward-subagent-text`. How to disable the subagent tool under `--tools`: UNVERIFIED. |
| C-12 | (R) data-egress.md: Claude Code is BAA-covered only with zero data retention for the organization; surfaces to confirm with Anthropic. |

## Cited company decisions

| Constraint | Source (repo commit `30fc4e4`) |
|---|---|
| Adapter contract and T2 decisions | `specs/adapter-interface/requirements.md` |
| Egress classes and BAA allowlist | `docs/security/data-egress.md` |
| Approvals only in the external ledger | `docs/architecture/approval-ledger.md` |

## Unresolved questions

- [ ] Exit codes and stream-json event schema for `claude -p` 2.1.318 (C-1, C-9). — owner: TBD, blocks: T4
- [ ] Egress proxy design inside the bubblewrap network namespace and the exact model API hosts (C-6). — owner: TBD, blocks: T4
- [ ] Which Anthropic account and key the runner uses, and whether it has a BAA and zero data retention (C-7, C-12). — owner: human, blocks: T3
- [ ] Pin 2.1.310 (README) or 2.1.318 (installed), and how auto-update is disabled (C-10). — owner: human, blocks: T3
- [ ] Turn limit and subagent disabling flags (C-8, C-11). — owner: TBD, blocks: T4
