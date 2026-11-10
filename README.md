# Custom Agent Harness

A prototype engineering control plane for Claude Code and Codex: keep the specification, candidate commit, verification results, review findings, and human decision together as durable artifacts.

The repository currently has two distinct tracks. The default `main` branch contains a working **offline Linux evidence-workflow pilot**. This review branch also contains the **macOS Rust first slice from [PR #13](https://github.com/wilsonhj/custom-agent-harness/pull/13)**. Its CLI supports task metadata and policy checks; agent and verification execution is disabled until strong descendant containment exists. Authenticated Claude/Codex compatibility remains unverified.

The Rust implementation and its review corrections are branch-specific; see the feature-packet handoff for exact revisions and verification evidence.

## What works today

| Track | Implemented scope | Current limits |
|---|---|---|
| Linux pilot on `main` | Isolated worktree; operator-configured worker, verifier, and reviewer processes; content-addressed evidence; signed human review attestation; durable handoff | Linux, Node 24, Git, SSH signing tools, and working Bubblewrap namespaces. No hosted-model adapter, task-phase approval, migration, or deployment execution. |
| Rust spine in PR #13 | Task lifecycle and repair policy; Claude/Codex launch plans and stream parsers; process, sandbox, and Git modules; SQLite command log, hash-chained events, artifact store; engine and restart-recovery implementation | macOS-first development. Agent and verification launches fail closed with `containment_unavailable`; fake-agent end-to-end execution is also disabled. Adapter transcripts and authenticated compatibility remain unverified. |
| Company Brain | Separate [companion repository](https://github.com/wilsonhj/company-brain-starter) for company context and reviewed knowledge | Harness retrieval and knowledge-update integration are planned. No operational connection is established by this repository. |

See the [Linux operator guide](docs/local-workflow.md) for the pilot's actual behavior and the [Rust design](docs/superpowers/specs/2026-11-09-unified-harness-design.md) for the proposed next system. Design documents and feature packets describe intent; their presence does not establish implementation or approval.

## Run the Linux pilot checks

Use a reviewed checkout on Linux with Node 24 and Git. Install Bubblewrap and SSH signing tools through your operating system before running the checks. No npm dependencies or model credentials are required for this test suite.

```bash
git clone --branch main https://github.com/wilsonhj/custom-agent-harness.git
cd custom-agent-harness
node --version
node harness/local/cli.mjs doctor
npm run check
HARNESS_REQUIRE_SANDBOX=1 npm test
```

`doctor` must report a usable sandbox. The CLI refuses to run without Linux Bubblewrap; there is no unsandboxed fallback. `HARNESS_REQUIRE_SANDBOX=1` makes an unavailable sandbox fail the test gate. This is not a macOS execution path.

To prepare an actual pilot run, follow [operator configuration, execution, and human signing](docs/local-workflow.md). The controller installation, tools, configuration, signer list, and run store have separate trust requirements; the guide supplies the complete procedure. A fixture reviewer demonstrates the transport contract, not AI review quality. A signed local review attestation does not approve a production release or change task-phase state.

## Inspect and test the Rust spine

This review branch contains the first Rust CLI slice. It implements task creation,
human decisions, execution, status, cancellation, audit verification and doctor.
Agent and verification launches currently fail with `containment_unavailable`:
macOS process polling and responsibility links cannot guarantee every descendant
remains tracked. `eng doctor` reports that failure and skips runtime version execution.
There is no environment, feature or debug override. Existing same-boot legacy runs
remain quarantined with their locks and workspaces intact; only a confirmed different
boot proves their descendants gone. The adapter transcripts are hand-written fixtures,
not evidence of a successful hosted-model run.

### Rust CLI commands

Requires macOS 26 on Apple silicon, a Rust toolchain (`rust-toolchain.toml`), Node 24, and
Claude Code and Codex at the pinned versions for future execution. The commands below
document the intended workflow; signing in does not enable execution while containment
is unavailable. Task 11 live compatibility checks also remain outstanding.

### 1. Build

```bash
cargo build --workspace          # builds eng and eng-shim (they must sit side by side)
export PATH="$PWD/target/debug:$PATH"
```

### 2. Sign in (once per runtime; the harness never reads the login)

```bash
eng login claude                 # starts Claude Code with a harness-owned config dir; type /login
eng login codex                  # runs `codex login` with a harness-owned CODEX_HOME
```

### 3. Check the environment

```bash
eng doctor                       # exit 0 only if every check passes
```

### 4. Run a task

```bash
eng task new --repo . --packet specs/my-feature     # prints the task id and packet digest
eng decide <task> approve-spec                      # needs a terminal; type yes
eng run <task>                                      # currently exit 5: containment unavailable
eng status <task>
eng cancel <task>                                   # also works while eng run is active
eng decide <task> accept                            # or reject / send-back
eng audit verify                                    # recomputes the hash chain
```

Every command accepts `--json` (one object with `ok`, `task`, `state`, `error`) and
`--state-dir <path>` (default `~/Library/Application Support/eng`). Exit codes: `0` ok, `2`
usage, `3` rejected by policy, `4` awaiting a human, `5` failed, `70` internal. The pipeline
(runtimes and verification commands) is read from `.eng/pipeline.toml` in the repository.

`ENG_CLAUDE_BIN`, `ENG_CODEX_BIN`, `ENG_NODE_BIN` and `ENG_SHIM_BIN` override where the runtimes
and the shim are found (default: `PATH`, and `eng-shim` beside `eng`); `ENG_EXTRA_READ_DIRS`
(colon-separated) lets the sandbox read more directories. Debug builds also honour
`ENG_FORCE_SANDBOX_UNAVAILABLE=1`, which makes the sandbox probe fail for real.

Node remains available to verification commands and runtime tools when Codex is a native
executable. During `eng run`, status and cancellation use that run's private local control
endpoint; the runner remains the only state-store writer. Cancellation is recorded before
process cleanup finishes; the runner retains the worktree lock until cleanup is confirmed.

### Authorizing hosted task content

Hosted task runs default to `PolicyDenied`. Following
[the data egress policy](docs/security/data-egress.md), the operator must review the
entire repository and all task inputs, tool output and derived content, then place
`egress-policy.json` in the private `--state-dir` directory:

```json
{
  "version": 1,
  "project_root": "/canonical/absolute/path/to/project",
  "egress_class": "any-provider",
  "allowed_runtimes": ["claude", "codex"],
  "reviewed_by": "operator name"
}
```

The state directory must be outside the source repository and task worktrees, mode
0700. The policy must have the same owner as that directory, mode 0600 (0400 also
works), and be a regular file with no symlink or hard-link aliases. Each state
directory authorizes one canonical project. `claude` means the Claude Code
subscription surface on Anthropic; `codex` means the Codex CLI subscription surface
on OpenAI. These entries require the operator's general engineering provider approval.

Only an explicit `any-provider` grant is supported. Missing, malformed, unknown,
`local-only`, and `baa-covered-providers` policies deny hosted task execution.
Subscription login does not establish BAA coverage. Mixed-class projects must use
their most restrictive overall class; this slice has no per-path widening. A
repository's pipeline cannot authorize itself. The local operator is trusted to
classify content and protect the policy; this is not a managed enterprise BAA setup.

Each agent effect records the exact policy snapshot and SHA-256 as harness evidence
(`Engine::egress_authorization`). The grant is checked before task preparation and
rechecked at the version and agent launch gates. A changed or deleted policy denies
the pending launch. Revocation affects future launches; stop an active task to end
an already authorized session. Previously disclosed content cannot be recalled.
`doctor` skips runtime version probes while containment is unavailable. An egress grant
does not override the independent containment gate.

### Testing the CLI

```bash
cargo build --workspace && ENG_BIN=target/debug/eng npm run conformance
```

The conformance cases drive only the `eng` binary with temporary fixtures. Positive
execution cases are explicitly skipped while the containment backend is unavailable.

### Rust verification

On the pinned Rust toolchain and Node 24:

```bash
node scripts/check-deps.mjs
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
ENG_BIN=target/debug/eng npm run conformance
```

The Rust suite has 29 newly ignored execution-dependent tests and the conformance
suite has 3 explicit skips because the strong containment backend is unavailable.
Passing remaining checks does not establish an end-to-end or live hosted-model workflow.
The [containment review brief](specs/rust-spine/containment-review-brief.md) lists every
new ignore/skip and the capabilities they no longer exercise. See [the feature packet](specs/rust-spine/)
and its handoff for exact results, pre-existing ignores and next actions.

## Trust boundaries and planned integration

Instructions in `AGENTS.md`, skills, repository text, and model output do not grant execution or approval authority. The Linux pilot enforces isolation outside the worker and records independent verification/review evidence. Its production approval policy remains fail closed; see [transition policy](harness/policy/transitions.ts).

The Rust design targets a human-approved packet → Claude implementation → harness verification → Codex review → bounded repair → human decision. A reviewer's pass ends the repair loop; accepting an exact candidate commit remains a human action. The signed, anchored approval ledger is deferred in that design. See [design sections 6 and 12](docs/superpowers/specs/2026-11-09-unified-harness-design.md#6-running-an-agent) for the distinction from the Linux pilot.

Company Brain is intended to supply versioned context, provenance, and reviewed knowledge proposals while the harness owns execution and evidence. Retrieval, `brain.propose_update`, research providers, production operations, deployment integrations, and a daemon/UI are future work; the current pilot does not implement them.

## Repository guide

| Path | Purpose |
|---|---|
| [AGENTS.md](AGENTS.md), [CLAUDE.md](CLAUDE.md) | Shared engineering agreements and Claude-specific guidance |
| [harness/local/](harness/local/) | Linux pilot controller, sandbox, process capture, snapshots, and artifacts |
| [tests/](tests/) | Policy, ledger-guard, workflow, and regression checks |
| [specs/](specs/) | Requirements, designs, tasks, acceptance tests, and handoffs |
| [docs/local-workflow.md](docs/local-workflow.md) | Complete pilot setup and operating procedure |
| [docs/architecture/approval-ledger.md](docs/architecture/approval-ledger.md) | Approval-ledger design |
| [docs/architecture/release-gate.md](docs/architecture/release-gate.md) | Release and migration gate design |
| [docs/security/data-egress.md](docs/security/data-egress.md) | Data-egress policy and unresolved deployment choices |
| [PR #13 Rust crates](crates) | Rust implementation present on this review branch |

Before contributing, read `AGENTS.md` and the relevant feature packet. Work in the assigned worktree, make the smallest scoped change, and report exact validation evidence. Agents cannot approve their own task phases or releases.
