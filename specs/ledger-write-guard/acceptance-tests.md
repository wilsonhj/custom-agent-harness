# Acceptance tests: ledger-write-guard

Each item names the command or procedure that proves it and the requirement it covers.
Check an item only when the result is recorded in `handoff.md`.

The README "Testing" section still holds placeholder commands, so this packet defines its own:
`node --test tests/ledger-guard.test.mjs` (Node 22).

## Behavior

- [ ] Edit/Write/MultiEdit/NotebookEdit into `<ledger-root>` is denied — `node --test tests/ledger-guard.test.mjs` — covers: tool writes denied
- [ ] `..` traversal, relative paths against `cwd`, and symlinks into the root are denied — `node --test tests/ledger-guard.test.mjs` — covers: tool writes denied
- [ ] `Bash` commands naming the root are denied — `node --test tests/ledger-guard.test.mjs` — covers: Bash denied
- [ ] Writes outside the root, and a sibling directory sharing the root's name prefix, are allowed — `node --test tests/ledger-guard.test.mjs` — covers: writes elsewhere unaffected
- [ ] Missing config, bad config, bad JSON input, and a missing path all deny — `node --test tests/ledger-guard.test.mjs` — covers: fail closed

## Invariants

- [ ] The root comes from the config file only; setting an environment variable cannot change it — `node --test tests/ledger-guard.test.mjs`
- [ ] Example files parse as JSON — `jq . config/claude/managed-settings.example.json config/claude/ledger-guard.config.example.json`

## Manual (needs a real governed session; not provable by the unit tests)

- [ ] With the example settings installed as managed settings and a real `<ledger-root>`, asking Claude Code to write a file there is denied by the deny rule alone (hook disabled) — record the transcript excerpt
- [ ] The same attempt is denied by the hook alone (deny rule removed) — record the transcript excerpt
- [ ] An agent cannot edit the managed settings file or the hook script (file ownership) — record `ls -l` output
- [ ] A hook crash blocks the call (`onFailure: "block"`) — record the transcript excerpt
- [ ] OS check: the agent user cannot write to `<ledger-root>` regardless of Claude Code — `sudo -u <agent-user> touch <ledger-root>/probe` fails

## Preview

Not applicable: no deployed service.
