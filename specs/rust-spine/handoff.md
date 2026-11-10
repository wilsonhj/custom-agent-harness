# Handoff: rust-spine / planning

| Field | Value |
|---|---|
| Base SHA | b56ae9d (origin/main) |
| Head SHA | uncommitted working tree on branch `docs/rust-harness-design` |
| Author (runtime / session ID) | Claude Code / lead architect session, 2026-11-09 |
| Date | 2026-11-09 |

## Changed files

- `docs/superpowers/specs/2026-11-09-unified-harness-design.md` — approved design
- `docs/superpowers/specs/research-2026-11-09/*.md` — six graded research reports
- `docs/superpowers/plans/2026-11-09-rust-spine.md` — implementation plan
- `specs/rust-spine/*` — this packet

## Implemented requirements

- None. This session produced the design, plan and packet only.

## Tests run and results

| Command | Result | Evidence |
|---|---|---|
| Local sandbox, proxy and process-kill probes | pass (as probes, not as tests) | `research-2026-11-09/1a-macos-sandbox.md` |
| `claude --help`, `codex --help`, `agent --help` probes | recorded | `research-2026-11-09/1b-subscription-clis.md` |
| Any `cargo` or `npm` command | not run | Rust toolchain not installed |

## Contract changes during implementation

Later tasks build against these, not the plan text, where they differ.

- T2 (a5d313c): modules are `pub mod` with no root re-exports (`harness_core::finding::Severity`).
  `Capabilities` is Serialize only. `RawFinding`/`RawReview` are not `Eq`/`Hash` (f32).
  `ReviewRejected { PathOutsideRepo{finding,path}, PassWithBlocking{finding}, BadLineRange{finding},
  ConfidenceOutOfRange{finding} }` (finding is 1-based). String ids: `new(impl Into<String>)`,
  `as_str`; hex ids: `parse(&str) -> Result<_, IdError>`, `as_str`, `LEN`. `MAX_CLAIM_CHARS = 2000`
  characters. `ModelFamily::OpenAi` is `"openai"` on the wire. A finding with a file and no lines is
  `Location` 0..0 (whole file); start without end is start..start.
- Integration commit after T2: `harness_exec::plan::{LaunchPlan, TreeGone}` are shared by T5, T6
  and T7 (re-exported at the crate root). `TreeGone::confirmed()` is crate-private (supervisor only);
  `TreeGone::for_tests()` exists under the `test-support` feature, enabled for this crate's own
  tests. The exec crate's dependencies are pre-declared (tokio, tokio-util, libc, thiserror; dev:
  tempfile) so parallel tasks do not edit `Cargo.toml`.

- T3 (95bbb8c, plus architect fix): `HumanDecision::principal() -> &str`, `seq() -> Seq` added;
  `HumanDecision` has no Deserialize; `Subject` serialises as `{"type", "value"}`.
  `Draft -> Planned` is ungated (architect's fix: it records the packet digest; the gate is
  `Planned -> Approved`). These listed edges always deny in this slice because no decision kind
  authorises them: `Planned -> Draft`, `Failed -> Repair`, `Verified -> Repair`, any
  `-> Superseded`, and every resume out of `Blocked`. The TS check that a resume happens at the
  blocked-at commit has no Rust counterpart yet; `Blocked` is not used by this slice.
- T8 (5e6979f): `ParseState::new(role: Role, argv: &[String])`. `AdapterPlan.argv[0]` is the bare
  name `claude`/`codex`; the engine substitutes the resolved binary. Claude's reviewer schema is
  passed inline to `--json-schema` (`review_schema` is used only by Codex; missing => PolicyDenied).
  Codex reads the prompt from stdin (argv ends with `-`). Claude's `own_sandbox_confirmed_off` is
  true only if an `init.sandbox` field shows it off; no such field is documented, so it is false
  until Task 11 records a real `init` event — the engine must therefore treat Claude runs as
  blocked on this until Task 11 decides the check.

- T7 (6f7dab9): `WorkspaceError { Git(GitError), Io(io::Error), InvalidTaskId(String),
  RootInsideRepo(PathBuf), BadPacketPath(String), PacketNotFound(String), PacketNotText(String),
  GitMetadataTouched }`; `GitError { Spawn(io::Error), Failed { args, code, stderr } }`;
  `SecretHit { file: String, line: u32 /* new-file line */, rule: &'static str }`;
  added `workspace::encode_packet(&[(String, String)]) -> Vec<u8>` (bytes `packet_digest` hashes,
  for the store). Any Git failure in `commit_candidate` maps to `SpawnError`; a checkpoint ref
  already pointing elsewhere is `CandidateChanged`. Engine order: `commit_candidate` only after
  `TreeGone` (scan-to-add gap); `reset_to` the new checkpoint before verification so tested bytes
  equal committed bytes. Secret scan also checks a `--text` diff (binary files); prefixes need 20+
  token chars; PEM rule also matches `BEGIN PRIVATE KEY`. Pilot regressions bug5, bug6a, bug6b,
  bug6c, bug7 are `#[ignore]` with reasons (pilot-only config/argv/store, or replaced by
  `ignored_worker_artifact_is_excluded_from_the_candidate_and_removed_by_reset`).
