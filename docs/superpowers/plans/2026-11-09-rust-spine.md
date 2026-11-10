# Rust Spine (first slice) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A macOS CLI, `eng`, that takes a human-approved spec packet, has Claude Code implement it in a sandboxed worktree, verifies it, has Codex review it read-only, repairs for up to two rounds, and stops for a human decision.

**Architecture:** A Cargo workspace of six crates with one dependency direction (`cli -> engine -> {store, exec, adapters} -> core`). `core` is pure decision logic; `store` is SQLite on one writer thread; `exec` is the only crate that spawns processes; adapters only build launch plans and parse output. Every state change is a command committed with its events and pending work in one transaction before any work runs.

**Tech Stack:** Rust (edition 2024, stable toolchain pinned in `rust-toolchain.toml`), serde, serde_json, sha2, thiserror, schemars, rusqlite (`bundled`), tokio, tokio-util, clap, proptest (dev). Node 24 `node:test` for the language-neutral conformance suite. macOS `/usr/bin/sandbox-exec`. Dependency versions: latest stable at Task 1, fixed by a committed `Cargo.lock`.

**Spec:** `docs/superpowers/specs/2026-11-09-unified-harness-design.md` (approved 2026-11-09). Evidence: `docs/superpowers/specs/research-2026-11-09/`. Feature packet: `specs/rust-spine/`.

## Global Constraints

- Platform: macOS 26 on Apple silicon only. Linux is the first follow-up, not this plan.
- Pinned runtimes: Claude Code `2.1.318`, codex-cli `0.166.0`. A different `--version` fails the run with `VersionMismatch` before any task content is sent.
- `harness-core` must not use `std::fs`, `std::process`, `std::time::SystemTime`, any RNG, tokio or rusqlite. Time and IDs arrive as data.
- `harness-adapters` must not depend on tokio and must never spawn a process.
- Only `harness-store` touches SQLite. Only `harness-exec` spawns processes or runs Git.
- Clippy denies `unwrap_used`, `expect_used`, `await_holding_lock` in non-test code.
- If the sandbox probe fails, nothing runs (`SandboxUnavailable`). There is no unsandboxed fallback and no flag to add one.
- The harness never reads, stores, copies or forwards a subscription login token.
- Before hosted spawn, enforce the content class and explicit project/provider authorization
  for every readable input. Missing or unknown class is `local-only`; missing authorization,
  unverifiable provider configuration or unenforced read isolation is `PolicyDenied`.
  BAA-restricted content requires a current verified exact runtime/account/settings entry
  (`docs/security/data-egress.md`, Egress classes, BAA configuration allowlist, Enforcement;
  design §9, packet R-17). A login, spec approval or host allowlist grants no egress authority.
- Before opening the task gate, establish descendant containment across detachment and
  responsibility disclaim. A persistent supervisor alone is insufficient; unavailable or
  unproven containment fails closed with `SandboxUnavailable` (design §6.2, packet R-11).
- Agents never run Git and never commit. The harness makes every commit under `refs/harness/tasks/<task-id>/cp/<n>`.
- Agent output is a claim. Only `HarnessEvidence` and `ReviewFindings` artifacts may be cited as evidence.
- A task reaches `Approved` or `Verified` only through a `HumanDecision` for an exact digest or commit.
- Defaults: `max_repair_rounds = 2`, `severity_threshold = High`, `review_chunk_max_lines = 400`, verification `max_attempts = 2`, kill grace `5 s`, group-empty poll bound `2 s`.
- State directory: `~/Library/Application Support/eng/` (mode `0700`; `state.db` mode `0600`).
- Binary name: `eng`.
- Every commit in this plan is made on the task's own branch in its own worktree; nothing is pushed without the owner asking.
- No code is copied from another project until the owner chooses a license (design §14 Q5).

## Review Focus

Failure modes the spec implies that are most likely to bite, each pinned by a test in the task that owns the code:

1. A state or worktree path reached through a symlink (`/tmp` vs `/private/tmp`) or containing spaces: deny rules must still hold. → Task 6, `profile_resolves_real_paths`.
2. An agent emits an oversized line, invalid UTF-8, or a truncated final line: the run fails with a typed code, never a panic or a hang. → Task 8, `malformed_stream_is_invalid_agent_output`; Task 5, `output_cap_triggers_kill`.
3. A subscription rate limit mid-run: the task pauses with `LimitExhausted`, and the round does not count against the repair limit. → Task 4, `limit_exhausted_does_not_consume_repair_round`.
4. A reviewer finding names a path outside the repository (`../`, absolute, symlink): the review is rejected. → Task 2, `finding_path_outside_repo_is_rejected`.
5. A second `eng` process started on the same state directory: it refuses to start instead of corrupting the log. → Task 9, `second_engine_refuses_to_start`.

## Execution

Method supplied by the owner: subagent-driven, parallel where interfaces allow, Sonnet agents for simple tasks and Opus agents for hard ones, with the lead architect integrating.

| Task | Depends on | Tier | Parallel group |
|---|---|---|---|
| 1 Workspace and CI | — | Sonnet | A |
| 2 Core types | 1 | Opus | B |
| 3 Core state machines | 2 | Opus | C |
| 4 Core `decide` | 3 | Opus | D |
| 5 Exec: supervisor | 2 | Opus | E (∥ 3, 4, 6, 7, 8) |
| 6 Exec: macOS sandbox and egress proxy | 2 | Opus | E |
| 7 Exec: workspace and host Git | 2 | Opus | E |
| 8 Adapters: Claude and Codex | 2 | Sonnet | E |
| 9 Store and engine | 4, 5, 6, 7, 8 | Opus | F |
| 10 CLI and conformance flow | 9 | Sonnet | G |
| 11 Live tests and first real run | 10, owner present | Lead + owner | H |

Group E tasks touch disjoint files and consume only Task 2's types, so they run in separate worktrees.

## File Structure

```
Cargo.toml  rust-toolchain.toml  Cargo.lock  deny.toml
scripts/check-deps.mjs                 dependency-direction and forbidden-API check
scripts/export-transitions.mjs         writes conformance/golden/transitions.json from the TS oracle
conformance/golden/transitions.json
conformance/fixtures/{make-repo.mjs, fake-claude.mjs, fake-codex.mjs, packet/, pipeline.toml}
conformance/cases/*.test.mjs           drive $ENG_BIN only
crates/core/src/{lib,ids,types,artifact,finding,outcome,capabilities,lifecycle,stage,policy,decide,prompt,diff}.rs
crates/store/src/{lib,schema.sql,writer,events,artifacts}.rs
crates/exec/src/{lib,supervisor,shim,sandbox_macos,egress,workspace,hostgit,secretscan}.rs
crates/adapters/src/{lib,claude,codex}.rs   crates/adapters/fixtures/{claude,codex}/*.jsonl
crates/engine/src/{lib,worker,recovery}.rs
crates/cli/src/{main,doctor,output}.rs
specs/rust-spine/{requirements,design,acceptance-tests,handoff}.md  specs/rust-spine/tasks.json
```

---

### Task 1: Workspace, toolchain and CI

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `deny.toml`, `crates/{core,store,exec,adapters,engine,cli}/Cargo.toml`, `crates/*/src/lib.rs` (`cli`: `main.rs`), `scripts/check-deps.mjs`, `scripts/export-transitions.mjs`, `conformance/golden/transitions.json`
- Modify: `.github/workflows/local-workflow.yml` (add a `rust` job on `macos-latest`), `package.json` (add `"conformance": "node --test conformance/cases"`)

**Interfaces:**
- Consumes: `harness/policy/transitions.ts` exports (`ALLOWED_TRANSITIONS`, `canTransition`).
- Produces: packages `harness-core`, `harness-store`, `harness-exec`, `harness-adapters`, `harness-engine`, `harness-cli` (binary `eng`); `conformance/golden/transitions.json` with shape `{ "states": [..16], "allowed": { "<from>": ["<to>", ..] }, "matrix": [ { "from", "to", "approval": bool, "blocked_record": bool, "allowed": bool } ] }`.

