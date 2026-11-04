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
- [ ] Classifier precedence on realistic histories: cancel during the verifier is
  `cancelled`, not `infrastructure_failure`; a recovered policy denial followed by all gates
  passing is `succeeded` with `policy_violations: 1`; a verifier `deadline_exceeded` or
  `output_limit_exceeded` on the candidate is `task_failure`; a pilot verifier failure with
  null repair budget is `task_failure`, not `budget_exhausted`; an unknown failure code is
  `infrastructure_failure` with `failure_code_known: false` — `test: classify-history`
- [ ] Each local-workflow failure path (one per `failureCode`) records that code in
  `run.json`, and no run-ledger code reads `m.failure` — `test: failure-code`,
  `grep -rn "\.failure" harness/ledger/` returns nothing
- [ ] A `job_end` with `outcome` but `provenance: "session-hook"` is rejected; so is
  `termination_reason: "succeeded"` from a session hook — `test: provenance`
  — covers: outcome from control-plane commands only
- [ ] An `outcome` whose evidence digest is missing from the evidence store is rejected —
  `test: evidence` — covers: outcome bound to evidence digests
- [ ] `eng runs verify` passes on an intact file and fails, naming the first bad line, after
  (a) editing a byte, (b) deleting a middle line, (c) swapping two lines —
  `test: verify` — covers: verify command
- [ ] `eng runs export --task <id>` writes only that task's records and a `local-only`
  header — `test: export` — covers: export
- [ ] A correction record supersedes the original in `show` and `list` output while the
  original line stays in the file — `test: correction` — covers: append-only
  corrections

## Invariants

- [ ] A session-hook `decision` or `job_end` naming a control-plane job's `job_id` is
  rejected, and the control plane's own next record for that job is still accepted —
  `test: hijack`
- [ ] Two successive corrections of one record (revisions 1 and 2) are both accepted and
  `show` uses revision 2; a retried revision 2 is a no-op; a skipped or repeated revision
  with different bytes is rejected — `test: correction-revisions`
- [ ] A session-hook `correction` is rejected; a control-plane correction whose
  replacement changes `job_id`, `task_id`, `kind` or `provenance`, or adds an outcome that
  fails the evidence check, is rejected — `test: correction-forgery`
- [ ] An outcome citing a digest of worker output is rejected; `passed` counts citing a
  verifier job with nonzero exit are rejected; `failed` counts citing that same failing
  verifier job are accepted — `test: evidence-meaning`
- [ ] Appending the same record twice (simulated crash after fsync) leaves one copy and
  returns success; same `id` with different bytes is rejected — `test: idempotent`
- [ ] A file ending in a partial line: the next append quarantines the tail, the chain
  verifies, and `verify` reports the quarantined bytes; a lock left by a dead PID blocks
  appends until `eng runs unlock` — `test: torn-tail`, `test: stale-lock`
- [ ] An append failure on a job's second `decision`: the producer writes none of that job's
  later records, and `reconcile` then appends the missing `decision`, the rest, and
  `job_end` in order, with the chain and `step_index` checks passing — `test: reconcile-order`
- [ ] `reconcile` on a run whose ledger append failed adds exactly the missing records and is
  a no-op on a second run; it writes no `job_end` for `running` or `awaiting-decision`
  without `--confirm-dead` — `test: reconcile`
- [ ] Records with an unknown field, a field over its length cap, a non-enum action, or
  `selected_action` outside `available_actions` are rejected with a reason —
  `test: schema-reject`
- [ ] For every string field the schema defines (generated, not hand-picked), two canaries
  are rejected: one with spaces (a fake PHI line) and one that fits the field's character
  pattern but is not a valid value (e.g. `smith-john-19610302` in a name field) —
  `test: no-free-text`
- [ ] Missing usage stays `null` in the written record and in `show` output, never `0` —
  `test: null-usage`
- [ ] A deterministic decision with a `probability` field is rejected; a stochastic one
  without it is rejected — `test: behavior`
- [ ] Two concurrent appends from the same stale head: exactly one lands, the other is
  rejected — `test: compare-and-append`
- [ ] As the agent OS user, opening `<run-ledger-root>/<project-id>.jsonl` for append fails
  with a permission error (procedure on the development machine, output saved to the
  evidence store) — `<write-boundary-check-command>`
- [ ] `harness/policy/transitions.ts` has no import from `harness/ledger/` and no filesystem
  access, so `canTransition` cannot read the run ledger —
  `grep -rnE "ledger/|node:fs|from 'fs'" harness/policy/` returns nothing

## Migration

No migration: new store (design.md "Migration plan"). Version-handling check instead:

- [ ] Reader accepts every released fixture version and reports an unknown major version
  as an error without crashing — `test: versions`

## Preview

Not applicable: no deployed surface.
