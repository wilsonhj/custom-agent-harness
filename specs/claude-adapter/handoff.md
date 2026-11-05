# Handoff: claude-adapter / packet creation

| Field | Value |
|---|---|
| Base SHA | 30fc4e4 (adapter-interface T2 decisions recorded) |
| Head SHA | 30fc4e4 (this packet lands in the next commit on this branch) |
| Author (runtime / session ID) | Claude Code / session_01AaDiMJVSVxQVfoGPARPgBn |
| Date | 2026-11-05 |

## Changed files

- `specs/claude-adapter/*` — new packet for the first runtime adapter (adapter-interface T2 decision D5).

## Implemented requirements

- None. Packet only; no code.

## Tests run and results

| Command | Result | Evidence |
|---|---|---|
| `claude --version`, `claude --help` | 2.1.318; flags recorded under "Claude evidence by requirement" | terminal output of this session |

## Review: Codex PR review bot (PR #10), 2026-11-05

| Finding | Resolution |
|---|---|
| P2: `-p --output-format stream-json` needs `--verbose` | Verified on 2.1.318 (the CLI refuses without it); added to C-1, design argv and acceptance test in 8882c9f. Re-review of 8882c9f: no findings. |

## T1 results (2026-11-05)

Sources: code.claude.com docs (headless, cli-reference, settings, setup, network-config,
sessions, sub-agents, data-usage), fetched as Markdown; and runs of the installed 2.1.318 in
a scratch git repository with `env -i`, an empty `HOME`, `DISABLE_AUTOUPDATER=1` and
`CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`, with no API key, reading only the stream-json
`init` and `result` events.

| Item | Result |
|---|---|
| Exit codes | Docs: 0 on success, non-zero on failure, 143 on SIGTERM. Observed: unauthenticated run exits 1 with `result.subtype "success"` and `is_error: true`; C-9 now requires checking `is_error` and the exit code |
| stream-json schema | First event `system`/`init` (session_id, tools, mcp_servers, permissionMode, apiKeySource, claude_code_version, ...); last line `result` (result text, is_error, session_id, num_turns, total_cost_usd, permission_denials) |
| `--bare` with `--restricted` | Tools at init: bare only Bash/Edit/Read; restricted only 19 tools incl. Task, WebSearch, Skill; both Edit/Read. Both flags required (C-2) |
| Session store | Docs: `~/.claude/projects/<project>/<id>.jsonl`; `CLAUDE_CONFIG_DIR` relocates it to the task home. Live confirmation deferred to T5 |
| Turn limit | `--max-turns` exists (docs) and is accepted, though absent from `--help` |
| Subagents | Off under `--bare --restricted` (no Task/Agent tool at init) |
| Auto-update and network | `DISABLE_AUTOUPDATER=1`; only `api.anthropic.com` needed with API-key auth and non-essential traffic disabled; `HTTPS_PROXY` honoured |

**Unintended billed call.** The `--restricted`-only probe (without `--bare`) found
credentials on this host despite `env -i` and an empty `HOME`, and made one real model call
(reported cost USD 0.055). This was not intended. It is also the strongest evidence for C-2
and C-7: only `--bare` confines auth to `ANTHROPIC_API_KEY`. All other probes failed
authentication before any model call.

## Known limitations

- Live behaviour that needs an API key (session store under `CLAUDE_CONFIG_DIR`, `--resume`, worktree config not loading) is deferred to T5.
- The bubblewrap sandbox has no network; C-6 needs an egress proxy that does not exist yet.
- No independent review yet.

## Unresolved decisions

- Anthropic account/key, BAA and zero-retention status, version pin — owner: human, in T3.

## Assumptions made

- Packet ID `claude-adapter` follows adapter-interface decision D5.
- `--bare` plus `--restricted` is assumed to be the right default for unattended runs; T1 confirms how they combine.

## Exact next action

Run T2: a human runs `/codex:adversarial-review` on `specs/claude-adapter/` and records the findings here.
