# Custom Agent Harness

A prototype engineering control plane for Claude Code and Codex: keep the specification, candidate commit, verification results, review findings, and human decision together as durable artifacts.

The repository currently has two distinct tracks. The default `main` branch contains a working **offline Linux evidence-workflow pilot**. The **macOS Rust spine is under development in [open PR #13](https://github.com/wilsonhj/custom-agent-harness/pull/13)**; its components can be tested, but its end-to-end CLI and live Claude/Codex workflow are unfinished.

Status checked November 9, 2026. Rust source references below are pinned to PR head `0c39575`; the PR may advance.

## What works today

| Track | Implemented scope | Current limits |
|---|---|---|
| Linux pilot on `main` | Isolated worktree; operator-configured worker, verifier, and reviewer processes; content-addressed evidence; signed human review attestation; durable handoff | Linux, Node 24, Git, SSH signing tools, and working Bubblewrap namespaces. No hosted-model adapter, task-phase approval, migration, or deployment execution. |
| Rust spine in PR #13 | Task lifecycle and repair policy; Claude/Codex launch plans and stream parsers; process, sandbox, and Git modules; SQLite command log, hash-chained events, artifact store; engine and restart-recovery implementation | macOS-first development. `eng` currently implements only `--version`. Adapter transcripts are provisional fixtures; authenticated runtime compatibility is not established. |
| Company Brain | Separate [companion repository](https://github.com/wilsonhj/company-brain-starter) for company context and reviewed knowledge | Harness retrieval and knowledge-update integration are planned. No operational connection is established by this repository. |

See the [Linux operator guide](docs/local-workflow.md) for the pilot's actual behavior and the [Rust design](https://github.com/wilsonhj/custom-agent-harness/blob/0c395755a2528c7b85ffca4da7614a5bb08061bc/docs/superpowers/specs/2026-11-09-unified-harness-design.md) for the proposed next system. Design documents and feature packets describe intent; their presence does not establish implementation or approval.

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

## Inspect the Rust spine

Use a separate checkout to keep the two tracks distinct:

```bash
git clone --branch feat/rust-spine --single-branch \
  https://github.com/wilsonhj/custom-agent-harness.git custom-agent-harness-rust
cd custom-agent-harness-rust
git checkout --detach 0c395755a2528c7b85ffca4da7614a5bb08061bc
```

The branch pins Rust `1.99.1` in `rust-toolchain.toml`. The following offline checks require that toolchain and the locked Cargo dependencies to be installed and cached already. They exercise components without logging into Claude/Codex or making model calls:

```bash
cargo test --offline --locked -p harness-core -p harness-adapters -p harness-store
cargo run --offline --locked -q -p harness-cli -- --version
```

The version command prints `eng 0.1.0`. It does not establish a working task runner: [the CLI entry point](https://github.com/wilsonhj/custom-agent-harness/blob/0c395755a2528c7b85ffca4da7614a5bb08061bc/crates/cli/src/main.rs) currently handles only that argument. Commands such as `eng run`, `eng doctor`, and `eng decide` remain implementation work.

### Testing evidence

On November 9, the component test command above passed **113 tests** at predecessor commit `a74395b`: 84 core, 18 adapter, and 11 store tests. That result covers deterministic logic and fixture-based parsing, not live model behavior. The subsequently added engine is outside that local component-test result.

At `a74395b`, [CI run 42728921398](https://github.com/wilsonhj/custom-agent-harness/actions/runs/42728921398) also passed both Linux pilot and macOS Rust jobs, including formatting, Clippy, and workspace tests. Check [PR #13](https://github.com/wilsonhj/custom-agent-harness/pull/13) for results on later commits; an earlier green run does not validate a newer head or uncommitted work.

The [adapter fixture notes](https://github.com/wilsonhj/custom-agent-harness/blob/0c395755a2528c7b85ffca4da7614a5bb08061bc/crates/adapters/fixtures/README.md) identify every transcript as hand-written and provisional. Claude's sandbox-state confirmation is unresolved; [the current fake-runtime notes](https://github.com/wilsonhj/custom-agent-harness/blob/0c395755a2528c7b85ffca4da7614a5bb08061bc/conformance/fixtures/README.md) explain why real Claude runs are refused pending that decision. Component tests should not be presented as an end-to-end Claude Code demonstration.

## A short code walkthrough

This project is suitable for an architecture walkthrough with live offline tests:

1. Start with the [six-crate implementation plan](https://github.com/wilsonhj/custom-agent-harness/blob/0c395755a2528c7b85ffca4da7614a5bb08061bc/docs/superpowers/plans/2026-11-09-rust-spine.md): pure decision logic, durable state, execution, adapters, engine, and CLI.
2. Open the [Claude adapter](https://github.com/wilsonhj/custom-agent-harness/blob/0c395755a2528c7b85ffca4da7614a5bb08061bc/crates/adapters/src/claude.rs). It builds a headless `claude -p` launch plan with stream JSON, explicit tools, strict MCP configuration, and a pinned version.
3. Run the two focused tests below. The first rejects a reported success when the process/event indicates failure. The second shows that a clean review still stops for a human decision.
4. Finish with the fixture limitations and remaining CLI/live-runtime work.

From the Rust checkout:

```bash
cargo test --offline --locked -p harness-adapters --test claude \
  claude_exit_1_with_subtype_success_is_failed -- --exact
cargo test --offline --locked -p harness-core --test decide \
  clean_review_stops_at_awaiting_human_review_clean -- --exact
```

Both focused tests were verified at `a74395b`. Their [adapter test](https://github.com/wilsonhj/custom-agent-harness/blob/0c395755a2528c7b85ffca4da7614a5bb08061bc/crates/adapters/tests/claude.rs) and [decision test](https://github.com/wilsonhj/custom-agent-harness/blob/0c395755a2528c7b85ffca4da7614a5bb08061bc/crates/core/tests/decide.rs) are also useful reading without a local toolchain.

## Trust boundaries and planned integration

Instructions in `AGENTS.md`, skills, repository text, and model output do not grant execution or approval authority. The Linux pilot enforces isolation outside the worker and records independent verification/review evidence. Its production approval policy remains fail closed; see [transition policy](harness/policy/transitions.ts).

The Rust design targets a human-approved packet → Claude implementation → harness verification → Codex review → bounded repair → human decision. A reviewer's pass ends the repair loop; accepting an exact candidate commit remains a human action. The signed, anchored approval ledger is deferred in that design. See [design sections 6 and 12](https://github.com/wilsonhj/custom-agent-harness/blob/0c395755a2528c7b85ffca4da7614a5bb08061bc/docs/superpowers/specs/2026-11-09-unified-harness-design.md#6-running-an-agent) for the distinction from the Linux pilot.

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
| [PR #13 Rust crates](https://github.com/wilsonhj/custom-agent-harness/tree/0c395755a2528c7b85ffca4da7614a5bb08061bc/crates) | In-progress Rust implementation; absent from default `main` |

Before contributing, read `AGENTS.md` and the relevant feature packet. Work in the assigned worktree, make the smallest scoped change, and report exact validation evidence. Agents cannot approve their own task phases or releases.
