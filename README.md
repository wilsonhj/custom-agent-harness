# Custom Agent Harness

A prototype engineering control plane for Claude Code and Codex: keep the specification, candidate commit, verification results, review findings, and human decision together as durable artifacts.

The repository has two distinct tracks: a working **offline Linux evidence-workflow pilot** (`harness/local/`) and the **macOS Rust spine** (`crates/`, the `eng` CLI). The Rust spine runs its full implement → verify → review → repair → human-decision flow with fake agents; live Claude/Codex runs are not yet established (plan Task 11).

## What works today

| Track | Implemented scope | Current limits |
|---|---|---|
| Linux pilot on `main` | Isolated worktree; operator-configured worker, verifier, and reviewer processes; content-addressed evidence; signed human review attestation; durable handoff | Linux, Node 24, Git, SSH signing tools, and working Bubblewrap namespaces. No hosted-model adapter, task-phase approval, migration, or deployment execution. |
| Rust spine (`eng`) | Task lifecycle and repair policy; Claude/Codex launch plans and stream parsers; gated spawn, macOS sandbox profile, egress proxy, process-tree kill and hardened host Git; SQLite command log, hash-chained events, artifact store; engine with restart recovery; `eng task new`, `decide`, `run`, `status`, `cancel`, `audit verify`, `doctor`, `login` | macOS 26 on Apple silicon only. Proven with fake agents (conformance suite); a real Claude run still starts (its prompt reaches Anthropic), but its result is turned into `PolicyDenied` because Claude's own sandbox cannot yet be confirmed off (Task 11). There is no check of data class or provider authorization before a hosted run. Adapter fixtures are hand-written and provisional. |
| Company Brain | Separate [companion repository](https://github.com/wilsonhj/company-brain-starter) for company context and reviewed knowledge | Harness retrieval and knowledge-update integration are planned. No operational connection is established by this repository. |

See the [Linux operator guide](docs/local-workflow.md) for the pilot's actual behavior and the [Rust design](docs/superpowers/specs/2026-11-09-unified-harness-design.md) for the Rust spine. Design documents and feature packets describe intent; their presence does not establish implementation or approval.

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

## Run the Rust spine (macOS)

Requires macOS 26 on Apple silicon (`/usr/bin/sandbox-exec`), the Rust toolchain pinned in `rust-toolchain.toml` (`1.99.1`), Node 24, and Claude Code `2.1.318` and codex-cli `0.166.0`. There is no Linux build and no unsandboxed fallback: if the sandbox probe fails, nothing runs.

```bash
cargo build --workspace          # builds eng and eng-shim (they must sit side by side)
export PATH="$PWD/target/debug:$PATH"
eng login claude                 # Claude Code with a harness-owned config dir; type /login
eng login codex                  # `codex login` with a harness-owned CODEX_HOME
eng doctor                       # exit 0 only if every check passes
eng task new --repo . --packet specs/my-feature
eng decide <task> approve-spec   # needs a terminal; type yes
eng run <task>                   # exit 4 = waiting for you
eng status <task>
eng decide <task> accept         # or reject / send-back
eng audit verify                 # recomputes the hash chain
```

Every command accepts `--json` and `--state-dir <path>` (default `~/Library/Application Support/eng`). Exit codes: `0` ok, `2` usage, `3` rejected by policy, `4` awaiting a human, `5` failed, `70` internal. The pipeline is read from `.eng/pipeline.toml` in the repository. `ENG_CLAUDE_BIN`, `ENG_CODEX_BIN`, `ENG_NODE_BIN` and `ENG_SHIM_BIN` override where runtimes and the shim are found; `ENG_EXTRA_READ_DIRS` lets the sandbox read more directories.

A real Claude run is not stopped before it starts: it runs, and its result is then turned into `PolicyDenied` because Claude's own sandbox cannot yet be confirmed off (Task 11). Only fake agents complete a task today. Do not point `eng` at content that must not reach a hosted provider: no data-class or provider-authorization check runs before a hosted run. `eng login` starts each vendor's own login flow; the harness does not read the stored login.

### Testing the Rust spine

```bash
node scripts/check-deps.mjs
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace && ENG_BIN=target/debug/eng npm run conformance
```

The conformance cases drive only the `eng` binary, with fake agents from `conformance/fixtures/`. Results for each commit are in CI (macOS `rust` job) and `specs/rust-spine/handoff.md`; an earlier green run does not validate a newer commit. The [adapter fixture notes](crates/adapters/fixtures/README.md) identify every transcript as hand-written and provisional, and [the fake-runtime notes](conformance/fixtures/README.md) explain why a real Claude run cannot succeed yet. Fake-agent tests are not an end-to-end Claude Code demonstration.

## A short code walkthrough

This project is suitable for an architecture walkthrough with live offline tests:

1. Start with the [six-crate implementation plan](docs/superpowers/plans/2026-11-09-rust-spine.md): pure decision logic, durable state, execution, adapters, engine, and CLI.
2. Open the [Claude adapter](crates/adapters/src/claude.rs). It builds a headless `claude -p` launch plan with stream JSON, explicit tools, strict MCP configuration, and a pinned version.
3. Run the two focused tests below. The first rejects a reported success when the process/event indicates failure. The second shows that a clean review still stops for a human decision.
4. Finish with the fixture limitations and the remaining live-runtime work (plan Task 11).

From a macOS checkout:

```bash
cargo test --offline --locked -p harness-adapters --test claude \
  claude_exit_1_with_subtype_success_is_failed -- --exact
cargo test --offline --locked -p harness-core --test decide \
  clean_review_stops_at_awaiting_human_review_clean -- --exact
```

Their [adapter test](crates/adapters/tests/claude.rs) and [decision test](crates/core/tests/decide.rs) are also useful reading without a local toolchain.

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
| [crates/](crates/), [conformance/](conformance/) | Rust spine: six crates (`cli -> engine -> {store, exec, adapters} -> core`) and the end-to-end conformance suite |

Before contributing, read `AGENTS.md` and the relevant feature packet. Work in the assigned worktree, make the smallest scoped change, and report exact validation evidence. Agents cannot approve their own task phases or releases.
