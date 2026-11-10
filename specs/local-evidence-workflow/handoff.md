# Handoff: local-evidence-workflow

| Field | Value |
|---|---|
| Base SHA | 7fce6c6 (main after PR #4 merged; the follow-up starts here) |
| Head SHA | The follow-up commit on `claude/wizardly-allen-3ovt4w` that adds this line (direct child of 7fce6c6) |
| Author (runtime / session ID) | Claude Code (lead integration) / session_01AaDiMJVSVxQVfoGPARPgBn; post-merge follow-up session_01AaDiMLni7kwVNftEwaHm1a; original implementation by Codex |
| Date | 2026-11-03 |

Branch: codex/local-evidence-workflow. Original base: upstream main 0b9d58f.

## Assumptions

See requirements.md. The local snapshot was reconstructed through the GitHub connector;
local bootstrap history differs from upstream. Publication uses upstream as its parent.
This is a local prototype, not activation of the approval ledger. No real human approval
has occurred; Claude agent reviews are recorded below (no Codex review). Deterministic reviewer fixtures test the
review interface; they do not establish the quality of an AI review.

## Research and environment

Original implementation host: Node 24.22.1, Git 2.51.1, OpenSSH 9.6p1; bubblewrap's network
namespace setup was denied there. The 2026-11-01 review host runs the real sandbox (see
Verification). No Codex or Claude executables are installed. The source
repository and PRs #2/#3 and packet-only branches were inspected before starting.

## Exact next action

PR #8 contains fixes for both verified candidate-entrypoint bypasses and is synced with
main at 019870f. Wait for CI (`verify`) on the published fixes and review them before
merging; then a human
configures real local worker/verifier/reviewer commands (entrypoints as absolute paths under
`toolsDirectory`) and runs `doctor` on the target host.

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

Run 2026-11-01 on the branch with main merged, Node 24.22.1 under /opt, bubblewrap and
ssh-keygen installed (same packages CI installs):

| Command | Result | Evidence |
|---|---|---|
| `HARNESS_REQUIRE_SANDBOX=1 npm test` | 80 tests: 79 pass, 0 fail, 1 skip (the namespaces-unavailable test, skipped because the sandbox is available) | terminal output of this session |
| `node --test tests/local-workflow-regressions.test.mjs` on the pre-fix code (5287b46) | bug1-bug5, bug8, bug9 fail; bug6a-c, bug7 pass, as intended | terminal output of this session |
| `npm run check` | Passed JavaScript syntax checks | terminal output |
| `git diff --check` | Passed | terminal output |

The tests include independent fixture processes, synthetic signed accept/reject decisions,
wrong/replayed signatures, changed candidates/configuration/trust, corrupted evidence,
failed/malformed reviews, timeouts, output caps, leases and interruption handling. Fixtures
use disposable software keys and never represent a genuine human decision. The CLI has
no signing command. The test-only executor does not provide a CLI isolation bypass.

## Review findings and resolutions (2026-11-01)

Six Claude review agents (correctness, tests, security, spec conformance) and a lead
`/code-review` reviewed 5287b46. Not a Codex review. Fixed in b515c37 and the next commit:

| Finding | Resolution | Regression test |
|---|---|---|
| Critical: a worker-created nested repository's `filter.*.clean` ran on the host via `git status` (sandbox escape, reproduced) | Workspace scan rejects any nested `.git` before host Git reads worker output; every host Git call disables submodule recursion | bug1 (fails on 5287b46, marker written on host) |
| Packet digest compared the source checkout's ignored files and modes with the worktree | Packet identified by its Git tree in the base and candidate commits | bug2a, bug2b |
| Host Git stored worker bytes before size limits applied | Limits and special-file checks run before `git add` | (covered by scan; existing limit behaviour) |
| Snapshot exec-bit rule differed from Git | Owner exec bit only | bug3 |
| `git()` trimmed the reviewer patch; 16 MiB buffer | Untrimmed patch, 256 MiB buffer | bug4 |
| Second Ctrl-C or Ctrl-C during decisions left a stale lock | Handlers kept for every lease-holding command | none (signal timing) |
| Tools-directory containment was one-way; `/opt` mountable | Two-way check; `/opt` rejected | bug5 |
| Store created before its location was checked | Checked before creation | updated boundaries test |
| `null` decision statement gave a TypeError | Rejected as non-object | none |
| Human signs a path, not a checked digest; key may be a plain file | Docs: sign a checked copy, hardware-backed or separate-account key | docs only |
| Ignored worker output failed with a misleading message | Message names the file as ignored/untracked; documented | bug7 |
| Mount-escape fix in 5287b46 had no regression test | Added | bug6a-c |
| Stale handoff, `git diff --check` failure, non-TaskState statuses | This handoff; README EOF; tasks use `Reviewing`/`Planned` | n/a |

A final security review of 7470c2f found no way past the nested-repository fix and three
follow-ups, fixed in the next commit: a packet file that is ignored on disk but absent from the
commit passed `prepare` (regression from the packet-tree change; the packet is now validated
in the prepared worktree, test bug8); a store path through a symlinked parent could create
directories in the repository before rejection (checked through the real path first, bug9);
worker `.gitattributes` can select filter drivers defined in the repository's own config
(documented). SIGHUP now also releases the lease.

Not changed (documented as known limitations in docs/local-workflow.md): reviewer prompt
injection, unbound empty directories and non-exec mode bits, and the evidence-store layout
that differs from approval-ledger.md.

## Post-merge follow-up (2026-11-03)

A `/code-review` of 5287b46 reported 8 unverified findings. Each was re-checked against
main at 7fce6c6:

| Finding | Status at 7fce6c6 | Resolution | Regression test |
|---|---|---|---|
| Packet digest taken from the source checkout | Already fixed (b515c37, 21364da) | none | bug2a, bug2b, bug8 |
| `capture()` waits for 'close', so a `setsid` descendant holding a pipe keeps it past the deadline | Confirmed: a 500 ms deadline was still pending at 3 s | After 'exit', kill the group and force-close pipes after 1 s; the job fails with "descendant kept output open after exit" | bug10 (hangs 30 s on 7fce6c6) |
| Verifier/reviewer "no workspace path" rule only rejected `{workspace}` | Confirmed: `['node', 'review.mjs']` ran a worker-written review that reached awaiting-decision; absolute `/work/...` also passed | Reject `/work` paths at config load; refuse relative arguments naming an existing candidate path before each verifier/reviewer run. cwd stays `/work`: moving it would make an `npm test` verifier silently test `/tools` | bug11a, bug11b (fail on 7fce6c6), bug11c |
| `git()` trim corrupts `ls-tree -z` and the patch | Patch: already fixed (b515c37). `ls-tree -z`: not a bug, the output ends in NUL, which `trim()` keeps | none | bug4 |
| Snapshot exec bit differs from Git | Already fixed (b515c37) | none | bug3 |
| Agent-set task statuses in tasks.json | Statuses were already changed to `Reviewing`/`Planned` | Not changed: editing them again would be an agent recording a transition; a human should set them | n/a |
| Full candidate re-hash before the reviewer, right after the post-verifier check, and again at the end | Confirmed | Each role's candidate is checked once, right after it (the worker's after `reset --hard`) | covered by existing candidate-tamper tests |
| `doctor()` duplicates the bwrap argv | Confirmed | Shared `bwrapPrefix()`; `doctor` reports the constant path | existing doctor and real-sandbox tests |

