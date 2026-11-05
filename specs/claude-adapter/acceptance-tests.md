# Acceptance tests: claude-adapter

Each item names the command or procedure that proves it and the requirement it covers.
Check an item only when the result is recorded in `handoff.md`.

The test files below do not exist yet; T5 creates them and they run with `npm test`. Tests
that call the real model API are marked "live" and need the T3 key; the rest use a fake
`claude` executable that records its argv, rejects argv the real CLI rejects (for example
`stream-json` without `--verbose`), and emits canned stream-json. Nothing here has
been run.

## Behavior

- [ ] Argv contains `-p --bare --restricted --output-format stream-json --verbose` and the profile's `--tools`/`--allowedTools`; cwd is the sandboxed worktree — `tests/claude-adapter.test.ts` — covers: C-1, C-2, C-3
- [ ] Argv never contains `--dangerously-skip-permissions`, `--allow-dangerously-skip-permissions`, `bypassPermissions`, `--continue`, `--fork-session` or `--no-session-persistence` — `tests/claude-adapter.test.ts` — covers: C-3, C-5
- [ ] MCP servers come only from the harness `--mcp-config` with `--strict-mcp-config`; a worktree `.mcp.json` is not loaded (live) — `tests/claude-adapter.test.ts` — covers: C-4
- [ ] First run passes `--session-id <uuid>`; continuation passes `--resume` with the recorded ID only — `tests/claude-adapter.test.ts` — covers: C-5
- [ ] A run over budget is recorded as failed — `tests/claude-adapter.test.ts` — covers: C-8
- [ ] Malformed stream-json and output claiming "approved" fail the run; valid output is stored as a runtime claim, not evidence — `tests/claude-adapter.test.ts` — covers: C-9
- [ ] A `claude --version` other than the pin refuses the run before launch — `tests/claude-adapter.test.ts` — covers: C-10
- [ ] Subagents are unavailable when the profile disallows delegation (live) — `tests/claude-adapter.test.ts` — covers: C-11

## Invariants

- [ ] From the sandbox, the model API host is reachable through the proxy and another host is refused (live) — `tests/claude-adapter-sandbox.test.ts` — covers: C-6
- [ ] A canary API key in the environment does not appear in the worktree, task home, artifacts or logs after a run — `tests/claude-adapter-sandbox.test.ts` — covers: C-7
- [ ] A worktree planted with `CLAUDE.md`, `.claude/settings.json` hooks and a `.claude/agents` file: none load (live) — `tests/claude-adapter-sandbox.test.ts` — covers: C-2
- [ ] A task whose egress class exceeds the configuration's approval is refused before launch — `tests/claude-adapter.test.ts` — covers: C-12
- [ ] Existing suites still pass under `npm test` — `npm test`

## Release

- [ ] The migration classifier reports `clean`, or full migration evidence is supplied while it does not exist — covers: design.md "Migration plan"
