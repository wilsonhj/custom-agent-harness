# Handoff: kiro-adapter / packet drafting

| Field | Value |
|---|---|
| Base SHA | 0b9d58f7dfb1bf8a05b8726921628b56e8d63261 |
| Head SHA | the commit that adds this file (`git log -1 -- specs/kiro-adapter/handoff.md`) |
| Author (runtime / session ID) | Claude Code / session_01AaDiMJVSVxQVfoGPARPgBn |
| Date | 2026-10-28 |

## Changed files

- `specs/kiro-adapter/*` — new feature packet from `specs/_template/`; requirements K-1..K-13.

## Implemented requirements

- None. Packet drafting only; no adapter code exists.

## Tests run and results

| Command | Result | Evidence |
|---|---|---|
| (none) | not run | No test commands exist yet; see acceptance-tests.md |

## Known limitations

- Kiro-side facts are from web-search snippets only. Fetching https://kiro.dev/docs/cli/
  failed (`getaddrinfo ENOTFOUND`, then proxy 403 on CONNECT): kiro.dev is not in the
  session's allowed network domains. Every Kiro claim is marked UNVERIFIED.
- Snippets disagreed on agent file format (JSON vs Markdown) and carried differing
  "updated" dates (Nov 2025 to Aug 2026).
- `/codex:review` has not been run on this packet.

## Unresolved decisions

- Who owns the open questions in requirements.md — owner: user
- Whether Kiro is ever used for `baa-covered-providers` work — owner: user

## Assumptions made

- Packet ID `kiro-adapter` supplied by the user; scope limited to the Kiro CLI surface.
- Task owner roles and worktree name `kiro-adapter` in tasks.json are placeholders.
- A runtime with no verified managed configuration is treated as `any-provider`
  (follows data-egress.md "A session whose runtime, account and settings do not match
  an entry is treated as any-provider").

## Exact next action

Allow kiro.dev in the environment's network settings, then run T1: read
https://kiro.dev/docs/cli/ and update requirements.md. Afterwards run
`/codex:adversarial-review` on the packet and record findings here.
