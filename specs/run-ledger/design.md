# Design: run-ledger

Status: Draft. Requirements: `requirements.md`.

## Concepts

| Term | Meaning here | RL-note equivalent |
|---|---|---|
| Job | One bounded run on one task in one worktree. Today: one local-workflow run (`job_id` = its `runId`). Later: one adapter call or one interactive session. | Episode |
| Decision point | A moment where the controller chooses what happens next: after implementation, after a review, after a verifier run, on budget pressure | Step |
| Controller | The code that makes that choice. Phase 1: the static rules in config (`max_repair_rounds`, `deadline_seconds`). | Policy |
| Verifier | A command the control plane runs whose result is evidence: acceptance tests, a Codex review, a schema check | Verifier |

Decision points are **coarse**: one per controller choice, not one per tool call. Tool
calls belong to the runtime's own loop, which this harness does not replace (README
non-goal "Replacing vendor-provided agent loops").

### First producer: the local evidence workflow

The local workflow controller (`harness/local/workflow.mjs`) is already a trusted,
non-agent process with a run lease and fsynced writes, so it emits the first records:

| Local-workflow event | Run-ledger record |
|---|---|
| `prepare` completes | `job_start` (`runtime.name: "local-command"`, `runtime.version` = the run's pinned `toolsDigest`; `controller.kind: "static"`, `config_digest` = the pinned `configDigest`) |
| Worker exits, controller records the candidate commit | `decision` (`selected_action: "verify"`) |
| Verifier exits | `decision` (`request_review` on exit 0, otherwise `stop`) |
| Reviewer verdict recorded | `decision` (`escalate` to the human decision on `pass`; `stop` on `changes_requested`, which fails the run) |
| Human decision recorded | `decision` (`decided_by: "human"`, `human_decision: "accept"` \| `"reject"`, `selected_action: "stop"`) |
| Run reaches `failed` or `decision-recorded` | `job_end` (`human_decision` copied from the run's decision statement) |

The pilot has no repair loop, so `repair` never appears in its records yet. Run states stay
local execution facts: the run ledger copies them as `local_run_state`, never as `TaskState`.
Evidence digests in these records resolve in that run's store (`<store>/<run-id>/objects/`).

`runtime.version` is the tools digest, not a hash of `argv[0]`: the pilot pins
`configDigest`, `toolsDigest` and `signersDigest` (`workflow.mjs`, `frozenConfig`), and
`argv[0]` is typically `/usr/bin/node`, whose hash identifies Node rather than the worker.

#### Pilot failure mapping

Today the pilot records a failure only as an error message (`m.failure = e.message` in
`workflow.mjs`; `process.mjs` produces `cancelled`, `deadline exceeded`, `output limit
exceeded` or a spawn error). The producer **must not parse these strings**. Task T2b first adds
a structured `failureCode` to the run manifest; the mapping below is over those codes:

| `failureCode` (added by T2b) | Where | `termination_reason` |
|---|---|---|
| `cancelled` | any stage | `cancelled` |
| `exit_nonzero` | verifier | `task_failure` (the candidate failed acceptance; the pilot cannot tell a test failure from a broken test, so it does not claim `error`) |
| `deadline_exceeded`, `output_limit_exceeded` | worker or verifier on the candidate | `task_failure`. A candidate that makes acceptance hang or flood output has failed; labelling it infrastructure would drop it from outcome comparisons. |
| `exit_nonzero`, `deadline_exceeded`, `output_limit_exceeded` | reviewer | `infrastructure_failure` (the reviewer is operator-supplied, not the candidate) |
| `review_changes_requested`, `packet_changed`, `candidate_changed`, `snapshot_limit` | controller check on the candidate | `task_failure` |
| `spawn_error`, `sandbox_unavailable`, `config_changed`, `invalid_review_output` | controller or environment | `infrastructure_failure` |
| missing or unknown code | any | `infrastructure_failure`, and the record carries `failure_code_known: false` so it is visibly unclassified |

## API and schema changes

New, nothing removed.

### Storage

- Append-only JSONL: `<run-ledger-root>/<project-id>.jsonl`, outside every worktree and
  outside the repository, same placement rules as `<ledger-root>`.
- Owned by the writer account (see requirements "Unresolved questions"), directory `0750`,
  file `0640`. Agent processes run as a user with no write access. Each runtime's managed
  configuration also denies `<run-ledger-root>/**` (defence in depth, as for the approval
  ledger).
- Egress class of the file: `local-only`. Records hold no content, but IDs, SHAs, and
  timing of `local-only` work are still metadata about it.

### Common envelope (every record)

| Field | Type | Notes |
|---|---|---|
| `schema_version` | `"run-ledger/v1"` | Readers reject unknown major versions. |
| `id` | SHA-256 hex of `job_id`, `kind`, `step_index` (or `""`), and for `correction` the corrected `id` and the correction's `revision` | Deterministic, so a retried append of the same record is recognised (see Writer). |
| `kind` | `"job_start"` \| `"decision"` \| `"job_end"` \| `"correction"` | |
| `project_id`, `task_id`, `job_id` | string | `task_id` = `specs/<id>/`. |
| `provenance` | `"control-plane"` \| `"session-hook"` | Who observed the event. Fixed per job by its `job_start`; every later record of the job must match (see Validation). Only `control-plane` records can carry outcomes. |
| `timestamp` | ISO 8601 UTC | Informational. |
| `prev_record_hash` | SHA-256 hex | Of the previous line's exact bytes; 64 zeros first. Tamper *evidence* only (no anchor, no signature). |

### `job_start`

| Field | Type |
|---|---|
| `runtime` | `{ name: "local-command" \| "claude-code" \| "codex-cli" \| "cursor-agent", version }`. For `local-command` (the local workflow's worker), `version` is the SHA-256 of the executable, which the controller already pins. |
| `adapter` | `{ name, version }` (e.g. `codex-plugin`, `1.0.6`) |
| `model_id` | string \| null (as the runtime reports it; never inferred) |
| `skills` | `{ name, version }[]` |
| `permission_profile` | `"analysis"` \| `"workspace"` \| `"preview-ops"` \| `"release"` \| null (null when the producer has no profile; the pilot does not) |
| `egress_ceiling` | egress class the session was allowed to read |
| `session_id` | UUID \| null (as recorded by the adapter; the pilot has none) |
| `worktree_id` | string \| null |
| `base_sha` | string |
| `evidence_store_id` | SHA-256 hex of the run store's real path, so `verify` can be pointed at the right store later without the path entering the ledger |
| `harness_sha` | commit of this repository the control plane ran from |
| `knowledge_revision` | string \| null |
| `task_family` | string \| null (taxonomy is an open question) |
| `task_state_at_start` | `TaskState` \| null. Only from an approval ledger the producer has verified; otherwise null. The pilot never reads the approval ledger, so it always writes null. |
| `controller` | `{ kind: "static" \| "learned", version, config_digest: { sha256 } }` |
| `budget` | `{ deadline_seconds, max_repair_rounds, max_cost_usd, per_process_deadline_ms }`, each number \| null. A null limit never triggers rule 5. The pilot has only `per_process_deadline_ms` (its `deadlineMs`). |

### `decision`

| Field | Type | Notes |
|---|---|---|
| `step_index` | integer, from 0 | Strictly increasing per job. |
| `observation` | object, fixed fields only | `phase` (`TaskState` \| null, same rule as `task_state_at_start`), `repair_rounds_used`, `elapsed_seconds`, `budget_remaining_seconds`, `cost_so_far_usd` (number \| null), `head_sha`, `last_verifier` (`{ name, result: "pass" \| "fail" \| "error" \| "not_run", blocking_findings, nonblocking_findings }` \| null), `tool_error_count` (integer \| null) |
| `available_actions` | enum[] | Subset of `continue`, `request_review`, `repair`, `verify`, `stop`, `escalate`. |
| `selected_action` | enum | Must be in `available_actions`. |
| `decided_by` | `"controller"` \| `"human"` | A human override is logged, not hidden. |
| `human_decision` | `"accept"` \| `"reject"` \| null | Set only when `decided_by: "human"`. |
| `behavior` | `{ policy_kind: "deterministic" }` \| `{ policy_kind: "stochastic", probability: number in (0,1] }` | |
| `result` | `{ verifier: { name, result, evidence_refs } \| null, latency_ms, usage: { input_tokens, output_tokens, cost_usd } }` | Each usage field number \| null. |

Adding an observation field is a minor schema change (see Migration plan); free-form
maps are not allowed.

### `job_end`

| Field | Type | Notes |
|---|---|---|
| `head_sha` | string | |
| `termination_reason` | `"succeeded"` \| `"task_failure"` \| `"budget_exhausted"` \| `"infrastructure_failure"` \| `"policy_violation"` \| `"cancelled"` | Classification rules below. |
| `outcome` | `{ acceptance: { passed, failed, not_run }, evidence_refs }` \| null | Counts from control-plane-run commands only. |
| `outcome_components` | `{ wall_seconds, cost_usd (number \| null), repair_rounds, review_rounds, blocking_findings_open, policy_violations }` | Raw components. `policy_violations` counts every denied attempt, including recovered ones. **No combined reward scalar** is stored; weighting is an analysis decision. |
| `usage_total` | `{ input_tokens, output_tokens, cost_usd }` | Each number \| null. |
| `approval_record_ids` | string[] | IDs of approval-ledger records written during the job; references only. |
| `local_run_state` | `"failed"` \| `"decision-recorded"` \| `"running"` \| null | Final local-workflow run state, for jobs it produced. `"running"` only for an interrupted run closed by `reconcile` (see Writer). Not a `TaskState`. |
| `human_decision` | `"accept"` \| `"reject"` \| null | The recorded human decision, if any. |
| `failure_code`, `failure_code_known` | enum \| null, boolean | From the producer's structured failure (see Pilot failure mapping). |

### `correction`

`{ corrects: <record id>, revision: <integer from 1>, reason_code: enum, replacement: <full record of the same kind> }`.
`revision` counts corrections of the same record: each must be exactly one more than the
highest already in the ledger for that record, so successive corrections get distinct IDs
while a retry of the same correction keeps its ID. Readers use the highest revision. `reason_code` is an enum
(`misclassified_termination`, `writer_bug`, `late_usage_report`), not text.

A correction is itself a `control-plane` record; session hooks cannot write one. Its
`replacement` is validated exactly like a new record (schema, provenance rules, evidence
checks) and must keep the corrected record's `id`, `kind`, `job_id`, `task_id` and
`provenance`. Otherwise a correction could turn a failed job into a success, or move a
record to another job.

### Termination classification

Applied by the control plane, in this order (first match wins):

1. Operator or upstream cancel → `cancelled`. Checked first, because a cancel kills the
   running process and can look like a malfunction.
2. The job **ended because of** a denied or blocked prohibited action → `policy_violation`.
   A denial the agent recovered from does not end the job; it is counted in
   `outcome_components.policy_violations` and classification continues.
3. A malfunction outside the candidate (sandbox, spawn, controller environment, reviewer
   process, verifier result `error` where the producer can tell `error` from `fail`) →
   `infrastructure_failure`. A verifier timeout or output overflow on the candidate is
   **not** a malfunction (see Pilot failure mapping).
4. Every gate the producer requires passed on `head_sha` → `succeeded`, even if that was
   the last allowed repair round or the budget is now used up. Gates: the acceptance
   commands, the required review verdict (`pass`; `changes_requested` fails it, as
   `harness/local/workflow.mjs` does), and, where the producer records one, the human
   decision (`accept`; `reject` fails it). A gate that did not run counts as not passed.
5. A gate did not pass **and** the controller could not take the next needed action
   (another repair, review or verify) because a **non-null** `deadline_seconds`,
   `max_repair_rounds` or `max_cost_usd` forbade it → `budget_exhausted`. Reaching a limit
   is not enough on its own; the limit must have blocked a needed step. A producer with no
   repair loop (the pilot) never blocks a repair, so its failures fall to rule 6.
6. Otherwise → `task_failure`.

`infrastructure_failure` and `policy_violation` jobs are kept for operations but flagged
for exclusion from any outcome comparison. The rules live in `harness/ledger/classify.mjs`
(task T2) as a pure function from a job's records to a reason, so they are tested directly.

### Validation (writer rejects the record if any fails)

- JSON Schema for the declared `schema_version`, `additionalProperties: false` at every level.
- Every string field has a **pattern or allowlist**, not only a length cap: SHAs and IDs
  are hex or UUID; versions match `^[0-9A-Za-z.+-]{1,64}$`; names (`skills.name`,
  `adapter.name`, `verifier.name`, `model_id`, `task_family`) must appear in a reviewed
  registry file in the repository (`harness/ledger/registry.json`). A name-shaped value
  that is not registered, such as `smith-john-19610302`, is rejected.
- `selected_action ∈ available_actions`; `step_index` follows the job's last one.
- `decision` and `job_end` reference an existing open `job_start` **with the same
  provenance**; one `job_start` and one `job_end` per job. A session hook therefore cannot
  append to, end, or block a control-plane job even if it learns the run ID (which every
  job receives on stdin).
- `provenance: "session-hook"` records may not carry `outcome`, `verifier` results, or
  `termination_reason: "succeeded"`, and may not be `correction` records.
- Outcome evidence is checked for meaning, not only existence: every `outcome.evidence_refs`
  digest must resolve, in the store named by `evidence_store_id`, to a job record listed in
  that run's `run.json` `jobs` with `role: "verifier"`. A digest of worker output, which
  lives in the same object directory, does not qualify. The evidence must **agree with** the
  outcome: `passed` counts need a verifier job with `exitCode: 0` and no failure; `failed`
  counts need one with a nonzero exit or a candidate-attributed failure code. The workflow
  saves each job's evidence before it checks the exit code (`workflow.mjs`, `run()`), so a
  failed outcome always has its own evidence.

### Writer, reader, CLI

New module `harness/ledger/`, Node 24 ESM `.mjs` with no npm dependencies, matching
`harness/local/` (T1 adds the directory to `scripts/check-local.mjs`):

- `run-record.mjs`: the v1 schema as data plus a hand-written validator (no JSON Schema
  package, since the repo takes no dependencies). `TaskState` values are imported from
  `harness/policy/transitions.ts` only in tests, to check the enum stays in sync.
- `classify.mjs`: the termination rules as a pure function (see above).
- `writer.mjs`: validates, takes an exclusive lock on `<run-ledger-root>/<project-id>.lock`,
  checks `prev_record_hash` against the head (compare-and-append), appends, fsyncs. Same
  serialization pattern as the approval ledger, with these additions:
  - **Idempotent retries.** IDs are deterministic. If a record with the same `id` and
    identical bytes is already present, the writer returns it instead of failing; same `id`
    with different bytes is rejected. A producer that crashed after the fsync can retry.
  - **Stale lock.** The lock file records the holder's PID, as `leased()` does. Node has no
    `flock`, so a killed writer leaves the lock behind; `eng runs unlock` removes it only
    after checking the PID is dead, the same manual rule as the local workflow's lease.
  - **Torn tail.** Before appending, the writer (holding the lock) checks the file ends in a
    newline. If not, it copies the unterminated bytes to `<project-id>.torn/<offset>` and
    truncates to the last newline. Those bytes never formed a record, so no record is
    edited; `verify` reports every quarantined tail.
- `reader.mjs`: walks the chain, validates each line, applies corrections.
- CLI subcommands of `harness/local/cli.mjs` (`npm run eng`), names provisional:
  `eng runs list <task-id>`, `eng runs show <job-id>`, `eng runs verify --store <path>`,
  `eng runs export [--task <id>] --out <path>` (export header records `local-only`),
  `eng runs reconcile --run <run-dir>`, `eng runs unlock`.

**Producer write order and failure.** The run ledger has no authority, so a ledger failure
never fails a run. The producer saves `run.json` first, then appends. Records of one job are
appended **strictly in order**: once an append for a job fails, the producer appends nothing
more for that job, so no later `decision` or `job_end` can land ahead of a missing one.
`reconcile` derives the records a run should have from its `run.json`, finds the first one
missing from the ledger, and appends from there in order; deterministic IDs make it safe to
run repeatedly. Because nothing later was written, the open-job and `step_index` rules
still hold and no backfill exception is needed. It never invents a `job_end` for a run still in
`running` or `awaiting-decision`; those stay open in `list`. For a `running` run whose
controller is gone, `reconcile --confirm-dead` (after the operator has checked the PID, as
for the lease) writes `job_end` with `infrastructure_failure` and `local_run_state:
"running"`, matching the workflow's rule that interrupted runs are never replayed.
Contention on the per-project file: a stale-head rejection is retried by the producer up to
a fixed count, then left to `reconcile`.

Session-hook events (interactive Claude Code / Codex plugin use) are written by hooks to a
spool `<local-artifacts-directory>/run-events/` and appended by the writer as
`provenance: "session-hook"`. The hook never touches the ledger file.

## Relation to the approval ledger

| | Approval ledger | Run ledger |
|---|---|---|
| Answers | Who authorized which transition | What a job did and what happened |
| Authority | Yes; `canTransition` reads it | None; never read by `canTransition` |
| Integrity | Hash chain, signed, remotely anchored | Hash chain only |
| Writer | Ledger writer for human / policy actors | Run-ledger writer for the control plane |

A run-ledger export may be attached to an approval as evidence, but only through the
evidence store (by digest), like any other file.

## Dependencies

- `harness/policy/transitions.ts` (`TaskState`, `EvidenceRef` types).
- Local evidence workflow (`harness/local/`): first producer, and its run store holds the
  evidence the records reference.
- A structured `failureCode` in the local workflow's run manifest (task T2b); today it records
  only `e.message`.
- `harness/local/artifacts.mjs` helpers (`digest`, `atomic`, `trustedPath`) for hashing,
  fsynced writes and trust-path checks; reuse rather than duplicate.
- Node 24 and the existing `npm test` / `npm run check` scripts. No new packages.

## Migration plan

No migration of existing data: the run ledger is a new store and nothing is persisted
today. Future schema changes follow these rules, checked at review:

- **Expand / contract:** minor versions (`run-ledger/v1.N`) only add optional fields;
  readers validate each record against its own declared version. Removing or
  retyping a field is a new major version, and readers keep accepting the old one.
- **Ordering vs deploy:** readers ship before writers emit a new version.
- **Backward compatibility:** the reader test suite keeps fixture files for every
  released version.
- **Rollback limits:** records written in a newer version stay in the file after a
  rollback; an older reader reports them as unknown-version and `eng runs verify` fails
  until the newer reader is restored. No data is rewritten.

## Rollout and rollback

- **Rollout:** ship writer, reader and CLI; enable the local workflow's writer with an
  optional `runLedger` path in the operator config (`config/local-workflow.example.json`
  shape; absent means off, so existing configs keep working); then enable session-hook
  spooling separately.
- **Rollback:** remove `runLedger` from the operator config. The file stays; nothing else reads it.
