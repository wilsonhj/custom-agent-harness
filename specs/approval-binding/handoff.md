# Handoff: approval-binding / packet drafted (no code changed)

| Field | Value |
|---|---|
| Base SHA | 0b9d58f7dfb1bf8a05b8726921628b56e8d63261 |
| Head SHA | uncommitted (branch `specs/follow-up-packets`) |
| Author (runtime / session ID) | Claude Code / session_01AaDiMLJT1xSNqousxB9YPS |
| Date | 2026-10-28 |

## Changed files

- `specs/approval-binding/*` — new packet (this one)
- `specs/docs-alignment/*` — sibling packet for the doc and template follow-up

## Implemented requirements

- None. This session drafted the packet only; no implementation task has started.

## Tests run and results

Claims from the PR #1 review were re-checked on 2026-10-27 against `main` (`0b9d58f`),
Node v22.22.0, with a scratch script outside the repository:

| Check | Result |
|---|---|
| Documented four-line output | matches |
| Sweep of all state pairs, with/without a forged record | 46 edges; 9 `ok:true` pairs, all `to = Blocked`; `approval` never changed a result |
| `Implementing -> Blocked` with a record whose `to_state` is `Released` | `{ok:true}` (record ignored) |
| `Blocked -> MigrationGated` with `blockedFrom = Implementing` / none | "not allowed" |
| `Blocked -> MigrationGated` with `blockedFrom = MigrationGated` | "approval policy not implemented" |
| Unknown `from` | throws `Cannot read properties of undefined (reading 'includes')` |
| Repository has `package.json` / `.github` / test file | none of them |

No test in this packet has been run, because the test file does not exist yet.

## Known limitations

- The review's Node v22.23.4 could not be reproduced; this environment has v22.22.0.
- No Codex review of this packet has been run.

## Unresolved decisions

- The four open questions in `requirements.md` — owner: repo owner.

## Assumptions made

- "Verified ledger line" is modelled as a branded type the future verifier constructs,
  because `canTransition` is a pure function and signature verification is Phase 1 ledger
  work, not in this repository yet.
- The unknown-state throw (a Low item in the review) is included here because it is in
  the same function; drop T2's second half if you want a smaller change.
- The user asked for the review's follow-ups to be split into two items; the issues could
  not be created with this token (403 on the REST and GraphQL issue APIs), so they need
  to be filed manually.

## Exact next action

Repo owner answers the open questions in `requirements.md`, then requests the
`Planned -> Approved` transition for this packet through the approval ledger (agents may
only request it). Before that, run `/codex:adversarial-review` on this packet and record
the findings here.
