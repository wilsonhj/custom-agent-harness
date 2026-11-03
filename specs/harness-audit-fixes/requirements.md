# Requirements: harness-audit-fixes — Fix verified state-machine and documentation defects

<!-- Line references are to base commit 0b9d58f unless noted. -->

## User-visible behavior

- `canTransition` resumes a `Blocked` task only when given the ledger record that entered
  `Blocked` (typed `BlockedRecord`, carrying `from_state` and `target.commit_sha`) **and**
  the current commit, and only when `to === record.from_state` and
  `currentCommitSha === record.target.commit_sha`. Any other resume attempt (no record,
  empty or malformed record, wrong state, different commit, no current commit) is denied.
  Covers audit item 1; replaces the bare `blockedFrom` argument (`transitions.ts` L88-97).
- `canTransition` rejects an approval whose `from_state` or `to_state` differs from the
  arguments, and rejects a missing or malformed approval where one is needed.
- `canTransition` stays fail-closed: every transition except entering `Blocked` is still
  denied with "approval policy not implemented" (`transitions.ts` L107-111). The human-vs-policy
  actor table is NOT decided here (see Unresolved questions).
- `ALLOWED_TRANSITIONS` gains `Failed` edges from `Verified`, `PreviewVerified` and
  `MigrationGated` (audit item 2; `docs/architecture/release-gate.md` L77-79 says a failed
  release is recorded as `Failed`).
- `Released` and `Observed` may enter `Blocked` (README L529 "Close or escalate"), and
  `Implementing` and `Repair` may enter `Superseded` (audit item 3).
- A test enumerates every state pair and checks reachability from `Draft`, that only
  `Closed`, `Cancelled` and `Superseded` are dead ends, and that `Released` is unreachable
  without passing through `MigrationGated`.
- Terminology: `untrusted_origin` everywhere (spelling of `docs/security/data-egress.md` L141);
  README state names in the successful path match `TaskState` (audit item 4).
- README no longer contradicts itself on when the release decision is enforced (README L688
  vs L707-709) and no longer marks the Codex review as done while the BAA allowlist and
  managed configuration are placeholders (README L679 vs `docs/security/data-egress.md`
  L38-62, L148-154) (audit item 5).

## Non-goals

- Deciding which transitions need a `human` vs `policy` actor, or checking `actor`/`target`
  of an approval (the `TODO(human)` at `transitions.ts` L107-108 stays).
- Resolving any design question listed under "Unresolved questions".
- Implementing the ledger reader, signature verification, or the anchor check.
- Changing the ledger or release-gate design beyond keeping text consistent with the code.

## Invariants

- `canTransition` never returns `ok: true` except for entering `Blocked`; agents cannot
  approve their own transitions (AGENTS.md "Approvals and state").
- A `Blocked` task cannot reach a state it did not enter from, nor skip review, preview or the
  migration gate (comment at `transitions.ts` L51-53 and `ALLOWED_TRANSITIONS.Blocked`).
- Nothing in this packet writes to the approval ledger directory or records an approval.

## Cited company decisions

| Constraint | Source (ADR / policy / brain doc ID @ revision) |
|---|---|
| Agents never mark phase transitions approved; only request them | AGENTS.md "Approvals and state" |
| Blocked task resumes only to the state it was blocked from; value comes from the verified ledger, never the agent's request | `docs/architecture/approval-ledger.md` L154-156 |
| Approval binds to the exact commit | `docs/architecture/approval-ledger.md` L30, L37-38 |
| Failed release is recorded as a transition to `Failed` or `Repair` | `docs/architecture/release-gate.md` L77-79 |
| `MigrationGated` is on every release path; `Released` needs it | `docs/architecture/release-gate.md` L8-15 |
| Spelling `untrusted_origin` | `docs/security/data-egress.md` L141 |
| BAA allowlist and managed config are placeholders / open | `docs/security/data-egress.md` L38-62, L148-154 |
| Phase 3 is when production deploys are confined to the release job | README L706-709 |

## Unresolved questions

