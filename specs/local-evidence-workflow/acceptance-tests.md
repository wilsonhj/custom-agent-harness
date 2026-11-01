# Acceptance tests: local-evidence-workflow

- `npm test`: fixture-repository orchestration, valid signed fixture decision, rejected
  signatures/targets/evidence, read-only reviewer contract, concurrency, failed commands,
  deadlines/output limits, incomplete packets, altered packet and candidate, tampering.
- `npm run check`: parse every new JS module with node --check.
- `HARNESS_REQUIRE_SANDBOX=1 npm test`: the same suite plus mandatory real bubblewrap
  workflow and denied outside-write/network/read tests. Without the variable, an unsupported
  host reports an explicit skip; this is not successful sandbox verification.
- `node harness/local/cli.mjs doctor`: fail closed if the real sandbox cannot execute.
- `git diff --check`: whitespace verification.

Every actual result and limitation is recorded in handoff.md. Test-only injected execution
is used for deterministic orchestration tests; the CLI never exposes an unsandboxed mode.
