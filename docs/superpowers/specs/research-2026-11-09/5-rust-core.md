# Track 5: Rust core architecture

Date: 2026-11-09. Design only; Rust is not installed on this machine, so no sketch below has
been compiled. Fact grades: (P) probed locally, (D) official doc/repo read today (URL given),
(S) secondary or general knowledge, UNVERIFIED. Unmarked statements are recommendations.

## 0. Facts this design rests on

| # | Fact | Grade |
|---|---|---|
| F1 | Pilot run states are bare strings (`'prepared'`, `'running'`, `'awaiting-decision'`, `'failed'`, `'decision-recorded'`) set inline in `workflow.mjs`; `run()` loops a hard-coded `['worker','verifier','reviewer']` and `await`s one blocking `executor` per role; failure is `m.failure = e.message`. | (P) read `harness/local/workflow.mjs` |
| F2 | Pilot lock: `leased()` exclusive-creates `controller.lock` per run root with `{pid}` only and unlinks it in `finally`, i.e. on controller exit, not on confirmed process death. | (P) `harness/local/artifacts.mjs` |
| F3 | Pilot process runner spawns `detached: true`, kills `-pid` (process group) with SIGKILL on deadline, cancel, output cap and again on close. | (P) `harness/local/process.mjs` |
| F4 | Pilot creates the candidate commit with host Git (`add --all`, `write-tree`, `commit-tree`, `reset --hard`) inside `run()`, with hooks/fsmonitor/submodules disabled. | (P) `workflow.mjs` |
| F5 | Callers read evidence by `path.join(root,'objects',hash)` (e.g. `recordDecision`), so the store layout leaks. | (P) `workflow.mjs` |
| F6 | `transitions.ts` has 16 `TaskState`s, an `ALLOWED_TRANSITIONS` table, Blocked-resume rules, and fails closed (`approval policy not implemented`). | (P) |
| F7 | Requirements A-1..A-22 (adapter interface) and T2 decision D3 (lock with PID + boot ID, human-only stale release) are proposed, not approved. | (P) `specs/adapter-interface/requirements.md` |
| F8 | Open PR `docs/run-ledger-packet` defines a closed six-value `termination_reason`, a closed `failureCode` set, deterministic record IDs, compare-and-append, and a pure classifier. | (P) `prs/docs_run-ledger-packet.diff` |
| F9 | Installed: Claude Code 2.1.318, codex-cli 0.166.0, Apple Git 2.50.1, sqlite3 3.51.0, macOS 26.6.2; no `cargo`/`rustc`. | (P) `--version` |
| F10 | `codex exec` has `--json` ("Print events to stdout as JSONL"), `-s read-only|workspace-write|danger-full-access`, `--output-schema <FILE>`, `--ephemeral`, `--ignore-user-config`; `--dangerously-bypass-approvals-and-sandbox` is "Intended solely for running in environments that are externally sandboxed"; `codex app-server` is marked experimental. | (P) `codex --help`, `codex exec --help` |
| F11 | `sandbox-exec` exists at `/usr/bin/sandbox-exec` and its man page says it is DEPRECATED. | (P) `man sandbox-exec` |
| F12 | Nested Seatbelt: inside an outer `sandbox-exec` profile `(allow default)`, a nested `sandbox-exec` succeeds; inside an outer profile that denies `file-write*` except a working dir (even with `/private/tmp` or `/private/var/folders` re-allowed), the nested call fails with `sandbox_apply: Operation not permitted` (exit 71). Only re-allowing all `file-write*` made nesting work. Cause not determined. | (P) throwaway experiment in scratchpad |
| F13 | `kern.bootsessionuuid` is readable via `sysctl` on this Mac (usable as a boot ID for lock records). | (P) |
| F14 | Codex `codex-rs`: Apache-2.0 ("Copyright 2025 OpenAI"); Cargo workspace resolver "2", edition 2024, ~168 members, including `core`, `protocol`, `app-server`, `app-server-protocol`, `app-server-daemon`, `exec`, `exec-server-protocol`, `linux-sandbox`, `sandboxing`, `bwrap`, `windows-sandbox-*`, `state`, `thread-store`, `worktree`, `git-utils`, `execpolicy`, `process-hardening`, `stdio-to-uds`, `uds`, many `utils/*`. Workspace deps include `tokio 1`, `sqlx =0.9.0` with `sqlite-bundled`, `ts-rs 11`, `schemars 0.8.22`; no `rusqlite`, no `nix`. Clippy denies `unwrap_used`, `expect_used`, `await_holding_lock`, `await_holding_invalid_type`. | (D) https://raw.githubusercontent.com/openai/codex/main/codex-rs/Cargo.toml, https://raw.githubusercontent.com/openai/codex/main/LICENSE, https://github.com/openai/codex/tree/main/codex-rs |
| F15 | Codex `linux-sandbox`: bubblewrap with `--ro-bind / /`, writable roots layered with `--bind`, `.git`/`gitdir:` targets/`.codex` re-mounted read-only, `--unshare-user`, `--unshare-pid`, net namespace when restricted, then seccomp in-process; Landlock is legacy. The README does not describe the macOS path. | (D) https://raw.githubusercontent.com/openai/codex/main/codex-rs/linux-sandbox/README.md |
| F16 | Codex `codex-rs/README.md` says nothing about crate roles; crate purposes in F14 are inferred from names only. | (D) same repo |
| F17 | T3 Code: a v2 `Orchestrator.ts` serializes commands and decides events, doing no provider or filesystem work while deciding; `EventSink.ts` commits events, projections, the accepted command receipt and outbox effects in one DB transaction; subscribers see events only after commit; command receipts make retries idempotent; `EffectWorker.ts` runs effects after intent is recorded and feeds results back; an ack means intent committed, not work finished; `CheckpointStore.ts` captures workspace state in hidden Git refs without adding commits to the user's branch (ref names not given); a provider that cannot roll back its conversation must reject a revert before touching files; projections are the source of truth and `ProjectionMaintenance.ts` prunes events (~1 day kept) so projections cannot be rebuilt; clients replay from a cursor or resume from a snapshot; effects tied to a lost provider process are retired on recovery before new work; retained events must stay decodable. License of T3 Code not checked. | (D) https://github.com/pingdotgg/t3code/blob/main/docs/internals/overview.md |
| F18 | tokio 1.54.1: `kill_on_drop` defaults to false; a dropped `Child` keeps running; reaping is "best-effort" with no timing guarantee; docs advise awaiting the child before dropping; `process_group` sets PGID. | (D) https://docs.rs/tokio/latest/tokio/process/struct.Command.html |
| F19 | tokio `select!` cancel safety: safe = `mpsc recv`, `broadcast recv`, `watch changed`, `accept`, `Signal::recv`, `AsyncRead::read`/`read_buf`, `AsyncWrite::write`/`write_buf`, `StreamExt::next`; unsafe = `read_exact`, `read_to_end`, `read_to_string`, `write_all`; queue-fairness loss on `Mutex::lock`, `RwLock`, `Semaphore::acquire`, `Notify::notified`. `read_line`/`lines` not listed. | (D) https://docs.rs/tokio/latest/tokio/macro.select.html |
| F20 | `flock(2)` locks belong to the open file description and are released when all descriptors referring to it are closed (including inherited ones). | (P) `man 2 flock` (partial read); semantics detail (S) |

