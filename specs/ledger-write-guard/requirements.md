# Requirements: ledger-write-guard — Claude Code write guard for the approval ledger

## User-visible behavior

- A Claude Code session governed by managed settings cannot create, edit, or delete files under
  `<ledger-root>` with the `Edit`, `Write`, `MultiEdit`, or `NotebookEdit` tools. The attempt is
  denied and the agent receives a one-line reason.
- A `Bash` tool call whose command text names `<ledger-root>` is denied with the same reason.
- Writes elsewhere (the worktree, `<local-artifacts-directory>/requests/`) are unaffected.
- A missing, unreadable, or malformed guard configuration, or malformed hook input, denies the
  call instead of allowing it (fail closed).

## Non-goals

- Replacing the OS permission boundary on `<ledger-root>`. That remains the real control.
- Preventing every shell evasion (variable expansion, `cd` plus relative paths, interpreters
  that build the path at run time). The hook cannot see those.
- Codex CLI and Cursor equivalents. They need their own packets (different hook formats).
- Read guards for `local-only` content. Those belong to the data-egress read hooks.
- Writing to, signing, or verifying the ledger itself.

## Invariants

- Agents never write to `<ledger-root>` and never mark their own approvals
  (`AGENTS.md`, "Approvals and state").
- The guard reads `<ledger-root>` from a root-owned configuration file beside the script, never
  from the environment, because anything in the hook's environment can be set by the session.
- The guard has no network access and sends no content off the machine.

## Cited company decisions

| Constraint | Source (ADR / policy / brain doc ID @ revision) |
|---|---|
| Agents may only request transitions; the ledger lives outside every worktree | `docs/architecture/approval-ledger.md`, "Storage" and "Who writes, and how it is enforced" (adopted 2026-10-26) |
| Managed config denies writes to `<ledger-root>/**` and a managed hook rejects shell commands referencing it, as defence in depth behind the OS permission | `docs/architecture/approval-ledger.md`, "Who writes, and how it is enforced" |
| Managed hooks only (`allowManagedHooksOnly`); each runtime has its own root-owned script (directory 755, script 555); fail closed with `onFailure: "block"` | `docs/security/data-egress.md`, "Enforcement" layers 3 and 4 |
| `Edit(...)` deny rules cover the built-in edit tools; `Write(path)` rules do not match; hook exit 2 blocks a `PreToolUse` call; a failed hook does not block unless `onFailure: "block"` | https://code.claude.com/docs/en/permissions and https://code.claude.com/docs/en/hooks, as read by a research subagent on 2026-10-28 (unverified against raw text) |

## Unresolved questions

- [ ] What is the real `<ledger-root>` path and `<harness-name>`? The ledger doc leaves both as
  placeholders. — owner: human, blocks: T2 deployment (not T1)
- [ ] Does `Edit(//<abs>/**)` match as expected on the installed Claude Code (2.1.312 here)?
  Needs a real session with the example file installed as managed settings. — owner: human, blocks: T2 sign-off
- [ ] Can an agent edit the managed or project settings file to remove the guard? Managed files are root-owned by design; confirm on the target machine. — owner: human, blocks: T2 sign-off
- [ ] Should reads of `<ledger-root>` through `Bash` (for example `cat`) also be denied? This packet denies any reference, as the ledger doc says. — owner: human, blocks: none
