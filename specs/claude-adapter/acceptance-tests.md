# Acceptance tests: claude-adapter

Each item names the command or procedure that proves it and the requirement it covers.
Check an item only when the result is recorded in `handoff.md`.

The test files below do not exist yet; T5 creates them and they run with `npm test`. Tests
that call the real model API are marked "live" and need the T3 key; the rest use a fake
`claude` executable that records its argv, rejects argv the real CLI rejects (for example
`stream-json` without `--verbose`), and emits canned stream-json. Nothing here has
been run.

## Behavior

- [ ] Argv contains `-p --bare --restricted --disable-slash-commands --output-format stream-json --verbose`, never `--add-dir`, and the profile's `--tools`/`--allowedTools`; cwd is the sandboxed worktree — `tests/claude-adapter.test.ts` — covers: C-1, C-2, C-3
- [ ] With the profile's tools, a Bash command or write outside the allowed set is denied (a `permission_denied` event), not merely absent from the argv (live) — `tests/claude-adapter.test.ts` — covers: C-3
- [ ] Argv never contains `--dangerously-skip-permissions`, `--allow-dangerously-skip-permissions`, `bypassPermissions`, `--continue`, `--fork-session` or `--no-session-persistence` — `tests/claude-adapter.test.ts` — covers: C-3, C-5
- [ ] MCP servers come only from the harness `--mcp-config` with `--strict-mcp-config`; a worktree `.mcp.json` is not loaded (live) — `tests/claude-adapter.test.ts` — covers: C-4
- [ ] First run passes `--session-id <uuid>` and the `init` event's `session_id` must equal it; continuation passes `--resume` with the recorded ID only; an ID recorded for another task is refused (A-7); a transcript edited between runs fails the hash check and the resume is refused — `tests/claude-adapter.test.ts` — covers: C-5
- [ ] A task that reaches its total budget across runs is cut off at the model proxy and recorded as failed, including when the spend comes from continuations; the continuation cap refuses one more run — `tests/claude-adapter.test.ts` — covers: C-8
- [ ] Exit 0 with a final `result` event that has `is_error: true` (or `subtype: "success"` with `is_error: true`), a non-zero exit, a missing `result` event, malformed stream-json and output claiming "approved" all fail the run; valid output is stored as a runtime claim, not evidence — `tests/claude-adapter.test.ts` — covers: C-9
- [ ] A `claude --version` other than the pin refuses the run before launch; the environment carries `DISABLE_AUTOUPDATER=1` and `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` — `tests/claude-adapter.test.ts` — covers: C-10
- [ ] Subagents are unavailable when the profile disallows delegation: the real CLI's `system`/`init` event lists no Task or Agent tool (checkable with no credentials, as the init event precedes the auth failure) — `tests/claude-adapter.test.ts` — covers: C-11

## Invariants

- [ ] From the sandbox, the model proxy is reachable; a direct connection, DNS lookup of another host and a `NO_PROXY` bypass from Bash all fail (live) — `tests/claude-adapter-sandbox.test.ts` — covers: C-6
- [ ] With Bash allowed, an injected `env` / `cat /proc/*/environ` finds only the dummy key; a request through the proxy carrying a foreign key or for a non-model API path is refused; the dummy key appears nowhere useful and the real key appears in no worktree, task home, artifact or log — `tests/claude-adapter-sandbox.test.ts` — covers: C-7
- [ ] A worktree and a task home planted with `CLAUDE.md`, `.claude/settings.json` hooks, `.claude/agents`, `.claude/skills` and plugin settings: none load; `init` shows the expected `apiKeySource` and no extra skills, agents or MCP servers (live) — `tests/claude-adapter-sandbox.test.ts` — covers: C-2
- [ ] A task whose egress class exceeds the key's approval is refused before launch, and the proxy refuses a key or organization not in the allowlist entry; a canary `local-only` file is unreadable from the session — `tests/claude-adapter.test.ts` — covers: C-12
- [ ] Existing suites still pass under `npm test` — `npm test`

## Release

- [ ] The migration classifier reports `clean`, or full migration evidence is supplied while it does not exist — covers: design.md "Migration plan"
