# Acceptance tests: run-ledger

Each item names the command or procedure that proves it and the requirement it covers.
Check an item only when the result is recorded in `handoff.md`.

`<test-command>` is a placeholder until T0 picks the test runner; it is not a runnable
command. Do not report any item below as passing before that.

## Behavior

- [ ] A complete job (start, three decisions, end) round-trips through writer and reader
  with every field intact — `<test-command> harness/ledger/run-ledger.test.ts -t roundtrip`
  — covers: list jobs; see decision points
- [ ] Each of the six termination reasons is produced by the classifier from a fixture
  event sequence, including precedence (a policy denial during a budget overrun is
  `policy_violation`) — `<test-command> -t termination` — covers: closed termination set
- [ ] A `job_end` with `outcome` but `provenance: "session-hook"` is rejected; so is
  `termination_reason: "succeeded"` from a session hook — `<test-command> -t provenance`
  — covers: outcome from control-plane commands only
- [ ] An `outcome` whose evidence digest is missing from the evidence store is rejected —
  `<test-command> -t evidence` — covers: outcome bound to evidence digests
- [ ] `eng runs verify` passes on an intact file and fails, naming the first bad line, after
  (a) editing a byte, (b) deleting a middle line, (c) swapping two lines —
  `<test-command> -t verify` — covers: verify command
- [ ] `eng runs export --task <id>` writes only that task's records and a header with the
  most restrictive egress class among them — `<test-command> -t export` — covers: export
- [ ] A correction record supersedes the original in `show` and `list` output while the
  original line stays in the file — `<test-command> -t correction` — covers: append-only
  corrections

## Invariants

- [ ] Records with an unknown field, a field over its length cap, a non-enum action, or
  `selected_action` outside `available_actions` are rejected with a reason —
  `<test-command> -t schema-reject`
- [ ] Planting a canary string (e.g. a fake PHI line) in any string field of a fixture is
  rejected by the schema, for every string field the schema defines (generated test, not
  hand-picked fields) — `<test-command> -t no-free-text`
- [ ] Missing usage stays `null` in the written record and in `show` output, never `0` —
  `<test-command> -t null-usage`
- [ ] A deterministic decision with a `probability` field is rejected; a stochastic one
  without it is rejected — `<test-command> -t behavior`
- [ ] Two concurrent appends from the same stale head: exactly one lands, the other is
  rejected — `<test-command> -t compare-and-append`
- [ ] As the agent OS user, opening `<run-ledger-root>/<project-id>.jsonl` for append fails
  with a permission error (procedure on the development machine, output saved to the
  evidence store) — `<write-boundary-check-command>`
- [ ] `harness/policy/transitions.ts` has no import from `harness/ledger/` —
  `grep -rn "ledger/" harness/policy/` returns nothing

## Migration

No migration: new store (design.md "Migration plan"). Version-handling check instead:

- [ ] Reader accepts every released fixture version and reports an unknown major version
  as an error without crashing — `<test-command> -t versions`

## Preview

Not applicable: no deployed surface.