- T6 (ed647fe): added `resolve(&SandboxProfile) -> Result<ResolvedProfile, ProfileError>`;
  `ResolvedProfile { worktree, worktree_writable, task_home, read_deny, read_allow_extra: Vec<String>,
  network }` with `params()` and `render()`; `ProfileError::Unresolvable { path, reason }`;
  consts `SANDBOX_EXEC`, `MACH_LOOKUP_ALLOW`; test-only `EgressProxy::start_allowing_local_ports_for_tests`.
  Missing `read_deny` paths resolve through their nearest real ancestor and stay denied; most specific
  path rule wins, deny wins ties; `read_deny` paths are also write-denied; `<wt>/.git` always
  write-denied; `wrap` puts `--` before the argv. Requests that never reach a host decision are
  logged with port 0. `wrap` does NOT set `HOME`/`TMPDIR`: the engine must set both to the task home.
  Not done: refusing allowed hosts that resolve to private/loopback addresses.
- T5 (514fe7c): `spawn_gated(shim: &Path, plan, limits, on_identity) -> Result<Supervised, FailureCode>`
  (failed callback => `SpawnError`); `Supervised::run(self, sink, cancel) -> (ExitInfo, Result<TreeGone,
  KillFailed>)`; `ExitInfo.cancelled: bool`; `Stream { Stdout, Stderr }`, `SpawnAbort(pub String)`,
  `KillFailed { survivors: Vec<i32> }`, `Supervised::identity()`; consts `KILL_GRACE`, `SWEEP_BOUND`,
  `POLL_INTERVAL` (5 ms), `SECOND_PASS_DELAY`; shim `GATE_FD`, `RAN_NOTHING = 125`, `EXEC_FAILED = 127`;
  binary `eng-shim` in harness-exec (the engine locates it next to `eng`). Oversized line =>
  `OutputLimitExceeded`. Descendants: 5 ms process-table scan + process group + parent chain +
  Apple-private "responsible process" link (isolated in `responsibility_spi()`). Known gap: a process
  that escapes in the same 5 ms window as its leader exiting; `kill_identity` after an engine restart
  has no recorded set. Follow-up: test whether the sandbox profile can deny `setsid`.
- T6 fix (f4fc34b, security review): reads of the whole real `$HOME` are denied by default
  (`default_read_deny` now includes `home`); ancestors of allowed trees inside `$HOME` can be stat'ed
  but not listed. No terminal access: `/dev/tty*` read/write/ioctl denied, no `file-ioctl`, no
  `pseudo-tty`. Case-insensitive denies for `WORKTREE_DENY_NAMES` directly under the worktree.
  Engine/adapters MUST launch runtimes by their real paths and pass those dirs in `read_allow_extra`:
  Claude `~/.local/share/claude/versions/<ver>` (the binary); Codex the
  `.../node_modules/@openai/codex` dir, launched as `node .../codex/bin/codex.js` with node's `bin`
  dir on PATH. Symlinks under `~/.local/bin` and nvm `bin` fail because they sit in denied `$HOME`.
- T4 (5a24630): `Facts.diff_lines` is now `Facts.diff_chunks: Option<u32>` (chunk count from
  `chunk_diff(base..candidate)`; `Some(0)` skips review as clean). `Rejection` adds
  `NeedEffectIds { needed }`, `InvalidOutcome`, `MissingFact { fact }`. `Task` adds
  `progress: Progress` and a public `Stop` enum. Events: `FindingsRecorded` gains `review: ArtifactId`;
  new `StageFailed { code }`; `TaskCreated { repo, base_commit, packet, pipeline }`;
  `HumanDecisionRecorded { kind, subject }` (principal from the envelope actor, which must be
  `Human`). `apply` returns `Corrupt` and refuses unknown event types. `Run` re-issues effects
  stopped at `LimitExhausted`/`LostOnRestart` with new ids. A failed verify still moves the task to
  `Reviewing` before the repair decision (no `Implementing -> Repair` edge). Reject and send-back
  must name the current candidate (or the packet digest before any checkpoint). Any other effect
  failure fails the task.

## Known limitations

- Adapter fixtures will be provisional until plan Task 11 records real transcripts.
- Two research reports contain a local home-directory path once each; scrub before the
  repository is made public.
- The human decision is protected by the sandbox and file permissions, not by a signature.

## Unresolved decisions

- Open-source license — owner: repository owner
- Projects that must send nothing to third parties — owner: repository owner (not needed by this slice)

## Assumptions made

- "Yes, I accept" (owner, 2026-11-09) approves the design, accepts the token exposure and
  the terms ambiguity, and permits live tests; the architect confirms again before the first
  authenticated run.
- The six research reports are accurate where graded (P) or (D); UNVERIFIED items are listed
  in design §11.

## Exact next action

Owner reviews `docs/superpowers/plans/2026-11-09-rust-spine.md`, installs the Rust toolchain
(plan Task 1 step 1), and tells the architect to start Task 1.
