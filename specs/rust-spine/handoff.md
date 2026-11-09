# Handoff: rust-spine / planning

| Field | Value |
|---|---|
| Base SHA | b56ae9d (origin/main) |
| Head SHA | uncommitted working tree on branch `docs/rust-harness-design` |
| Author (runtime / session ID) | Claude Code / lead architect session, 2026-11-09 |
| Date | 2026-11-09 |

## Changed files

- `docs/superpowers/specs/2026-11-09-unified-harness-design.md` — approved design
- `docs/superpowers/specs/research-2026-11-09/*.md` — six graded research reports
- `docs/superpowers/plans/2026-11-09-rust-spine.md` — implementation plan
- `specs/rust-spine/*` — this packet

## Implemented requirements

- None. This session produced the design, plan and packet only.

## Tests run and results

| Command | Result | Evidence |
|---|---|---|
| Local sandbox, proxy and process-kill probes | pass (as probes, not as tests) | `research-2026-11-09/1a-macos-sandbox.md` |
| `claude --help`, `codex --help`, `agent --help` probes | recorded | `research-2026-11-09/1b-subscription-clis.md` |
| Any `cargo` or `npm` command | not run | Rust toolchain not installed |

## Contract changes during implementation

Later tasks build against these, not the plan text, where they differ.

- T2 (a5d313c): modules are `pub mod` with no root re-exports (`harness_core::finding::Severity`).
  `Capabilities` is Serialize only. `RawFinding`/`RawReview` are not `Eq`/`Hash` (f32).
  `ReviewRejected { PathOutsideRepo{finding,path}, PassWithBlocking{finding}, BadLineRange{finding},
  ConfidenceOutOfRange{finding} }` (finding is 1-based). String ids: `new(impl Into<String>)`,
  `as_str`; hex ids: `parse(&str) -> Result<_, IdError>`, `as_str`, `LEN`. `MAX_CLAIM_CHARS = 2000`
  characters. `ModelFamily::OpenAi` is `"openai"` on the wire. A finding with a file and no lines is
  `Location` 0..0 (whole file); start without end is start..start.
- Integration commit after T2: `harness_exec::plan::{LaunchPlan, TreeGone}` are shared by T5, T6
  and T7 (re-exported at the crate root). `TreeGone::confirmed()` is crate-private (supervisor only);
  `TreeGone::for_tests()` exists under the `test-support` feature, enabled for this crate's own
  tests. The exec crate's dependencies are pre-declared (tokio, tokio-util, libc, thiserror; dev:
  tempfile) so parallel tasks do not edit `Cargo.toml`.

## Known limitations

- Adapter fixtures will be provisional until plan Task 11 records real transcripts.
- Two research reports contain a local home-directory path once each; scrub before the
  repository is made public.
- The human decision is protected by the sandbox and file permissions, not by a signature.

## Unresolved decisions

- Open-source license — owner: repository owner
- Projects that must send nothing to third parties — owner: repository owner (not needed by this slice)

## Assumptions made

- "Yes, I accept" (owner, 2026-11-09) approves the design, accepts the token exposure and
  the terms ambiguity, and permits live tests; the architect confirms again before the first
  authenticated run.
- The six research reports are accurate where graded (P) or (D); UNVERIFIED items are listed
  in design §11.

## Exact next action

Owner reviews `docs/superpowers/plans/2026-11-09-rust-spine.md`, installs the Rust toolchain
(plan Task 1 step 1), and tells the architect to start Task 1.