- [ ] **Step 1: Owner installs the Rust toolchain.** The harness builder does not download or run installers. Owner runs `rustup` from https://rustup.rs, then `rustc --version` prints a stable version. Record that exact version in `rust-toolchain.toml` (`channel = "<version>"`, `components = ["clippy", "rustfmt"]`).
- [ ] **Step 2: Write `scripts/check-deps.mjs`.** It runs `cargo metadata --format-version 1` and exits non-zero if: `harness-core` depends on `tokio` or `rusqlite`; `harness-adapters` depends on `tokio`; any crate other than `harness-store` depends on `rusqlite`; any edge violates `cli -> engine -> {store, exec, adapters} -> core`; or `crates/core/src` contains the strings `std::fs`, `std::process`, `SystemTime::now`, `rand::`.
- [ ] **Step 3: Run it before the crates exist.** Run: `node scripts/check-deps.mjs`. Expected: FAIL (`cargo metadata` finds no workspace).
- [ ] **Step 4: Create the workspace.** Six member crates, edition 2024, `[workspace.lints.clippy] unwrap_used = "deny"`, `expect_used = "deny"`, `await_holding_lock = "deny"`; each crate sets `[lints] workspace = true`. `crates/cli` has `[[bin]] name = "eng"` and `main` prints `eng <CARGO_PKG_VERSION>` for `--version`.
- [ ] **Step 5: Write `scripts/export-transitions.mjs`.** Import the TS module with Node type stripping; emit all 16×16 pairs × `approval ∈ {false,true}` × `blocked_record ∈ {false,true}` using the existing `canTransition`, sorted by `(from, to, approval, blocked_record)`.
- [ ] **Step 6: Verify.** Run: `node scripts/export-transitions.mjs && node scripts/check-deps.mjs && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo run -q -p harness-cli -- --version`. Expected: all pass; last line `eng 0.1.0`; `transitions.json` has 16 states and 1024 matrix rows.
- [ ] **Step 7: Commit.** `git add -A && git commit -m "build: Rust workspace, lint policy, dependency check, transitions golden"`

### Task 2: Core types and wire format

**Files:**
- Create: `crates/core/src/{ids,types,artifact,finding,outcome,capabilities}.rs`
- Test: `crates/core/tests/wire.rs`

**Interfaces:**
- Consumes: nothing.
- Produces (all `Serialize + Deserialize`, `snake_case` on the wire, enums tagged with `"type"` where they carry data):

```rust
// ids.rs — newtypes over String; constructed from data, never generated in core
pub struct TaskId; pub struct CommandId; pub struct EffectId; pub struct EventId;
pub struct ArtifactId;          // lowercase hex sha256, 64 chars; ArtifactId::of(bytes: &[u8])
pub struct CommitSha;           // 40 lowercase hex
pub struct IdempotencyKey; pub struct Seq(pub u64); pub struct Timestamp(pub i64); // unix micros

// types.rs
pub enum Stage { Research, Spec, Implement, Review, Repair, Operate }
pub enum RuntimeKind { Claude, Codex }
pub enum ModelFamily { Anthropic, OpenAi }
pub enum Role { Implementer, Reviewer }
pub enum Actor { Human { principal: String }, Engine, Effect { effect: EffectId } }
pub struct RepoRef { pub path: String, pub git_common_dir: String }
pub struct Budget { pub max_repair_rounds: u8, pub run_deadline_ms: u64, pub max_turns: u32 }
pub struct Envelope<T> { pub v: u16, pub id: String, pub task: TaskId, pub at: Timestamp,
                         pub actor: Actor, #[serde(flatten)] pub body: T }
pub const WIRE_VERSION: u16 = 1;

// artifact.rs
pub enum MediaType { Json, Jsonl, Patch, Markdown, Text, Binary }
pub enum Provenance { Input, RuntimeClaim, HarnessEvidence, ReviewFindings, HumanStatement }
pub struct Artifact { pub id: ArtifactId, pub size: u64, pub media_type: MediaType,
                      pub label: Provenance, pub produced_by: Option<EffectId> }
impl Provenance { pub fn is_evidence(self) -> bool }   // true only for HarnessEvidence, ReviewFindings

// finding.rs
pub enum Severity { Critical, High, Medium, Low }      // Ord: Critical > High > Medium > Low
pub enum FindingCategory { Correctness, Security, SpecMismatch, Tests, Other }
pub enum Verdict { Pass, ChangesRequested }
pub struct Location { pub path: String, pub line_start: u32, pub line_end: u32 }
pub struct RawFinding { pub severity: Severity, pub category: FindingCategory,
    pub file: Option<String>, pub line_start: Option<u32>, pub line_end: Option<u32>,
    pub claim: String, pub failure_scenario: String, pub recommendation: String,
    pub suggested_check: Option<String>, pub confidence: f32 }
pub struct RawReview { pub verdict: Verdict, pub summary: String, pub findings: Vec<RawFinding> }
pub struct Finding { pub id: String, pub severity: Severity, pub blocking: bool,
    pub category: FindingCategory, pub location: Option<Location>, pub claim: String,
    pub failure_scenario: String, pub recommendation: String,
    pub reviewed_commit: CommitSha, pub reviewer: RuntimeKind }
pub struct Review { pub verdict: Verdict, pub summary: String, pub findings: Vec<Finding> }
pub fn validate_review(raw: RawReview, commit: &CommitSha, reviewer: RuntimeKind,
                       threshold: Severity) -> Result<Review, ReviewRejected>;
pub fn review_json_schema() -> serde_json::Value;       // schemars, for RawReview

// outcome.rs
pub enum FailureCode { ExitNonzero, DeadlineExceeded, OutputLimitExceeded, SpawnError,
    SandboxUnavailable, VersionMismatch, ConfigChanged, InvalidAgentOutput,
    PacketChanged, CandidateChanged, GitMetadataTouched, SecretInDiff,
    PolicyDenied, LostOnRestart }
pub enum RunOutcome { Succeeded, Failed { code: FailureCode },
                      LimitExhausted { reset_at: Option<Timestamp> }, Cancelled }
pub enum TerminationReason { Succeeded, TaskFailure, BudgetExhausted,
                             InfrastructureFailure, PolicyViolation, Cancelled }

// capabilities.rs
pub enum OwnSandbox { MustDisable, Compatible }
pub enum StructuredOutput { None, JsonSchemaFile, JsonSchemaInline }
pub struct Capabilities { pub runtime: RuntimeKind, pub model_family: ModelFamily,
    pub pinned_version: &'static str, pub roles: &'static [Role],
    pub structured_output: StructuredOutput, pub session_resume: bool,
    pub reports_usage: bool, pub own_sandbox: OwnSandbox }
```

`validate_review` rules: assign ids `F1..Fn` in order; `blocking = severity >= threshold`; a `file` must be repo-relative with no `..` component and not absolute, else `ReviewRejected::PathOutsideRepo`; `line_start <= line_end`; `claim` at most 2,000 chars else truncated; `Verdict::Pass` with any blocking finding is `ReviewRejected::PassWithBlocking`.

- [ ] **Step 1: Write the failing tests** in `crates/core/tests/wire.rs`:

```rust
#[test] fn artifact_id_is_sha256_hex() {
    assert_eq!(ArtifactId::of(b"abc").as_str(),
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"); }
#[test] fn envelope_roundtrips_with_flattened_body() { /* to_string then from_str equals input; JSON has "v":1 and "type" at top level */ }
#[test] fn unknown_field_in_raw_review_is_rejected() { /* serde deny_unknown_fields */ }
#[test] fn only_two_provenances_are_evidence() { /* is_evidence true for exactly HarnessEvidence, ReviewFindings */ }
#[test] fn blocking_is_derived_from_threshold_not_model() { /* High finding, threshold High => blocking; Medium => not */ }
#[test] fn finding_path_outside_repo_is_rejected() { /* "../x", "/etc/passwd", "a/../../b" => PathOutsideRepo */ }
#[test] fn pass_with_blocking_finding_is_rejected() {}
#[test] fn review_schema_names_every_raw_finding_field() { /* schema properties == struct fields */ }
```

- [ ] **Step 2: Run.** `cargo test -p harness-core --test wire`. Expected: FAIL (types undefined).
- [ ] **Step 3: Implement the types above.** `RawReview` and `RawFinding` use `#[serde(deny_unknown_fields)]`. No other logic.
- [ ] **Step 4: Run.** Same command plus `node scripts/check-deps.mjs`. Expected: PASS.
- [ ] **Step 5: Commit.** `git commit -am "feat(core): ids, wire types, findings validation, outcomes, capabilities"`

