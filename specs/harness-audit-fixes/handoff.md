# Handoff: harness-audit-fixes / T1-T5

| Field | Value |
|---|---|
| Base SHA | 0b9d58f |
| Head SHA | uncommitted working tree on top of 0b9d58f (not committed, per instruction) |
| Author (runtime / session ID) | Claude Code (Claude Sonnet 5.5) / session_01AaDiMLB7jVNUA9MfagD1Te (sub-agent) |
| Date | 2026-10-28 |

## Changed files

- `harness/policy/transitions.ts` — `BlockedRecord`/`TransitionContext` replace bare `blockedFrom`; resume pinned to record `from_state` and `target.commit_sha`; approval `from_state`/`to_state` must match; missing/empty/malformed records denied; new edges (`Verified|PreviewVerified|MigrationGated -> Failed`, `Released|Observed -> Blocked`, `Implementing|Repair -> Superseded`); `RESUMABLE_STATES` replaces `ACTIVE`. Actor table `TODO(human)` and fail-closed default kept.
- `tests/transitions.test.ts` — new, 26 tests (pair table, reachability, dead ends, Released-needs-MigrationGated, resume, approval shape, fail closed).
- `docs/architecture/approval-ledger.md` — "Reading the ledger" describes record + commit-pinned resume.
- `docs/architecture/release-gate.md` — "Failure handling" lists which states can record `Failed`.
- `docs/security/data-egress.md` — prose spelling `untrusted_origin` (L141-142).
- `README.md` — `untrusted_origin`; state names match `TaskState`; escalate wording; Codex review unticked and reworded; release-decision wording reconciled with Phase 3; Testing section now has the real unit-test command.
- `specs/harness-audit-fixes/*` — this packet.

## Implemented requirements

- Commit-pinned, record-based resume; approval from/to match; missing/empty record rejection — T2
- Failed, Blocked, Superseded edges — T3
- Edge/reachability tests — T4
- Docs consistency and terminology, README overclaims — T5

## Tests run and results

| Command | Result | Evidence |
|---|---|---|
| `node --experimental-strip-types --test tests/*.test.ts` | pass: 26 tests, 5 suites, 0 fail | terminal output of this session |
| Mutation check (scratch copy: drop commit pin / drop `MigrationGated -> Failed` / drop approval match / add `Verified -> MigrationGated`) | each made the suite fail (1, 3, 2, 2 failures) | scratchpad, deleted afterwards |
| `grep` for `untrusted-origin`, `Preview verified`, `Migration gated`, `[x]`, `enforced from Phase 1` in README.md and docs | no matches | terminal output |

## Known limitations

- No independent review has been run (T6 is Planned). `/codex:review` was not invoked from this session.
- `canTransition` is still a fail-closed stub for everything except entering `Blocked`; nothing can be approved until the actor table exists.
- The ledger reader and signature/chain verification do not exist; `BlockedRecord` shape checks do not prove the record came from a verified ledger. The control plane must supply it only from the verified ledger.
- `currentCommitSha` is trusted as supplied by the control plane; the approval's own `target.commit_sha` is not compared with it (part of the `TODO(human)` actor/target policy).
- The test command takes a file glob (`tests/*.test.ts`); Node 22 does not accept a bare directory.

## Unresolved decisions

All recorded with doc line refs and a candidate option in `requirements.md` "Unresolved questions". Not resolved, no design docs edited to resolve them. — owner: human
1. Circular release-gate ordering (`MigrationGated -> Released` needs the production migration, an unapproved production write).
2. `allowed_signers` trust root lives in the anchor repository.
3. Commit-time clock used as `verify-time`.
4. Confused-deputy policy actor/classifier in the repo under change.
5. Agent-writable egress class storage.
6. Production logs required as release evidence but classed `local-only`.
7. Human vs policy actor table (`TODO(human)` in `transitions.ts`).

## Assumptions made

- Entering `Blocked` is a ledger record with `to_state: "Blocked"` (ledger L17-18, L154-156), so `BlockedRecord` is a narrowed `ApprovalRecord`.
- `Released` and `Observed` are resumable from `Blocked` (same state, same commit), so an escalated released task is not limited to cancel/supersede. Change `RESUMABLE_STATES` if a human decides otherwise.
- The old `blockedFrom` parameter had no callers in this repository, so the signature change is breaking only for the not-yet-written control plane.
- The "audit's suggested option" text was not available to this session; options in `requirements.md` are labelled as this author's candidate options.
- `tasks.json` statuses are self-reported progress, not approvals; no approval was requested or recorded.

## Exact next action

Run `/codex:review` on the working-tree diff of `README.md`, `docs/`, `harness/policy/transitions.ts`, `tests/`, `specs/harness-audit-fixes/` (use `/codex:adversarial-review` on the resume logic), record findings and resolutions in this file, then a human decides the seven unresolved items and requests the `Implementing -> Reviewing` transition through the external ledger.
