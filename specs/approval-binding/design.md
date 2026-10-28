# Design: approval-binding

## API and schema changes

All in `harness/policy/transitions.ts`; no persisted data changes.

- **Signature.** Replace the free `blockedFrom?: TaskState` argument with a record proving
  the stop, pending the answer to the first open question (recommended shape):

  ```ts
  // Constructed only by the ledger verifier (not implemented yet).
  export type VerifiedLedgerRecord = ApprovalRecord & { readonly __verified: unique symbol };
  canTransition(from, to, approval, blockedEntry?: VerifiedLedgerRecord)
  ```

  Resume is legal only if `blockedEntry.to_state === "Blocked"`,
  `ACTIVE.includes(blockedEntry.from_state)`, and `to === blockedEntry.from_state`.
  Because nothing can construct the brand yet, resume cannot succeed — same behavior as
  today, but the free argument is gone.
- **Blocked-entry path.** If `approval` is defined and
  `approval.from_state !== from || approval.to_state !== to`, return
  `{ ok: false, reason: "approval record does not match transition" }`. If `approval` is
  undefined, return `{ ok: true }` unchanged.
- **Unknown states.** Guard with `Object.hasOwn(ALLOWED_TRANSITIONS, from)`; an unknown
  `from` or `to` returns `"transition <from> -> <to> is not allowed"` instead of throwing.
- **TODO text.** Replace "~5-10 lines" with the two constraints in `requirements.md`,
  citing `docs/architecture/release-gate.md`. Still fail closed.
- **Comments.** Correct the stale comments at `transitions.ts:51` and `:102` ("resume is
  allowed", "stopping work is always possible") to describe the real behavior.
- **Tests.** New `harness/policy/transitions.test.ts` using `node:test` and
  `node:assert/strict`; no dependencies, no `package.json` required to run it.

## Dependencies

- Node with TypeScript type stripping (Node 22.18+ default, or Node 24). No new packages.
- Packet `docs-alignment` D2 may add a graph edge; if it does, update the snapshot test.

## Migration plan

No migration. This repository has no database, schema or persisted data; the change is
a pure function, its test, and comments.

Release-gate note: `docs/architecture/release-gate.md` allows "no migration" only on a
`clean` result from a deterministic classifier, and no classifier exists for this
repository yet. The acceptance test therefore keeps one classifier line, marked
not-available until the classifier ships. The assertion that the diff touches only
`harness/`, `docs/`, `specs/` and `README.md` is a stand-in, not the classifier.

## Rollout and rollback

- **Rollout:** a normal PR; the independent `/codex:adversarial-review` runs on the
  diff before the `Reviewing -> Verified` request. The change only removes ways to get
  `ok: true`, so nothing downstream can regress to a more permissive result.
- **Rollback:** revert the merge commit. Nothing consumes `canTransition` yet, so the
  revert returns to the `0b9d58f` behavior.
