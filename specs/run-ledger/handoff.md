# Handoff: run-ledger / packet draft

<!-- Overwrite at the end of every work session. Facts only; link evidence. -->

| Field | Value |
|---|---|
| Base SHA | `0b9d58f7dfb1bf8a05b8726921628b56e8d63261` |
| Head SHA | uncommitted (packet files only) |
| Author (runtime / session ID) | Claude Code (cloud) / `session_01AaDiMJcmHQEvQBbrRCSVcB` |
| Date | 2026-11-03 |
| Task state | `Draft` (no approval-ledger record exists; none requested) |

## Changed files

- `specs/run-ledger/requirements.md`: behavior, non-goals, invariants, cited decisions, open questions
- `specs/run-ledger/design.md`: record schema, storage, validation, termination rules, versioning
- `specs/run-ledger/tasks.json`: T0 (human decisions) to T7 (independent review)
- `specs/run-ledger/acceptance-tests.md`: tests per requirement and invariant (commands are placeholders)
- `specs/run-ledger/handoff.md`: this file

No code changed.

## Implemented requirements

- None. This session drafted the packet only.

## Tests run and results

| Command | Result | Evidence |
|---|---|---|
| (none) | not run | No test runner exists yet (blocks T1, see requirements "Unresolved questions") |

## Codex review

Not run. The `codex` CLI and plugin are not installed in the cloud container this draft was
written in. Run `/codex:adversarial-review` on this packet locally before requesting
`Draft -> Planned`, and record findings and resolutions here.

## Known limitations

- Session-hook events can be forged by the session that emits them; the design marks them
  `provenance: "session-hook"` and bars them from carrying outcomes, but does not prevent
  forged decision records.
- The run ledger has tamper *evidence* (hash chain) but no anchor, so its tail can be
  truncated undetected. Acceptable only because it carries no authority.
- The evidence store it depends on is designed but not implemented.

## Unresolved decisions

- Test runner and toolchain: owner <maintainer> (blocks T1)
- Writer OS identity: owner <maintainer> (blocks T2)
- File rotation: owner <maintainer> (blocks T2)
- Accept or exclude session-hook events: owner <maintainer> (blocks T4)
- Retention, task-family taxonomy, split assignment: owner <maintainer> (non-blocking)

## Assumptions made

- Decision points are one per controller choice, not one per tool call, because the
  harness does not replace vendor agent loops (README non-goal).
- The run ledger is `local-only` as a whole, since IDs and timing of `local-only` work
  are metadata about it (data-egress "Default to most restrictive").
- Field and transition-record choices draw on the uploaded RL research note (2026-10-28),
  which is not independently verified; no requirement depends on its numerical results.
- CLI names (`eng runs ...`) follow the README "Proposed CLI surface" and are provisional.

## Exact next action

Maintainer reviews `specs/run-ledger/` and answers the three T0 questions that block
T1/T2, then runs `/codex:adversarial-review` on the packet and requests the
`Draft -> Planned` transition.
