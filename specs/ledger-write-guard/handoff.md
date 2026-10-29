# Handoff: ledger-write-guard / T1, T2 (draft)

| Field | Value |
|---|---|
| Base SHA | 0b9d58f7dfb1bf8a05b8726921628b56e8d63261 |
| Head SHA | b83af68 was the verified candidate; later commits on this branch change this file only |
| Author (runtime / session ID) | Claude Code / session_01AaDiMMkK2h8xk5xLXw1ZwG |
| Date | 2026-10-28 |

## Changed files

All new; no existing file was modified.

- `specs/ledger-write-guard/{requirements.md,design.md,tasks.json,acceptance-tests.md,handoff.md}` — feature packet, copied from `specs/_template/`
- `harness/hooks/claude/ledger-guard.mjs` — `PreToolUse` hook; denies writes to, and Bash commands naming, the ledger root
- `config/claude/ledger-guard.config.example.json` — shape of the root-owned config the hook reads
- `config/claude/managed-settings.example.json` — managed-settings fragment: deny rule, hook registration, `allowManagedHooksOnly`
- `tests/ledger-guard.test.mjs` — `node:test` suite (11 tests)

## Implemented requirements

- Edit/Write/MultiEdit/NotebookEdit into the root denied, including `..` and relative-to-`cwd` routes, and symlinks to existing files or directories in the root — T1. **Not** a symlink to a file that does not exist yet (see Known limitations, defect D1)
- Bash commands naming the root denied; other commands allowed — T1
- Writes elsewhere and a name-prefix sibling allowed — T1
- Fail closed on bad input, bad or missing config, missing path — T1
- Root taken from the config file only, not the environment — T1
- Example settings and config files written and parse as JSON — T2 (partly; see limitations)

## Tests run and results

| Command | Result | Evidence |
|---|---|---|
| `node --test tests/ledger-guard.test.mjs` | pass, 11 of 11 | terminal output, 2026-10-28 |
| Mutation check: containment always false; prefix match instead of path containment; `cwd` ignored; Bash check removed; symlink resolution removed | each mutant failed at least one test (1 to 6 tests); hook restored and diffed identical | terminal output, 2026-10-28 |
| `jq . config/claude/managed-settings.example.json config/claude/ledger-guard.config.example.json specs/ledger-write-guard/tasks.json` | pass | terminal output |
| `node --check harness/hooks/claude/ledger-guard.mjs` | pass | terminal output |
| Verify-change run on `git archive` of b83af68 (disposable copy): the three commands above | pass (11 of 11 tests) | terminal output, 2026-10-29 |
| Same run, 8 mutants (adds: fail open on bad JSON, environment override of the root, fail open on config) | 7 caught; 1 survived and is equivalent (an empty root list is still rejected by the next check) | terminal output, 2026-10-29 |
| 12 independent adversarial probes (not added to the suite) | 11 as expected; **1 failed: D1** | terminal output, 2026-10-29 |
| Freshness: HEAD, porcelain status and SHA-256 of 10 tracked files before and after | identical | `baseline.sha256` and `after.sha256` in the session scratchpad |
| Manual checks listed in `acceptance-tests.md` | **not run** | needs a real governed session |
| Stable test-case IDs and a generated catalog | **unavailable**: the project has no such mechanism; tests are identified by name only | n/a |

## Known limitations

- **D1 (confirmed defect, not fixed): dangling-symlink bypass.** A symlink in the worktree that points at a ledger file that does not exist yet is allowed, because `realpathLoose` cannot resolve it and falls back to the link's own path. A real write through such a link creates the file inside the ledger (reproduced). The OS permission on the ledger root would still block it. Violates the requirement "cannot create files under `<ledger-root>`". Fix sketch: in `realpathLoose`, when `realpathSync` fails, `lstatSync` the path; if it is a symlink, `readlinkSync` it, resolve relative to its directory, and recurse. Add a regression test first. Needs remediation authority.
- **Case-insensitive filesystems (unverified risk).** Containment is case-sensitive. On default macOS (APFS) `<ledger-root>/../LEDGER/x` may name the same directory and would be allowed. Cannot be checked from the Linux sandbox. Check on the target Mac.
- **The tests share an author with the hook.** The independent probes above are the only check written separately, and they are not in the suite.

- The Bash check is a substring match. It over-blocks reads and misses constructed paths (variable expansion, `cd` plus relative paths, interpreters). The OS permission on the ledger root is the real boundary.
- `~` is not expanded in file paths.
- The deny-rule syntax `Edit(//<abs>/**)` comes from a research subagent's summary of the permissions docs. It was not run against a real session here.
- The managed settings fragment is an example. It sets `allowManagedHooksOnly`, so the data-egress read hooks must be merged into the same managed file.
- The hook cannot tell a legitimate human or policy writer from an agent. It is meant for agent sessions only; the ledger writer runs as a different OS user outside Claude Code.
- Only Claude Code is covered. Codex and Cursor need their own packets.

## Unresolved decisions

- Real `<ledger-root>` path and `<harness-name>` — owner: human
- Whether Bash reads of the ledger should also be denied (this draft denies any reference, following `docs/architecture/approval-ledger.md`) — owner: human
- Whether an agent can edit the managed settings or hook on the target machine — owner: human, check before sign-off

## Assumptions made

- Package ID `ledger-write-guard` and the file locations above. The README layout proposes `harness/` and `config/` but does not name these subpaths.
- Node 22 is available where governed sessions run (22.22.0 present here).
- README "Testing" commands are placeholders, so the packet defines `node --test tests/ledger-guard.test.mjs` as its own acceptance command. The README was not edited.
- The macOS managed hooks directory `/Library/Application Support/ClaudeCode/hooks/` is the install location, following `docs/security/data-egress.md`.

## Exact next action

Decide whether to remediate D1 on this branch (add a failing regression test for the dangling symlink, then fix `realpathLoose`). Then run `/codex:adversarial-review` on the diff and record its findings and resolutions here (CLAUDE.md), and request the `Planned -> Approved` transition through the approval ledger. Agents do not mark it approved. After a human decides, complete the manual checks in `acceptance-tests.md` in a real governed session and on the target Mac (case sensitivity).
