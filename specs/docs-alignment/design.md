# Design: docs-alignment

## API and schema changes

Documentation and templates only; no code, schema or persisted data.

| ID | File | Change |
|---|---|---|
| D1 | `docs/architecture/approval-ledger.md` | Add a "Canonical bytes" subsection under "Approver identity": the rules in `requirements.md`, the line/signature relation, and one worked example. Update the `prev_record_hash` row to say it hashes the exact line bytes as defined there. |
| D2 | `docs/architecture/release-gate.md` | Rewrite "Failure handling": migration applied but release not yet `Released` → `MigrationGated -> Repair` (or `Blocked`); failure after `Released` → `Released -> Failed`, then `Failed -> Repair`. Add where the applied migration is recorded (per the answered question). Update the "Open questions" bullet that says the rules are decided in the `TODO(human)` block once `approval-binding` T5 lands. |
| D3 | `docs/security/data-egress.md` | Add the class order, define "highest class" with it, add the unmatched-session sentence next to the existing `any-provider` fallback, add a design-status note and the hook-guardrail wording, change "mode 600" to "files `0600`, directory `0700`". |
| D4 | `README.md` | Line ~511 diagram tokens; line ~462 `untrusted_origin`; Phase 1 sentence at ~688 reworded as an exit criterion; capability row ~102 "Run ledger and budgets" split or pointed at "not designed yet". |
| D5 | `specs/_template/acceptance-tests.md`, `specs/_template/tasks.json` | Migration section: keep a classifier line when the design says "No migration". `tasks.json`: statuses `todo` / `in_progress` / `done` and a one-line note (README of the template or a `_note` field) that this is a checklist, not `TaskState`. |

Concrete wording for D4's exit-criterion sentence (replace "The release *decision* is
enforced from Phase 1."):

> Exit criterion: when Phase 1 is done, the release decision is enforced by
> `canTransition` against signed ledger records. Until then the checkbox above is open
> and `canTransition` only allows entry into `Blocked`.

## Dependencies

- `approval-binding` T5 edits one bullet in `release-gate.md` ("Open questions"); the two
  packets touch different hunks, but sequence merges to avoid a conflict.
- D1's worked example needs an independent canonicalizer run (a one-off script) to produce
  the bytes and digest; the script is not checked in.

## Migration plan

No migration. Documentation and template text only.

Release-gate note: no classifier exists for this repository yet, so the template's
classifier line (D5) is exactly the line this packet's own acceptance test cannot satisfy
today; it is recorded as not-available, not as passing.

## Rollout and rollback

- **Rollout:** a normal PR reviewed with `/codex:adversarial-review`. Merge after the
  open questions are answered; D3 and D4 can ship earlier because they have no open
  question beyond D3's confirmation.
- **Rollback:** revert the merge commit.
