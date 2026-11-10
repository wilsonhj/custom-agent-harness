//! The durable loop: commands go through `harness_core::decide` into the store in one
//! transaction; effect workers perform the intended work and submit its outcome as a new
//! command; recovery settles whatever was in flight when the last engine died (design §5.4).
//!
//! Effects of one task run one at a time, in intent order. Every process runs gated
//! (`eng-shim`), sandboxed (`sandbox-exec`), with `HOME` and `TMPDIR` set to the task home and
//! an environment that is only the adapter plan's variables plus `PATH`.

mod recovery;
mod worker;

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use harness_core::artifact::{Artifact, MediaType, Provenance};
use harness_core::decide::{Command, EffectOutcome, EffectSpec, Facts, Rejection, Task, decide};
use harness_core::diff::chunk_diff;
use harness_core::finding::Review;
use harness_core::ids::{ArtifactId, CommitSha, EffectId, Seq, TaskId, Timestamp};
use harness_core::lifecycle::{DecisionKind, TaskState};
use harness_core::policy::Pipeline;
use harness_core::types::{Envelope, RuntimeKind};
use harness_exec::TreeGone;
use harness_exec::hostgit::git;
use harness_store::{ChainBroken, EventRecord, Receipt, Store, StoreError};
use tokio_util::sync::CancellationToken;

pub use harness_store::OutboxState;

/// Where state lives and how to launch each runtime.
///
/// Runtimes are launched by their real paths: every configured binary is canonicalised, and
/// the directories a runtime reads are passed to the sandbox as readable, because the sandbox
/// denies the whole real `$HOME` (where Claude Code and nvm install).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineConfig {
    /// `~/Library/Application Support/eng` by default (the CLI decides). Created mode 0700.
    pub state_dir: PathBuf,
    /// The `claude` executable (a symlink is resolved). Its directory is readable in the
    /// sandbox.
    pub claude_bin: PathBuf,
    /// The Codex entry point: with `node_bin`, the `@openai/codex/bin/codex.js` script (a
    /// symlink to it is resolved), launched as `node <codex.js>` with the package directory
    /// readable; without `node_bin`, an executable launched directly.
    pub codex_bin: PathBuf,
    /// Node, for Codex. Its directory is put first on `PATH` and is readable in the sandbox.
    pub node_bin: Option<PathBuf>,
    /// The `eng-shim` gate binary (shipped next to `eng`).
    pub shim_bin: PathBuf,
    /// Further directories every sandboxed process may read (tools that verification commands
    /// or wrapper scripts need).
    pub extra_read_dirs: Vec<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Chain(#[from] ChainBroken),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("no such task {0}")]
    UnknownTask(TaskId),
    #[error("pipeline: {0}")]
    Pipeline(String),
    #[error("git: {0}")]
    Git(String),
    #[error("workspace: {0}")]
    Workspace(String),
    /// The kill sequence could not prove a run's process tree gone. The worktree lock stays
    /// held and the task waits for a human.
    #[error("task {task}: a process tree could not be confirmed dead; the worktree stays locked")]
    KillFailed { task: TaskId },
    /// An effect of the task is `starting` or `started` with no live worker: a kill failed
    /// earlier. A human must inspect the worktree.
    #[error("task {task} has an effect whose process could not be confirmed dead")]
    WorkspaceLocked { task: TaskId },
    /// The engine's own outcome was refused by `decide` (an engine bug, never a runtime's).
    #[error("outcome of effect {effect} rejected: {reason}")]
    OutcomeRejected { effect: EffectId, reason: String },
    #[error("corrupt state: {0}")]
    Corrupt(String),
}

pub struct Engine {
    cfg: EngineConfig,
    store: Store,
    /// The real `$HOME`, denied to every sandboxed process.
    home: PathBuf,
    /// Per task: proof that no process has run in its worktree since the last kill sequence.
    /// Taken before a new process is spawned there.
    proofs: Mutex<HashMap<TaskId, TreeGone>>,
    /// Runtimes whose `--version` matched the pin in this engine's lifetime.
    versions_ok: Mutex<HashSet<RuntimeKind>>,
}