## 1. Cargo workspace layout

Principle: few, deep crates. A crate exists only if it has a distinct dependency profile
(pure vs I/O vs async) or a distinct contributor audience. Six crates for the first slice,
three more later.

```
harness/                      (workspace; edition 2024, resolver 2/3, MSRV pinned)
  crates/
    core/        harness-core       PURE. Types, state machines, decide(), classify(), policy.
                                    deps: serde, serde_json, sha2, thiserror. No std::fs,
                                    no std::process, no tokio, no clock, no randomness.
    store/       harness-store      SYNC I/O. SQLite (rusqlite, bundled), event log + hash
                                    chain, outbox, command receipts, projections,
                                    content-addressed artifact files. deps: core, rusqlite.
    exec/        harness-exec       ASYNC I/O (tokio). Process supervision (gated spawn,
                                    process groups, streams, deadlines, kill), Sandbox
                                    (macOS, Linux), Workspace (worktrees, lock, snapshot,
                                    host-Git commits, checkpoint refs). deps: core, tokio.
    adapters/    harness-adapters   Sans-IO translators: claude, codex. Build launch plans,
                                    parse output lines into AgentEvents. deps: core only
                                    (+ serde_json). Fixture-tested from recorded transcripts.
    engine/      harness-engine     The loop: submit Command -> store tx -> effect workers ->
                                    outcome Command. Restart recovery. Public API used by
                                    CLI now and daemon later. deps: core, store, exec, adapters.
    cli/         harness            Binary. clap, output formatting, Ctrl-C -> Cancel command.
                                    deps: engine (and core types for printing). Nothing else.
  later:
    providers/   harness-providers  ModelProvider (OpenRouter, fake), DeploymentProvider
                                    (Vercel, Cloudflare). async, reqwest. deps: core.
    daemon/      harnessd           Unix-socket server over engine API. deps: engine.
    proxy/       harness-proxy      Local egress allowlist proxy for sandboxed agents (if the
                                    sandbox track needs one). deps: tokio, hyper.
  conformance/                      Language-neutral CLI tests (see section 7).
```

Dependency direction (arrows = "depends on"; nothing points upward, nothing points at `cli`):

```
cli -> engine -> { store, exec, adapters, providers(later) } -> core
daemon(later) -> engine
```

Rules enforced in CI (`cargo deny`/a 20-line script over `cargo metadata`):
- `harness-core` has no dependency on tokio, rusqlite, std::process (grep), std::fs (grep),
  `SystemTime::now`, or any RNG. Time and IDs come in as data on the command.
- `harness-adapters` does not depend on tokio: it never spawns a process (see section 3).
- Only `harness-store` touches SQLite; only `harness-exec` spawns processes or runs Git.
- Copy Codex's clippy policy (F14): deny `unwrap_used`, `expect_used`, `await_holding_lock`.

Why not Codex's granularity (F14: ~168 crates): that suits a large team with incremental
compile pressure; for a solo maintainer every crate boundary is a public API to keep stable.
Split a module into a crate only when compile time or a second consumer demands it.

Why `rusqlite` over `sqlx` (Codex uses sqlx, F14): the store is sync by design (single writer
thread, section 5) so it cannot leak async into callers, and rusqlite's `bundled` feature pins
the SQLite version. Grade: recommendation; rusqlite features (S).

## 2. Core types (Rust sketches, uncompiled)

### 2.1 Identity

```rust
// All IDs are strings on the wire, stable forever, never reused.
pub struct TaskId(Ulid);      // ULID: sortable, generated by the engine, passed into core
pub struct CommandId(Ulid);
pub struct EffectId(Ulid);
pub struct ArtifactId([u8; 32]);   // sha256 of the bytes; Display = lowercase hex
pub struct IdempotencyKey(String); // caller-chosen; unique per (task, key)
pub struct Seq(u64);               // global event sequence, gapless, assigned by the store
pub struct Timestamp(i64);         // ms since epoch, supplied by the engine, never read in core
```

### 2.2 Task and Stage

```rust
pub struct Task {
    pub id: TaskId,
    pub repo: RepoRef,              // canonical repo path + its git common dir, both recorded
    pub packet: Option<ArtifactId>, // tree digest of specs/<feature>/ at the base commit
    pub base_commit: CommitSha,
    pub state: TaskState,
    pub stage: Option<StageCursor>, // which stage is active, and its sub-state
    pub budget: Budget,             // max_repair_rounds, deadlines, max_cost_usd (Option each)
    pub repair_rounds_used: u8,
    pub checkpoints: Vec<Checkpoint>,
    pub version: u64,               // task_seq of last applied event (optimistic concurrency)
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage { Research, Spec, Implement, Review, Repair, Operate }
// Verify is not a Stage: it is an effect the harness runs inside Implement/Repair, because
// its output is evidence and no agent performs it.

pub struct StageCursor { pub stage: Stage, pub run: StageRunState, pub round: u8 }
```