Each is audit-verified and needs a human decision. None is resolved or worked around by this
packet. "Candidate option" is this author's suggestion; the original audit text was not
available to the session that wrote this packet, so these are not a quotation of the audit.

- [ ] **Circular release-gate ordering.** `MigrationGated -> Released` requires the production
  migration to be already applied as evidence (`release-gate.md` L65-68), but running a
  production migration is itself a production write that no ledger record approves
  (the only gate record before it is `PreviewVerified -> MigrationGated`, L34-43; the ledger
  `target.deployment_target` is only named at L69-70). Candidate option: split approval so a
  human record authorizes "run migration M against deployment_target T at commit C" (new
  transition or state between `MigrationGated` and `Released`), and require the apply log as
  evidence only for the following `-> Released` record. — owner: human, blocks: none
- [ ] **`allowed_signers` trust root lives in the anchor repository.** The file that decides
  which keys are valid sits in the same repository whose commits it authenticates
  (`approval-ledger.md` L66-81, L107-113), so whoever controls the anchor repo controls the
  trust root. Candidate option: pin the trust root outside it (root-owned file or key
  fingerprint configured on the verifying host), and accept anchor-repo changes only when
  signed by an already-pinned key. — owner: human, blocks: none
- [ ] **Commit-time clock.** The anchor commit time is the `verify-time` for signatures
  (`approval-ledger.md` L73-75, L115-117), but a git commit date is writer-supplied.
  Candidate option: take the time from the remote's push receipt (forge API / event
  timestamp) or add the RFC 3161 timestamp already listed as optional (L177). — owner: human,
  blocks: none
- [ ] **Confused-deputy policy actor and classifier in the repo under change.** The policy
  actor (`approval-ledger.md` L105-106, L133) and the migration classifier
  (`release-gate.md` L50-63) are code in, or configured by, the repository whose change they
  approve; `policy_digest` is "the `harness/policy/` module at that commit" (L32), so a
  candidate commit can rewrite its own judge. Candidate option: evaluate with a pinned,
  separately reviewed policy and classifier revision (not the candidate commit) and record
  that revision's digest. — owner: human, blocks: none
- [ ] **Agent-writable egress class storage.** Egress class is recorded in company-brain
  `egress_class` fields and `config/harness.local.yaml` (`data-egress.md` L126-131), which
  agent sessions can edit, while enforcement elsewhere relies on root-owned state
  (L72-94). An agent could widen its own class. Candidate option: keep classes in the
  root-owned `<egress-profile-file>` (or a root-owned manifest) and let agents only submit
  narrowing requests. — owner: human, blocks: none
- [ ] **Production logs required as evidence but classed local-only.** Release evidence
  requires time-bounded logs and metrics (README L568; `release-gate.md` L71), while
  production logs are `local-only` (`data-egress.md` L24) and cannot go to hosted
  reviewers. Candidate option: define a redacted, bounded log summary as a derived artifact
  with an explicit sanitization review (ties to the redaction question at `data-egress.md`
  L155-156), or accept digest-only references reviewed in a local-only session. — owner:
  human, blocks: none
- [ ] **Human vs policy actor table.** Which transitions need `human` and which a `policy`
  actor may approve (`transitions.ts` L107-108; `approval-ledger.md` L171-173;
  `release-gate.md` L129-131). Stub and fail-closed behaviour are kept unchanged. Candidate
  option: human for `Planned -> Approved`, `Verified -> PreviewVerified`,
  `MigrationGated -> Released` and any `uncertain`/`migration` classifier result; policy only
  where deterministic evidence is complete. — owner: human, blocks: none

## Assumptions

- Entering `Blocked` is recorded in the ledger as a record with `to_state: "Blocked"` and the
  prior state in `from_state` (`approval-ledger.md` L17-18, L154-156), so a `BlockedRecord`
  is a normal `ApprovalRecord` narrowed to that shape.
- `Released` and `Observed` are resumable from `Blocked` (back to the same state, same commit);
  otherwise an escalated released task could only be cancelled or superseded.
