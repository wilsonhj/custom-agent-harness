# Design: claude-adapter

## API and schema changes

- New `harness/adapters/claude.ts` implementing the adapter-interface `Adapter`
  (`start`, `status`, `cancel`, `collect`) for runtime `claude`.
- Argv builder: `claude -p --bare --restricted --output-format stream-json --session-id
  <uuid>|--resume <uuid> --settings <harness file> --tools ... --allowedTools ...
  --permission-mode dontAsk --permission-prompts none --mcp-config <file>
  --strict-mcp-config --max-budget-usd <n>`; a test asserts forbidden flags never appear.
- Stream-json parser that maps events to the harness result schema (A-5).
- Harness-run egress proxy reachable from the sandbox, allowing only the model API host
  (C-6); design detail is open.

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
