# Acceptance tests: approval-binding

Each item names the command or procedure that proves it and the requirement it covers.
Check an item only when the result is recorded in `handoff.md`.

Test command (all items below unless stated): `node --test harness/policy/transitions.test.ts`

## Behavior

- [ ] The four documented README/PR lines print exactly as documented —
  `node --test harness/policy/transitions.test.ts` (test "documented check") — covers:
  fail-closed behavior
- [ ] A forged `ApprovalRecord` on `MigrationGated -> Released` stays
  `"approval policy not implemented"`; the result equals the no-`approval` result —
  covers: only entry into `Blocked` succeeds
- [ ] Exhaustive sweep over all 16×16 state pairs, with and without a forged record:
  `ok: true` exactly when `to === "Blocked"` and the edge is in the graph — covers:
  fail-closed behavior
- [ ] Each ACTIVE state resumed with a verified entry record for that state returns
  `"approval policy not implemented"` — covers: resume binding
- [ ] Resume with a mismatched record, a record whose `to_state` is not `Blocked`, or no
  record returns `"... is not allowed"` — covers: resume binding
- [ ] `Implementing -> Blocked` with a record whose `to_state` is `Released` returns
  `ok: false`; with a matching record or no record returns `ok: true` — covers:
  mismatched-record rejection
- [ ] `PreviewVerified -> MigrationGated` is in the graph and returns
  `"approval policy not implemented"` — covers: untested gate edge
- [ ] Unknown `from` and unknown `to` return `ok: false` and do not throw — covers:
  unknown-state handling
- [ ] Edge snapshot: `ALLOWED_TRANSITIONS` deep-equals the checked-in list (46 edges);
  removing `PreviewVerified -> MigrationGated` makes this test fail (verify by
  temporarily deleting the edge and recording the failing output) — covers: edge
  coverage
- [ ] The TODO block text contains both release-gate constraints (human record with exact
  commit and `deployment_target` for `MigrationGated -> Released`; policy actor only on
  classifier `clean` for `PreviewVerified -> MigrationGated`) —
  `grep -n "deployment_target\|clean" harness/policy/transitions.ts`

## Invariants

- [ ] No code path returns `ok: true` for a non-`Blocked` target — covered by the sweep
  above
- [ ] The diff does not touch the ledger directory or any signing key —
  `git diff --name-only main... | grep -E '^(harness/policy/transitions(\.test)?\.ts|README\.md|docs/architecture/release-gate\.md|specs/approval-binding/)'`
  lists every changed file and nothing else appears
- [ ] `node -e` one-liner from the PR #1 body still prints the documented four lines

## Migration

No migration (see `design.md`). One classifier line is kept so the gate's "no migration"
rule is not silently dropped:

- [ ] Migration classifier reports `clean` for the diff — `<migration-classifier-command>`
  — **not available**: no classifier exists for this repository yet. Record "n/a: no
  classifier" in `handoff.md`; do not report this item as passing.

## Preview

- [ ] n/a: no deployment. Record "n/a" in `handoff.md`.
