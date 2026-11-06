# Handoff: claude-adapter / packet creation

| Field | Value |
|---|---|
| Base SHA | 30fc4e4 (adapter-interface T2 decisions recorded) |
| Head SHA | 63f10d1 (packet before this session's review fixes; they land in the next commit) |
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
| Auto-update and network | Docs only (not probed): `DISABLE_AUTOUPDATER=1`; only `api.anthropic.com` needed with API-key auth and non-essential traffic disabled; `HTTPS_PROXY` and `ANTHROPIC_BASE_URL` honoured |

**Unintended billed call.** The `--restricted`-only probe (without `--bare`) found
credentials on this host despite `env -i` and an empty `HOME`, and made one real model call
(reported cost USD 0.055). This was not intended. Its credential source was not recorded
(the `init` event's `apiKeySource` was not captured), so the conclusion rests on this one
observation plus the documented `--bare` auth rule. It is also the strongest evidence for C-2
and C-7: only `--bare` confines auth to `ANTHROPIC_API_KEY`. All other probes failed
authentication before any model call.

## Review: Claude subagent adversarial review (NOT Codex), 2026-11-06

Stand-in for T2 because `/codex:adversarial-review` is not installed in the authoring
session. A fresh Claude subagent reviewed 63f10d1 read-only (no credentialed `claude` runs).
Key claims were checked against the Claude Code docs before fixing. All findings fixed in
the next commit:

| Finding | Resolution |
|---|---|
| High: API key readable by the model (env, `/proc`), and a host-only allowlist lets Bash send data to the API under another key | C-7: dummy key in the sandbox, `ANTHROPIC_BASE_URL` to a harness model proxy that injects the real key, strips client credentials and forwards only model API paths; `CLAUDE_CODE_SUBPROCESS_ENV_SCRUB=1`; injection and foreign-key tests |
| High: `--max-budget-usd` is a client-side estimate, can be exceeded and resets on resume | C-8: per-task total budget metered at the proxy with a hard cutoff and a continuation cap; per-run flags advisory |
| High: `--resume` trusts a transcript the previous run can rewrite | C-5: harness-owned transcript copy and hash in the run ledger; task home rebuilt from it before resume; tamper test |
| Medium-high: `--bare` removes the hook and login-pinning layers data-egress.md relies on; C-12 had no mechanism | C-12: allowlist entries name Console organization and key ID, enforced at the proxy; data-egress.md amendment added as a human-owned open question; canary test |
| Medium: `--add-dir` still loads skills under `--bare` | C-2: no `--add-dir`, `--disable-slash-commands`, managed-settings path controlled |
| Medium: no egress route; test too weak | C-6 reworded; route is an open question; test adds direct connect, DNS and `NO_PROXY` bypass |
| Medium: tests did not prove their requirements | C-3 denied-action test, C-11 checked on the real CLI, C-2 plants config in the task home, C-5 cross-task and tamper cases |
| Low-medium: managed settings apply under `--restricted` | C-2 controls that path; `init.apiKeySource` asserted |
| Low: A-7 "runtime-assigned" vs harness-assigned ID | C-5 states the harness-assigned ID is A-7's recorded ID and checks `init.session_id` |
| Low: stale head SHA, probe evidence, citations | Head SHA updated; docs-only items marked; C-8 citation replaced |

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

A human runs `/codex:adversarial-review` on `specs/claude-adapter/` (T2 still owed; the Claude review above does not satisfy it), then makes the T3 decisions.