### Task 3: Core state machines

**Files:**
- Create: `crates/core/src/{lifecycle,stage}.rs`
- Test: `crates/core/tests/lifecycle.rs`

**Interfaces:**
- Consumes: Task 2 types; `conformance/golden/transitions.json`.
- Produces:

```rust
// lifecycle.rs
pub enum TaskState { Draft, Planned, Approved, Implementing, Reviewing, Verified,
    PreviewVerified, MigrationGated, Released, Observed, Closed,
    Blocked, Failed, Cancelled, Repair, Superseded }
pub const ALLOWED: &[(TaskState, &[TaskState])];
pub enum DecisionKind { ApproveSpec, Accept, Reject, SendBack }
pub enum Subject { PacketDigest(ArtifactId), Commit(CommitSha) }
pub struct HumanDecision { /* private fields */ }
impl HumanDecision {
    pub fn from_event(kind: DecisionKind, subject: Subject, principal: String, seq: Seq) -> Self;
    pub fn kind(&self) -> DecisionKind; pub fn subject(&self) -> &Subject; }
pub struct TransitionContext { pub packet: Option<ArtifactId>, pub candidate: Option<CommitSha>,
                               pub blocked_from: Option<TaskState> }
pub enum Denied { NotInTable, ApprovalRequired, WrongSubject, WrongKind, Unreachable }
pub fn can_transition(from: TaskState, to: TaskState, decision: Option<&HumanDecision>,
                      ctx: &TransitionContext) -> Result<(), Denied>;

// stage.rs
pub enum HumanGate { ReviewClean, RepairLimitReached, LostOnRestart, LimitExhausted }
pub enum StageRunState { Pending, EffectsInFlight { effects: Vec<EffectId> },
    AwaitingHuman { gate: HumanGate }, Succeeded, Failed { code: FailureCode }, Cancelled }
```

Rules: `Planned -> Approved` needs `ApproveSpec` whose subject equals `ctx.packet`. `Reviewing -> Verified` needs `Accept` whose subject equals `ctx.candidate`. `Approved -> Implementing`, `Implementing -> Reviewing`, `Reviewing -> Repair`, `Repair -> Reviewing`, and any `-> Failed | Cancelled | Blocked` listed in `ALLOWED` need no decision. Any transition into `PreviewVerified`, `MigrationGated`, `Released`, `Observed`, `Closed` returns `Denied::Unreachable`. `Blocked` resume follows the TS rule (only back to `blocked_from`).

- [ ] **Step 1: Write the failing tests:**

```rust
#[test] fn allowed_table_equals_golden() { /* ALLOWED as map == golden["allowed"] */ }
#[test] fn without_a_decision_matrix_matches_golden_for_ungated_and_denies_gated() {
    /* for every golden row with approval=false: can_transition(.., None, ..).is_ok() == row.allowed
       except the ungated rows listed in the rules above, which must be Ok */ }
#[test] fn approve_spec_for_other_digest_is_wrong_subject() {}
#[test] fn accept_for_other_commit_is_wrong_subject() {}
#[test] fn accept_cannot_approve_a_spec() { /* WrongKind */ }
#[test] fn release_path_states_are_unreachable() {}
```

- [ ] **Step 2: Run.** `cargo test -p harness-core --test lifecycle`. Expected: FAIL.
- [ ] **Step 3: Implement.** `ALLOWED` copied from the TS table verbatim; `can_transition` is one `match`.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit.** `git commit -am "feat(core): task lifecycle with human-decision gate; stage run states"`

### Task 4: Core `decide`, policy, prompts and diff chunking

**Files:**
- Create: `crates/core/src/{policy,decide,prompt,diff}.rs`
- Test: `crates/core/tests/{decide,props}.rs`

**Interfaces:**
- Consumes: Tasks 2–3.
- Produces:

```rust
// policy.rs
pub struct StageRoute { pub runtime: RuntimeKind, pub role: Role }
pub struct Pipeline { pub implement: StageRoute, pub review: StageRoute,
    pub verify_commands: Vec<Vec<String>>, pub budget: Budget,
    pub severity_threshold: Severity, pub review_chunk_max_lines: u32 }
impl Pipeline { pub fn parse_toml(s: &str) -> Result<Pipeline, PipelineError>; }
// PipelineError::SameFamily when implement and review runtimes share a ModelFamily.

// decide.rs
pub struct Checkpoint { pub n: u32, pub commit: CommitSha }
pub struct Task { pub id: TaskId, pub repo: RepoRef, pub packet: ArtifactId,
    pub pipeline: ArtifactId, pub base_commit: CommitSha, pub state: TaskState,
    pub run: StageRunState, pub round: u8, pub checkpoints: Vec<Checkpoint>, pub version: u64 }
pub enum Command {
    CreateTask { repo: RepoRef, base_commit: CommitSha, packet: ArtifactId,
                 pipeline: ArtifactId, key: IdempotencyKey },
    RecordHumanDecision { kind: DecisionKind, subject: Subject, key: IdempotencyKey },
    Run { key: IdempotencyKey },
    RecordEffectOutcome { effect: EffectId, outcome: EffectOutcome },
    Cancel { key: IdempotencyKey } }
pub enum EffectSpec {
    AgentRun { effect: EffectId, role: Role, runtime: RuntimeKind, base: CommitSha,
               inputs: Vec<ArtifactId>, chunk: Option<u32>, deadline_ms: u64 },
    Verify { effect: EffectId, command_index: u32, candidate: CommitSha, deadline_ms: u64,
             max_attempts: u8 },
    Checkpoint { effect: EffectId, expected_parent: CommitSha, author_time: Timestamp } }
pub enum EffectOutcome {
    AgentRun { outcome: RunOutcome, transcript: ArtifactId, review: Option<ArtifactId> },
    Verify { outcome: RunOutcome, evidence: ArtifactId },
    Checkpoint { outcome: RunOutcome, commit: Option<CommitSha>, diff: Option<ArtifactId> } }
pub enum Event { TaskCreated{..}, HumanDecisionRecorded{..}, TaskStateChanged{from,to},
    EffectIntended{spec}, EffectFinished{effect,outcome}, CheckpointCreated{n,commit},
    VerificationFinished{effect,passed,evidence}, FindingsRecorded{effect,findings,blocking:u32},
    RepairRoundStarted{round}, AwaitingHuman{gate,evidence:Vec<ArtifactId>},
    TaskCancelled, #[serde(other)] Unknown }
pub struct Decision { pub events: Vec<Event>, pub effects: Vec<EffectSpec> }
pub enum Rejection { UnknownTask, Denied(Denied), WrongState, StaleEffect, PipelineInvalid }
pub struct Facts<'a> { pub pipeline: &'a Pipeline, pub review: Option<&'a Review>,
                       pub diff_lines: Option<u32>, pub new_effect_ids: &'a [EffectId] }
pub fn decide(task: Option<&Task>, cmd: &Envelope<Command>, facts: &Facts) -> Result<Decision, Rejection>;
pub fn apply(task: Option<Task>, event: &Envelope<Event>) -> Result<Task, Corrupt>;
pub fn classify(role: Option<Role>, outcome: &RunOutcome) -> TerminationReason;

// prompt.rs
pub struct PromptInput<'a> { pub role: Role, pub packet_files: &'a [(String, String)],
    pub agents_md: Option<&'a str>, pub findings: Option<&'a Review>, pub diff: Option<&'a str> }
pub fn build_prompt(input: &PromptInput) -> String;
// diff.rs
pub struct DiffChunk { pub index: u32, pub files: Vec<String>, pub text: String, pub changed_lines: u32 }
pub fn chunk_diff(unified: &str, max_lines: u32) -> Vec<DiffChunk>;   // split on file boundaries; a file larger than max_lines is its own chunk
```

