# Conformance fixtures

Fake agent runtimes used by the engine's integration tests (`crates/engine/tests/`) and, from
Task 10, by the conformance suite. They never call a model or the network.

The current production R-11 gate returns `ContainmentUnavailable` before invoking
any real or fake runtime. Execution-dependent tests are explicitly skipped; Task 11
is unavailable until a real strong containment backend exists.

| File | Stands in for | Output format |
|---|---|---|
| `fake-claude.mjs` | `claude` 2.1.318 | stream-json, shapes from `crates/adapters/fixtures/claude/` |
| `fake-codex.mjs` | `codex` 0.166.0 | `exec --json`, shapes from `crates/adapters/fixtures/codex/` |

Both print the pinned version for `--version`, read the prompt on stdin, and pick a scenario
from the `FAKE_SCENARIO` environment variable (see the comment at the top of each file). The
engine passes no environment of its own to a run beyond the adapter plan, `PATH`, `HOME` and
`TMPDIR`, so tests set the scenario in a small wrapper script that execs `node` on the fake.

At the adapter-parser level, fake Claude's synthetic `init` event carries
`"sandbox": {"enabled": false}`, the field checked before reporting
`own_sandbox_confirmed_off`. Real Claude Code documents no such field (see
`crates/adapters/fixtures/README.md`); its compatibility remains unverified. Scenario
`sandbox_unconfirmed` omits that field to exercise the adapter-local check; reaching
the later engine confirmation check would yield `PolicyDenied`. Current production
refusal is earlier: `ContainmentUnavailable` prevents invocation of every runtime,
so these synthetic flags do not establish runtime compatibility or reopen Task 11.