The pipeline is data, not code: a `Pipeline` (from config, digested and frozen into the task
at creation like the pilot's `configDigest`) lists stages and, per stage, the adapter, the
sandbox profile, and verify commands. This fixes "hard-coded to three roles" (F1).

### 2.3 Two state machines, both closed enums with exhaustive transition functions

**Task lifecycle** (approval-gated; port of `transitions.ts`, F6). Keep the 16 states and the
table verbatim so `tests/transitions.test.ts` becomes a conformance oracle:

```rust
pub enum TaskState { Draft, Planned, Approved, Implementing, Reviewing, Verified,
    PreviewVerified, MigrationGated, Released, Observed, Closed,
    Blocked, Failed, Cancelled, Repair, Superseded }

pub const ALLOWED: &[(TaskState, &[TaskState])] = &[
    (Draft, &[Planned, Blocked, Cancelled]),
    (Planned, &[Approved, Draft, Blocked, Cancelled, Superseded]),
    (Approved, &[Implementing, Blocked, Cancelled, Superseded]),
    (Implementing, &[Reviewing, Blocked, Failed, Cancelled, Superseded]),
    (Reviewing, &[Verified, Repair, Blocked, Failed, Cancelled]),
    (Repair, &[Reviewing, Blocked, Failed, Cancelled, Superseded]),
    (Verified, &[PreviewVerified, Repair, Blocked, Failed, Cancelled]),
    (PreviewVerified, &[MigrationGated, Repair, Blocked, Failed, Cancelled]),
    (MigrationGated, &[Released, Repair, Blocked, Failed, Cancelled]),
    (Released, &[Observed, Blocked, Failed]),
    (Observed, &[Closed, Blocked, Failed]),
    (Blocked, &[Cancelled, Superseded]),   // resume handled by can_transition, as in TS
    (Failed, &[Repair, Cancelled, Superseded]),
    (Closed, &[]), (Cancelled, &[]), (Superseded, &[]),
];

pub fn can_transition(from: TaskState, to: TaskState,
    approval: Option<&VerifiedApproval>, ctx: &TransitionContext) -> Result<(), Denied>;
```

`VerifiedApproval` has private fields; only the ledger reader (outside core, it shells out to
`ssh-keygen -Y verify`) constructs it. Core still repeats the shape checks so a forged value
fails closed. The approval policy remains `TODO(human)` exactly as in TS.

**Stage run** (execution facts, no approval needed; replaces the pilot's bare strings):

```rust
pub enum StageRunState {
    Pending,
    EffectsInFlight { effects: SmallVec<[EffectId; 2]> },
    AwaitingHuman { reason: HumanGate },          // the slice's terminal stop
    Succeeded,
    Failed { code: FailureCode },
    Cancelled,
}
pub enum HumanGate { ReviewPassed, RepairBudgetExhausted, LostOnRestart, VerifyUnavailable }
```

Transitions are a single `match (state, event)` in `fn apply(&mut Task, &Event)`. Any
unlisted pair is a bug (returns `Err(Corrupt)`), so the compiler-enforced exhaustive match is
the transition table; `#[test] fn table_snapshot()` serialises every (state, event-kind) pair
to a golden JSON that the web/Swift clients can also render.

### 2.4 Commands and events (serializable, versioned)

```rust
#[derive(Serialize, Deserialize)]
pub struct Envelope<T> {
    pub v: u16,                     // wire schema major; bumped only on breaking change
    pub id: CommandId,              // or EventId for events
    pub task: TaskId,
    pub at: Timestamp,
    pub actor: Actor,               // Human{principal} | Engine | Effect{effect_id}
    #[serde(flatten)] pub body: T,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Command {
    CreateTask { repo: RepoRef, packet_path: String, pipeline: ArtifactId, key: IdempotencyKey },
    StartStage { stage: Stage, key: IdempotencyKey },
    RecordEffectOutcome { effect: EffectId, outcome: EffectOutcome },   // key = effect id
    Cancel { reason: CancelReason, key: IdempotencyKey },
    RecordHumanDecision { decision: HumanDecision, statement: ArtifactId,
                          signature: ArtifactId, key: IdempotencyKey },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    TaskCreated { base_commit: CommitSha, packet: ArtifactId, pipeline: ArtifactId },
    StageStarted { stage: Stage, round: u8 },
    EffectIntended { effect: EffectId, spec: EffectSpec },     // the outbox row, mirrored
    EffectStarted { effect: EffectId, process: ProcessIdentity },
    AgentEventsRecorded { effect: EffectId, transcript: ArtifactId, usage: Option<Usage> },
    CheckpointCreated { commit: CommitSha, git_ref: String },
    VerificationFinished { effect: EffectId, passed: bool, evidence: ArtifactId },
    FindingsRecorded { effect: EffectId, findings: ArtifactId, blocking: u32 },
    EffectFinished { effect: EffectId, outcome: RunOutcome },
    RepairRoundStarted { round: u8 },
    AwaitingHuman { gate: HumanGate, evidence: ArtifactId },
    TaskStateChanged { from: TaskState, to: TaskState, approval: Option<String> },
    TaskCancelled { reason: CancelReason },
    #[serde(other)] Unknown,     // clients built against older schemas must not crash
}

pub struct EventRecord { pub seq: Seq, pub task_seq: u64, pub prev_hash: [u8;32],
                         pub hash: [u8;32], pub event: Envelope<Event> }

/// The whole of the domain logic: pure, total, deterministic.
pub fn decide(task: Option<&Task>, cmd: &Envelope<Command>, policy: &Policy)
    -> Result<Decision, Rejection>;
pub struct Decision { pub events: Vec<Event>, pub effects: Vec<EffectSpec> }
```

`EffectSpec` is a closed enum, which is what makes restart behaviour enumerable (section 4):

```rust
pub enum EffectSpec {
    AgentRun { stage: Stage, adapter: RuntimeKind, profile: SandboxProfile,
               base: CommitSha, inputs: Vec<ArtifactId>, resume: Option<SessionRef>,
               deadline_ms: u64 },
    Verify { command: VerifierId, candidate: CommitSha, deadline_ms: u64 },
    Checkpoint { expected_parent: CommitSha, author_time: Timestamp, message: String },
    ExternalRead { kind: ReadKind, max_attempts: u8 },      // later: OpenRouter, Vercel, ...
}
```

### 2.5 Artifacts (content-addressed)

```rust
pub struct Artifact {
    pub id: ArtifactId,          // sha256(bytes)
    pub size: u64,
    pub media_type: MediaType,   // closed: Json, Jsonl, Patch, Markdown, Text, Binary
    pub label: Provenance,
    pub produced_by: Option<EffectId>,
}
pub enum Provenance {
    Input,            // spec packet, pipeline config: frozen at task creation
    RuntimeClaim,     // anything an agent wrote or said (A-6): never evidence
    HarnessEvidence,  // output of a command the harness ran itself (A-17)
    ReviewFindings,   // schema-valid reviewer output (A-17)
    HumanStatement,   // signed decision statement and its signature
}
```

Only `HarnessEvidence` and `ReviewFindings` may be cited by `can_transition` or by
`AwaitingHuman.evidence`; core checks the label, not the caller.

### 2.6 Finding

```rust
pub struct Finding {
    pub id: String,                 // stable within one review: "F1".."Fn"
    pub severity: Severity,         // Critical | High | Medium | Low  (pilot's set)
    pub blocking: bool,             // derived by policy from severity, not taken from model
    pub category: FindingCategory,  // closed enum: Correctness, Security, SpecMismatch, Tests, Other
    pub location: Option<Location>, // { path (repo-relative, validated), line_start, line_end }
    pub message: String,            // capped length
    pub reviewed_commit: CommitSha, // must equal the candidate, as the pilot checks
    pub reviewer: RuntimeKind,
}
pub struct Review { pub verdict: Verdict /* Pass | ChangesRequested */, pub findings: Vec<Finding> }
// Invariant from the pilot: Pass => findings with blocking == true is empty.
```

The JSON Schema for `Review` is generated from these types (schemars) and handed to
`codex exec --output-schema` (F10), so the schema the reviewer sees and the parser core uses
cannot drift.

### 2.7 RunOutcome with a closed failure set

Adopt the open PR's two levels (F8) instead of inventing a third:

```rust
pub enum RunOutcome { Succeeded, Failed(FailureCode), Cancelled }

#[non_exhaustive] // within the crate only; wire form is a closed string set
pub enum FailureCode {
    ExitNonzero, DeadlineExceeded, OutputLimitExceeded,           // process
    SpawnError, SandboxUnavailable, VersionMismatch, ConfigChanged,  // environment
    InvalidAgentOutput, ReviewChangesRequested,                   // output/schema
    PacketChanged, CandidateChanged, SnapshotLimit, GitMetadataTouched, // workspace integrity
    PolicyDenied, BudgetExhausted,                                // policy
    LostOnRestart,                                                // NEW: effect orphaned by a crash
}

pub enum TerminationReason { Succeeded, TaskFailure, BudgetExhausted,
    InfrastructureFailure, PolicyViolation, Cancelled }

pub fn classify(role: EffectRole, code: FailureCode, history: &[Event]) -> TerminationReason;
// Pure port of the PR's six ordered rules; LostOnRestart => InfrastructureFailure.
```

Strings never cross from a process into a failure code: the supervisor returns typed exits.

### 2.8 Static per-adapter capabilities

```rust
pub struct Capabilities {
    pub runtime: RuntimeKind,                 // Claude | Codex (| Cursor later)
    pub model_family: ModelFamily,            // Anthropic | OpenAI | ... ; review must differ
    pub pinned_version: &'static str,         // "2.1.318", "0.166.0" (F9); A-9
    pub auth: AuthMode,                       // SubscriptionLogin (harness never sees tokens)
    pub profiles: &'static [SandboxProfile],  // which it can be run under (A-3)
    pub writes_workspace: bool,               // false for reviewers
    pub structured_output: StructuredOutput,  // None | JsonSchemaFile (codex --output-schema)
    pub session_resume: bool,                 // claude --resume <id>; codex resume (UNVERIFIED)
    pub reports_usage: bool,
    pub own_sandbox: OwnSandbox,              // MustDisable | Compatible  (see F12)
    pub bidirectional: bool,                  // app-server style approvals; false for slice
}
```

These are `const`s in each adapter, compared at startup against `--version` output; mismatch
is `VersionMismatch` before any task bytes reach the runtime (A-20).

## 3. Seams

A seam gets a trait only when two implementations exist in the first year. Every trait below
is object-safe and sized for 2-5 functions.

### 3.1 `RuntimeAdapter` (real: claude, codex) — sans-IO

The adapter never spawns. It turns a run spec into an argv/env/stdin plan and turns output
lines into normalized events. The supervisor (exec crate) does all I/O, sandboxing, deadlines
and killing once, for every adapter. This keeps each adapter ~300 lines, fixture-testable
from recorded transcripts, and makes an out-of-process adapter possible later without
redesign.

```rust
pub trait RuntimeAdapter: Send + Sync {
    fn capabilities(&self) -> &'static Capabilities;
    fn plan(&self, run: &AgentRunSpec) -> Result<LaunchPlan, AdapterError>;   // argv, env allowlist, stdin bytes, files to stage
    fn parse_line(&self, st: &mut ParseState, line: &[u8]) -> Result<Vec<AgentEvent>, AdapterError>;
    fn finish(&self, st: ParseState, exit: ExitInfo) -> AgentRunResult;       // claims + session ref + usage
}
```

- Claude: `claude -p --output-format stream-json --verbose --session-id <uuid> | --resume <uuid> --settings <harness file>` (from `specs/claude-adapter/design.md`, (P) read; flags exist in `claude --help` (P)). `parse_line` checks `init.session_id`/`apiKeySource` per that packet.
- Codex: `codex exec --json -s read-only --output-schema <review.schema.json> --ephemeral --ignore-user-config` (F10). Whether `-s read-only` is used or `--dangerously-bypass-approvals-and-sandbox` inside the harness sandbox depends on F12 (section 5.4).
- `codex app-server` is not in the slice: experimental (F10) and bidirectional. When added,
  `parse_line` returns `AgentEvent::Reply(bytes)` for approval responses and the supervisor
  writes them to stdin; the trait does not change.

### 3.2 `Sandbox` (real: macOS Seatbelt, Linux bubblewrap) — also sans-IO

```rust
pub trait Sandbox: Send + Sync {
    fn probe(&self) -> Result<SandboxInfo, SandboxUnavailable>;   // run once at startup; fail closed
    fn wrap(&self, profile: &ProfileSpec, plan: LaunchPlan) -> Result<LaunchPlan, SandboxError>;
}
```

- Linux: port `sandboxCommand` (pilot) and borrow Codex's ordering (F15): read-only base,
  writable worktree, `.git` and gitdir targets re-bound read-only, `--unshare-pid`,
  `--die-with-parent`, net namespace unless allowed.
- macOS: generate an SBPL profile and wrap with `/usr/bin/sandbox-exec -p` (deprecated but
  present, F11). The profile generator is pure and snapshot-tested.
- `ProfileSpec` is resolved by core from the task (A-2): `ReadOnly { mounts }` or
  `WorkspaceWrite { worktree, home, request_dir }` plus `Network::{None, Allow(hosts)}`.

### 3.3 `ModelProvider` (real: OpenRouter, fake) — later, async

```rust
#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn id(&self) -> ProviderId;
    async fn complete(&self, req: AdvisorRequest, cancel: CancellationToken) -> Result<AdvisorResponse, ProviderError>;
}
```
Text in, findings out, no tools (brief). The fake is not test scaffolding only: it is the
offline/privacy mode. Called only from an `ExternalRead` effect.

### 3.4 `DeploymentProvider` (real: Vercel, Cloudflare) — later, read-only by type

```rust
#[async_trait]
pub trait DeploymentProvider: Send + Sync {
    async fn list_deployments(&self, project: &ProjectRef, limit: u16) -> Result<Vec<Deployment>, OpsError>;
    async fn deployment(&self, id: &DeploymentId) -> Result<Deployment, OpsError>;
    async fn logs(&self, id: &DeploymentId, window: TimeWindow) -> Result<ArtifactBytes, OpsError>;
}
```
No mutating method exists on the trait. A production change is a human action recorded as a
`HumanStatement`, not an API call the harness can make.

### 3.5 Seams that should NOT exist yet

| Candidate | Why no trait now | What to do instead |
|---|---|---|
| `EvidenceStore` | One implementation (local CAS). A trait would not fix the layout leak (F5); encapsulation does. | Concrete `ArtifactStore` in `harness-store` with `put(bytes, label) -> ArtifactId`, `get(&ArtifactId) -> Bytes` (re-hashes), `verify_all()`. No path ever leaves the crate; tests use a temp dir instance. Add a trait when a remote store exists. |
| `WorktreeLock` | One implementation. | Concrete `Workspace::acquire(task) -> WorkspaceGuard` in exec (section 4.4). |
| `Judgment` | Rules are pure functions in core; the hosted model is shadow-only (brief), so it never decides. A trait in core would have to be async or sync-with-I/O, both wrong. | Rules: `core::policy::next_action(&Observation) -> Action`. Hosted judge: an `ExternalRead { kind: ShadowJudge }` effect whose response is stored as an artifact and an event, never read by `decide`. Introduce `trait Judge` when the hosted model is allowed to decide; its input (`Observation`) and output (`Action`) types already exist, so that is a small change. |
| `ResearchProvider` | Research is manual (Perplexity by hand); no second implementation. | Research output is an `Input` artifact the human drops into the packet. If Perplexity-via-API is wanted, it is a `ModelProvider` with a different model id. |
| `Clock`, `IdGenerator` | Injected as data (`Timestamp`, IDs on the envelope). | Engine fills them in; core stays deterministic. |
| `Vcs` (Git abstraction) | Only Git. | `harness-exec::git` module, hardened host Git (port the pilot's `gitArgs`/`gitEnv`). |

## 4. Durable execution

### 4.1 SQLite schema (one DB per user, `~/Library/Application Support/<harness>/state.db`, 0600)

```sql
PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;

CREATE TABLE schema_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE tasks (                         -- projection; rebuildable from events
  id TEXT PRIMARY KEY, state TEXT NOT NULL, version INTEGER NOT NULL,
  snapshot TEXT NOT NULL,                    -- JSON of core::Task
  updated_seq INTEGER NOT NULL);

CREATE TABLE commands (                      -- receipts = idempotency
  id TEXT PRIMARY KEY, task_id TEXT NOT NULL, idempotency_key TEXT NOT NULL,
  body_hash BLOB NOT NULL,                   -- same key + different body => rejected
  body TEXT NOT NULL, received_at INTEGER NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('accepted','rejected')),
  rejection TEXT, first_seq INTEGER, last_seq INTEGER,
  UNIQUE (task_id, idempotency_key));

CREATE TABLE events (                        -- append-only, hash-chained, never pruned
  seq INTEGER PRIMARY KEY,                   -- gapless global order (assigned in tx)
  task_id TEXT NOT NULL, task_seq INTEGER NOT NULL,
  command_id TEXT NOT NULL REFERENCES commands(id),
  v INTEGER NOT NULL, type TEXT NOT NULL, body TEXT NOT NULL,   -- canonical JSON
  at INTEGER NOT NULL,
  prev_hash BLOB NOT NULL, hash BLOB NOT NULL,                  -- sha256(prev_hash || seq || body)
  UNIQUE (task_id, task_seq));
CREATE TRIGGER events_no_update BEFORE UPDATE ON events BEGIN SELECT RAISE(ABORT,'append-only'); END;
CREATE TRIGGER events_no_delete BEFORE DELETE ON events BEGIN SELECT RAISE(ABORT,'append-only'); END;

CREATE TABLE outbox (                        -- pending and historical effects
  effect_id TEXT PRIMARY KEY, task_id TEXT NOT NULL, kind TEXT NOT NULL,
  spec TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('intended','starting','started','done','abandoned')),
  attempt INTEGER NOT NULL DEFAULT 0, max_attempts INTEGER NOT NULL,
  process TEXT,                              -- JSON ProcessIdentity once started
  intended_seq INTEGER NOT NULL, outcome_seq INTEGER);
CREATE INDEX outbox_pending ON outbox(state) WHERE state IN ('intended','starting','started');

CREATE TABLE artifacts (                     -- index; bytes live in objects/<aa>/<sha256>
  id TEXT PRIMARY KEY, size INTEGER NOT NULL, media_type TEXT NOT NULL,
  label TEXT NOT NULL, produced_by TEXT, created_seq INTEGER NOT NULL);

CREATE TABLE workspaces (                    -- lock records (section 4.4)
  task_id TEXT PRIMARY KEY, path TEXT NOT NULL UNIQUE, holder_effect TEXT,
  pgid INTEGER, leader_pid INTEGER, leader_start_us INTEGER, boot_id TEXT);

CREATE TABLE anchors (                        -- optional: exported head hashes
  seq INTEGER PRIMARY KEY, head_hash BLOB NOT NULL, exported_at INTEGER NOT NULL);
```

Decisions:
- The **event log is the audit chain**. A separate audit table would be a second log that can
  disagree with the first. `harness audit verify` walks `events` and recomputes hashes; the
  run-ledger JSONL of the open PR (F8) becomes an **export** (`harness runs export`) derived
  from events, not a second writer. Unlike T3 Code (F17) events are never pruned: audit beats
  disk; at this volume (tens of events per task) it does not matter.
- The **approval ledger stays outside SQLite** (separate OS account, signed, anchored, per
  `approval-ledger.md`). The engine only reads it. The DB being owned by the controller user
  is precisely why it carries no authority.
- Artifact bytes are files written with the pilot's `put` discipline (exclusive create,
  fsync file and directory) **before** the transaction that references them; an orphan file
  after a crash is harmless garbage, a dangling reference cannot happen.

### 4.2 The loop: commit intent, act, record outcome

```
submit(cmd):                                    -- single writer thread
  BEGIN IMMEDIATE
    if receipt(task, key) exists: same body_hash -> return it; else reject
    task = load projection
    decision = core::decide(task, cmd, policy)  -- pure
    append events (chain), insert outbox rows (state='intended'),
    update projection, insert receipt
  COMMIT  -> notify subscribers(seq range) -> wake effect workers

effect worker (per outbox row):
  1. tx: intended -> starting, attempt += 1
  2. gated spawn: start `harness __shim` as new process-group leader; shim blocks reading
     one byte on fd 3
  3. tx: record ProcessIdentity{pgid, pid, start time, boot_id}; starting -> started;
     append EffectStarted
  4. write the go byte; shim execs the sandboxed command (sandbox-exec/bwrap ... agent)
  5. stream output -> adapter.parse_line -> artifacts (RuntimeClaim) ; enforce caps/deadline
  6. wait leader; kill group; confirm group empty (section 5.2)
  7. post-processing that is itself an effect step: snapshot scan, checkpoint commit
  8. submit(RecordEffectOutcome{effect, outcome}) with key = effect_id  -- idempotent
```

The gated spawn closes the classic gap "crashed after spawn, before recording the PID": if
the engine dies before step 3 commits, the shim reads EOF on fd 3 and exits without running
anything, so a `starting` row on restart provably ran nothing. (Same idea as Codex's
`arg0` multitool re-exec, F14 member `arg0`; the gating itself is our design.)

### 4.3 Restart: exactly what happens per in-flight effect

Recovery runs before the engine accepts any new command (as T3 Code does, F17).

| Outbox state at crash | Agent run (Claude/Codex) | Verification command | External read (OpenRouter, Vercel logs) | Checkpoint (host Git) |
|---|---|---|---|---|
| `intended` | Start it normally. | Start it normally. | Start it normally. | Run it. |
| `starting` | Shim got EOF; nothing ran. Mark `abandoned`, re-intend with same spec (counts no attempt against budget). | Same. | Same. | Same. |
| `started`, process gone (boot_id differs, or pid absent, or pid's start time differs) | **Never replayed.** Record `Failed(LostOnRestart)` with the partial transcript artifact. Worktree is tainted: reset to the last checkpoint ref (`git read-tree -u --reset` on a harness-owned worktree), stage goes `AwaitingHuman{LostOnRestart}`. Rationale: non-deterministic, spent subscription quota, may have half-written files; pilot rule "interrupted runs are not replayed" (F1). Optional later: offer `claude --resume <session>` as a human-chosen action if `capabilities.session_resume`. | Discard partial output; if candidate commit unchanged and `attempt < max_attempts` (default 2) re-run; else `Failed(LostOnRestart)`. Safe because verification mounts the candidate read-only and only partial-free complete runs become evidence. | Re-run if `attempt < max_attempts`; reads are idempotent. Paid calls count toward budget on each attempt (may double-pay once). | Re-run: idempotent by construction. `commit-tree` with author/committer time taken from the intent yields the same SHA; ref update uses `git update-ref <ref> <new> <expected-old>` (compare-and-swap). |
| `started`, process still alive (same boot, same pid + start time) | Kill the group (5.2), confirm death, then as row above. Never adopt a running agent: its stdout pipe died with the old engine. | Kill, then as above. | Kill (if a subprocess), then as above. | Wait/kill, then re-run. |

### 4.4 Worktree lock (fixes F2)

- Per-**worktree**, not per run: `workspaces` row keyed by task and unique by path.
- Liveness of the **controller**: the engine holds an `flock` on `<state>/engine.lock` for its
  lifetime; the OS releases it on crash (F20), so a second engine refuses to start and a
  crashed engine never needs manual cleanup of that lock. Do not pass this fd to children
  (CLOEXEC; Rust std sets CLOEXEC by default (S)).
- Liveness of the **work**: the row records `pgid`, `leader_pid`, `leader_start_us` and
  `boot_id` (`kern.bootsessionuuid` on macOS, F13; `/proc/sys/kernel/random/boot_id` on
  Linux (S)). The lock is released only when the supervisor has confirmed the group empty
  (5.2). On restart, a row whose identity can be confirmed dead is released automatically;
  a row whose state cannot be determined (e.g. pid alive with matching start time but kill
  fails) blocks and needs a human (`harness workspace release --task X` after inspection),
  matching A-11 / D3.

### 4.5 Checkpoints: hidden Git refs vs adapter-created commits

Recommendation: **the harness creates every commit; hidden refs pin them**. Agents never
commit (their sandbox mounts `.git` read-only, as the pilot and Codex do, F15).

- After each agent run that may write: scan the worktree (port `scanWorkspace`), then on the
  host with hardened Git: `add --all`, `write-tree`, `commit-tree -p <prev>`, and
  `update-ref refs/harness/tasks/<task-id>/cp/<n> <sha> <expected-old>`. The worktree is
  detached; no user branch moves (same property T3 Code states, F17).
- `refs/harness/...` is outside `refs/heads` and `refs/tags`, so default fetch/push refspecs
  ignore it and `git gc` keeps the objects (S; verify with a test). `git push --mirror` would
  publish them: document it.
- Revert = reset the worktree to a checkpoint. Agent conversation cannot be rolled back
  (claude/codex sessions), so following T3 Code's rule (F17), revert starts a **new** agent
  session with the findings file, never `--resume` across a revert.
- On human `accept`: create `refs/heads/harness/<task-id>` at the final checkpoint (the only
  user-visible ref the harness writes). Refs are deleted only by an explicit `harness gc`.
- Adapter-created commits (agent runs `git commit`) are rejected: they would run repository
  hooks under the agent's identity and violate A-16.

## 5. Async strategy

### 5.1 Where tokio lives, and where it must not

| Crate | Async? | Why |
|---|---|---|
| core | No | Pure. Any `async fn` here is a review-blocking defect. |
| adapters | No | Sans-IO translators (3.1). |
| store | No | One dedicated OS thread owns the `rusqlite::Connection`; engine talks to it through an `mpsc` of closures with `oneshot` replies. This thread *is* the command serializer (F17's serialized commands). |
| exec | Yes | Child stdout/stderr streams, deadlines, signals, cancellation. |
| engine | Yes | Orchestrates workers; owns the runtime. |
| providers / proxy / daemon | Yes | HTTP (reqwest/hyper), sockets. |
| cli | Minimal | `#[tokio::main(flavor = "current_thread")]` or a runtime built in `main`; no tokio types in its args. |

Public API of `harness-engine` exposes `async fn submit(...)` and `fn subscribe(after: Seq) ->
impl Stream<Item = EventRecord>`; no tokio *types* (JoinHandle, Sender) leak to callers.

### 5.2 Cancellation composed with process-tree kill

- `CancellationToken` (tokio-util) tree: engine root -> task -> effect. Cancel is first a
  **command** (`Cancel` -> `TaskCancelled` event committed), then the token trips. A crash
  between the two is fine: recovery sees a cancelled task with started effects and kills them.
- Kill sequence (one function, used by cancel, deadline, output cap, shutdown, recovery):
  1. `killpg(pgid, SIGTERM)`; 2. wait up to grace (5 s) for leader exit; 3. `killpg(pgid,
  SIGKILL)`; 4. `child.wait().await` (reap leader); 5. poll `killpg(pgid, 0)` until `ESRCH`
  (bounded, e.g. 2 s); 6. on Linux additionally rely on bwrap PID-namespace teardown
  (`--unshare-pid`, `--die-with-parent`); 7. only then release the workspace lock and record
  the outcome. If step 5 times out: `Failed(...)` plus lock left held for a human.
- Known gap on macOS: a descendant that calls `setsid()` leaves the process group and
  survives `killpg`; macOS has no PID namespaces or child subreaper (S). Mitigations for the
  sandbox track to choose from: descendant enumeration by parent PID chain before killing
  (`proc_listchildpids`, UNVERIFIED API name), a dedicated OS user per agent, or an SBPL rule
  limiting process creation (UNVERIFIED whether SBPL can deny `setsid`). Flag as open.
- Ctrl-C: agent processes are in their own process group, so the terminal's SIGINT does not
  reach them; the CLI must translate SIGINT into a `Cancel` command (and a second Ctrl-C into
  immediate kill).

### 5.3 Rust pitfalls and the pattern that avoids each

| Pitfall | Pattern |
|---|---|
| Cancellation safety in `select!` loops: `read_exact`/`read_to_end`/`write_all` lose data when a sibling branch wins (F19); `read_line` is not documented either way. | Wrap each pipe once in `FramedRead::new(stdout, LinesCodec::new_with_max_length(N))` created *outside* the loop and poll `StreamExt::next` (cancel safe, F19). Never call `write_all` inside `select!`; do stdin writes in their own task. |
| Holding a `std::sync::Mutex` guard across `.await` (deadlock / `!Send`). | Clippy `await_holding_lock` = deny (as Codex, F14). State that crosses awaits lives in one task that owns it (actor). |
| Half-closed pipes: (a) child never reads its stdin and we block writing a large context; (b) a grandchild inherits stdout so EOF never comes after the leader exits; (c) stderr fills its pipe buffer and the child blocks while we only read stdout. | (a) Write stdin from a separate task, then drop it to send EOF; treat EPIPE as non-fatal (pilot does, F3). (b) Completion = leader `wait()` + group kill + bounded drain timeout, never "stdout EOF". (c) Always drain stdout and stderr concurrently, with byte caps that trigger the kill sequence. |
| Zombies: tokio reaps "best-effort" with no timing guarantee and a dropped `Child` keeps running (F18). | Never drop a `Child` unawaited: the supervisor owns it in a struct whose only exit path goes through the kill sequence; `kill_on_drop(true)` as a last-resort backstop only. Descendants reparent to launchd/init, not to us. |
| Blocking calls on the async runtime (SQLite, `git` via `std::process`, hashing large files). | SQLite on its own thread (5.1); Git through `tokio::process`; hashing via `spawn_blocking`. |
| `JoinHandle` dropped silently swallows panics/errors in effect workers. | Workers in a `JoinSet` owned by the engine; a worker panic becomes `Failed(InfrastructureFailure)` via the same outcome path. |
| Signals: SIGPIPE ignored by Rust runtime by default (S), SIGCHLD handling owned by tokio. | Do not install a custom SIGCHLD handler; use `tokio::signal` for SIGINT/SIGTERM. |
| Unbounded agent output (memory). | Byte caps per stream and per line (pilot's `maxOutputBytes`), transcript spooled to a temp file then `put` into CAS. |

### 5.4 The nested-sandbox problem (affects adapters and the sandbox track)

F12 shows a nested `sandbox-exec` fails under an outer profile that restricts file writes.
Claude Code's Bash sandbox and Codex's `-s read-only`/`workspace-write` both rely on applying
their own Seatbelt profile on macOS (UNVERIFIED from vendor docs today; Codex README does not
describe macOS, F15). If the harness wraps them in its own restrictive profile, their inner
sandbox apply will likely fail. Options: (a) harness sandbox outermost and tell the agent not
to sandbox (`codex --dangerously-bypass-approvals-and-sandbox`, which the help text says is
for "environments that are externally sandboxed", F10); (b) rely on the agent's sandbox
(violates "harness controls the boundary"). Recommend (a), recorded as
`Capabilities.own_sandbox = MustDisable`, and a probe in `Sandbox::probe` that checks it.
Needs a real agent run to confirm; not done (no model calls allowed).

## 6. Path to clients

- **Engine API is the protocol.** `submit(Envelope<Command>) -> Receipt` and
  `subscribe(after: Option<Seq>) -> Stream<EventRecord> | Snapshot`. The CLI calls it
  in-process today; the daemon exposes the same two calls over a Unix socket
  (`<state>/harness.sock`, mode 0600, peer UID checked), JSON Lines framing: one request per
  line, one response or event per line. Codex ships a `stdio-to-uds` crate (F14), a hint that
  the same "JSON lines over stdio or UDS" shape works for them.
- **Web UI:** the daemon adds a localhost WebSocket (or SSE) with a per-launch token, carrying
  the identical JSON. **macOS app:** connects to the same socket. **iOS:** remote only, via a
  relay the user runs (out of scope); it consumes the same events and can submit only
  `RecordHumanDecision`/`Cancel`.
- **Types for clients:** generate JSON Schema from core types (schemars, as Codex does, F14),
  then TypeScript (ts-rs or from schema) and Swift (from schema). Commit the generated schema;
  CI fails on unreviewed change.

Decide now (cheap now, expensive later):
1. Stable string IDs (ULID for task/command/effect, sha256 hex for artifacts) and never reuse.
2. Global gapless `seq` plus per-task `task_seq`; events immutable; ordering = `seq`.
3. Every event and command is an `Envelope` with `v`; additive-only changes within a major;
   `#[serde(other)] Unknown` on enums clients deserialize; unknown fields ignored by clients,
   rejected by the engine (`deny_unknown_fields` on commands only).
4. Projection snapshot = `Task` JSON with `as_of_seq`, so a client can start from a snapshot
   and replay `seq > as_of_seq`.
5. Commands are acknowledged at intent commit (F17 semantics); clients learn results from
   events, never from the ack.
6. Idempotency keys on every command (a phone retrying a decision must not double-apply).

Can wait: auth beyond UID/token, remote relay, push notifications, pagination, multi-user,
snapshots compaction, the daemon itself (CLI in-process is enough until a GUI exists).

## 7. Migration from the Node pilot

### 7.1 Conformance suite

The existing tests import modules directly (`prepare`, `run({executor})`, `canTransition`);
they cannot test a Rust binary. Convert them into a language-neutral suite:

- `conformance/` holds fixtures (tiny Git repo builder script, spec packet, pipeline config,
  fake agent scripts that emit recorded stream-json/JSONL, fake verifier/reviewer) and cases
  as data: `steps: [cli args, stdin] -> expect: exit code, JSON stdout fields, store queries
  via harness inspect --json, files present/absent`.
- Runner: Node `node:test` (already in the repo, no new dependency). The binary under test is
  `$HARNESS_BIN`. Run the suite against a thin Node CLI wrapper first to validate the cases,
  then against the Rust binary. The Node pilot stays the oracle until parity.
- `transitions.test.ts`: export `ALLOWED_TRANSITIONS`, `RESUMABLE_STATES` and a matrix of
  `canTransition` results for all 16x16 pairs (with/without blocked record, approval) to
  `conformance/golden/transitions.json`; Rust unit tests assert equality. `harness policy
  check-transition --json` exposes the same function for CLI cases.

### 7.2 Port order (highest risk-reduction first)

1. State machine golden table (`transitions.test.ts`, 27 cases) — pure, unblocks core.
2. Process supervision: "bounded runner kills process group descendants on deadline and
   cancellation", lease exclusivity and "interrupted jobs are not replayed".
3. Sandbox fail-closed: "default CLI execution fails closed when isolation is unavailable";
   denied read/write/network (Linux case runs in Linux CI; macOS equivalent added).
4. Workspace integrity regressions `bug1`..`bug9` (nested repo filters, ignored files, modes,
   symlinked store, `{workspace}` argv, assume-unchanged) — these encode hard-won security
   lessons; port verbatim.
5. Evidence: "content-addressed evidence corruption is detected on restart and import",
   candidate-changed invalidation, config/tools digest changes.
6. Human decision signatures (canonical bytes, wrong principal/namespace, cross-run replay).
7. Run-ledger PR acceptance tests (F8), re-pointed at `harness runs export`/classifier.
8. `ledger-guard.test.mjs` last: it tests a Claude Code hook script, not the core; port when
   the hook becomes `harness hook ledger-guard`.

### 7.3 What to delete (after the corresponding conformance cases pass on Rust)

- `harness/local/workflow.mjs`, `artifacts.mjs`, `process.mjs`, `sandbox.mjs`,
  `snapshot.mjs`, `cli.mjs` — whole pilot.
- `harness/policy/transitions.ts` — replaced by `harness-core`; keep the golden JSON.
- The open run-ledger PR's planned Node modules (`harness/ledger/*.mjs`): recommend merging
  only its requirements/design as the spec for events + export + classifier, not building it
  in Node (human decision).
- Keep: `specs/`, `docs/`, the approval-ledger design, `harness/hooks/claude` until ported.

## 8. Open-source readiness for a solo maintainer

Crate boundaries a newcomer can work in without the whole picture:
- **`harness-adapters`**: add or fix an agent by writing `plan` + `parse_line` against
  recorded transcript fixtures. No async, no DB, no sandbox knowledge needed.
- **`harness-exec::sandbox`**: profile generators are pure string builders with snapshot
  tests; platform probe tests run in CI on that OS.
- **`harness-core`**: property tests (proptest) over `decide`/`apply`; a contributor can
  change policy and see exactly which goldens change.
- `harness-store`/`engine` are the maintainer's: few people should touch durability code.

Extension points that are not Rust:
| Extension | Mechanism | Now? |
|---|---|---|
| Skills | Markdown directories the harness copies into the agent's harness-owned home per stage (both Claude Code and Codex have skill systems; Codex lists `skills` crates, F14). Digest frozen into the task. | Yes |
| Tools | MCP servers declared per stage in the pipeline config, passed to agents through their own config files; the harness never proxies MCP itself. | Yes (config only) |
| Pipelines, prompts, finding schema | TOML pipeline + Markdown prompt templates + generated JSON Schema. | Yes |
| Verifiers | Any executable configured outside the repo (pilot rule), run sandboxed. | Yes |
| Adapters as out-of-process plugins | A plugin speaking "LaunchPlan request / AgentEvent JSONL" over stdio. | **Not now.** |

Out-of-process adapters are not worth it yet: there are two first-party adapters, each a small
sans-IO translator; a plugin protocol would be a second public wire format to version and
secure (a plugin would choose argv for a process the harness then runs, which makes it part of
the trust base). Because adapters are already sans-IO with serializable inputs and outputs,
lifting them out later is mechanical. Revisit when a third-party runtime (Cursor, Kiro,
aider) wants in.

Licensing: Codex is Apache-2.0 (F14), so borrowing patterns (and code with attribution) is
compatible with an Apache-2.0 or MIT/Apache dual-licensed harness; T3 Code's license was not
checked.

## 9. Build order for the first usable slice

Slice: spec packet -> Claude implements in a sandboxed worktree -> harness verifies ->
Codex reviews read-only -> bounded repair -> stop for human decision.

Crates (6): `harness-core`, `harness-store`, `harness-exec`, `harness-adapters`,
`harness-engine`, `harness` (cli). Not built: providers, daemon, proxy, deployment,
research, judgment, app-server, protocol crate.

Types needed (only): `TaskId`, `CommandId`, `EffectId`, `ArtifactId`, `Seq`, `Timestamp`,
`IdempotencyKey`, `CommitSha`; `Task`, `Stage` (Spec/Implement/Review/Repair used),
`StageRunState`, `HumanGate`, `TaskState` + `ALLOWED` + `can_transition` (ported, used only to
gate `Approved -> Implementing -> Reviewing -> Repair/Verified` display; approval policy still
fails closed, so the slice ends at `AwaitingHuman`, never auto-`Verified`); `Envelope`,
`Command` (5 variants), `Event` (~12 variants), `EffectSpec` (AgentRun, Verify, Checkpoint);
`Artifact` + `Provenance`; `Review`, `Finding`, `Severity`; `RunOutcome`, `FailureCode`,
`TerminationReason`, `classify`; `Capabilities`, `AgentRunSpec`, `LaunchPlan`, `AgentEvent`;
`ProfileSpec`; `Budget { max_repair_rounds }`.

Order, each step with its proof:
1. **Conformance scaffolding** (Node, before any Rust): golden `transitions.json`, fixture
   repo builder, fake agents. -> verify: suite green against the Node pilot where applicable.
2. **core**: types, table port, `decide` for the slice, `classify`. -> verify: golden table
   equality; proptest "no event sequence reaches Succeeded without a HarnessEvidence verify
   and a Pass review on the same commit"; "repair rounds never exceed budget".
3. **store**: schema, single-writer thread, idempotent submit, hash chain, CAS. -> verify:
   crash tests via a `failpoints` feature (abort after each statement, reopen, `audit verify`
   passes, retried command returns same receipt).
4. **exec**: gated spawn, kill sequence, caps, deadlines; `Sandbox` macOS + Linux with
   fail-closed probe; worktree create/lock/scan/commit/checkpoint ref. -> verify: ported
   pilot process + lease + bug1..9 cases; macOS write-denial probe like F12.
5. **adapters**: claude stream-json and codex `exec --json` parsers from **recorded real
   transcripts** (the human records them; this track may not make model calls). -> verify:
   fixture tests incl. truncated/malformed lines => `InvalidAgentOutput`.
6. **engine**: loop + recovery table (4.3). -> verify: kill -9 the engine at each effect state
   in conformance tests; outcomes match the table.
7. **cli**: `harness task new --packet specs/<id>`, `harness run <task>`, `harness status
   [--json]`, `harness cancel`, `harness decide request|record` (port of the pilot's signed
   decision), `harness audit verify`. -> verify: full-flow conformance case with fake agents,
   then one supervised real run by the human.

## Open questions only the human can answer

1. Accept the slice ending at `AwaitingHuman` (no automatic `Verified`) until the approval
   policy `TODO(human)` is written?
2. On macOS, harness Seatbelt outermost with agents' own sandboxes disabled (F12, F10)? This
   depends on a real Claude/Codex run inside an outer profile, which needs your go-ahead.
3. Subscription logins (Claude keychain, Codex `CODEX_HOME` auth) vs A-8's harness-owned home:
   which credential paths may the sandbox expose? (sandbox/auth track)
4. Merge the run-ledger PR as spec only and implement it in Rust as an event export, rather
   than as Node modules?
5. After a crash mid-agent-run: always stop for you (recommended), or allow one automatic
   retry from the last checkpoint within the repair budget?
6. License for the open-source release (Apache-2.0 vs MIT/Apache dual)?
7. Is a separate OS user per agent run acceptable on macOS if process-group kill proves
   escapable (5.2)?