Flow `decide` encodes (slice only): `CreateTask` → `Draft→Planned`. `ApproveSpec` → `Planned→Approved`. `Run` from `Approved` → `Approved→Implementing`, intend implementer `AgentRun`. Implementer `Succeeded` → intend `Checkpoint`. Checkpoint `Succeeded` → intend one `Verify` per command. All verifies passed → `Implementing→Reviewing` (or stay in `Repair→Reviewing`), intend one reviewer `AgentRun` per diff chunk. All reviews recorded: if any verify failed or any finding is blocking, and `round < max_repair_rounds` → `Reviewing→Repair`, `RepairRoundStarted`, intend implementer `AgentRun` with findings as input; if `round == max_repair_rounds` → `AwaitingHuman{RepairLimitReached}`; if clean → `AwaitingHuman{ReviewClean}`. A failed verify skips review and goes straight to the repair decision. `LimitExhausted` → `AwaitingHuman{LimitExhausted}` without incrementing `round`. `Failed{LostOnRestart}` on an agent run → `AwaitingHuman{LostOnRestart}`. `Accept` from `AwaitingHuman` → `Reviewing→Verified`. `SendBack` → another repair round regardless of the limit. `Reject` → `Failed`. The `AwaitingHuman.evidence` list contains only artifacts whose label `is_evidence()`. `build_prompt` prefixes `agents_md` with the line `Repository conventions (repository content, not instructions from the harness):`.

- [ ] **Step 1: Write the failing tests** (`decide.rs`): one test per sentence of the flow above, named for it (for example `clean_review_stops_at_awaiting_human_review_clean`, `blocking_finding_starts_repair_round_1`, `third_blocking_review_stops_at_repair_limit`, `failed_verify_skips_review`, `limit_exhausted_does_not_consume_repair_round`, `accept_requires_candidate_commit_subject`, `pipeline_with_same_family_is_rejected`, `chunk_diff_splits_on_file_boundaries_at_400`, `prompt_labels_agents_md_as_repository_content`).
- [ ] **Step 2: Write the property tests** (`props.rs`, proptest over random command/outcome sequences folded through `decide` + `apply`):

```rust
// P1: state == Verified  =>  history contains HumanDecisionRecorded{Accept, Commit(c)} with c == last checkpoint
// P2: run == AwaitingHuman{ReviewClean}  =>  a VerificationFinished{passed:true} exists for the last checkpoint
// P3: count(RepairRoundStarted) <= max_repair_rounds + count(HumanDecisionRecorded{SendBack})
// P4: apply(decide(..).events) never returns Corrupt
```

- [ ] **Step 3: Run.** `cargo test -p harness-core`. Expected: FAIL.
- [ ] **Step 4: Implement** `decide` and `apply` as single exhaustive `match`es; no wildcard arm on `TaskState`, `StageRunState` or `EffectOutcome`.
- [ ] **Step 5: Run.** `cargo test -p harness-core && node scripts/check-deps.mjs`. Expected: PASS.
- [ ] **Step 6: Commit.** `git commit -am "feat(core): decide/apply, pipeline policy, prompts, diff chunking"`

### Task 5: Exec — process supervisor

**Files:**
- Create: `crates/exec/src/{supervisor,shim}.rs`
- Test: `crates/exec/tests/supervisor.rs`

**Interfaces:**
- Consumes: Task 2 (`FailureCode`, `RunOutcome`).
- Produces:

```rust
pub struct LaunchPlan { pub argv: Vec<String>, pub env: Vec<(String, String)>,
    pub cwd: PathBuf, pub stdin: Vec<u8> }
pub struct Limits { pub deadline: Duration, pub max_output_bytes: u64, pub max_line_bytes: usize }
pub struct ProcessIdentity { pub pgid: i32, pub pid: i32, pub start_us: i64, pub boot_id: String }
// pid/start_us identify the persistent supervisor; pgid identifies the separate task group.
pub struct TreeGone(());                       // constructed only after confirmed descendant cleanup
pub struct KillFailed { /* retains the cleanup handle; contains no TreeGone */ }
pub struct ExitInfo { pub code: Option<i32>, pub signal: Option<i32>, pub failure: Option<FailureCode> }
pub trait LineSink: Send { fn line(&mut self, stream: Stream, bytes: &[u8]); }
pub struct Supervised { /* owns the persistent supervisor and separate task child */ }
pub async fn spawn_gated(plan: LaunchPlan, limits: Limits,
    on_identity: impl FnOnce(ProcessIdentity) -> Result<(), SpawnAbort>) -> Result<Supervised, FailureCode>;
impl Supervised {
    pub async fn run(self, sink: &mut dyn LineSink, cancel: CancellationToken)
        -> Result<(ExitInfo, TreeGone), KillFailed>; }
pub fn boot_id() -> String;                    // sysctl kern.bootsessionuuid
pub fn identity_is_alive(id: &ProcessIdentity) -> bool;
pub async fn kill_identity(id: &ProcessIdentity) -> Result<TreeGone, KillFailed>;
```

Behaviour: start a separate persistent unsandboxed supervisor and a gated task child in its
own process group. Only the task child may `exec` `argv`; it waits for one byte on fd 3.
`on_identity` records the supervisor identity and task pgid before the gate opens; callback
failure or parent loss before that byte means no task command runs. The supervisor remains
alive after task exit and throughout cleanup (design §6.2; research `1a-macos-sandbox.md` §6).

Before opening the gate, a backend must establish containment of arbitrary task descendants,
including double-fork/`setsid` and responsibility-disclaim cases. Merely loading the private
lookup functions, keeping an anchor alive or observing an empty responsibility snapshot is
insufficient. No capable backend is established by this plan; until one is, return
`SandboxUnavailable` without opening the gate. Ancestry polling is not a proof fallback.

On normal exit, cancellation, deadline or output failure: `SIGTERM` the task group, wait
5 s, then `SIGKILL` as needed and use the established backend to confirm every descendant
is gone (cleanup bound 2 s). Reap the supervisor only after that proof; exclude the live
supervisor from its own sweep. `run` and `kill_identity` return `KillFailed` on tracking
error, premature supervisor loss or timeout, never `TreeGone`; preserve recovery identity
and the worktree lock, and stop for the human. Only successful confirmed cleanup returns
`TreeGone` (R-11). Stdout and stderr drain concurrently through length-limited line framing;
stdin is written from its own task and `EPIPE` is ignored.

- [ ] **Step 1: Write the failing tests** (use non-secret local helpers; the detachment and
  responsibility-disclaim cases need a synthetic native helper):

```rust
#[tokio::test] #[ignore = "R-11: needs a containment backend"] async fn shim_runs_nothing_if_identity_callback_fails() { /* marker file absent */ }
#[tokio::test] #[ignore = "R-11: needs a containment backend"] async fn deadline_kills_group_and_reports_deadline_exceeded() {}
#[tokio::test] #[ignore = "R-11: needs a containment backend"] async fn detached_grandchild_is_killed_before_tree_gone() { /* child double-forks + setsid; its pid is dead when run() returns */ }
#[tokio::test] async fn unproved_descendant_containment_prevents_task_spawn() { /* task marker absent; SandboxUnavailable */ }
#[tokio::test] async fn responsibility_disclaim_cannot_produce_false_tree_gone() { /* refuse before gate unless containment is established; otherwise confirm the helper is gone */ }
#[tokio::test] #[ignore = "R-11: needs a containment backend"] async fn normal_task_exit_keeps_supervisor_alive_until_descendant_cleanup() {}
#[tokio::test] async fn tracking_error_or_cleanup_timeout_retains_lock_without_tree_gone() {}
#[tokio::test] #[ignore = "R-11: needs a containment backend"] async fn output_cap_triggers_kill() { /* `yes` => OutputLimitExceeded */ }
#[tokio::test] #[ignore = "R-11: needs a containment backend"] async fn oversized_line_is_a_failure_not_a_panic() {}
#[tokio::test] #[ignore = "R-11: needs a containment backend"] async fn stderr_flood_does_not_deadlock() {}
#[tokio::test] #[ignore = "R-11: needs a containment backend"] async fn cancel_token_yields_cancelled_exit() {}
#[tokio::test] #[ignore = "R-11: needs a containment backend"] async fn supervisor_loss_during_run_yields_kill_failed_never_tree_gone_and_lock_is_retained() { /* supervisor lost mid-run => KillFailed, never TreeGone; lock retained */ }
```

Tests that run now (no task process is spawned): `unproved_descendant_containment_prevents_task_spawn`,
`responsibility_disclaim_cannot_produce_false_tree_gone` in its refuse-before-gate form, and
`tracking_error_or_cleanup_timeout_retains_lock_without_tree_gone` driven without a spawn.
Every test that needs a task process to actually run is execution-dependent: write and compile
it, but mark it `#[ignore = "R-11: needs a containment backend"]` (as above) until a backend
exists (T13); it must then be un-ignored and pass as part of that task.

