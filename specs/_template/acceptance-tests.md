# Acceptance tests: <feature-id>

Each item names the command or procedure that proves it and the requirement it covers.
Check an item only when the result is recorded in `handoff.md`.

## Behavior

- [ ] <Requirement> — `<test command>` — covers: <requirements.md bullet>

## Invariants

- [ ] <Invariant still holds> — `<test command>`

## Migration (delete if design.md says "No migration")

- [ ] Migration applies cleanly on a preview database — `<command>`
- [ ] Currently deployed code runs against the new schema — `<command>`
- [ ] Recovery procedure rehearsed or documented — `<link>`

## Preview

- [ ] Smoke / browser checks pass on the preview deployment — `<command>`
