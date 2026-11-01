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

No migration: no application database schema or deployed-service data changes. The
adapter does add local persisted state (job artifacts, evidence-store files, usage
records); these are new append-only files outside the release path, so there is nothing
for the migration gate (`docs/architecture/release-gate.md`) to expand or contract.

## Rollout and rollback

- **Rollout:** adapter ships disabled. Enabling it for a project needs, in order: the
  human provider decision (K-17), the egress-profile entry and managed config, and a
  passing `tests/kiro/*` suite. Enabling is a configuration change recorded by a human in root-owned configuration the agent user cannot write (K-22);
  it is not a task-state transition, so the approval ledger (per-`task_id` transitions)
  is not used for it. The task that delivers the adapter still moves through the ledger
  as usual.
- **Rollback:** disable the runtime in config and remove the managed-config and hook
  entries. Already-written artifacts, evidence and usage records stay (append-only) and
  are not undone; secrets that were exposed to a session must be rotated.