- [ ] **Step 2: Run.** `cargo test -p harness-exec --test supervisor`. Expected: the tests that run now FAIL; ignored tests are listed as ignored.
- [ ] **Step 3: Implement.** Expose `pub fn descendant_tracking_available() -> bool` only as
  a capability check for containment satisfying the cases above, not symbol availability.
  `eng doctor` (Task 10), `spawn_gated` and recovery must fail closed when that capability
  cannot be established. Research §6's responsibility sweep is evidence of a limited probe,
  not evidence that arbitrary descendants cannot disclaim responsibility.
- [ ] **Step 4: Run.** Expected: PASS for the tests that run now (not the ignored execution-dependent ones), three consecutive times (`for i in 1 2 3; do cargo test -p harness-exec --test supervisor || break; done`).
- [ ] **Step 5: Commit.** `git commit -am "feat(exec): gated spawn, bounded streams, process-tree kill with TreeGone proof"`

### Task 6: Exec — macOS sandbox and egress proxy

**Files:**
- Create: `crates/exec/src/{sandbox_macos,egress}.rs`
- Test: `crates/exec/tests/{sandbox_profile,sandbox_live,egress}.rs`

**Interfaces:**
- Consumes: Task 5 `LaunchPlan`.
- Produces:

```rust
pub enum Network { None, ProxyOnly { port: u16 } }
pub struct SandboxProfile { pub worktree: PathBuf, pub worktree_writable: bool,
    pub task_home: PathBuf, pub read_deny: Vec<PathBuf>, pub read_allow_extra: Vec<PathBuf>,
    pub network: Network }
pub struct SandboxInfo { pub sandbox_exec: PathBuf }
pub fn probe() -> Result<SandboxInfo, FailureCode>;             // Err(SandboxUnavailable)
pub fn render_profile(p: &SandboxProfile) -> Result<String, ProfileError>;   // pure SBPL text
pub fn wrap(p: &SandboxProfile, plan: LaunchPlan) -> Result<LaunchPlan, ProfileError>;
pub fn default_read_deny(home: &Path, state_dir: &Path, worktree: &Path, git_common_dir: &Path) -> Vec<PathBuf>;

pub struct EgressProxy { /* listening on 127.0.0.1:<port> */ }
pub struct EgressDecision { pub host: String, pub port: u16, pub allowed: bool, pub at_us: i64 }
impl EgressProxy {
    pub async fn start(allow_hosts: Vec<String>) -> std::io::Result<EgressProxy>;
    pub fn port(&self) -> u16;
    pub async fn stop(self) -> Vec<EgressDecision>; }
```

Rules: `render_profile` starts `(deny default)`. Required existing roots (`worktree`,
`task_home`, state and Git common directories) and existing rule targets use
`std::fs::canonicalize` before rendering and are passed as `-D` parameters. For a deny
target missing with `NotFound`, walk upward using `symlink_metadata` to the nearest existing
ancestor, canonicalise that ancestor, then append the missing normal components. Reject
`..`, broken symlinks, permission errors and other resolution failures with
`ProfileError::Unresolvable`; never skip a deny rule. A required root that is missing also
fails. This preserves denies on later-created paths and resolves aliases such as
`/tmp` to `/private/tmp` (design §6.1; R-9; research `1a-macos-sandbox.md` §1).

`default_read_deny` returns: `~/.ssh`, `~/.aws`, the state directory, the Git common dir,
`<worktree>/.git`, and under the worktree `.claude`, `.mcp.json`, `.codex`, `.agents`,
`.cursor`, whether or not optional targets exist. Writes: `worktree` only if
`worktree_writable`, plus `task_home`; never the Git paths. `Network::ProxyOnly` allows
outbound only to `localhost:<port>`. The proxy accepts `CONNECT host:443` only, allows exact
host matches from `allow_hosts`, answers `403` otherwise, and records every decision. Host
permission does not authorize content egress (design §9). `wrap` prepends
`/usr/bin/sandbox-exec -p <profile> -D ...` and adds `HTTPS_PROXY`, `HTTP_PROXY`,
`NODE_USE_ENV_PROXY=1` to `env`.

- [ ] **Step 1: Write the failing tests:**

```rust
// sandbox_profile.rs (pure)
#[test] fn profile_starts_with_deny_default() {}
#[test] fn profile_resolves_real_paths() { /* worktree given via a symlink and with a space in its name renders the canonical path; no "/tmp/" literal survives */ }
#[test] fn absent_optional_deny_targets_resolve_through_existing_ancestor() { /* clean repo lacks agent config; home lacks .ssh/.aws; every deny remains */ }
#[test] fn missing_deny_suffix_under_symlinked_ancestor_uses_real_path() {}
#[test] fn missing_required_root_or_broken_symlink_is_an_error() {}
#[test] fn unresolvable_path_is_an_error_not_a_skipped_rule() {}
#[test] fn readonly_profile_has_no_worktree_write_rule() {}
// sandbox_live.rs (runs /usr/bin/sandbox-exec with /bin/sh, /usr/bin/touch, /bin/cat)
#[test] fn write_outside_worktree_is_denied() {}
#[test] fn write_via_private_tmp_alias_is_denied() {}
#[test] fn read_of_ssh_dir_and_state_dir_is_denied() {}
#[test] fn write_to_git_file_and_common_dir_is_denied() {}
#[test] fn read_of_repo_agent_config_is_denied_but_agents_md_is_readable() {}
#[test] fn optional_deny_target_created_after_profile_render_is_denied() { /* fixture uses non-secret files */ }
#[test] fn nested_sandbox_exec_fails_inside_profile() { /* documents the nesting fact */ }
#[test] fn network_none_blocks_loopback_and_remote() {}
// egress.rs
#[tokio::test] async fn allowed_host_connects_and_other_host_gets_403() { /* against a local TLS-less CONNECT target */ }
#[tokio::test] async fn direct_connection_from_sandbox_is_blocked_proxy_works() {}
#[tokio::test] async fn every_decision_is_logged() {}
```

- [ ] **Step 2: Run.** `cargo test -p harness-exec --test sandbox_profile --test sandbox_live --test egress`. Expected: FAIL.
- [ ] **Step 3: Implement,** starting from the probed profile in `research-2026-11-09/1a-macos-sandbox.md` (written fresh; nothing copied from other projects).
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit.** `git commit -am "feat(exec): macOS sandbox profile generator, fail-closed probe, egress allowlist proxy"`

### Task 7: Exec — workspace, host Git, secret scan

**Files:**
- Create: `crates/exec/src/{workspace,hostgit,secretscan}.rs`
- Test: `crates/exec/tests/workspace.rs`

**Interfaces:**
- Consumes: Task 2 ids; Task 5 `TreeGone`.
- Produces:

```rust
pub struct Workspace { pub task: TaskId, pub path: PathBuf, pub git_common_dir: PathBuf }
pub async fn create(repo: &Path, task: &TaskId, base: &CommitSha, root: &Path) -> Result<Workspace, WorkspaceError>;
pub async fn packet_digest(repo: &Path, base: &CommitSha, packet_path: &str) -> Result<(ArtifactId, Vec<(String, String)>), WorkspaceError>;
pub async fn scan(ws: &Workspace) -> Result<(), FailureCode>;        // GitMetadataTouched on nested repo, gitlink, or changed .git
pub async fn commit_candidate(ws: &Workspace, parent: &CommitSha, n: u32, author_time: Timestamp)
    -> Result<(CommitSha, String /* unified diff parent..commit */), FailureCode>;
pub async fn reset_to(ws: &Workspace, commit: &CommitSha, _proof: TreeGone) -> Result<(), WorkspaceError>;
pub fn scan_for_secrets(diff: &str) -> Vec<SecretHit>;               // SecretHit { file, line, rule }
pub async fn git(repo: &Path, args: &[&str]) -> Result<Vec<u8>, GitError>;
```

