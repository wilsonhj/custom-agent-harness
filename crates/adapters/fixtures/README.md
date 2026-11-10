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
- UNVERIFIED: the wording of a usage-limit message (the parser matches "usage limit",
  "rate limit", "quota" case-insensitively), and the exit code on `turn.failed`.

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
- `codex/review_ok.jsonl`, `codex/review_not_schema.jsonl`, `codex/limit_message.jsonl`.
