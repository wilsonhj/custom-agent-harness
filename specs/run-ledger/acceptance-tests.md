# Acceptance tests: run-ledger

Each item names the command or procedure that proves it and the requirement it covers.
Check an item only when the result is recorded in `handoff.md`.

Tests live in `tests/run-ledger.test.mjs` and run in CI with `npm run check` and `npm test`
(`.github/workflows/local-workflow.yml`). To run one case:
`node --experimental-strip-types --test --test-name-pattern=<name> tests/run-ledger.test.mjs`.
Below, `test: <name>` means that command with that pattern. None of these tests exist
yet; nothing below has been run.

## Behavior

- [ ] A fixture local-workflow run (prepare, worker, verifier, reviewer, recorded decision)
  produces `job_start`, three `decision` records and one `job_end` with
  `local_run_state: "decision-recorded"`; a failing verifier produces `job_end` with
  `local_run_state: "failed"` — `test: local-workflow-producer` — covers: list jobs; see
  decision points
- [ ] A local-workflow config without `runLedger` behaves exactly as before (existing
  `tests/local-workflow*.test.mjs` pass unchanged) — `npm test`
- [ ] A complete job (start, three decisions, end) round-trips through writer and reader
  with every field intact — `test: roundtrip`
  — covers: list jobs; see decision points
- [ ] Each of the six termination reasons is produced by the classifier from a fixture
  event sequence, including precedence (a policy denial during a budget overrun is
  `policy_violation`; a job that passes acceptance on its last allowed repair round is
  `succeeded`, not `budget_exhausted`; a job whose last verifier fails with no repair round
  left is `budget_exhausted`; passing acceptance followed by a `changes_requested` review,
  or by a human `reject` decision, is `task_failure`, not `succeeded`) —
  `test: termination` — covers: closed termination set
- [ ] A `job_end` with `outcome` but `provenance: "session-hook"` is rejected; so is
  `termination_reason: "succeeded"` from a session hook — `test: provenance`
  — covers: outcome from control-plane commands only
- [ ] An `outcome` whose evidence digest is missing from the evidence store is rejected —
  `test: evidence` — covers: outcome bound to evidence digests
- [ ] `eng runs verify` passes on an intact file and fails, naming the first bad line, after
  (a) editing a byte, (b) deleting a middle line, (c) swapping two lines —
  `test: verify` — covers: verify command
- [ ] `eng runs export --task <id>` writes only that task's records and a header with the
  most restrictive egress class among them — `test: export` — covers: export
- [ ] A correction record supersedes the original in `show` and `list` output while the
  original line stays in the file — `test: correction` — covers: append-only
  corrections

## Invariants

- [ ] Records with an unknown field, a field over its length cap, a non-enum action, or
  `selected_action` outside `available_actions` are rejected with a reason —
  `test: schema-reject`
- [ ] Planting a canary string (e.g. a fake PHI line) in any string field of a fixture is
  rejected by the schema, for every string field the schema defines (generated test, not
  hand-picked fields) — `test: no-free-text`
- [ ] Missing usage stays `null` in the written record and in `show` output, never `0` —
  `test: null-usage`
- [ ] A deterministic decision with a `probability` field is rejected; a stochastic one
  without it is rejected — `test: behavior`
- [ ] Two concurrent appends from the same stale head: exactly one lands, the other is
  rejected — `test: compare-and-append`
- [ ] As the agent OS user, opening `<run-ledger-root>/<project-id>.jsonl` for append fails
  with a permission error (procedure on the development machine, output saved to the
  evidence store) — `<write-boundary-check-command>`
- [ ] `harness/policy/transitions.ts` has no import from `harness/ledger/` —
  `grep -rn "ledger/" harness/policy/` returns nothing

## Migration

No migration: new store (design.md "Migration plan"). Version-handling check instead:

- [ ] Reader accepts every released fixture version and reports an unknown major version
  as an error without crashing — `test: versions`

## Preview

Not applicable: no deployed surface.