/// A sortable unique id: 48-bit milliseconds then 80 random bits, Crockford base32 in
/// lowercase (26 characters, ULID layout). Monotonic within this process.
pub fn new_id() -> String {
    static LAST: Mutex<(u64, u128)> = Mutex::new((0, 0));
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
        & ((1 << 48) - 1);
    let (ms, rand) = {
        let mut last = LAST.lock().unwrap_or_else(PoisonError::into_inner);
        if now_ms <= last.0 {
            last.1 = (last.1 + 1) & ((1 << 80) - 1);
        } else {
            *last = (now_ms, random80());
        }
        *last
    };
    let value: u128 = (u128::from(ms) << 80) | rand;
    const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";
    (0..26)
        .rev()
        .map(|i| char::from(ALPHABET[((value >> (i * 5)) & 31) as usize]))
        .collect()
}

fn random80() -> u128 {
    use std::io::Read as _;
    let mut buf = [0u8; 16];
    let read = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut buf[..10]));
    if read.is_err() {
        // No randomness: fall back to time and pid; ids stay unique through the counter.
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        return (nanos ^ (u128::from(std::process::id()) << 64)) & ((1 << 80) - 1);
    }
    u128::from_be_bytes(buf) >> 48
}

/// Now, in Unix microseconds.
pub fn now() -> Timestamp {
    Timestamp(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_micros()).unwrap_or(i64::MAX)),
    )
}

