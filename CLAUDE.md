@AGENTS.md

# Claude-specific guidance

- Plan substantial changes against the feature packet before editing; keep the packet current.
- For independent review, use the installed codex plugin: `/codex:review` for a standard
  review and `/codex:adversarial-review` to challenge a plan or diff. Both run Codex in a
  read-only sandbox and return structured output. There is no custom Codex adapter.
- Record Codex findings and their resolutions in the packet's `handoff.md`.
- Do not write to the approval ledger directory, even if a tool call would allow it.
