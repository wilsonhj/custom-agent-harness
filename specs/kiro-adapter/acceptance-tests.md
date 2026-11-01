# Acceptance tests: kiro-adapter

Each item names the command or procedure that proves it and the requirement it covers.
Check an item only when the result is recorded in `handoff.md`.

No adapter, test harness or `kiro-cli` exists in this repository yet, so the commands
below are named placeholders in the form `tests/kiro/<name>` to be created by T5. They do
not exist and nothing here has been run. Do not report any item as passing before then.

## Behavior

- [ ] Run is non-interactive, executes in the task worktree at the given base SHA, and records exit status and output — `tests/kiro/run-basic` — covers: K-1
- [ ] Two concurrent launches on one worktree: exactly one starts; lock is released after exit, crash and cancel — `tests/kiro/worktree-lock` — covers: K-2
- [ ] Output that is unparseable, fails the schema, or claims "approved" does not advance task state — `tests/kiro/output-validation` — covers: K-11
- [ ] Output is stored by the adapter with a verifiable sha256; the Kiro process cannot write the evidence store — `tests/kiro/evidence-store` — covers: K-11, K-16
- [ ] Deadline and cancel kill the whole process tree, including MCP/subagent children — `tests/kiro/cancel` — covers: K-13
- [ ] Compatibility test fails on a `kiro-cli` version other than the pinned one — `tests/kiro/version-pin` — covers: K-12
- [ ] Forbidden flags are never passed; the permission profile maps to explicit tool permissions — `tests/kiro/flags` — covers: K-18
- [ ] Kiro sees `AGENTS.md`, and a planted steering file cannot weaken it — `tests/kiro/instructions` — covers: K-20

## Invariants

- [ ] Run as the adapter's OS user, the Kiro process cannot read or write `<ledger-root>`, the signing keys or the anchor deploy key — `tests/kiro/ledger-denied` — covers: K-3
- [ ] A transition request is written only under `<local-artifacts-directory>/requests/`; Kiro cannot mark any transition approved — `tests/kiro/request-only` — covers: K-3, K-16
- [ ] Canary `local-only` file is denied via file read, grep, glob, shell `cat`, symlink, `../` traversal, `$(...)`, interpreter one-liner, `git show`, MCP and retrieval — `tests/kiro/canary-read` — covers: K-5, K-9. Include an MCP call allowed by a scoped `mcp` allow rule and one in a trusted workspace; denial must come from `deny` rules, not from prompting
- [ ] The same canary is denied when Kiro runs with no `--agent`, with a different agent, with a subagent, and with a worktree-planted `.kiro/` agent, hook or MCP file — `tests/kiro/canary-agents` — covers: K-5, K-7, K-8
- [ ] Canary content placed in steering or agent `resources` is not loaded into the session — `tests/kiro/canary-context` — covers: K-19
- [ ] On V3, a subagent whose own rules allow a tool the parent denies is still blocked; on a 2.x engine the adapter refuses to run — `tests/kiro/subagent-limits` — covers: K-7
- [ ] A worktree-planted `.kiro/` file cannot override managed config or the harness-supplied agent — `tests/kiro/config-precedence` — covers: K-4, K-8
- [ ] An unpinned login/account is refused — `tests/kiro/unpinned-account` — covers: K-4
- [ ] Launch against a `baa-covered-providers` or `local-only` fixture is refused; the class is assigned by the adapter, not the session — `tests/kiro/egress-gate` — covers: K-10, K-14, Launch procedure step 3
- [ ] With no recorded provider approval, a non-fixture Kiro job is refused — `tests/kiro/provider-approval` — covers: K-17, Launch procedure step 2
- [ ] A canary secret in the parent environment (including `SSH_AUTH_SOCK`) is absent from the Kiro process and from stored artifacts and logs — `tests/kiro/env-scrub` — covers: K-15
- [ ] A crashing, timing-out or non-0/2-exiting PreToolUse hook still results in the tool call being blocked (wrapper exits 2 on any error); a read is denied by admin `deny` rules even with all hooks disabled — `tests/kiro/hook-failure` — covers: K-5
- [ ] `--agent <missing>` exits 4 and the adapter treats any tool calls made by the built-in default agent before that as a failed, untrusted run — `tests/kiro/agent-missing` — covers: K-1, K-5
- [ ] The run uses `--sandbox` and exits if the sandbox cannot start; MCP wrapping failure is fatal — `tests/kiro/sandbox-required` — covers: K-9, K-18
- [ ] An unattended `--no-interactive --v3` run with a planted worktree `.kiro/` hook, agent, MCP server and steering file: the adapter's pre-launch step removes or rejects them, and a read of a canary is still denied by admin `deny` rules if the planted config does load — `tests/kiro/workspace-config` — covers: K-5, K-8
- [ ] Enabling flag, provider decision and egress-profile entry are not writable by the Kiro process user — `tests/kiro/enable-flag-root-owned` — covers: K-22, Launch procedure step 1
- [ ] A `tools` list containing `@builtin` or `*` is rejected by the adapter's agent validation; only `subagent/<name>` entries are accepted — `tests/kiro/subagent-tools` — covers: K-7
- [ ] A hook run from a runtime-specific root-owned script identifies its runtime from the script, not from input — `tests/kiro/hook-identity` — covers: K-6

## Release

- [ ] The migration classifier (`<migration-classifier-command>`, docs/architecture/release-gate.md) reports the release diff as `clean`; otherwise full migration evidence is required — covers: design.md "Migration plan"
