# Conformance fixtures

Fake agent runtimes used by the engine's integration tests (`crates/engine/tests/`) and, from
Task 10, by the conformance suite. They never call a model or the network.

| File | Stands in for | Output format |
|---|---|---|
| `fake-claude.mjs` | `claude` 2.1.318 | stream-json, shapes from `crates/adapters/fixtures/claude/` |
| `fake-codex.mjs` | `codex` 0.166.0 | `exec --json`, shapes from `crates/adapters/fixtures/codex/` |

Both print the pinned version for `--version`, read the prompt on stdin, and pick a scenario
from the `FAKE_SCENARIO` environment variable (see the comment at the top of each file). The
engine passes no environment of its own to a run beyond the adapter plan, `PATH`, `HOME` and
`TMPDIR`, so tests set the scenario in a small wrapper script that execs `node` on the fake.

The fake Claude's `init` event carries `"sandbox": {"enabled": false}`, the field the adapter
checks before it reports `own_sandbox_confirmed_off`. Real Claude Code documents no such field
(see `crates/adapters/fixtures/README.md`); until Task 11 decides the check, real Claude runs
are refused with `PolicyDenied`. Scenario `sandbox_unconfirmed` reproduces that.
