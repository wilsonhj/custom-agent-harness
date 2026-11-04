# Handoff: run-ledger / packet draft

<!-- Overwrite at the end of every work session. Facts only; link evidence. -->

| Field | Value |
|---|---|
| Base SHA | `7fce6c645d7f37be1815a1a36619d7fa9095f134` (`main`, merged into the branch) |
| Head SHA | see PR #7 (packet files only) |
| Author (runtime / session ID) | Claude Code (cloud) / `session_01AaDiMJcmHQEvQBbrRCSVcB` |
| Date | 2026-11-03 |
| Task state | `Draft` (no approval-ledger record exists; none requested) |

## Changed files

Second session: the first draft was written against a stale `main` (`0b9d58f`). After
merging `main` (`7fce6c6`) the packet now uses the existing toolchain (`npm test`,
`npm run check`, Node 24, no dependencies), makes the local evidence workflow the first
producer (new task T3b), resolves evidence digests in its run store, and replaces the
resolved test-runner question with an evidence-location question.

- `specs/run-ledger/requirements.md`: behavior, non-goals, invariants, cited decisions, open questions
- `specs/run-ledger/design.md`: record schema, storage, validation, termination rules, versioning
- `specs/run-ledger/tasks.json`: T0 (human decisions) to T7 (independent review)
- `specs/run-ledger/acceptance-tests.md`: tests per requirement and invariant (`npm test` patterns; tests not written yet)
- `specs/run-ledger/handoff.md`: this file

No code changed.

## Implemented requirements

- None. This session drafted the packet only.

## Tests run and results

| Command | Result | Evidence |
|---|---|---|
| `npm run check` | pass (cloud container, Node 22.22.0) | Syntax checks only; packet is docs |
| `npm test` | 39 pass, 39 fail, 2 skipped (cloud container) | Same counts on unmodified `main` `7fce6c6` in the same container: the container has Node 22 (repo needs Node 24) and no bubblewrap. Not evidence either way; CI (`verify`, Node 24 + bubblewrap) is the record. No run-ledger tests exist yet. |

## Independent reviews

| Source | Finding | Resolution |
|---|---|---|
| Codex GitHub review on `110b431` (P2) | Termination order labelled a job that passes on its last allowed repair round `budget_exhausted` | Fixed: success is checked before budget; `budget_exhausted` now needs a limit that blocked a needed step. Fixture cases added to `test: termination`. |
| Codex GitHub review on `5601cb1` (P2) | Passing acceptance then a `changes_requested` review was classified `succeeded`, though `harness/local/workflow.mjs` fails that run | Fixed: `succeeded` requires every producer gate (acceptance, review `pass`, human `accept` where recorded); producer maps `changes_requested` to `stop`. Fixtures for rejected review and rejected decision added. |
| Independent Claude adversarial review (fresh subagent, read-only), 2026-11-04. **Not a Codex review.** | H1 correction could forge success; H2 session hooks could hijack or block control-plane jobs; H3 classification depended on free-text `m.failure`, and a candidate could dodge statistics via verifier timeout; H4 null pilot budget turned every verifier failure into `budget_exhausted`; M1 no `job_end` for interrupted runs, human decision not recorded; M2 retries duplicated records; M3 stale lock and torn line could block the ledger; M4 evidence check was existence-only, store unnamed, controller-only access; M5 false claim that the worker executable is pinned, fields with no pilot source; M6 cancel and recovered-denial precedence; L1 length-capped names leak; L2 export test could not fail; L3 untethered tests | All fixed in the packet: correction and provenance rules, structured `failureCode` (new task T2b) and pilot failure mapping, nullable budgets, human-decision record, deterministic IDs, unlock and torn-tail quarantine, `reconcile`, evidence-meaning check and `evidence_store_id`, `toolsDigest` as worker version, reordered precedence, registry allowlist, `classify.mjs` owned by T2, new tests. M4's access constraint is recorded under the writer-identity question. |
| Codex GitHub review on `ecdb61d` (P2, two findings) | (a) Requiring verifier `exitCode: 0` made failed outcomes unrecordable; (b) a failed intermediate append followed by later appends left a gap `reconcile` could not fill | (a) Evidence must agree with the outcome: exit 0 for passed counts, nonzero or candidate failure for failed counts (verified that `run()` stores evidence before the exit check). (b) Per-job appends are strictly ordered; after a failure the producer stops for that job and `reconcile` resumes from the first missing record. Tests `evidence-meaning` updated, `reconcile-order` added. |
| Codex GitHub review on `ecdb61d` (P2, third finding) | Two corrections of the same record got the same deterministic ID, so the second was rejected | Corrections carry a `revision` (strictly next per record) that is part of the ID; readers use the highest. Test `correction-revisions` added. |
| Codex GitHub review on `ff5084a` (P2, two findings) | (a) No mapping for a worker nonzero exit, an existing failure path; (b) validating a correction's replacement "exactly like a new record" rejected corrections of closed jobs and earlier decisions | (a) Worker `exit_nonzero` maps to `task_failure`; fixture added to `classify-history`. (b) Replacements get record-level checks and keep `step_index`, but append-order checks are not reapplied; test `correction-position` added. |

`/codex:adversarial-review` via the local plugin is still not run: the `codex` CLI and
plugin are not installed in the cloud container this draft was written in. Run it locally
before requesting `Draft -> Planned`, and record findings and resolutions here.

## Known limitations

- Session-hook events can be forged by the session that emits them; the design marks them
  `provenance: "session-hook"`, bars them from outcomes and corrections, and confines them
  to jobs they started, but a session can still forge records for its own jobs.
- The pilot cannot tell a failing acceptance test from a broken one (both exit nonzero), so
  every verifier nonzero exit is `task_failure`.
- The run ledger has tamper *evidence* (hash chain) but no anchor, so its tail can be
  truncated undetected. Acceptable only because it carries no authority.
- Evidence lives in two layouts (run store vs `<local-artifacts-directory>/evidence/`); the
  packet picks the run store for local-workflow jobs pending a maintainer decision.

## Unresolved decisions

- Writer OS identity: owner <maintainer> (blocks T2)
- Evidence location: owner <maintainer> (blocks T2)
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

Maintainer reviews `specs/run-ledger/` and answers the three T0 questions (writer identity,
evidence location, file rotation) that block T2, then runs `/codex:adversarial-review` on
the packet and requests the `Draft -> Planned` transition.