/// Test-only crash points (`failpoints` feature): when `ENG_FAILPOINT` names this point, the
/// process sends itself `SIGKILL` here.
#[cfg(feature = "failpoints")]
pub(crate) fn failpoint(name: &str) {
    if std::env::var("ENG_FAILPOINT").as_deref() == Ok(name) {
        let _ = std::process::Command::new("/bin/kill")
            .args(["-KILL", &std::process::id().to_string()])
            .status();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}

#[cfg(not(feature = "failpoints"))]
#[inline(always)]
pub(crate) fn failpoint(_name: &str) {}

/// What `decide` needs beyond the task and the command, owned.
struct OwnedFacts {
    pipeline: Pipeline,
    review: Option<Review>,
    diff_chunks: Option<u32>,
}

impl Engine {
    /// Opens the store (taking the engine lock) and runs recovery before returning.
    pub async fn open(cfg: EngineConfig) -> Result<Engine, EngineError> {
        let store = Store::open(&cfg.state_dir)?;
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| EngineError::Workspace("HOME is not set".into()))?;
        let engine = Engine {
            cfg,
            store,
            home,
            proofs: Mutex::new(HashMap::new()),
            versions_ok: Mutex::new(HashSet::new()),
        };
        engine.recover().await?;
        Ok(engine)
    }

    pub fn config(&self) -> &EngineConfig {
        &self.cfg
    }

    /// Stores caller-provided input (the encoded packet, the pipeline TOML) labelled `Input`.
    pub fn put_input(&self, bytes: &[u8], media: MediaType) -> Result<ArtifactId, EngineError> {
        Ok(self
            .store
            .put_artifact(bytes, media, Provenance::Input, None)?)
    }

    pub fn artifact(&self, id: &ArtifactId) -> Result<Option<Artifact>, EngineError> {
        Ok(self.store.artifact(id)?)
    }

    /// The artifact's bytes, verified against its id.
    pub fn read_artifact(&self, id: &ArtifactId) -> Result<Vec<u8>, EngineError> {
        Ok(self.store.get_artifact(id)?)
    }

    pub fn task(&self, id: &TaskId) -> Result<Option<Task>, EngineError> {
        Ok(self.store.task(id)?)
    }

    pub fn events_after(&self, seq: Seq, limit: u32) -> Result<Vec<EventRecord>, EngineError> {
        Ok(self.store.events_after(seq, limit)?)
    }

    pub fn verify_chain(&self) -> Result<Seq, EngineError> {
        Ok(self.store.verify_chain()?)
    }

    /// Decides and commits `cmd`. A rejected command comes back as a receipt with
    /// `accepted == false`. After an accepted `Accept`, creates branch `harness/<task>` at the
    /// accepted checkpoint.
    pub async fn submit(&self, cmd: Envelope<Command>) -> Result<Receipt, EngineError> {
        let task = self.store.task(&cmd.task)?;
        let Some(facts) = self.facts_for(task.as_ref(), &cmd).await? else {
            return Ok(Receipt {
                command: harness_core::ids::CommandId::new(cmd.id.clone()),
                accepted: false,
                rejection: Some(Rejection::UnknownTask.to_string()),
                first_seq: None,
                last_seq: None,
            });
        };
        let accept = matches!(
            cmd.body,
            Command::RecordHumanDecision {
                kind: DecisionKind::Accept,
                ..
            }
        );
        let task_id = cmd.task.clone();
        let receipt = self.submit_with(cmd, &facts)?;
        if receipt.accepted
            && accept
            && let Some(task) = self.store.task(&task_id)?
        {
            self.ensure_branch(&task).await?;
        }
        Ok(receipt)
    }

    /// Submits with fresh effect ids, retrying once with as many as `decide` asks for.
    fn submit_with(
        &self,
        cmd: Envelope<Command>,
        facts: &OwnedFacts,
    ) -> Result<Receipt, EngineError> {
        let mut n =
            2 + facts.pipeline.verify_commands.len() + facts.diff_chunks.unwrap_or(0) as usize;
        loop {
            let ids: Vec<EffectId> = (0..n).map(|_| EffectId::new(new_id())).collect();
            let needed = Cell::new(None);
            let decide_fn = |t: Option<&Task>, c: &Envelope<Command>| {
                let r = decide(
                    t,
                    c,
                    &Facts {
                        pipeline: &facts.pipeline,
                        review: facts.review.as_ref(),
                        diff_chunks: facts.diff_chunks,
                        new_effect_ids: &ids,
                    },
                );
                if let Err(Rejection::NeedEffectIds { needed: k }) = &r {
                    needed.set(Some(*k));
                }
                r
            };
            let receipt = self.store.submit(cmd.clone(), &decide_fn)?;
            match needed.get() {
                Some(k) if !receipt.accepted && k > n => n = k,
                _ => return Ok(receipt),
            }
        }
    }

    /// `None` when the command names a task that does not exist (and is not `CreateTask`).
    async fn facts_for(
        &self,
        task: Option<&Task>,
        cmd: &Envelope<Command>,
    ) -> Result<Option<OwnedFacts>, EngineError> {
        let pipeline_id = match (&cmd.body, task) {
            (Command::CreateTask { pipeline, .. }, _) => pipeline.clone(),
            (_, Some(t)) => t.pipeline.clone(),
            (_, None) => return Ok(None),
        };
        let pipeline = self.pipeline(&pipeline_id)?;
        let mut facts = OwnedFacts {
            pipeline,
            review: None,
            diff_chunks: None,
        };
        if let Command::RecordEffectOutcome { effect, outcome } = &cmd.body {
            match outcome {
                EffectOutcome::AgentRun {
                    review: Some(id), ..
                } => {
                    let bytes = self.store.get_artifact(id)?;
                    let review: Review = serde_json::from_slice(&bytes)
                        .map_err(|e| EngineError::Corrupt(format!("review {id}: {e}")))?;
                    facts.review = Some(review);
                }
                EffectOutcome::Verify { .. } => {
                    let candidate = task.and_then(|t| {
                        t.progress.in_flight.iter().find_map(|s| match s {
                            EffectSpec::Verify {
                                effect: e,
                                candidate,
                                ..
                            } if e == effect => Some(candidate.clone()),
                            _ => None,
                        })
                    });
                    if let (Some(t), Some(candidate)) = (task, candidate) {
                        let diff = self.candidate_diff(t, &candidate).await?;
                        let chunks = chunk_diff(&diff, facts.pipeline.review_chunk_max_lines).len();
                        facts.diff_chunks = Some(u32::try_from(chunks).unwrap_or(u32::MAX));
                    }
                }
                EffectOutcome::AgentRun { review: None, .. } | EffectOutcome::Checkpoint { .. } => {
                }
            }
        }
        Ok(Some(facts))
    }

    fn pipeline(&self, id: &ArtifactId) -> Result<Pipeline, EngineError> {
        let bytes = self.store.get_artifact(id)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| EngineError::Pipeline("pipeline is not UTF-8".into()))?;
        Pipeline::parse_toml(text).map_err(|e| EngineError::Pipeline(e.to_string()))
    }

    /// `git diff <task base>..<candidate>` with the harness's diff flags (no external diff
    /// drivers, no textconv).
    async fn candidate_diff(
        &self,
        task: &Task,
        candidate: &CommitSha,
    ) -> Result<String, EngineError> {
        let out = git(
            Path::new(&task.repo.path),
            &[
                "-c",
                "core.quotePath=false",
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                "--no-renames",
                "--no-relative",
                "--full-index",
                "--src-prefix=a/",
                "--dst-prefix=b/",
                task.base_commit.as_str(),
                candidate.as_str(),
            ],
        )
        .await
        .map_err(|e| EngineError::Git(e.to_string()))?;
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    /// Points `refs/heads/harness/<task>` at the accepted checkpoint (idempotent; a branch
    /// that already names another commit is an error).
    async fn ensure_branch(&self, task: &Task) -> Result<(), EngineError> {
        if task.state != TaskState::Verified {
            return Ok(());
        }
        let Some(cp) = task.checkpoints.last() else {
            return Ok(());
        };
        let repo = Path::new(&task.repo.path);
        let name = format!("refs/heads/harness/{}", task.id);
        let existing = git(repo, &["rev-parse", "--verify", "-q", &name]).await;
        if let Ok(out) = existing {
            let current = String::from_utf8_lossy(&out).trim().to_owned();
            if current == cp.commit.as_str() {
                return Ok(());
            }
            return Err(EngineError::Git(format!(
                "{name} already points at {current}, not {}",
                cp.commit
            )));
        }
        git(
            repo,
            &["update-ref", &name, cp.commit.as_str(), &"0".repeat(40)],
        )
        .await
        .map_err(|e| EngineError::Git(e.to_string()))?;
        Ok(())
    }

    /// Runs the task's intended effects, one at a time, until none is left (the task then
    /// waits for a human, has failed, or was cancelled). Stops early, leaving intended effects
    /// for a later call, once `cancel` is tripped; a running process is killed and its outcome
    /// recorded as `Cancelled`.
    pub async fn run_until_idle(
        &self,
        task: &TaskId,
        cancel: CancellationToken,
    ) -> Result<Task, EngineError> {
        loop {
            let rows: Vec<_> = self
                .store
                .pending_effects()?
                .into_iter()
                .filter(|r| r.task == *task)
                .collect();
            if rows.iter().any(|r| r.state != OutboxState::Intended) {
                return Err(EngineError::WorkspaceLocked { task: task.clone() });
            }
            let Some(row) = rows.into_iter().next() else {
                break;
            };
            if cancel.is_cancelled() {
                break;
            }
            self.run_effect(row, &cancel).await?;
        }
        self.store
            .task(task)?
            .ok_or_else(|| EngineError::UnknownTask(task.clone()))
    }

    fn take_proof(&self, task: &TaskId) -> Option<TreeGone> {
        self.proofs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(task)
    }

    fn keep_proof(&self, task: &TaskId, proof: TreeGone) {
        self.proofs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(task.clone(), proof);
    }

    /// Releases the task's worktree lock. Only callable with proof that the process tree that
    /// held it is gone.
    fn release(&self, task: &TaskId, _proof: &TreeGone) -> Result<(), EngineError> {
        Ok(self.store.release_workspace(task)?)
    }
}