Rules: `git` always passes `-c core.hooksPath=/dev/null -c core.fsmonitor=false -c protocol.file.allow=never -c submodule.recurse=false --no-optional-locks` and a scrubbed environment (`GIT_CONFIG_NOSYSTEM=1`, `GIT_CONFIG_GLOBAL=/dev/null`, `GIT_TERMINAL_PROMPT=0`). `create` runs `git worktree add --detach <root>/<task-id> <base>`. `commit_candidate` runs `add --all`, `write-tree`, `commit-tree -p <parent>` with author and committer time taken from `author_time` and identity `eng <eng@localhost>`, then `update-ref refs/harness/tasks/<task>/cp/<n> <new> <expected-old>`; with the same inputs it returns the same SHA. It returns `Err(SecretInDiff)` when `scan_for_secrets` finds a hit and creates no ref. `scan_for_secrets` rules: `sk-ant-`, `sk-proj-`, `sk-or-`, `ghp_`/`gho_`/`github_pat_`, AWS `AKIA[0-9A-Z]{16}`, PEM `-----BEGIN .* PRIVATE KEY-----`, and JSON keys `accessToken`/`refreshToken`/`id_token` with a value of 20+ characters; only added lines are scanned.

- [ ] **Step 1: Write the failing tests** (each builds a throwaway repo under a temp dir):

```rust
#[tokio::test] async fn create_detaches_at_base_and_moves_no_branch() {}
#[tokio::test] async fn commit_candidate_is_deterministic_for_same_inputs() {}
#[tokio::test] async fn update_ref_is_compare_and_swap() { /* wrong expected-old fails, ref unchanged */ }
#[tokio::test] async fn repo_hooks_do_not_run_on_host_commit() { /* pre-commit hook writes a marker; marker absent */ }
#[tokio::test] async fn nested_repo_or_gitlink_is_git_metadata_touched() {}
#[tokio::test] async fn hidden_ref_is_not_pushed_by_default_refspec() { /* push to a bare remote; ls-remote has no refs/harness */ }
#[tokio::test] async fn token_shaped_string_in_added_line_fails_with_secret_in_diff() {}
#[tokio::test] async fn token_shaped_string_in_removed_line_is_ignored() {}
#[tokio::test] async fn packet_digest_changes_when_any_packet_file_changes() {}
```

- [ ] **Step 2: Run.** `cargo test -p harness-exec --test workspace`. Expected: FAIL.
- [ ] **Step 3: Implement.** Port the regression cases `bug1`–`bug9` from `tests/local-workflow-regressions.test.mjs` as further tests in the same file, one test per case, same names.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit.** `git commit -am "feat(exec): worktrees, hardened host Git, checkpoint refs, secret scan"`

### Task 8: Adapters — Claude and Codex

**Files:**
- Create: `crates/adapters/src/{lib,claude,codex}.rs`, `crates/adapters/fixtures/{claude,codex}/*.jsonl`
- Test: `crates/adapters/tests/{claude,codex}.rs`

**Interfaces:**
- Consumes: Task 2 (`Capabilities`, `Role`, `RunOutcome`, `RawReview`, `review_json_schema`).
- Produces:

```rust
pub struct AgentRunSpec { pub role: Role, pub prompt: String, pub worktree: PathBuf,
    pub config_dir: PathBuf, pub mcp_config: PathBuf, pub review_schema: Option<PathBuf>,
    pub max_turns: u32 }
pub struct AdapterPlan { pub argv: Vec<String>, pub env: Vec<(String, String)>, pub stdin: Vec<u8>,
    pub egress_hosts: Vec<String> }
pub struct ParseState { /* opaque */ }
pub struct Usage { pub input_tokens: Option<u64>, pub output_tokens: Option<u64>, pub turns: Option<u32> }
pub struct AgentRunResult { pub outcome: RunOutcome, pub final_text: Option<String>,
    pub review: Option<RawReview>, pub session: Option<String>, pub usage: Option<Usage>,
    pub own_sandbox_confirmed_off: bool }
pub trait RuntimeAdapter: Send + Sync {
    fn capabilities(&self) -> &'static Capabilities;
    fn version_argv(&self) -> Vec<String>;
    fn check_version(&self, stdout: &str) -> Result<(), FailureCode>;      // VersionMismatch
    fn plan(&self, run: &AgentRunSpec) -> Result<AdapterPlan, FailureCode>;
    fn parse_line(&self, st: &mut ParseState, line: &[u8]);
    fn finish(&self, st: ParseState, exit_code: Option<i32>) -> AgentRunResult; }
pub fn adapter_for(kind: RuntimeKind) -> &'static dyn RuntimeAdapter;
```

Claude `plan`: argv is `claude -p --output-format stream-json --verbose --restricted --setting-sources user --strict-mcp-config --mcp-config <mcp_config> --disable-slash-commands --max-turns <n> --settings <json with sandbox.enabled=false> --tools <list>`; `--tools` is `Bash,Read,Edit,Write,Grep,Glob` for `Implementer` and `Read,Grep,Glob` plus `--json-schema <schema>` for `Reviewer`; env is `CLAUDE_CONFIG_DIR=<config_dir>`, `CLAUDE_CODE_DISABLE_CLAUDE_MDS=1`, `DISABLE_AUTOUPDATER=1`, `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`; the prompt goes on stdin. The argv never contains `--bare`, `--dangerously-skip-permissions`, `--add-dir` or `bypassPermissions`. Codex `plan`: `codex exec --json -s danger-full-access --skip-git-repo-check -C <worktree>` plus `--output-schema <schema>` for `Reviewer`; env `CODEX_HOME=<config_dir>`; prompt on stdin. `finish`: `Succeeded` only if exit code is 0 **and** the final result event reports no error **and**, for a reviewer, the output parses as `RawReview`; a rate-limit event gives `LimitExhausted`; anything unparseable gives `Failed{InvalidAgentOutput}`. `egress_hosts`: Claude `api.anthropic.com`; Codex `chatgpt.com`, `api.openai.com` (both lists are provisional until Task 11 step 3).

- [ ] **Step 1: Write fixtures.** Hand-write provisional transcripts from the formats documented in `research-2026-11-09/1b-subscription-clis.md` §1 and `specs/claude-adapter/requirements.md` (C-1 evidence): `claude/ok.jsonl`, `claude/is_error_true_subtype_success.jsonl`, `claude/rate_limit.jsonl`, `claude/truncated.jsonl`, `claude/invalid_utf8.jsonl`, `codex/review_ok.jsonl`, `codex/review_not_schema.jsonl`, `codex/limit_message.jsonl`. Each file's first line is a comment-free JSON object; put provenance in `fixtures/README.md` with the word PROVISIONAL.
- [ ] **Step 2: Write the failing tests:**

```rust
#[test] fn claude_implementer_argv_names_shell_and_never_forbidden_flags() {}
#[test] fn claude_reviewer_has_no_shell_tool() {}
#[test] fn claude_exit_1_with_subtype_success_is_failed() {}
#[test] fn claude_rate_limit_event_is_limit_exhausted() {}
#[test] fn malformed_stream_is_invalid_agent_output() { /* truncated.jsonl, invalid_utf8.jsonl */ }
#[test] fn codex_reviewer_passes_output_schema_and_disables_own_sandbox() {}
#[test] fn codex_non_schema_output_is_invalid_agent_output() {}
#[test] fn version_mismatch_is_detected() { /* "2.1.319 (Claude Code)" => VersionMismatch */ }
#[test] fn pipeline_families_differ() { /* claude.capabilities().model_family != codex.capabilities().model_family */ }
```

- [ ] **Step 3: Run.** `cargo test -p harness-adapters`. Expected: FAIL.
- [ ] **Step 4: Implement.** Run: `cargo test -p harness-adapters && node scripts/check-deps.mjs`. Expected: PASS.
- [ ] **Step 5: Commit.** `git commit -am "feat(adapters): Claude and Codex launch plans and stream parsers (provisional fixtures)"`

### Task 9: Store and engine

**Files:**
- Create: `crates/store/src/{lib,schema.sql,writer,events,artifacts}.rs`, `crates/engine/src/{lib,worker,recovery}.rs`
- Test: `crates/store/tests/store.rs`, `crates/engine/tests/{flow,recovery}.rs`

**Interfaces:**
- Consumes: Tasks 4–8.
- Produces:

