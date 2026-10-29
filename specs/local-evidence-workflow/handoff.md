# Handoff: local-evidence-workflow

Base: upstream main 0b9d58f7dfb1bf8a05b8726921628b56e8d63261.
Assigned implementation worktree: /workspace/scratch/1d7a828570fb/custom-agent-harness.
Branch: codex/local-evidence-workflow. Other branches/PRs are not modified.

## Assumptions

See requirements.md. The local snapshot was reconstructed through the GitHub connector;
local bootstrap history differs from upstream. Publication uses upstream as its parent.
This is a local prototype, not activation of the approval ledger. No real human approval
or independent agent review has occurred. Deterministic reviewer fixtures will test the
review interface; they do not establish the quality of an AI review.

## Research and environment

Node 24.22.1, Git 2.51.1, OpenSSH 9.6p1. Bubblewrap is installed but its network namespace
setup is denied by this host. No Codex or Claude executables are installed. The source
repository and PRs #2/#3 and packet-only branches were inspected before starting.

## Exact next action

Inspect the PR and required real-sandbox CI gate. On a supported Linux host, configure
independent local worker/verifier/reviewer commands and run doctor followed by the pilot.
A human must inspect and sign any real acceptance decision. No task phase is approved here.

## Implemented

- Dependency-free Node 24 CLI and local workflow controller, content-addressed artifacts,
  fsynced manifest/event writes, per-run lease and restart inspection.
- Detached candidate worktree, pinned packet/config/tools/trust, process deadlines,
  output caps, cancellation and group cleanup, Linux bubblewrap without fallback.
- Commit-bound verifier/reviewer evidence, base-to-candidate patch on reviewer stdin,
  strict review response and current-time namespace-bound SSH decision verification.
- Complete filesystem-to-Git-object comparison catches ignored files and stat-cache tricks.
- Operator guide, example external configuration and CI requiring actual sandbox tests.

## Verification

| Command | Result |
|---|---|
| npm test | 28 tests: 27 pass, 0 fail, 1 explicit real-sandbox skip; Node 24.22.1 |
| npm run check | Passed JavaScript syntax checks |
| git diff --check | Passed |
| node harness/local/cli.mjs status (missing --run) | Correctly exits 1 with structured error |
| node harness/local/cli.mjs doctor | Host does not permit required namespace execution; exits 1 |

The tests include independent fixture processes, synthetic signed accept/reject decisions,
wrong/replayed signatures, changed candidates/configuration/trust, corrupted evidence,
failed/malformed reviews, timeouts, output caps, leases and interruption handling. Fixtures
use disposable software keys and never represent a genuine human decision. The CLI has
no signing command. The test-only executor does not provide a CLI isolation bypass.

## Review and limitations

Implementation self-review found and corrected: missing patch context for reviewers;
ignored/untracked files and Git assume-unchanged flags could hide differing tested bytes;
tool files needed ownership/mode checks in addition to directory checks. Regression
assertions now cover patch context and both snapshot bypasses.

Real sandbox success is not claimed on this host. CI sets HARNESS_REQUIRE_SANDBOX=1 and
must pass the full real-bubblewrap workflow and denied read/write/network probes. There
is no Codex/Claude executable here, so independent live AI review was not run. A human
should request it on the published commit. macOS/Windows, hosted adapters, resource quotas,
production operations and the global approval ledger remain outside this slice.

## Remote validation

PR: https://github.com/wilsonhj/custom-agent-harness/pull/4

The first published implementation, 622cc163e263ba098794f0503c7bf08bf15690ee, passed
the required real sandbox gate on Ubuntu 22.04 / Node 24.22.1 / bubblewrap 0.6.1-1ubuntu0.3:
https://github.com/wilsonhj/custom-agent-harness/actions/runs/40799507693

The job log explicitly records the real bubblewrap workflow and denied read/write/network
test as passing. Its one skip is the opposite-host test (namespace-unavailable behavior),
which passed locally. Both execution environments report 27 pass, 0 fail, 1 host-specific
skip. The final follow-up also exercises the signed fixture decision after real sandbox
execution and avoids duplicate feature-branch push/PR runs. Final-head CI remains the gate.
