# Design: kiro-adapter

## API and schema changes

- New adapter module under `harness/adapters/` (not yet created) implementing the
  common adapter interface for runtime `kiro`: start, cancel, collect artifacts.
- Row for Kiro in the runtime-capabilities table of `docs/security/data-egress.md`,
  filled only from verified primary docs.
- Managed Kiro configuration and a root-owned read-hook script, if Kiro supports them
  (K-4, K-5, K-6). Design is blocked on the unresolved questions in `requirements.md`.

## Dependencies

- `kiro-cli` (version to be pinned, K-12).
- Adapter interface and worktree management, both still "Planned" in the README.
- Network access to kiro.dev for the research task (T1).

## Migration plan

No migration: no database schema or persisted data changes.

## Rollout and rollback

- **Rollout:** adapter ships disabled; enabling it for a project requires the egress
  profile entry and a passing compatibility test. Human approval via the ledger.
- **Rollback:** disable the runtime in config; no state to undo.
