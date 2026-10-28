# Handoff: docs-alignment / packet drafted (no docs changed)

| Field | Value |
|---|---|
| Base SHA | 0b9d58f7dfb1bf8a05b8726921628b56e8d63261 |
| Head SHA | uncommitted (branch `specs/follow-up-packets`) |
| Author (runtime / session ID) | Claude Code / session_01AaDiMLJT1xSNqousxB9YPS |
| Date | 2026-10-28 |

## Changed files

- `specs/docs-alignment/*` — new packet (this one)
- `specs/approval-binding/*` — sibling packet for `transitions.ts` and its tests

## Implemented requirements

- None. Packet only; no implementation task has started.

## Tests run and results

Claims from the PR #1 review were re-checked on 2026-10-27 against `main` (`0b9d58f`):

| Claim | Result |
|---|---|
| README diagram uses "Preview verified" / "Migration gated" (line ~511) | confirmed |
| README says `untrusted-origin` (~462); egress doc says `untrusted_origin: true` (~141) | confirmed |
| README ~688 "The release *decision* is enforced from Phase 1" | confirmed; sits right after the open Phase 1 checkbox |
| "Run ledger and budgets" row cites `approval-ledger.md` (~102) | confirmed; that doc has no budget design |
| Canonical bytes: only "compact JSON with sorted keys, all fields except `signature`" (`approval-ledger.md` ~97) | confirmed under-specified |
| `release-gate.md` ~79 "`Failed` or `Repair`"; `MigrationGated -> Failed` and `Released -> Repair` illegal | confirmed (ran `canTransition`) |
| Egress class order undefined; "highest class" at `data-egress.md` ~90 | confirmed order not defined |
| `data-egress.md` "mode 600" (~73) | confirmed |
| Template `acceptance-tests.md` says to delete the migration section under "No migration"; gate requires a `clean` classifier | confirmed |
| `tasks.json` reuses `"Planned"` | confirmed |

Not verified: that the egress doc has no other statement of an unmatched-session rule —
I found only the "treated as `any-provider`" sentence. No acceptance item in this packet
has been run, because the edits do not exist yet.

## Known limitations

- D1 and D2 depend on owner decisions (open questions in `requirements.md`).
- No Codex review of this packet has been run.

## Unresolved decisions

- The four open questions in `requirements.md` — owner: repo owner.

## Assumptions made

- Egress "order" is expressed by restrictiveness; a runtime allowed to read
  `baa-covered-providers` content may also read `any-provider` content, never `local-only`.
- The review's follow-ups are packaged as two packets (code vs docs) to match the two
  follow-up items; issues could not be created with this token (403 on REST and GraphQL),
  so they need to be filed manually.

## Exact next action

Repo owner answers the open questions in `requirements.md` (D3 and D4/D5 can proceed
without them), then requests `Planned -> Approved` through the approval ledger (agents may
only request it). Run `/codex:adversarial-review` on this packet first and record the
findings here.
