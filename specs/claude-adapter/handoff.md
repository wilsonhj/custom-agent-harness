# Handoff: claude-adapter / packet creation

| Field | Value |
|---|---|
| Base SHA | 30fc4e4 (adapter-interface T2 decisions recorded) |
| Head SHA | 30fc4e4 (this packet lands in the next commit on this branch) |
| Author (runtime / session ID) | Claude Code / session_01AaDiMJVSVxQVfoGPARPgBn |
| Date | 2026-11-05 |

## Changed files

- `specs/claude-adapter/*` — new packet for the first runtime adapter (adapter-interface T2 decision D5).

## Implemented requirements

- None. Packet only; no code.

## Tests run and results

| Command | Result | Evidence |
|---|---|---|
| `claude --version`, `claude --help` | 2.1.318; flags recorded under "Claude evidence by requirement" | terminal output of this session |

## Review: Codex PR review bot (PR #10), 2026-11-05

| Finding | Resolution |
|---|---|
| P2: `-p --output-format stream-json` needs `--verbose` | Verified on 2.1.318 (the CLI refuses without it); added to C-1, design argv and acceptance test in 8882c9f. Re-review of 8882c9f: no findings. |

## Known limitations

- Claude facts come from the installed CLI's help text only; no live run was made (it needs an API key and spends budget).
- The bubblewrap sandbox has no network; C-6 needs an egress proxy that does not exist yet.
- No independent review yet.

## Unresolved decisions

- Anthropic account/key, BAA and zero-retention status, version pin — owner: human, in T3.

## Assumptions made

- Packet ID `claude-adapter` follows adapter-interface decision D5.
- `--bare` plus `--restricted` is assumed to be the right default for unattended runs; T1 confirms how they combine.

## Exact next action

Run T1: settle the UNVERIFIED items listed in requirements.md from the installed CLI and Claude Code documentation.
