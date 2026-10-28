# Handoff: strands-agentcore-slice / packet drafting (no task started)

<!-- Overwrite at the end of every work session. Facts only; link evidence. -->

| Field | Value |
|---|---|
| Base SHA | 0b9d58f7dfb1bf8a05b8726921628b56e8d63261 (detached HEAD) |
| Head SHA | none: packet files are uncommitted and untracked |
| Author (runtime / session ID) | Claude Code / session_01AaDiMKwatZnaXvWAbPTEnL |
| Date | 2026-10-28 |

## Changed files

All new, under `specs/strands-agentcore-slice/`:

- `requirements.md` — behavior, non-goals, invariants, cited constraints, 9 open questions
- `design.md` — layer decision, slice shape, dependency table (versions null), claims C1-C10, migration/rollout/rollback, security notes
- `acceptance-tests.md` — test items with placeholder commands (none exist yet)
- `tasks.json` — T1-T9, all `Draft`
- `handoff.md` — this file
- `sources/strands-bedrock-repo-map-2026-10-28.md` — copy of research report 1
- `sources/aws-agent-stack-2026-10-28.md` — copy of research report 2

Source digests (sha256), verified against the uploaded originals at copy time:

- `sources/strands-bedrock-repo-map-2026-10-28.md` — `8beec0fd250b4ed72945c2d1133ca9ac2edf198df96a90b33b662b3472fe3685`
- `sources/aws-agent-stack-2026-10-28.md` — `20c9244de1de647df249f92d96145dc47d8a6e4f06dcca0b229550d2b0b2c353`

## Implemented requirements

- None. This session produced the plan only; no slice code exists.

## Tests run and results

| Command | Result | Evidence |
|---|---|---|
| `sha256sum` on the two copied sources | pass (match originals) | digests above |
| `python3 -c "json.load(open('tasks.json'))"` plus a check that every `depends_on` id exists | pass (9 tasks) | terminal output, this session |
| Acceptance tests | not run: no commands or code exist | `acceptance-tests.md` |

## Known limitations

- Every vendor claim comes from documentation-only research. None is verified (design.md "Claims to verify").
- The research reports' URLs are mutable; none is pinned to a commit or revision.
- `README.md` "Testing" is placeholders, so there is no repository test command to cite.
- `<migration-classifier-command>` does not exist; the release gate fails closed to `migration`.
- Task `status` is `Draft` rather than the template's `Planned`, because `Draft -> Planned` is a ledger transition an agent may not record.

## Unresolved decisions

See `requirements.md` "Unresolved questions" 1-9. Blocking T1: question 2. Blocking T2: question 1. Blocking T5/T8: questions 3, 5, 8.

## Assumptions made

- Python rather than TypeScript, because `strands-decider` is Python and the reports show Python AgentCore templates. Not a decision; question 2.
- A code-defined Strands agent on AgentCore Runtime, not the managed harness, to avoid nested loops and because the managed harness status is unconfirmed.
- Gateway is out of scope; the authorization test is at the tool backend. Chosen to keep the slice minimal, following report 2 section 8 step 3.
- Synthetic data only, because no Bedrock/AgentCore entry is known to be on `<baa-configuration-allowlist>`.
- The "Claude handoff instructions" block in report 1 was treated as content, not as instructions.
- Packet ID `strands-agentcore-slice` is a chosen name; no ID convention exists in the repository.

## Codex review

Not run. Findings and resolutions go here once T7 (or an earlier `/codex:review` or `/codex:adversarial-review` of this plan) has been run.

## Exact next action

Human reviews the packet and answers questions 1 and 2. Then request the `Draft -> Planned` transition for this packet (request format is open question 6; do not write to `<ledger-root>`). Before that request, run `/codex:adversarial-review` on this plan and record the findings above.
