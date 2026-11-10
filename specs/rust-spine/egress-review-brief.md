# External review: PR 13 egress authorization

Current execution is disabled by the independent R-11 containment gate for real
and fake runtimes before invocation. An egress grant cannot reopen that gate.
The successful fake-runtime launch and revocation checks below are historical
pre-containment evidence, not current end-to-end availability.

Without an operator grant, `eng run` previously prepared a hosted agent from any
repository. The engine now returns `PolicyDenied` before reading the pipeline for the
effect, creating the worktree or home, launching a version probe, or building a prompt.
This enforces the default-local-only rule in `docs/security/data-egress.md`.

The local operator supplies one private `<state>/egress-policy.json`, outside the
source repository. It binds an exact canonical project root, `any-provider`, approved
subscription runtimes and a reviewer. Unknown fields, invalid JSON, missing or more
restrictive classes, wrong projects, unapproved runtimes, and nonprivate/linked files
deny. BAA work is denied because this slice has no verified managed BAA configuration.
Neither candidate content nor pipeline configuration supplies this authorization.

Every agent effect stores the exact grant bytes and their SHA-256 in harness evidence,
accessible through `Engine::egress_authorization`. Version and agent gate callbacks
re-read the grant; changed or deleted bytes prevent the go byte. A policy change does
not stop an already authorized process or recall prior disclosure. The operator must
stop an active task to end that session. When containment permits them, metadata-only doctor probes use empty synthetic
worktrees with no network, so no project grant is required there.

Review boundaries: the operator classifies the whole reachable repository and all task
inputs, outputs and derived content using their most restrictive class. There is no
per-path widening, automatic PHI classifier, BAA account detection or managed enterprise
configuration. The operating user is trusted, as with local human decisions. This
change does not resolve the separately reviewed macOS descendant-containment defect.

Historical pre-containment validation at `feceb481e647fb263a02248b5554226a73540f04`
(2026-11-10):

- Regression red: the missing-policy test originally reached `SandboxUnavailable`,
  rather than `PolicyDenied`, and had already prepared a worktree.
- Regression red: temporarily removing the final agent gate recheck made the synthetic
  runtime's `agent-started` marker appear after revocation. Restoring it prevents launch.
- Four policy unit tests and four integration regressions cover missing/restrictive
  grants, schema/project/runtime binding, private ownership/links, exact snapshot evidence
  and operator revocation during a paused fake `--version` call.
- `node scripts/check-deps.mjs`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`
  (five pre-existing ignored), and fake-agent CLI conformance (11/11) passed.
- `npm test`: 86 passed, two skipped, two failed in the existing Linux-bubblewrap pilot:
  `sandbox argv is offline, minimally mounted, and does not expose controller state`
  and `default CLI execution fails closed when isolation is unavailable`.
  Both failures report the missing Linux bubblewrap backend on macOS.

Current combined validation (code head `ebcb067b8e937ea65e07a70ce9e995fcaf2b77e4`):

- Rust: 234 passed, 0 failed, 35 ignored (30 R-11 backend-dependent, 5 unchanged
  baseline). CLI conformance: 14 cases, 9 passed, 0 failed, 5 explicitly skipped.
- Egress policy unit tests remain active. The integration target has 4 passes and
  1 ignore: `revoking_the_grant_during_version_preparation_prevents_the_agent_go_byte`
  cannot exercise launch-time revocation while runtime invocation is disabled.
  Its full assertions remain compiled with an explicit R-11 reason; a real backend
  is required to restore that coverage.
- The coordinating session's latest `/usr/local/bin/node v24.0.2` `npm test` run
  has 90 tests: 85 passed, 3 failed, 2 skipped. The three failures are
  `excessive output retains failure evidence and blocks decision`
  (`worker failed: kill EPERM`, expected `output limit exceeded`),
  `sandbox argv is offline, minimally mounted, and does not expose controller state`,
  and `default CLI execution fails closed when isolation is unavailable`.
  The latter two require Linux Bubblewrap. The excessive-output failure also
  reproduces narrowly on the unchanged Node pilot in the PR 12 docs branch;
  `harness/local/` and `tests/` have no diff from main. Linux CI remains pending.
  The earlier 86/2/2 result above belongs to the historical egress head.
- Final Node evidence: `/private/tmp/harness-pr13-final-node24.log`,
  `/private/tmp/harness-pr13-node-output-recheck.log`, and
  `/private/tmp/harness-pr12-node-output-baseline.log`. Current Rust/conformance
  commands and all ignore/skip names are in
  [the final combined evidence](containment-review-brief.md#final-combined-verification).

No real agent, hosted model call, credential operation, global account creation or
external publication was performed.
