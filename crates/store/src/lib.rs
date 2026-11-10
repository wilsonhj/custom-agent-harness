//! SQLite-backed command log, hash-chained events, outbox and content-addressed artifacts.
//! The only crate that touches SQLite.
//!
//! One dedicated writer thread owns the connection (research 5 §5.1); every call is a job sent
//! to it over a channel. [`Store::submit`] runs one `BEGIN IMMEDIATE` transaction that checks
//! the idempotency receipt, folds the decision's events into the task projection, appends them
//! to the hash chain, inserts an outbox row per intended effect and writes the receipt. The
//! caller's `decide` runs on the caller's thread: the writer hands it the task and waits for the
//! decision, so no caller code (and no I/O) ever runs on the writer thread.
//!
//! Artifact bytes are written and fsynced before the transaction that indexes them, so a crash
//! can leave an unreferenced object file but never a reference to missing bytes.
//!
//! A second [`Store::open`] on the same state directory fails with [`StoreError::Locked`]: the
//! store holds an exclusive `flock` on `<state_dir>/engine.lock` for its lifetime (the file is
//! opened close-on-exec, so children never inherit it, and the OS drops the lock on a crash).

mod artifacts;
mod events;
mod writer;

use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread::JoinHandle;

use harness_core::artifact::{Artifact, MediaType, Provenance};
use harness_core::decide::{Command, Decision, EffectSpec, Event, Rejection, Task};
use harness_core::ids::{ArtifactId, CommandId, EffectId, Seq, TaskId};
use harness_core::types::Envelope;
use rusqlite::Connection;

pub use writer::ChainBroken;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// Another store (another `eng` process) holds `<state_dir>/engine.lock`.
    #[error("another engine holds the state directory lock")]
    Locked,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// The idempotency key was already used with a different command body.
    #[error("idempotency key {key:?} was already used with a different command")]
    IdempotencyConflict { key: String },
    /// Stored bytes do not hash to their id.
    #[error("artifact {0} is corrupt")]
    CorruptArtifact(ArtifactId),
    #[error("artifact {0} not found")]
    NotFound(ArtifactId),
    /// An event cites an artifact that carries no reference with the label it needs from the
    /// effect that must have produced it (`by: None`: from any effect of the same task).
    #[error("artifact {id} has no {label:?} reference produced by {by:?}")]
    Unlabelled {
        id: ArtifactId,
        label: Provenance,
        by: Option<EffectId>,
    },
    /// The log and the code disagree (`apply` refused an event, or a row does not parse).
    #[error("corrupt state: {0}")]
    Corrupt(String),
    /// `mark_effect` asked for a state change the row's current state does not allow.
    #[error("effect {effect} cannot move from {from} to {to}")]
    BadEffectTransition {
        effect: EffectId,
        from: String,
        to: &'static str,
    },
    #[error("unknown effect {0}")]
    UnknownEffect(EffectId),
    /// The workspace is locked by another effect.
    #[error("workspace of task {task} is held by effect {holder}")]
    WorkspaceHeld { task: TaskId, holder: EffectId },
    /// The writer thread is gone (it panicked, or the store is shutting down).
    #[error("the store writer thread is gone")]
    WriterGone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    pub command: CommandId,
    pub accepted: bool,
    pub rejection: Option<String>,
    pub first_seq: Option<Seq>,
    pub last_seq: Option<Seq>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutboxState {
    Intended,
    Starting,
    Started,
    Done,
    Abandoned,
}

impl OutboxState {
    pub fn as_str(self) -> &'static str {
        match self {
            OutboxState::Intended => "intended",
            OutboxState::Starting => "starting",
            OutboxState::Started => "started",
            OutboxState::Done => "done",
            OutboxState::Abandoned => "abandoned",
        }
    }

    fn parse(s: &str) -> Option<OutboxState> {
        Some(match s {
            "intended" => OutboxState::Intended,
            "starting" => OutboxState::Starting,
            "started" => OutboxState::Started,
            "done" => OutboxState::Done,
            "abandoned" => OutboxState::Abandoned,
            _ => return None,
        })
    }
}

