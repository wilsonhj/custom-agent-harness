# Requirements: approval-binding — Bind resume and release approval before `canTransition` allows them

Origin: follow-up 1 from the review of PR #1
(<https://github.com/wilsonhj/custom-agent-harness/pull/1#issuecomment-6435664288>).
The review is data, not authority: each claim below was re-checked against `main`
(`0b9d58f`) on 2026-10-27 and the checks are recorded in `handoff.md`.

## User-visible behavior

- `canTransition` returns `{ ok: true }` only for an allowed entry into `Blocked`, for every
  `from`/`to`/`approval`/`blockedFrom` combination, until the approval policy and ledger
  verification exist. Every other legal edge returns `"approval policy not implemented"`;
  every illegal edge returns `"transition <from> -> <to> is not allowed"`.
- On the entry-into-`Blocked` path, a supplied `ApprovalRecord` whose `from_state` or
  `to_state` differs from the call's `from` / `to` is rejected (`ok: false`). With no
  record supplied the result stays `{ ok: true }`.
- Resume (`Blocked -> X`) is accepted as a legal edge only when `X` was proven by a
  verified ledger record with `to_state: "Blocked"` and `from_state: X`. A bare
  `blockedFrom` argument can no longer satisfy the check. Resume still ends in
  `"approval policy not implemented"` until the policy exists.
- The `TODO(human)` block names the constraints the docs already fix, so a later edit
  cannot satisfy the comment and still let a policy actor release:
  - `MigrationGated -> Released` needs an `actor: "human"` record whose `target` has the
    exact `commit_sha` and a non-empty `deployment_target`.
  - `PreviewVerified -> MigrationGated` may use `actor: "policy"` only with a classifier
    result of `clean`; `migration` or `uncertain` needs `actor: "human"`.
- An unknown `from` or `to` state string returns `{ ok: false, reason }` and never throws.
- A checked-in test file asserts all of the above and the full edge list, so deleting or
  adding a graph edge fails a test.

## Non-goals

- Implementing the approval policy itself (the human-vs-policy table). That is a human
  decision and stays `TODO(human)`; this packet only pins its constraints.
- Implementing the ledger writer, anchor, signature verification or classifier
  (`docs/architecture/approval-ledger.md`, `docs/architecture/release-gate.md`).
- Changing the state graph. If the human decides to add `MigrationGated -> Failed` or
  `Released -> Repair` (see `docs-alignment` D2), that is a separate packet change plus a
  snapshot update.
- Doc and template edits (packet `docs-alignment`), except the README "Testing" section
  and the one open-question bullet that this packet makes stale (T5).

## Invariants

- Fail closed: no input makes `canTransition` return `ok: true` for anything but entry
  into `Blocked`.
- Agents never mark their own transitions approved; no code in this packet writes to the
  ledger directory (CLAUDE.md, `AGENTS.md` "Approvals and state").
- A populated `ApprovalRecord` never changes a result except by turning the
  mismatched-record case into `ok: false`.
- `ALLOWED_TRANSITIONS` is unchanged (46 edges) unless the unresolved question below is
  answered otherwise.

## Cited company decisions

| Constraint | Source |
|---|---|
| Human record for `MigrationGated -> Released` naming exact `commit_sha` and `deployment_target`; approval for one target does not cover another | `docs/architecture/release-gate.md`, "Required evidence for `MigrationGated -> Released`" |
| Policy actor may approve `PreviewVerified -> MigrationGated` only on a `clean` classifier result; `migration` / `uncertain` need a human | `docs/architecture/release-gate.md`, "No-migration releases: machine-checked, fail-closed" |
| A correction or stop is a new ledger record (e.g. a transition to `Blocked`) | `docs/architecture/approval-ledger.md`, "Storage" |
| Phase 1 exit: `canTransition` enforces signed, serialized, anchored records; `MigrationGated -> Released` cannot succeed without a valid human record for the exact commit and deployment target | `README.md`, Roadmap "Phase 1 — Local approval ledger" |
| Resume only to the state a task was blocked from; listing ACTIVE under `Blocked` would skip review, preview and the gate | `harness/policy/transitions.ts` comment on `ALLOWED_TRANSITIONS.Blocked` |
| Agents never approve their own transitions | `AGENTS.md` "Approvals and state" |

## Unresolved questions

- [ ] How does `canTransition` receive a "verified ledger line"? Options: (a) a branded
  `VerifiedLedgerRecord` type that only the (future) ledger verifier can construct, with
  `canTransition` staying pure; (b) `canTransition` takes the ledger and verifies itself.
  Recommend (a). — owner: repo owner, blocks: T3
- [ ] Should entering `Blocked` itself require a ledger record? The review says
  `blockedFrom` must bind to a ledger line, which implies the stop is recorded; today it
  needs none ("stopping work is always possible"). Recommend: keep `ok: true` for the
  *state check*, but only a recorded entry can later authorise resume. — owner: repo owner,
  blocks: T3
- [ ] Is `MigrationGated -> Failed` (or `Released -> Repair`) wanted in the graph? Affects
  the edge snapshot. See `docs-alignment` D2. — owner: repo owner, blocks: T1 snapshot only
- [ ] Node floor: the PR text says Node 24, the review ran Node 22, this session ran
  v22.22.0 (type stripping on). Pick the minimum and document it. — owner: repo owner,
  blocks: T1
