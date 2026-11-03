# Design: harness-audit-fixes

## API and schema changes

All in `harness/policy/transitions.ts` (types and one pure function; no persistence).

- New `ResumableState` / `RESUMABLE_STATES` (replaces private `ACTIVE`): the states a task may
  be blocked from and resume to. Adds `Released` and `Observed`.
- New `BlockedRecord extends ApprovalRecord { to_state: "Blocked"; from_state: ResumableState }`.
  The commit pin is `target.commit_sha` (`approval-ledger.md` L30).
- New `TransitionContext { blockedRecord?: BlockedRecord; currentCommitSha?: string }`.
  Both values come from the control plane (verified ledger and the worktree's HEAD), never
  from the agent's request (`approval-ledger.md` L154-156).
- `canTransition(from, to, approval, context?)`: the old 4th parameter `blockedFrom` is
  removed. There are no other callers in this repository.
  Check order (all deny by default):
  1. `from`/`to` must be known states.
  2. Resuming (`from === "Blocked"` and `to` not `Cancelled`/`Superseded`): require a
     well-formed `blockedRecord` (`to_state === "Blocked"`, `from_state` resumable,
     non-empty `target.commit_sha`, non-empty `task_id`), a non-empty `currentCommitSha`,
     `to === blockedRecord.from_state` and `currentCommitSha === blockedRecord.target.commit_sha`.
     Otherwise the edge must be in `ALLOWED_TRANSITIONS`.
  3. If an approval is supplied it must be well-formed, have `from_state === from` and
     `to_state === to`, and (when resuming) the same `task_id` as the blocked record.
  4. Entering `Blocked` needs no approval (unchanged).
  5. Otherwise a missing approval is "approval required"; with one present, still
     "approval policy not implemented" (fail closed, `TODO(human)` kept).
- `ALLOWED_TRANSITIONS` edges added: `Verified|PreviewVerified|MigrationGated -> Failed`;
  `Released|Observed -> Blocked`; `Implementing|Repair -> Superseded`.

Docs kept consistent: `approval-ledger.md` "Reading the ledger" (blocked record + commit),
`release-gate.md` "Failure handling" (which states can record `Failed`), README (state names,
`untrusted_origin`, escalate wording, Codex review checkbox, release-decision wording),
`data-egress.md` L141-142 prose spelling.

## Dependencies

- Node >= 22.6 for `--experimental-strip-types` (checked: v22.22.0). Tests use `node:test`
  and `node:assert/strict`; no packages. Source avoids enums and parameter properties, which
  type stripping does not support.

## Migration plan

No migration: no database schema or persisted data. The `canTransition` signature change has
no callers in this repository; the control plane that will call it does not exist yet.

## Rollout and rollback

- **Rollout:** working-tree change only, not committed. Review with `/codex:review` (or
  `/codex:adversarial-review`) per CLAUDE.md, record findings in `handoff.md`, then a human
  approves the phase transition through the external ledger (agents may only request it).
- **Rollback:** discard the working-tree changes (or revert the eventual commit). The prior
  behaviour is the fail-closed stub, so reverting loses only the stricter checks and edges.
