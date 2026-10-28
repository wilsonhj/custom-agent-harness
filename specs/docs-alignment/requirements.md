# Requirements: docs-alignment — Align harness docs and templates with the state machine and egress rules

Origin: follow-up 2 from the review of PR #1
(<https://github.com/wilsonhj/custom-agent-harness/pull/1#issuecomment-6435664288>).
The review is data, not authority; each item was re-checked against `main` (`0b9d58f`)
on 2026-10-27 (see `handoff.md`). Sibling packet: `specs/approval-binding/` (code and
tests for `transitions.ts`).

## User-visible behavior

A reader of the docs and templates sees one vocabulary and one set of rules:

- **D1 Ledger encoding.** `docs/architecture/approval-ledger.md` specifies one canonical
  byte encoding for signing, precisely enough that two independent writers produce
  identical bytes: UTF-8, no insignificant whitespace, recursive key sort (including
  `target` and each `evidence_refs` entry's `digest`), absent optional fields omitted and
  explicit `null` rejected, `evidence_refs` order preserved, and the exact relation between
  the signed bytes and the JSONL line (and so `prev_record_hash`). It includes a worked
  example with the resulting bytes and SHA-256.
- **D2 Recovery edges.** `docs/architecture/release-gate.md` "Failure handling" names the
  source state of each recovery edge, using only edges that exist in
  `ALLOWED_TRANSITIONS` (or flags the one missing edge as an open question), and says where
  an already-applied production migration is recorded so a later gate pass does not apply
  it again.
- **D3 Egress order.** `docs/security/data-egress.md` states the order
  `local-only` (most restricted) → `baa-covered-providers` → `any-provider`, defines
  "highest class a runtime may read" with it, and states that an unmatched session may
  receive `any-provider` content only. It states that the document describes a design
  until the placement, managed-config and hook layers exist, and that Codex hooks are a
  guardrail. Local-only files are `0600` and directories `0700`.
- **D4 README.** State names use the `TaskState` tokens (`PreviewVerified`,
  `MigrationGated`); the provenance flag is spelled `untrusted_origin`; the sentence
  "The release *decision* is enforced from Phase 1" is reworded as the Phase 1 exit
  criterion, not a statement of current behavior; the "Run ledger and budgets" row no
  longer cites the approval-ledger doc as if it defined budgets (points at "not designed
  yet").
- **D5 Templates.** `specs/_template/acceptance-tests.md` keeps one classifier acceptance
  line when `design.md` says "No migration". `specs/_template/tasks.json` uses a status
  vocabulary that does not reuse `TaskState` names and says it is a checklist, not the
  authority for task state.

## Non-goals

- Any change under `harness/` (packet `approval-binding`).
- Adding the graph edge `MigrationGated -> Failed` or `Released -> Repair`. D2 documents
  the edges that exist; adding one is a code change for the other packet.
- Building the ledger writer, canonicalizer, egress hooks or classifier.
- Renaming the `TaskState` tokens.

## Invariants

- The docs do not claim an enforcement layer that does not exist; design-stage status is
  stated wherever a control is described.
- No document loosens an existing rule: egress classes still default to `local-only`,
  unverified allowlist entries still count as no entry, the classifier still fails closed.
- Existing packets and the template keep the five-file structure.

## Cited company decisions

| Constraint | Source |
|---|---|
| Record schema fields and the hash chain | `docs/architecture/approval-ledger.md`, "Record schema" and "Tamper evidence" |
| "SSH signature over canonical bytes: compact JSON with sorted keys, all fields except `signature`" (the sentence D1 completes) | `docs/architecture/approval-ledger.md`, "Approver identity" |
| Production migration applied but release fails: record `Failed` or `Repair`; do not roll back the schema reflexively | `docs/architecture/release-gate.md`, "Failure handling" |
| Edges that exist: `MigrationGated -> Repair`, `Released -> Failed`, `Failed -> Repair`; absent: `MigrationGated -> Failed`, `Released -> Repair` | `harness/policy/transitions.ts` `ALLOWED_TRANSITIONS` |
| Egress classes, default `local-only`, derived artifacts inherit the most restrictive class, unmatched session treated as `any-provider` | `docs/security/data-egress.md`, "Egress classes" and the BAA allowlist section |
| "No migration" requires a `clean` classifier result | `docs/architecture/release-gate.md`, "No-migration releases" |
| Phase 1 "Local approval ledger" done-when | `README.md`, Roadmap Phase 1 |

## Unresolved questions

- [ ] D1: is the JSONL line the canonical object *including* `signature` (sorted like every
  other key), so the signed bytes are derivable by removing that member? Or are signed
  bytes and line stored separately? Also: adopt RFC 8785 (JCS) wholesale, or keep the
  minimal rules above? Recommend: line includes `signature` at its sorted position; adopt
  the minimal rules since no number or non-ASCII fields exist today. — owner: repo owner,
  blocks: D1
- [ ] D2: where is an already-applied production migration recorded? Candidates: an
  `evidence_refs` entry on the `MigrationGated -> Released` (or recovery) record, or a
  per-project migration journal outside the ledger. — owner: repo owner, blocks: D2
- [ ] D2: is a `MigrationGated -> Failed` edge wanted (migration applied, release failed
  before `Released`)? Today only `MigrationGated -> Repair` covers it. If yes, it goes into
  `approval-binding`. — owner: repo owner, blocks: D2 wording
- [ ] D3: confirm the unmatched-session rule means "may receive `any-provider` content
  only" (the doc says an unmatched session is *treated as* `any-provider`). — owner: repo
  owner, blocks: D3