/// Who led a run's process tree: the store's copy of `harness_exec::ProcessIdentity` (the
/// store depends on core only).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProcessRecord {
    pub pgid: i32,
    pub pid: i32,
    pub start_us: i64,
    pub boot_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxRow {
    pub effect: EffectId,
    pub task: TaskId,
    pub spec: EffectSpec,
    pub state: OutboxState,
    /// Starts counted so far (a start that provably ran nothing is not counted).
    pub attempt: u32,
    pub max_attempts: u32,
    pub process: Option<ProcessRecord>,
    pub intended_seq: Seq,
    pub outcome_seq: Option<Seq>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRecord {
    pub seq: Seq,
    pub task_seq: u64,
    pub prev_hash: [u8; 32],
    pub hash: [u8; 32],
    pub event: Envelope<Event>,
}

/// A task's worktree lock record. `holder` is `None` once released; `last_process` is the
/// last process tree that ran in the worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRow {
    pub task: TaskId,
    pub path: PathBuf,
    pub holder: Option<EffectId>,
    pub last_process: Option<ProcessRecord>,
}

type Job = Box<dyn FnOnce(&mut Connection) + Send>;

/// The pure decision `Store::submit` applies: `harness_core::decide::decide` with the
/// caller's facts bound.
pub type DecideFn<'a> =
    dyn Fn(Option<&Task>, &Envelope<Command>) -> Result<Decision, Rejection> + 'a;

pub struct Store {
    tx: Option<mpsc::Sender<Job>>,
    thread: Option<JoinHandle<()>>,
    objects: PathBuf,
    /// Held for the store's lifetime; dropped (unlocked) after the writer thread has closed
    /// the database.
    _lock: File,
}

const SCHEMA: &str = include_str!("schema.sql");
const SCHEMA_VERSION: &str = "2";

