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
| `prepare` completes | `job_start` (`runtime.name: "local-command"`; `controller.kind: "static"`, `config_digest` = the run's pinned `configDigest`) |
| Worker exits, controller records the candidate commit | `decision` (`selected_action: "verify"`) |
| Verifier exits | `decision` (`request_review` on exit 0, otherwise `stop`) |
| Reviewer verdict recorded | `decision` (`escalate` to the human decision) |
| Run reaches `failed` or `decision-recorded` | `job_end` |

The pilot has no repair loop, so `repair` never appears in its records yet. Run states stay
local execution facts: the run ledger copies them as `local_run_state`, never as `TaskState`.
Evidence digests in these records resolve in that run's store (`<store>/<run-id>/objects/`).

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
| `id` | string (UUID, `crypto.randomUUID`) | Same ID scheme as local-workflow runs; no dependency. |
| `kind` | `"job_start"` \| `"decision"` \| `"job_end"` \| `"correction"` | |
| `project_id`, `task_id`, `job_id` | string | `task_id` = `specs/<id>/`. |
| `provenance` | `"control-plane"` \| `"session-hook"` | Who observed the event. Only `control-plane` records can carry outcomes (see Validation). |
| `timestamp` | ISO 8601 UTC | Informational. |
| `prev_record_hash` | SHA-256 hex | Of the previous line's exact bytes; 64 zeros first. Tamper *evidence* only (no anchor, no signature). |

### `job_start`

| Field | Type |
|---|---|
| `runtime` | `{ name: "local-command" \| "claude-code" \| "codex-cli" \| "cursor-agent", version }`. For `local-command` (the local workflow's worker), `version` is the SHA-256 of the executable, which the controller already pins. |
| `adapter` | `{ name, version }` (e.g. `codex-plugin`, `1.0.6`) |
| `model_id` | string \| null (as the runtime reports it; never inferred) |
| `skills` | `{ name, version }[]` |
| `permission_profile` | `"analysis"` \| `"workspace"` \| `"preview-ops"` \| `"release"` |
| `egress_ceiling` | egress class the session was allowed to read |
| `session_id` | string (explicit, as recorded by the adapter) |
| `worktree_id`, `base_sha` | string |
| `harness_sha` | commit of this repository the control plane ran from |
| `knowledge_revision` | string \| null |
| `task_family` | string \| null (taxonomy is an open question) |
| `task_state_at_start` | `TaskState`, read from the verified approval ledger |
| `controller` | `{ kind: "static" \| "learned", version, config_digest: { sha256 } }` |
| `budget` | `{ deadline_seconds, max_repair_rounds, max_cost_usd: number \| null }` |

### `decision`

| Field | Type | Notes |
|---|---|---|
| `step_index` | integer, from 0 | Strictly increasing per job. |
| `observation` | object, fixed fields only | `phase` (`TaskState`), `repair_rounds_used`, `elapsed_seconds`, `budget_remaining_seconds`, `cost_so_far_usd` (number \| null), `head_sha`, `last_verifier` (`{ name, result: "pass" \| "fail" \| "error" \| "not_run", blocking_findings, nonblocking_findings }` \| null), `tool_error_count` (integer \| null) |
| `available_actions` | enum[] | Subset of `continue`, `request_review`, `repair`, `verify`, `stop`, `escalate`. |
| `selected_action` | enum | Must be in `available_actions`. |
| `decided_by` | `"controller"` \| `"human"` | A human override is logged, not hidden. |
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
| `outcome_components` | `{ wall_seconds, cost_usd, repair_rounds, review_rounds, blocking_findings_open, policy_violations }` | Raw components. **No combined reward scalar** is stored; weighting is an analysis decision. |
| `usage_total` | `{ input_tokens, output_tokens, cost_usd }` | Each number \| null. |
| `approval_record_ids` | string[] | IDs of approval-ledger records written during the job; references only. |
| `local_run_state` | `"failed"` \| `"decision-recorded"` \| null | Final local-workflow run state, for jobs it produced. Not a `TaskState`. |

### `correction`

`{ corrects: <record id>, reason_code: enum, replacement: <full record of the same kind> }`.
Readers use the latest correction for a record. `reason_code` is an enum
(`misclassified_termination`, `writer_bug`, `late_usage_report`), not text.

### Termination classification

Applied by the control plane, in this order (first match wins):

1. A policy denial or managed-hook block of a prohibited action → `policy_violation`.
2. Sandbox, dependency, runtime crash, network, or verifier malfunction (verifier result
   `error`) → `infrastructure_failure`.
3. Operator or upstream cancel → `cancelled`.
4. Acceptance commands all pass on `head_sha` → `succeeded`, even if that was the last
   allowed repair round or the budget is now used up.
5. Acceptance did not pass **and** the controller could not take the next needed action
   (another repair, review or verify) because `deadline_seconds`, `max_repair_rounds` or
   `max_cost_usd` forbade it → `budget_exhausted`. Reaching a limit is not enough on its
   own; the limit must have blocked a needed step.
6. Otherwise → `task_failure`.

`infrastructure_failure` and `policy_violation` jobs are kept for operations but flagged
for exclusion from any outcome comparison.

### Validation (writer rejects the record if any fails)

- JSON Schema for the declared `schema_version`, `additionalProperties: false` at every level.
- String length cap per field type (IDs, versions, SHAs); no field accepts arbitrary text.
- `selected_action ∈ available_actions`; `step_index` follows the job's last one.
- `decision` and `job_end` reference an existing open `job_start`; one `job_end` per job.
- `provenance: "session-hook"` records may not carry `outcome`, `verifier` results, or
  `termination_reason: "succeeded"`.
- Every `evidence_refs` digest resolves in the job's evidence store: `<store>/<run-id>/objects/` for local-workflow jobs (see requirements "Unresolved questions").

### Writer, reader, CLI

New module `harness/ledger/`, Node 24 ESM `.mjs` with no npm dependencies, matching
`harness/local/` (T1 adds the directory to `scripts/check-local.mjs`):

- `run-record.mjs`: the v1 schema as data plus a hand-written validator (no JSON Schema
  package, since the repo takes no dependencies). `TaskState` values are imported from
  `harness/policy/transitions.ts` only in tests, to check the enum stays in sync.
- `writer.mjs`: validates, takes an exclusive lock on `<run-ledger-root>/<project-id>.lock`,
  checks `prev_record_hash` against the head (compare-and-append), appends, fsyncs. Same
  serialization pattern as the approval ledger.
- `reader.mjs`: walks the chain, validates each line, applies corrections.
- CLI subcommands of `harness/local/cli.mjs` (`npm run eng`), names provisional:
  `eng runs list <task-id>`, `eng runs show <job-id>`, `eng runs verify`,
  `eng runs export [--task <id>] --out <path>` (export header records the egress class).

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
