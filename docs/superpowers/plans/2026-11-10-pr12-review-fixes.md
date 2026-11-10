# PR12 Review Fixes Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans for this scoped documentation
> correction. Execution is already authorized; no task phase or approval is changed.

**Goal:** Correct the four verified PR12 specification defects without changing runtime code.

**Architecture:** Keep the approved design, Rust implementation plan and feature packet
consistent with the existing data-egress policy and the R-11 descendant-cleanup invariant.
Record capabilities that remain unverified rather than claiming a working containment backend.

**Tech Stack:** Markdown; existing Node checks for repository regressions.

**Spec:** `specs/rust-spine/requirements.md`, design §§6, 9, 11, 14, and
`docs/security/data-egress.md` (Egress classes, BAA configuration allowlist, Enforcement).

## Global Constraints

- Work only on `codex/review-fixes-pr12`; do not change approval records or task statuses (`AGENTS.md`).
- Missing or unknown egress class means `local-only`; class widening is reviewed (`docs/security/data-egress.md`).
- The harness never reads, stores, copies or forwards subscription login tokens (design §4, decision 6).
- Release a worktree lock only after every descendant is confirmed gone (R-11).
- No authenticated agent runs, real credential probes, pushes or PR writes in this correction.

## Review Focus

1. Unclassified input must deny hosted execution before task content reaches a runtime.
2. A permitted host must not substitute for authorization of content and provider configuration.
3. A detached or responsibility-disclaiming child must not yield false `TreeGone` proof.
4. Missing optional deny targets must remain denied after creation; resolution errors fail closed.
5. Probe records and committed fixtures must contain no real credential values.

### Task 1: Align the contracts

**Files:** Modify `docs/superpowers/specs/2026-11-09-unified-harness-design.md`,
`docs/superpowers/plans/2026-11-09-rust-spine.md`, and
`specs/rust-spine/{requirements,acceptance-tests,handoff}.md`.

**Interfaces:** Consumes the policy and invariants above; produces matching design,
implementation instructions and future acceptance criteria, with no runtime implementation claim.

- [ ] Replace permissive privacy defaults with explicit content/provider authorization and a fail-closed pre-spawn check.
- [ ] Preserve R-11 with fallible cleanup, a live separate supervisor, and refusal to run when actual descendant containment cannot be established.
- [ ] Distinguish existing required roots from missing deny targets resolved through their nearest existing ancestor.
- [ ] Replace real credential extraction with synthetic non-secret canaries and screened fixture reconstruction.

### Task 2: Verify and hand off

**Files:** Update `specs/rust-spine/handoff.md` with assumptions, exact results and next action.

**Interfaces:** Consumes Task 1's diff; produces a committed change for independent external review.

- [ ] Review the six-file diff against all five review-focus cases. No new prose-mirroring tests.
- [ ] Run `npm run check` and `git diff --check`; expected: exit 0.
- [ ] Run `npm test`; record exact failures, including the known unsupported macOS Linux-pilot path.
- [ ] Record unexecuted Rust acceptance criteria and containment limitations, then commit the scoped documentation change.
- [ ] Hand the exact commit to the parent session for independent external review; do not self-approve.