fn create_private_dir(dir: &Path) -> Result<(), StoreError> {
    match fs::create_dir(dir) {
        Ok(()) => fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

impl Store {
    /// Opens (creating if needed) `state_dir` (mode 0700), takes the exclusive engine lock,
    /// opens `state.db` (mode 0600) with WAL, `synchronous=FULL` and foreign keys, and applies
    /// the schema.
    pub fn open(state_dir: &Path) -> Result<Store, StoreError> {
        if let Some(parent) = state_dir.parent() {
            fs::create_dir_all(parent)?;
        }
        create_private_dir(state_dir)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(state_dir.join("engine.lock"))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(fs::TryLockError::WouldBlock) => return Err(StoreError::Locked),
            Err(fs::TryLockError::Error(e)) => return Err(e.into()),
        }
        let objects = state_dir.join("objects");
        create_private_dir(&objects)?;

        let db = state_dir.join("state.db");
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&db)?;
        let mut conn = Connection::open(&db)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        {
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            tx.execute_batch(SCHEMA)?;
            tx.execute(
                "INSERT OR IGNORE INTO schema_meta (key, value) VALUES ('version', ?1)",
                [SCHEMA_VERSION],
            )?;
            let version: String = tx.query_row(
                "SELECT value FROM schema_meta WHERE key = 'version'",
                [],
                |r| r.get(0),
            )?;
            if version != SCHEMA_VERSION {
                return Err(StoreError::Corrupt(format!(
                    "schema version {version}, this build reads {SCHEMA_VERSION}"
                )));
            }
            tx.commit()?;
        }

        let (tx, rx) = mpsc::channel::<Job>();
        let thread = std::thread::Builder::new()
            .name("eng-store-writer".into())
            .spawn(move || {
                let mut conn = conn;
                while let Ok(job) = rx.recv() {
                    job(&mut conn);
                }
            })?;
        Ok(Store {
            tx: Some(tx),
            thread: Some(thread),
            objects,
            _lock: lock,
        })
    }

    /// Runs `f` on the writer thread and waits for its result.
    fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<T, StoreError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        let job: Job = Box::new(move |conn| {
            let _ = reply_tx.send(f(conn));
        });
        self.tx
            .as_ref()
            .ok_or(StoreError::WriterGone)?
            .send(job)
            .map_err(|_| StoreError::WriterGone)?;
        reply_rx.recv().map_err(|_| StoreError::WriterGone)?
    }

    /// Commits `cmd`, its events and its intended effects in one transaction, or returns the
    /// receipt of the earlier command with the same idempotency key (the effect id for
    /// `RecordEffectOutcome`). A rejected command changes nothing and leaves no receipt, so
    /// it can be retried with different facts.
    pub fn submit(
        &self,
        cmd: Envelope<Command>,
        decide: &DecideFn<'_>,
    ) -> Result<Receipt, StoreError> {
        let (to_caller, from_writer) = mpsc::channel::<writer::ToCaller>();
        let (to_writer, from_caller) = mpsc::channel::<Result<Decision, Rejection>>();
        let job_cmd = cmd.clone();
        let job: Job = Box::new(move |conn| {
            let ask = |task: Option<Task>| -> Option<Result<Decision, Rejection>> {
                to_caller.send(writer::ToCaller::Ask(task)).ok()?;
                from_caller.recv().ok()
            };
            let result = writer::submit(conn, &job_cmd, &ask);
            let _ = to_caller.send(writer::ToCaller::Done(result));
        });
        self.tx
            .as_ref()
            .ok_or(StoreError::WriterGone)?
            .send(job)
            .map_err(|_| StoreError::WriterGone)?;
        loop {
            match from_writer.recv().map_err(|_| StoreError::WriterGone)? {
                writer::ToCaller::Ask(task) => {
                    let decision = decide(task.as_ref(), &cmd);
                    to_writer
                        .send(decision)
                        .map_err(|_| StoreError::WriterGone)?;
                }
                writer::ToCaller::Done(result) => return result,
            }
        }
    }

    /// Stores `bytes` under their SHA-256 and records the reference `(id, label, by)`. The bytes
    /// are written once and are durable before the reference row is written. Labels belong to
    /// references, not to bytes: the same bytes may carry several references (an agent's claim
    /// and the harness's evidence), and recording an existing reference again is a no-op. A
    /// claim never becomes evidence, because every evidence check asks for a reference with
    /// the evidence label produced by a given effect ([`Store::has_label`]).
    pub fn put_artifact(
        &self,
        bytes: &[u8],
        media: MediaType,
        label: Provenance,
        by: Option<&EffectId>,
    ) -> Result<ArtifactId, StoreError> {
        let id = ArtifactId::of(bytes);
        artifacts::write_object(&self.objects, &id, bytes)?;
        writer::failpoint("artifact:after_file");
        let size = bytes.len() as u64;
        let by = by.cloned();
        let row_id = id.clone();
        self.call(move |conn| {
            writer::index_artifact(conn, &row_id, size, media, label, by.as_ref())
        })?;
        Ok(id)
    }

    /// The artifact's bytes, re-hashed on read. `NotFound` unless some reference names it.
    pub fn get_artifact(&self, id: &ArtifactId) -> Result<Vec<u8>, StoreError> {
        if self.artifact_refs(id)?.is_empty() {
            return Err(StoreError::NotFound(id.clone()));
        }
        artifacts::read_object(&self.objects, id)
    }

    /// Every reference to the artifact (size, media type, label, producer), oldest first.
    pub fn artifact_refs(&self, id: &ArtifactId) -> Result<Vec<Artifact>, StoreError> {
        let id = id.clone();
        self.call(move |conn| writer::artifact_refs(conn, &id))
    }

    /// Does an artifact with this id carry `label` produced by effect `by` (`None`: recorded
    /// with no producing effect)?
    pub fn has_label(
        &self,
        id: &ArtifactId,
        label: Provenance,
        by: Option<&EffectId>,
    ) -> Result<bool, StoreError> {
        let id = id.clone();
        let by = by.cloned();
        self.call(move |conn| writer::has_label(conn, &id, label, by.as_ref()))
    }

    /// Does an artifact with this id carry `label` produced by some effect of `task`?
    pub fn labelled_in_task(
        &self,
        id: &ArtifactId,
        label: Provenance,
        task: &TaskId,
    ) -> Result<bool, StoreError> {
        let id = id.clone();
        let task = task.clone();
        self.call(move |conn| writer::labelled_in_task(conn, &id, label, &task))
    }

    pub fn task(&self, id: &TaskId) -> Result<Option<Task>, StoreError> {
        let id = id.clone();
        self.call(move |conn| writer::load_task(conn, &id))
    }

    /// Every task id, in creation order.
    pub fn task_ids(&self) -> Result<Vec<TaskId>, StoreError> {
        self.call(writer::task_ids)
    }

    /// Task projections and their event history from one writer job. No command can be
    /// committed between these reads, so a status snapshot has one sequence boundary.
    pub fn snapshot(&self) -> Result<(Vec<Task>, Vec<EventRecord>), StoreError> {
        self.call(|conn| {
            let events = writer::events_after(conn, Seq(0), u32::MAX)?;
            let tasks = writer::task_ids(conn)?
                .iter()
                .map(|id| {
                    writer::load_task(conn, id)?
                        .ok_or_else(|| StoreError::Corrupt(format!("missing task {id}")))
                })
                .collect::<Result<_, _>>()?;
            Ok((tasks, events))
        })
    }

    pub fn events_after(&self, seq: Seq, limit: u32) -> Result<Vec<EventRecord>, StoreError> {
        self.call(move |conn| writer::events_after(conn, seq, limit))
    }

    /// Outbox rows still `intended`, `starting` or `started`, in intent order.
    pub fn pending_effects(&self) -> Result<Vec<OutboxRow>, StoreError> {
        self.call(writer::pending_effects)
    }

    pub fn effect(&self, id: &EffectId) -> Result<Option<OutboxRow>, StoreError> {
        let id = id.clone();
        self.call(move |conn| writer::effect(conn, &id))
    }

    /// Moves an outbox row. Allowed: `intended -> starting` (counts an attempt),
    /// `starting -> started` (with the process, if one was spawned: it is recorded on the row
    /// and the task's workspace is locked by this effect in the same transaction),
    /// `starting -> intended` (nothing ran: the attempt is uncounted), `started -> intended`
    /// (retry), and any pending state to `abandoned`. `done` is set only by `submit`.
    pub fn mark_effect(
        &self,
        id: &EffectId,
        to: OutboxState,
        process: Option<&ProcessRecord>,
    ) -> Result<(), StoreError> {
        let id = id.clone();
        let process = process.cloned();
        self.call(move |conn| writer::mark_effect(conn, &id, to, process.as_ref()))
    }

    /// Records the task's worktree path (first call wins; the path never changes).
    pub fn record_workspace(&self, task: &TaskId, path: &Path) -> Result<(), StoreError> {
        let task = task.clone();
        let path = path.to_string_lossy().into_owned();
        self.call(move |conn| writer::record_workspace(conn, &task, &path))
    }

    pub fn workspace(&self, task: &TaskId) -> Result<Option<WorkspaceRow>, StoreError> {
        let task = task.clone();
        self.call(move |conn| writer::workspace(conn, &task))
    }

    /// Clears the workspace's holder. The engine calls this only with a `TreeGone` in hand.
    pub fn release_workspace(&self, task: &TaskId) -> Result<(), StoreError> {
        let task = task.clone();
        self.call(move |conn| writer::release_workspace(conn, &task))
    }

    /// Recomputes the hash chain from the first event; returns the head sequence number
    /// (`Seq(0)` when the log is empty).
    pub fn verify_chain(&self) -> Result<Seq, ChainBroken> {
        match self.call(|conn| Ok(writer::verify_chain(conn))) {
            Ok(r) => r,
            Err(e) => Err(ChainBroken::Unreadable(e.to_string())),
        }
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The snake_case wire name of a unit enum value (`MediaType`, `Provenance`, `TaskState`).
macro_rules! wire_name {
    ($v:expr) => {
        match serde_json::to_value($v)? {
            serde_json::Value::String(s) => s,
            other => other.to_string(),
        }
    };
}
pub(crate) use wire_name;
