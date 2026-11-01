# Design: ledger-write-guard

## API and schema changes

New files (no existing file changes):

| Path | Purpose |
|---|---|
| `harness/hooks/claude/ledger-guard.mjs` | `PreToolUse` hook for Claude Code. Reads the hook JSON on stdin, exits 2 with a reason on stderr to deny, exits 0 to allow. Node 22, no dependencies. |
| `config/claude/ledger-guard.config.example.json` | Shape of the root-owned config the hook reads from beside itself: `{ "ledger_roots": ["<absolute path>", ...] }`. |
| `config/claude/managed-settings.example.json` | Fragment to merge into Claude Code managed settings: a `permissions.deny` rule, the hook registration, `allowManagedHooksOnly`. |
| `tests/ledger-guard.test.mjs` | `node:test` suite. Copies the hook into a temp directory with its own config and drives it with sample hook inputs. |

Hook behavior:

1. Load `ledger-guard.config.json` from the script's own directory. Any failure denies.
2. Each configured root is checked as written and as resolved through symlinks.
3. `Edit`, `Write`, `MultiEdit`, `NotebookEdit`: take `tool_input.file_path` or
   `tool_input.notebook_path`, resolve it against the input's `cwd`, resolve symlinks on the
   deepest existing ancestor, and deny if the result is the root or inside it. A missing path
   denies. Containment uses `path.relative`, so `ledger-old` does not match `ledger`.
4. `Bash`: deny if `tool_input.command` contains any root string. This is a text match, so it
   over-blocks reads and under-blocks constructed paths. The OS permission covers the rest.
5. Any other tool or event: allow. The managed matcher already limits which calls arrive.

Managed settings fragment (see the example file):

- `permissions.deny`: `Edit(//<ledger-root>/**)`. An `Edit` rule covers the built-in edit tools;
  a `Write(...)` rule would never match.
- `hooks.PreToolUse`: matcher `Edit|Write|MultiEdit|NotebookEdit|Bash`, a `command` handler,
  `onFailure: "block"`, short `timeout`.
- `allowManagedHooksOnly: true`, as `docs/security/data-egress.md` already requires.

## Dependencies

- Node 22 on the machine that runs governed sessions (22.22.0 verified here).
- Claude Code managed settings (root-owned) for deployment. Hooks merge across levels, so the
  egress read hooks from `docs/security/data-egress.md` register next to this one.
- No new packages. No other packets.

## Migration plan

No migration. No database schema or persisted data changes.

## Rollout and rollback

- **Rollout:** install the script and its config in the root-owned managed hooks directory
  (directory 755, files 555), merge the settings fragment into the managed settings file, then
  run the manual checks in `acceptance-tests.md` in a real session before relying on it.
  A human approves the `Verified` transition through the ledger. This packet does not.
- **Rollback:** remove the hook entry and the deny rule from the managed settings file. The
  OS permission on `<ledger-root>` is unchanged and still blocks agent writes.
