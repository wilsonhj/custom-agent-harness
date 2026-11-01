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
- Adapter interface and harness worktree ownership, both still "Planned" in the README:
  `specs/adapter-interface/`, which this packet depends on.
- Network access to kiro.dev for the research task (T1).

## Migration plan

No migration: no application database schema or deployed-service data changes. The
adapter does add local persisted state (job artifacts, evidence-store files, usage
records); these are new append-only files outside the release path. The migration gate
(`docs/architecture/release-gate.md`) still applies: the release may skip migration
evidence only if the deterministic classifier reports the diff as `clean`.

## Rollout and rollback

- **Rollout:** adapter ships disabled. Whether a given task may launch Kiro is decided only
  by the "Launch procedure" in requirements.md. A human sets the enable flag (its step 1)
  only after the `tests/kiro/*` suite passes. Enabling is a configuration change recorded
  by a human in root-owned configuration the agent user cannot write (K-22); it is not a
  task-state transition, so the approval ledger (per-`task_id` transitions)
  is not used for it. The task that delivers the adapter still moves through the ledger
  as usual.
- **Rollback:** disable the runtime in config and remove the managed-config and hook
  entries. Already-written artifacts, evidence and usage records stay (append-only) and
  are not undone; secrets that were exposed to a session must be rotated.
