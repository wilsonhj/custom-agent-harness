# Acceptance tests: rust-spine

Each item names the command that proves it and the requirement it covers.
Check an item only when the result is recorded in `handoff.md`.

## Behavior

- [ ] Crate dependency direction and forbidden APIs — `node scripts/check-deps.mjs` — covers: R-1, R-2
- [ ] Lint policy and unit tests — `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` — covers: R-1 to R-16
- [ ] Idempotent commands, hash chain, crash consistency — `cargo test -p harness-store` — covers: R-3, R-4
- [ ] Restart recovery per effect type — `cargo test -p harness-engine --test recovery` — covers: R-5, R-11
- [ ] Repair loop and limits — `cargo test -p harness-core --test decide --test props` — covers: R-6, R-16
- [ ] Human-decision gate — `cargo test -p harness-core --test lifecycle` and `ENG_BIN=target/debug/eng node --test conformance/cases/decide.test.mjs` — covers: R-7
- [ ] Sandbox denial and fail-closed probe — `cargo test -p harness-exec --test sandbox_profile --test sandbox_live` and `conformance/cases/flow.test.mjs` (sandbox-unavailable case) — covers: R-8, R-9
- [ ] Egress allowlist — `cargo test -p harness-exec --test egress` — covers: R-10
- [ ] Operator egress authorization, policy audit binding and revocation — `cargo test -p harness-engine --test egress_policy` — covers: `docs/security/data-egress.md`
- [ ] Process-tree kill — `cargo test -p harness-exec --test supervisor` — covers: R-11
- [ ] Secret scan and hardened host Git — `cargo test -p harness-exec --test workspace` — covers: R-12
- [ ] Adapter plans, parsers, version pin — `cargo test -p harness-adapters` — covers: R-13, R-14, R-15
- [ ] Full flow with fake agents — `ENG_BIN=target/debug/eng npm run conformance` — covers: R-3 to R-8

## Invariants

- [ ] No path to `Verified` without a human `Accept` for that commit — `cargo test -p harness-core --test props`
- [ ] Transition table equals the TypeScript oracle — `cargo test -p harness-core --test lifecycle allowed_table_equals_golden`
- [ ] Existing Node suites still pass — `npm test`

## Migration

No migration (see design.md).

## Preview

Not applicable: no deployment. In its place, supervised live tests with the owner present:

- [ ] Claude and Codex run inside the harness sandbox with their own sandboxes off — plan Task 11 step 2 — covers: R-15
- [ ] An untrusted repository's hooks and MCP servers do not load — plan Task 11 step 5 — covers: R-9
- [ ] Recorded transcripts replace provisional fixtures and parsers pass — `cargo test -p harness-adapters` — covers: R-14