```rust
// harness-store
pub struct Store;
impl Store {
    pub fn open(state_dir: &Path) -> Result<Store, StoreError>;            // takes flock on <state_dir>/engine.lock
    pub fn submit(&self, cmd: Envelope<Command>,
        decide: &dyn Fn(Option<&Task>, &Envelope<Command>) -> Result<Decision, Rejection>)
        -> Result<Receipt, StoreError>;
    pub fn put_artifact(&self, bytes: &[u8], media: MediaType, label: Provenance,
                        by: Option<&EffectId>) -> Result<ArtifactId, StoreError>;
    pub fn get_artifact(&self, id: &ArtifactId) -> Result<Vec<u8>, StoreError>;   // re-hashes on read
    pub fn task(&self, id: &TaskId) -> Result<Option<Task>, StoreError>;
    pub fn events_after(&self, seq: Seq, limit: u32) -> Result<Vec<EventRecord>, StoreError>;
    pub fn pending_effects(&self) -> Result<Vec<OutboxRow>, StoreError>;
    pub fn mark_effect(&self, id: &EffectId, to: OutboxState, process: Option<&ProcessIdentity>) -> Result<(), StoreError>;
    pub fn verify_chain(&self) -> Result<Seq, ChainBroken>; }
pub struct Receipt { pub command: CommandId, pub accepted: bool, pub rejection: Option<String>,
                     pub first_seq: Option<Seq>, pub last_seq: Option<Seq> }
pub enum OutboxState { Intended, Starting, Started, Done, Abandoned }
pub struct EventRecord { pub seq: Seq, pub task_seq: u64, pub prev_hash: [u8; 32],
                         pub hash: [u8; 32], pub event: Envelope<Event> }

// harness-engine
pub struct EngineConfig { pub state_dir: PathBuf, pub claude_bin: PathBuf, pub codex_bin: PathBuf }
pub struct Engine;
impl Engine {
    pub async fn open(cfg: EngineConfig) -> Result<Engine, EngineError>;   // runs recovery first
    pub async fn submit(&self, cmd: Envelope<Command>) -> Result<Receipt, EngineError>;
    pub async fn run_until_idle(&self, task: &TaskId, cancel: CancellationToken) -> Result<Task, EngineError>;
    pub fn task(&self, id: &TaskId) -> Result<Option<Task>, EngineError>;
    pub fn events_after(&self, seq: Seq, limit: u32) -> Result<Vec<EventRecord>, EngineError>;
    pub fn verify_chain(&self) -> Result<Seq, EngineError>; }
```

Store rules: schema as in `research-2026-11-09/5-rust-core.md` §4.1 without the `anchors` table; `PRAGMA journal_mode=WAL, synchronous=FULL, foreign_keys=ON`; one dedicated writer thread owns the connection; `hash = sha256(prev_hash || seq_be || canonical_json(body))`; same idempotency key with the same body returns the first receipt, with a different body is rejected; artifact bytes are written (exclusive create, fsync file and directory) before the transaction that references them; objects live at `objects/<first two hex>/<sha256>` and no other crate learns that path.

Engine rules: one worker per outbox row following the eight steps of §4.2; recovery follows
the table in design §5.4, subject to R-11's requirement for actual descendant-cleanup proof.
Before an `AgentRun`, resolve reviewed project/per-path egress classifications and explicit
provider authorization, including exact runtime/account/settings and any required verified
BAA entry. Missing/unknown policy, unauthorized content/provider, or read isolation that
cannot enforce the declared ceiling returns `PolicyDenied` before spawn or task-content
delivery; the proxy's host list cannot substitute for this check (design §9; R-17).

Then resolve the adapter, check its version, build the profile (`worktree_writable = role ==
Implementer`), start the egress proxy with `AdapterPlan.egress_hosts`, wrap, spawn through
Task 5's containment gate, and feed lines to the adapter. Screen output before artifact
retention; store only non-secret output as `RuntimeClaim`, and for a reviewer store the
validated non-secret `Review` as `ReviewFindings`. Do not retain raw authenticated transcripts
or uncertain credential-bearing content; record a non-secret failure summary instead
(decision 6; design §§6.3, 11). A `Verify` effect runs `pipeline.verify_commands[i]` under a
read-only profile with `Network::None` and stores non-secret combined output as
`HarnessEvidence`. Workspace lock rows are released only with `TreeGone`; cleanup failure
or unavailable containment retains the lock and stops recovery for the human (R-11).

- [ ] **Step 1: Write the failing store tests:**

```rust
#[test] fn retried_command_returns_same_receipt() {}
#[test] fn same_key_different_body_is_rejected() {}
#[test] fn events_are_gapless_and_chain_verifies() {}
#[test] fn update_or_delete_of_events_is_refused_by_trigger() {}
#[test] fn tampered_event_body_breaks_chain_at_that_seq() {}
#[test] fn corrupted_artifact_is_detected_on_read() {}
#[test] fn abort_after_each_statement_then_reopen_is_consistent() { /* failpoints feature: for each failpoint, chain verifies and resubmitting yields one receipt */ }
#[test] fn second_engine_refuses_to_start() { /* second Store::open on the same dir => StoreError::Locked */ }
```

- [ ] **Step 2: Write the failing engine tests** (agents are `conformance/fixtures/fake-claude.mjs` and `fake-codex.mjs`, scripts that edit files and emit the Task 8 fixture formats, configured through `EngineConfig`):

```rust
#[tokio::test] #[ignore = "R-11: needs a containment backend"] async fn clean_flow_stops_at_awaiting_human_with_one_checkpoint() {}
#[tokio::test] async fn missing_or_unknown_class_denies_hosted_spawn() { /* PolicyDenied; fake-agent marker absent */ }
#[tokio::test] async fn unauthorized_provider_or_unverified_baa_config_denies_spawn() {}
#[tokio::test] #[ignore = "R-11: needs a containment backend"] async fn reviewed_any_provider_input_allows_explicitly_authorized_runtime() {}
#[tokio::test] async fn derived_input_uses_most_restrictive_class_before_spawn() {}
#[tokio::test] #[ignore = "R-11: needs a containment backend"] async fn secret_bearing_output_is_not_retained_as_an_artifact() { /* synthetic canary only */ }
#[tokio::test] #[ignore = "R-11: needs a containment backend"] async fn blocking_finding_runs_two_repair_rounds_then_stops() {}
#[tokio::test] #[ignore = "R-11: needs a containment backend"] async fn failing_verify_command_triggers_repair_without_review() {}
#[tokio::test] #[ignore = "R-11: needs a containment backend"] async fn agent_that_writes_a_token_fails_with_secret_in_diff() {}
#[tokio::test] #[ignore = "R-11: needs a containment backend"] async fn accept_moves_to_verified_and_creates_branch_harness_task_id() {}
// recovery.rs — kill -9 a child engine process at a named failpoint, reopen, assert
#[tokio::test] #[ignore = "R-11: needs a containment backend"] async fn crash_in_starting_reruns_effect_and_nothing_ran_twice() {}
#[tokio::test] #[ignore = "R-11: needs a containment backend"] async fn crash_during_agent_run_gives_lost_on_restart_and_resets_worktree_only_after_tree_gone() { /* no TreeGone => worktree and lock retained; stop for the human */ }
#[tokio::test] #[ignore = "R-11: needs a containment backend"] async fn crash_during_verify_retries_once() {}
#[tokio::test] #[ignore = "R-11: needs a containment backend"] async fn crash_during_checkpoint_yields_same_commit_sha() {}
#[tokio::test] #[ignore = "R-11: needs a containment backend"] async fn lock_is_not_released_while_orphan_is_alive() {}
```

Run now: all store tests and the policy-denial engine tests that refuse before spawn
(`missing_or_unknown_class_denies_hosted_spawn`, `unauthorized_provider_or_unverified_baa_config_denies_spawn`,
`derived_input_uses_most_restrictive_class_before_spawn`). Execution-dependent (marked
`#[ignore = "R-11: needs a containment backend"]`, written and compiled): every engine and
recovery test that needs a running agent, verify command or live orphan, i.e. the rest of the
engine list above, including the crash-during-agent-run test, which resets the worktree only
once `TreeGone` exists and otherwise retains the worktree and lock and stops for the human (R-5, R-11).

