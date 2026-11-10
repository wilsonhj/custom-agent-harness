# External review: PR 13 egress authorization

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
stop an active task to end that session. Metadata-only doctor probes use empty synthetic
worktrees with no network, so no project grant is required there.

Review boundaries: the operator classifies the whole reachable repository and all task
inputs, outputs and derived content using their most restrictive class. There is no
per-path widening, automatic PHI classifier, BAA account detection or managed enterprise
configuration. The operating user is trusted, as with local human decisions. This
change does not resolve the separately reviewed macOS descendant-containment defect.

Validation:

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

No real agent, hosted model call, credential operation, global account creation or
external publication was performed.
