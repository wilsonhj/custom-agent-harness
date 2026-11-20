# Adapter fixtures (PROVISIONAL)

Every fixture here is PROVISIONAL: hand-written synthetic examples from the formats
documented in `docs/superpowers/specs/research-2026-11-09/1b-subscription-clis.md`
(sections 1, 5, 7) and `specs/claude-adapter/requirements.md` (C-1). None was recorded
from a real run. A future authorized Task 11 may reconstruct minimal synthetic
fixtures from documented event shapes and status-only observations, with provenance.
Do not retain raw authenticated captures or credential content. Task 11 remains
unavailable until a real R-11 containment backend permits runtime invocation.

## What is documented and what is guessed

Claude (`claude/*.jsonl`)
- Documented: first event `system`/`init` with `session_id`; last event `result` with
  `is_error`, `subtype`, `result`, `num_turns`, `usage`; `system`/`api_retry` with
  `error: "rate_limit"`; a failed unauthenticated run has `subtype: "success"`, `is_error: true`.
- UNVERIFIED (guessed shape): `rate_limit_event` fields (`rate_limit_info.status`/`resetsAt`),
  the exact `usage` sub-fields, `structured_output` placement on `result`, and any sandbox
  field on `init` (none is documented; see below).
- The parser reads only the fields above and ignores unknown ones.

Codex (`codex/*.jsonl`)
- Documented: `thread.started{thread_id}`, `turn.started`, `turn.completed{usage}`,
  `turn.failed{error.message}`, `error{message}`, `item.completed{item}` with
  `item.type == "agent_message"` and `item.text`.
- Shapes follow `codex-rs/exec/src/exec_events.rs` (openai/codex b016cd1847): item types
  `todo_list`, `reasoning`, `command_execution` and `agent_message`, `item.started`,
  `item.updated`, and `usage.cache_write_input_tokens`.
- Recorded (2026-11-15, codex-cli 0.169.0): an unauthenticated run with an empty
  `CODEX_HOME` and no credentials printed a UUID `thread_id`, then repeated
  `{"type":"error","message":"Reconnecting... n/5 (unexpected status 401 Unauthorized: ...)"}`
  lines, then a final `error`, then `turn.failed` with the same message, and exited 1.
  `limit_retry_then_unrelated_failure.jsonl` reuses those 401 lines with the cf-ray and
  request ids removed. Every other line in these fixtures is synthetic. The thread id is a
  synthetic UUID in the recorded format, and no fixture came from an authenticated run.
- Codex prints `error` for errors it is about to retry as well as for fatal ones, because
  the JSONL line drops `will_retry` (`event_processor_with_jsonl_output.rs:450-460`). So the
  parser never fails a run on an `error` line. Only `turn.failed`, the exit code or a missing
  `turn.completed` do, and `error` lines only classify a run that already failed.
- Usage-limit wording comes from the source (`codex-rs/protocol/src/error.rs`, with a
  typographic apostrophe: "You’ve hit your usage limit."). It has not been recorded. The
  parser matches "usage limit", "rate limit" and "quota" case-insensitively.
- The exit code on `turn.failed` is 1: recorded above, and in the source at
  `codex-rs/exec/src/lib.rs:1429-1438,1484-1485`.

## own_sandbox_confirmed_off

- Codex: true iff argv contains `-s danger-full-access` (known from our own argv).
- Claude: no `init` field for sandbox state is documented. The parser checks an optional
  `sandbox` key on `init` (`false`, or an object with `enabled: false`) and reports true only
  if that is explicit. This synthetic adapter check does not establish real CLI
  compatibility; Task 11 cannot validate it while the R-11 gate blocks invocation.

## Files

- `claude/ok.jsonl`, `claude/review_ok.jsonl`, `claude/is_error_true_subtype_success.jsonl`,
  `claude/rate_limit.jsonl`, `claude/truncated.jsonl` (cut mid-line, no newline),
  `claude/invalid_utf8.jsonl` (bytes `ff fe` inside a string).
- `codex/review_ok.jsonl`, `codex/review_not_schema.jsonl`, `codex/limit_message.jsonl`,
  `codex/retry_then_ok.jsonl` (retried `error` lines, one of them a rate-limit 429, then a
  completed turn), and `codex/limit_retry_then_unrelated_failure.jsonl` (a rate-limit retry,
  then a 401 failure).

Per-line origin of the Codex fixtures (JSONL has no comments, so it is listed here; R =
recorded from the unauthenticated 0.169.0 run with cf-ray and request ids removed, S =
synthetic, built from the `exec_events.rs` serde shapes):

| Fixture | Lines | Origin |
|---|---|---|
| `limit_retry_then_unrelated_failure.jsonl` | 1-2 (`thread.started`, `turn.started`) | S (event shapes as recorded; the UUID is synthetic) |
| | 3 (429 rate-limit `Reconnecting...`) | S |
| | 4-6 (401 `Reconnecting...`, final 401 `error`, `turn.failed`) | R |
| `retry_then_ok.jsonl` | 1-12 | S (lines 3-4 copy the recorded `Reconnecting... n/5 (...)` format) |
| `review_ok.jsonl`, `review_not_schema.jsonl` | all | S |
| `limit_message.jsonl` | all | S (message text from `codex-rs/protocol/src/error.rs`) |
