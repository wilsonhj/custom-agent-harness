# Design: claude-adapter

## API and schema changes

- New `harness/adapters/claude.ts` implementing the adapter-interface `Adapter`
  (`start`, `status`, `cancel`, `collect`) for runtime `claude`.
- Argv builder: `claude -p --bare --restricted --disable-slash-commands --output-format
  stream-json --verbose --session-id <uuid>|--resume <uuid> --settings <harness file>
  --tools ... --allowedTools ... --permission-mode dontAsk --permission-prompts none
  --mcp-config <file> --strict-mcp-config --max-budget-usd <n> --max-turns <n>` (never
  `--add-dir`), with environment `ANTHROPIC_API_KEY=<dummy>`,
  `ANTHROPIC_BASE_URL=<sandbox route to the model proxy>`, `CLAUDE_CONFIG_DIR=<task home>`,
  `CLAUDE_CODE_SUBPROCESS_ENV_SCRUB=1`, `DISABLE_AUTOUPDATER=1` and
  `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`; a test asserts forbidden flags never appear.
- Stream-json parser that maps events to the harness result schema (A-5) and checks
  `init.session_id` and `init.apiKeySource`.
- Harness model proxy outside the sandbox (C-6, C-7, C-8, C-12): strips client credentials,
  adds the real key for the approved Console organization, forwards only model API paths,
  meters spend per task with a hard cutoff. How the sandbox reaches it is open.
- Transcript integrity (C-5): after each run the adapter copies the transcript out of the
  task home, records its SHA-256 in the run ledger, and rebuilds the task home from that
  copy before `--resume`.

## Dependencies

- `specs/adapter-interface/` T4 (reviewed interface, sandbox, lock, run ledger).
- Claude Code CLI at the pinned version, installed under an allowed runtime root.
- An Anthropic API key held outside the repository and store (C-7).

## Migration plan

No migration: no database schema or deployed-service data. The release gate
(`docs/architecture/release-gate.md`) applies: migration evidence may be skipped only if
the classifier reports `clean`; while the classifier does not exist, full evidence is needed.

## Rollout and rollback

- **Rollout:** disabled by default; enabled per project in root-owned configuration after
  the human decisions in T3 and a passing `tests/claude-adapter*` suite.
- **Rollback:** disable the runtime in configuration; recorded runs stay.