- [ ] **Step 3: Run.** `cargo test -p harness-store -p harness-engine`. Expected: FAIL.
- [ ] **Step 4: Implement store, then engine.**
- [ ] **Step 5: Run.** `cargo test --workspace && node scripts/check-deps.mjs`. Expected: PASS for the tests that run now; execution-dependent tests are reported as ignored.
- [ ] **Step 6: Commit.** `git commit -am "feat: durable store and engine loop with restart recovery"`

### Task 10: CLI, doctor and conformance flow

**Files:**
- Create: `crates/cli/src/{main,doctor,output}.rs`, `conformance/cases/{flow,decide,audit,doctor}.test.mjs`, `conformance/fixtures/{make-repo.mjs,packet/*,pipeline.toml}`
- Modify: `README.md` (replace "Runnable local pilot" quick start with the `eng` commands below; keep the pilot section under a "Legacy pilot" heading), `docs/local-workflow.md` (one line at top pointing to the new flow)

**Interfaces:**
- Consumes: Task 9 `Engine`.
- Produces — commands, all accepting `--json` and `--state-dir <path>` (default from Global Constraints):

```
eng doctor                                   exit 0 only if sandbox probe, descendant tracking, pinned versions and config dirs are all OK
eng task new --repo <path> --packet specs/<id> [--pipeline .eng/pipeline.toml] [--base <sha>]   prints task id and packet digest
eng decide <task> approve-spec               subject = the task's packet digest
eng run <task>                               runs until AwaitingHuman, Failed or Cancelled; Ctrl-C submits Cancel, second Ctrl-C kills
eng status [<task>]                          state, round, gate, findings summary, last checkpoint
eng decide <task> accept|reject|send-back    subject = last checkpoint commit
eng cancel <task>
eng audit verify                             recomputes the hash chain; prints head seq
eng login claude|codex                       execs the official CLI's login with the harness-owned config dir; eng reads nothing it produces
```

`eng decide` refuses unless stdin is a terminal and prints the digest or commit and waits for the typed word `yes`; `--json` output is one object per command with `ok`, `task`, `state`, `error` fields. Exit codes: `0` ok, `2` usage, `3` rejected by policy, `4` awaiting human, `5` failed, `70` internal.

- [ ] **Step 1: Write the failing conformance cases** (Node `node:test`, spawning `$ENG_BIN` with a temp `--state-dir`, fake agents on `PATH` via `ENG_CLAUDE_BIN`/`ENG_CODEX_BIN`):
  - `flow.test.mjs`: new → approve-spec → run exits `4` with `gate: "review_clean"`; `status --json` lists one checkpoint; `refs/harness/tasks/<id>/cp/1` exists; no branch moved. Execution-dependent: written and compiled, but skipped (`{ skip: "R-11: needs a containment backend" }`).
  - `flow.test.mjs`: `run` before `approve-spec` exits `3`.
  - `flow.test.mjs`: with `ENG_FORCE_SANDBOX_UNAVAILABLE=1` (honoured only in debug builds), `run` exits `5` with `error: "sandbox_unavailable"` and no agent process was started.
  - `decide.test.mjs`: `decide accept` with stdin not a terminal exits `3`; with a pseudo-terminal and `yes` the state is `verified` and branch `harness/<id>` exists. Execution-dependent: written and compiled, but skipped (`{ skip: "R-11: needs a containment backend" }`).
  - `audit.test.mjs`: after a flow, `audit verify` exits `0`; after flipping one byte in `state.db` events body via `sqlite3`, it exits `5`. Execution-dependent: written and compiled, but skipped (`{ skip: "R-11: needs a containment backend" }`).
  - `doctor.test.mjs`: with a fake `claude` printing version `9.9.9`, `doctor` exits non-zero and names `version_mismatch`.
  - Run now: the `run`-before-`approve-spec` case, the `ENG_FORCE_SANDBOX_UNAVAILABLE=1` case and the `doctor.test.mjs` case (until a backend exists `eng doctor` exits non-zero anyway, so the version-mismatch case must assert that `version_mismatch` is named). The other cases need a running agent and are execution-dependent as marked.
- [ ] **Step 2: Run.** `cargo build -p harness-cli && ENG_BIN=target/debug/eng npm run conformance`. Expected: FAIL.
- [ ] **Step 3: Implement the CLI** as argument parsing plus calls into `Engine`; no logic beyond formatting and the terminal check.
- [ ] **Step 4: Run.** Same command, then `cargo test --workspace`. Expected: PASS for the cases that run now; skipped and ignored execution-dependent cases are reported as such.
- [ ] **Step 5: Update the packet.** Fill `specs/rust-spine/handoff.md` with base and head SHA, commands run and results, and the exact next action "Owner: run Task 11 with the architect once the R-11 containment backend (T13) exists".
- [ ] **Step 6: Commit.** `git commit -am "feat(cli): eng commands, doctor, conformance flow; docs"`

### Task 11: Live tests and the first real run (owner present)

**Files:**
- Modify: `crates/adapters/fixtures/**` (reconstruct synthetic fixtures from validated event shapes), `crates/adapters/src/{claude,codex}.rs` (only where those shapes contradict the provisional format), `specs/rust-spine/handoff.md`

**Interfaces:**
- Consumes: the built `eng`. **All live steps below are blocked until the R-11 containment backend (T13) exists**: they start task processes, and `eng doctor` cannot exit 0 without it. Each authenticated step needs the owner's explicit go-ahead
  and design §9 project/provider authorization; R-11 containment must be established first.
  Use only synthetic non-secret repository content for probes. Design acceptance alone
  does not authorize credential inspection or project disclosure (design §14).

- [ ] **Step 1:** Owner runs `eng login claude` and `eng login codex`. Then `eng doctor` exits 0.
- [ ] **Step 2 (design §11.1):** Run a one-line implementer task on a throwaway repo. Expected: Claude completes inside the harness sandbox with its own sandbox off; `own_sandbox_confirmed_off` is true. Same for a Codex review. If either cannot run under the outer profile, stop and return to the architect: decision 7 must be revisited.
- [ ] **Step 3 (§11.3):** From the egress log, record the exact hosts each CLI needed; set `egress_hosts` to that list; confirm a non-listed host got 403.
- [ ] **Step 4 (§11.2):** In a separate disposable config/home containing only synthetic
  non-secret canaries, use a local helper to check canary readability under the run profile.
  Emit only `readable`, `denied` or `unavailable`; never emit file contents. Do not inspect
  real config directories, tokens or Keychain login items. Record only these statuses in
  `handoff.md`; real credential readability remains UNVERIFIED, and this probe establishes
  no real credential-isolation guarantee (decision 6; design §6.3).
- [ ] **Step 5 (§11.4):** On a throwaway repo containing `.claude/settings.json` with a hook that writes a marker and a `.mcp.json` server that writes a marker, run an implementer task. Expected: neither marker exists; the harness MCP config loaded. If a marker exists, evaluate `--safe-mode` per design §11.4.
- [ ] **Step 6 (§11.6):** Review event shapes from the authorized runs without retaining raw
  authenticated transcripts or probe artifacts. Reconstruct minimal fixtures with synthetic
  values; screen them before retention, sharing or commit. Record runtime/version and
  reconstruction provenance in `fixtures/README.md`; remove PROVISIONAL only for validated
  event shapes. Run `cargo test -p harness-adapters`; fix parser differences. Expected: PASS.
- [ ] **Step 7:** Run one real task end to end on a small real packet chosen by the owner. Expected: stops at `AwaitingHuman`; the owner inspects findings and runs `eng decide`.
- [ ] **Step 8: Commit.** Review the exact staged diff for non-secret synthetic fixtures and
  metadata only, then `git commit -am "test: reconstructed fixtures and live-test results"`.

---

## Self-review record

- Spec coverage: design §5 → Tasks 1–4, 9; §6.1–6.2 → Tasks 5–6; §6.3 → Tasks 6, 7 (secret scan), 8, 11.4; §6.4 → Task 8; §6.5 → Tasks 3, 4, 10; §7 first slice → Tasks 4, 9, 10; §10 → all; §11 → Task 11. Not in this plan by design: research, spec drafting, operate, counsel, routing table, daemon, Linux sandbox, skills rendering, session resume (design §11.5 moves to the plan that adds resume).
- Deviation from the design, for the owner to note: the egress proxy is built in this slice (design §5 drew it as "later"), because §6.1 needs it for any agent to reach its vendor.