Validation on this host (Node 24.22.1 under /opt, bubblewrap and ssh-keygen installed as in
CI): `HARNESS_REQUIRE_SANDBOX=1 npm test` 84 tests, 83 pass, 0 fail, 1 skip (the
restricted-host denial test, skipped because the sandbox is available); `npm run check`
passed; `node harness/local/cli.mjs doctor` ok; `git diff --check` passed. Before the change
the same suite gave 80 tests, 79 pass, 1 skip.

Codex review (chatgpt-codex-connector on PR #8, commit d8fc97a), one P2 finding: `bug10` let
the shell exit before its background child called `setsid`, so the exit-time group kill
could reach the child and the pipe closed normally (it also failed the first CI run).
Resolved in 72966ed: the grandchild is spawned detached from Node (setsid before exec) and
the child exits only after its `spawn` event. The test fails after 30 s on 7fce6c6 and
passes in about 1 s with the fix; CI `verify` passed on 72966ed.

## PR #8 verified review fixes (2026-11-05)

Synced `claude/wizardly-allen-3ovt4w` with main at
019870f2c71fc9fda24782853ef68050aebfa1ee before changing implementation. The merge was
clean and preserves the adapter-interface feature packet and kiro-adapter updates.

Both findings from the review of ef06780 were reproduced. `node review` found a
worker-written `review.js`, and `node /proc/self/cwd/review.js` reached the candidate
through procfs. Each could forge a passing review and reach `awaiting-decision`.

The relative-argument guard now includes Node's implicit .js/.json/.node suffixes.
Absolute verifier/reviewer argument paths, including `--name=value`, must be inside
configured runtime mounts or the tools directory (host path or /tools). This excludes
procfs and fd aliases without enumerating their spellings. Relative traversal is resolved
from the sandbox's /work cwd, covering ../proc aliases and ../work candidate paths even
though the host worktree has a different path. The working directory remains
/work, and ordinary options and non-path operands remain supported. This implements the
trusted-entrypoint contract in this packet's design.md, API and schema, and requirements.md
items 2 and 5. It is still an argument guard, not an interpreter parser: shell strings and
trusted tools that themselves import candidate code remain the documented limitations.

Regression tests bug12 and bug13 cover verifier and reviewer, fixture and real-bubblewrap
execution, bare and --name=value absolute aliases, and valid trusted arguments. Before the
implementation change, all four extensionless cases reached `awaiting-decision` and the
absolute-alias configuration test failed with a missing expected rejection. The valid
argument case passed.

Codex review of c7bf209 (PR #8), one P2 finding: an operand longer than the filename limit
(for example a `node -e` script of 300+ characters with no `/`) made `lstat` throw
`ENAMETOOLONG`, failing the run before the trusted command started. Such a name cannot be a
worker-written file, so the check now treats `ENAMETOOLONG` like `ENOENT`. Regression test
bug11d failed with ENAMETOOLONG before the change and passes after it;
`HARNESS_REQUIRE_SANDBOX=1 npm test` 96 tests, 95 pass, 0 fail, 1 skip (the
restricted-host denial test); `npm run check` and `git diff --check` passed.

Validation of the fixes on Node 24.22.1: `HARNESS_REQUIRE_SANDBOX=1 npm test` reports
95 tests, 94 pass, 0 fail, 1 expected restricted-host skip. This includes both roles and
the ../work variants in real bubblewrap. `npm run check`, `git diff --check`, and
`node harness/local/cli.mjs doctor` (ok: true) pass. Tests ran with additional execution
permissions because the command sandbox restricts nested namespace setup; isolation in
the workflow itself remained mandatory. Remote CI on the published commit is the final gate.

## Review and limitations

Implementation self-review found and corrected: missing patch context for reviewers;
ignored/untracked files and Git assume-unchanged flags could hide differing tested bytes;
tool files needed ownership/mode checks in addition to directory checks. Regression
assertions now cover patch context and both snapshot bypasses.

The original implementation host could not run the real sandbox; the 2026-11-01 review host
and CI did (see Verification). CI sets HARNESS_REQUIRE_SANDBOX=1 and
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
which passed locally. At that commit both environments reported 27 pass, 0 fail, 1
host-specific skip; current counts are in the Verification table. The final follow-up also exercises the signed fixture decision after real sandbox
execution and avoids duplicate feature-branch push/PR runs. Final-head CI remains the gate.
