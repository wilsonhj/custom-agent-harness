# Handoff: rust-spine / PR12 review corrections

| Field | Value |
|---|---|
| Base SHA | 2cafc1d286bc0cdaf71b858d032e0592e87bcc86 (PR12 merged with `origin/main` dd5a200) |
| Head SHA | Commit containing this handoff on `codex/review-fixes-pr12` |
| Author (runtime / session ID) | Codex / PR12 review-corrections session |
| Date | 2026-11-10 |

## Changed files

- `docs/superpowers/plans/2026-11-10-pr12-review-fixes.md` — scoped correction plan
- `docs/superpowers/specs/2026-11-09-unified-harness-design.md` — corrected safety contracts
- `docs/superpowers/plans/2026-11-09-rust-spine.md` — matching implementation instructions
- `specs/rust-spine/{requirements,acceptance-tests,handoff}.md` — traceability and review handoff

## Implemented requirements

- None. This session corrects documentation only; it does not implement or validate the
  Rust runtime, change task status or approve any phase.

## Review findings and resolutions

| Verified finding | Correction | Source |
|---|---|---|
| Privacy defaults allowed subscription runtimes despite unknown classification. | Restore `local-only` for missing/unknown classes and require explicit content/provider authorization before hosted spawn; add R-17 and planned checks. | `docs/security/data-egress.md` — Egress classes, BAA configuration allowlist, Enforcement |
| The supervisor became the task, was reaped before the sweep, and `run` always returned `TreeGone`. | Separate the live supervisor and task child; use fallible cleanup, retain the lock on failure, and refuse task spawn unless actual containment covers detachment and responsibility disclaim. A live anchor alone is insufficient. | R-11; design §6.2; research `1a-macos-sandbox.md` §6 |
| Canonicalisation required every optional deny target to exist. | Resolve missing deny targets through their nearest existing ancestor, preserving the deny; keep required-root and other resolution failures fatal. | R-9; design §6.1; research `1a-macos-sandbox.md` §1 |
| A live probe requested real login/Keychain values and then saved transcripts as fixtures. | Use synthetic non-secret canaries with readability status only, retain real credential accessibility as UNVERIFIED, and reconstruct screened synthetic fixtures. | Design §4 decision 6, §§6.3, 11; `docs/security/data-egress.md` |

## Tests run and results

| Command | Result | Evidence |
|---|---|---|
| Original planning sandbox, proxy and process-kill probes | Historical probes only; not rerun and not proof of complete containment | `research-2026-11-09/1a-macos-sandbox.md` |
| `npm run check` | PASS, exit 0 | `JavaScript syntax checks passed` on 2026-11-10 |
| `git diff --check` | PASS, exit 0 | No whitespace errors on 2026-11-10 |
| `npm test` | FAIL, exit 1: 90 tests, 86 pass, 2 fail, 2 skipped | macOS run; exact failures below; full local output `/private/tmp/pr12-review-fixes-npm-test.log` |
| Rust acceptance commands in `acceptance-tests.md` | Not run; this branch contains specifications, not the Rust implementation | Future acceptance criteria only |

The two Node failures are the existing Linux-pilot platform limitation documented in
README "Run the Linux pilot checks", not Rust verification:

- `tests/local-workflow.test.mjs:239` — `sandbox argv is offline, minimally mounted, and
  does not expose controller state`: `Linux bubblewrap is required; no unsandboxed fallback`.
- `tests/local-workflow.test.mjs:273` — `default CLI execution fails closed when isolation
  is unavailable`: expected `/worker failed/`, received
  `Linux bubblewrap is required; no unsandboxed fallback`.

No runtime code or tests were changed. The six-file document review maps each finding to
the design, plan and future acceptance criteria above; it does not establish runtime safety.

## Known limitations

- Adapter fixtures remain provisional until Task 11 validates event shapes and reconstructs
  screened synthetic fixtures; raw authenticated transcripts are not fixture inputs.
- No backend is established here that proves descendant containment across responsibility
  disclaim. R-11 remains binding; all task execution must fail closed until that capability
  is established. The correction does not claim that a persistent anchor solves this case.
- Actual credential accessibility and the provider configuration/BAA allowlist remain
  unverified. Synthetic canaries do not establish real credential isolation.
- Two research reports contain a local home-directory path once each; scrub before the
  repository is made public.
- The human decision is protected by the sandbox and file permissions, not by a signature.

## Unresolved decisions

- Open-source license — owner: repository owner
- Project/content/provider authorizations and any verified BAA configuration — owner:
  repository owner; missing/unknown inputs remain `local-only` and block hosted execution
- A containment backend meeting R-11 — owner: architect; blocks task execution until established

## Assumptions made

- The owner authorized these four verified documentation corrections and isolated branch
  execution. This does not authorize authenticated runs, credential inspection or approval
  transitions (`AGENTS.md`; design §14).
- The original design acceptance records design intent and the accepted exposure/terms
  ambiguity. Each authenticated step still needs explicit go-ahead and project/provider
  authorization; no blanket egress authorization is inferred from it.
- Research grades describe their original probes and sources. They do not establish a
  general descendant-containment capability or real credential isolation.
- Runtime implementation and any backend decision are separate work; this correction
  preserves the fail-closed contract instead of selecting an unverified backend.

## Exact next action

Parent session: submit the exact correction commit and this findings-to-fixes mapping to
the shared independent external review. Record reviewer findings and resolutions here;
do not mark a phase approved or start live authenticated tests. The runtime backend work
must separately establish R-11 containment before any task execution.
