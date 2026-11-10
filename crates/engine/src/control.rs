//! Transient control of the CLI's live run, not a daemon (design §5.5).
//! Only its Engine owns a Store; clients can inspect status or submit a human cancellation.

use std::fs;
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

use harness_core::decide::{Command, Event, Task};
use harness_core::ids::{Seq, TaskId};
use harness_core::types::{Actor, Envelope};
use harness_store::StoreError;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{UnixListener, UnixStream};
use tokio_util::sync::CancellationToken;

use crate::{Engine, EngineConfig, EngineError, new_id};

const ENDPOINT: &str = "run-control";
const REQUEST_LIMIT: u64 = 64 * 1024;
const RESPONSE_LIMIT: u64 = 64 * 1024 * 1024;
const CLIENT_TIMEOUT: Duration = Duration::from_secs(5);
const SERVER_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, Serialize, Deserialize)]
pub struct StatusSnapshot {
    pub seq: Seq,
    pub tasks: Vec<Task>,
    pub events: Vec<Envelope<Event>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CancelReply {
    pub accepted: bool,
    pub rejection: Option<String>,
    pub task: Option<Task>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Status,
    Cancel { command: Box<Envelope<Command>> },
}

#[derive(Serialize, Deserialize)]
enum Reply {
    Status(StatusSnapshot),
    Cancel(Box<CancelReply>),
    Error(String),
}

fn error(e: impl std::fmt::Display) -> EngineError {
    EngineError::Control(e.to_string())
}

/// Status is read through the live owner when it holds the exclusive store lock.
pub async fn status(cfg: &EngineConfig) -> Result<StatusSnapshot, EngineError> {
    match Engine::open(cfg.clone()).await {
        Ok(engine) => engine.status_snapshot(),
        Err(EngineError::Store(StoreError::Locked)) => {
            match request(&cfg.state_dir, Request::Status).await {
                Ok(Reply::Status(snapshot)) => Ok(snapshot),
                Ok(Reply::Error(e)) => Err(error(e)),
                Ok(_) => Err(error("unexpected status response")),
                // The owner may have exited between the lock check and the connection.
                Err(e) => match Engine::open(cfg.clone()).await {
                    Ok(engine) => engine.status_snapshot(),
                    Err(EngineError::Store(StoreError::Locked)) => Err(e),
                    Err(e) => Err(e),
                },
            }
        }
        Err(e) => Err(e),
    }
}

fn is_cancel(command: &Envelope<Command>) -> bool {
    matches!(command.body, Command::Cancel { .. }) && matches!(command.actor, Actor::Human { .. })
}

/// Submits cancellation through the same writer as the run; no other command is accepted.
pub async fn cancel_task(
    cfg: &EngineConfig,
    command: Envelope<Command>,
) -> Result<CancelReply, EngineError> {
    if !is_cancel(&command) {
        return Err(error("only human cancellation is supported"));
    }
    match Engine::open(cfg.clone()).await {
        Ok(engine) => engine.cancel_reply(command).await,
        Err(EngineError::Store(StoreError::Locked)) => {
            match request(
                &cfg.state_dir,
                Request::Cancel {
                    command: Box::new(command.clone()),
                },
            )
            .await
            {
                Ok(Reply::Cancel(reply)) => Ok(*reply),
                Ok(Reply::Error(e)) => Err(error(e)),
                Ok(_) => Err(error("unexpected cancellation response")),
                Err(e) => match Engine::open(cfg.clone()).await {
                    Ok(engine) => engine.cancel_reply(command).await,
                    Err(EngineError::Store(StoreError::Locked)) => Err(e),
                    Err(e) => Err(e),
                },
            }
        }
        Err(e) => Err(e),
    }
}

impl Engine {
    pub fn status_snapshot(&self) -> Result<StatusSnapshot, EngineError> {
        let (tasks, events) = self.store.snapshot()?;
        let seq = events.last().map_or(Seq(0), |e| e.seq);
        Ok(StatusSnapshot {
            seq,
            tasks,
            events: events.into_iter().map(|e| e.event).collect(),
        })
    }

    async fn cancel_reply(&self, command: Envelope<Command>) -> Result<CancelReply, EngineError> {
        let task = command.task.clone();
        let receipt = self.submit(command).await?;
        Ok(CancelReply {
            accepted: receipt.accepted,
            rejection: receipt.rejection,
            task: self.task(&task)?,
        })
    }

    /// Serves only status/cancel while driving the normal effect loop. The same supervisor
    /// cancellation token and TreeGone proof still govern process cleanup and lock release.
    pub async fn run_with_control(
        &self,
        task: &TaskId,
        cancel: CancellationToken,
    ) -> Result<Task, EngineError> {
        let control = Control::bind(&self.cfg.state_dir)?;
        let run = self.run_until_idle(task, cancel.clone());
        tokio::pin!(run);
        tokio::select! {
            result = &mut run => result,
            result = control.serve(self, task, &cancel) => {
                // A transport failure must not drop a running worker future before cleanup.
                cancel.cancel();
                run.await?;
                Err(result.err().unwrap_or_else(|| error("control listener stopped")))
            }
        }
    }
}

struct Control {
    listener: UnixListener,
    state: PathBuf,
    dir: PathBuf,
}

impl Control {
    fn bind(state: &Path) -> Result<Self, EngineError> {
        // A short private directory avoids macOS's 104-byte Unix socket path limit even
        // when --state-dir is long. The endpoint pointer stays inside the denied state dir.
        let dir = fs::canonicalize("/tmp")?.join(format!("eng-control-{}", new_id()));
        fs::DirBuilder::new().mode(0o700).create(&dir)?;
        let listener = match UnixListener::bind(dir.join("socket")) {
            Ok(listener) => listener,
            Err(e) => {
                let _ = fs::remove_dir(&dir);
                return Err(e.into());
            }
        };
        let control = Self {
            listener,
            state: state.to_path_buf(),
            dir,
        };
        use std::io::Write as _;
        let pending = state.join("run-control.pending");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&pending)?;
        file.write_all(control.dir.join("socket").as_os_str().as_encoded_bytes())?;
        fs::rename(pending, state.join(ENDPOINT))?;
        Ok(control)
    }

    async fn serve(
        &self,
        engine: &Engine,
        active: &TaskId,
        cancel: &CancellationToken,
    ) -> Result<(), EngineError> {
        loop {
            let (mut stream, _) = self.listener.accept().await?;
            let _ = tokio::time::timeout(SERVER_TIMEOUT, async {
                let mut bytes = Vec::new();
                (&mut stream)
                    .take(REQUEST_LIMIT + 1)
                    .read_to_end(&mut bytes)
                    .await?;
                let request = if bytes.len() as u64 > REQUEST_LIMIT {
                    None
                } else {
                    serde_json::from_slice(&bytes).ok()
                };
                let mut cancel_active = false;
                let reply = match request {
                    Some(Request::Status) => engine.status_snapshot().map(Reply::Status),
                    Some(Request::Cancel { command }) if is_cancel(&command) => {
                        let same_task = command.task == *active;
                        engine.cancel_reply(*command).await.map(|reply| {
                            cancel_active = same_task && reply.accepted;
                            Reply::Cancel(Box::new(reply))
                        })
                    }
                    _ => Err(error("invalid control request")),
                }
                .unwrap_or_else(|e| Reply::Error(e.to_string()));
                // Wake the effect loop after attempting the response. A timeout or a
                // disconnected client must still cancel an accepted request.
                let _cancel_on_drop = cancel_active.then(|| cancel.clone().drop_guard());
                stream
                    .write_all(&serde_json::to_vec(&reply).map_err(error)?)
                    .await
                    .map_err(EngineError::from)
            })
            .await;
        }
    }
}

impl Drop for Control {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.state.join(ENDPOINT));
        let _ = fs::remove_file(self.state.join("run-control.pending"));
        let _ = fs::remove_file(self.dir.join("socket"));
        let _ = fs::remove_dir(&self.dir);
    }
}

pub(crate) fn clear_stale_endpoint(state: &Path) {
    // Called only after Store::open acquires the exclusive writer lock.
    if let Ok(socket) = fs::read_to_string(state.join(ENDPOINT)) {
        let socket = Path::new(&socket);
        if let Some(dir) = socket.parent()
            && dir.parent() == fs::canonicalize("/tmp").ok().as_deref()
            && dir
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.starts_with("eng-control-") && s.len() == 38)
            && fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir())
            && socket.file_name().is_some_and(|s| s == "socket")
        {
            let _ = fs::remove_file(socket);
            let _ = fs::remove_dir(dir);
        }
    }
    let _ = fs::remove_file(state.join(ENDPOINT));
    let _ = fs::remove_file(state.join("run-control.pending"));
}

async fn request(state: &Path, request: Request) -> Result<Reply, EngineError> {
    tokio::time::timeout(CLIENT_TIMEOUT, async {
        let path = fs::read_to_string(state.join(ENDPOINT)).map_err(error)?;
        let mut stream = UnixStream::connect(path).await.map_err(error)?;
        stream
            .write_all(&serde_json::to_vec(&request).map_err(error)?)
            .await
            .map_err(error)?;
        stream.shutdown().await.map_err(error)?;
        let mut bytes = Vec::new();
        stream
            .take(RESPONSE_LIMIT + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(error)?;
        if bytes.len() as u64 > RESPONSE_LIMIT {
            return Err(error("control response too large"));
        }
        serde_json::from_slice(&bytes).map_err(error)
    })
    .await
    .map_err(error)?
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;

    #[tokio::test]
    async fn private_server_handles_malformed_and_partial_clients_with_long_state_path() {
        let root = fs::canonicalize("/tmp")
            .unwrap()
            .join(format!("eng-control-test-{}", new_id()));
        let cfg = EngineConfig {
            state_dir: root.join("long-state-path-".repeat(8)).join("state"),
            claude_bin: "/nonexistent/claude".into(),
            codex_bin: "/nonexistent/codex".into(),
            node_bin: None,
            codex_uses_node: false,
            shim_bin: "/nonexistent/shim".into(),
            extra_read_dirs: vec![],
        };
        // Opening an empty store and serving control metadata launches no process and
        // needs no egress grant. The production containment gate remains untouched.
        let engine = Engine::open(cfg.clone()).await.unwrap();
        let control = Control::bind(&cfg.state_dir).unwrap();
        let socket = control.dir.join("socket");
        let private_dir = control.dir.clone();
        let endpoint = cfg.state_dir.join(ENDPOINT);
        assert_eq!(
            fs::metadata(&private_dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&endpoint).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let active = TaskId::new("metadata-only");
        let cancel = CancellationToken::new();
        let clients = async {
            let mut bad = UnixStream::connect(&socket).await.unwrap();
            bad.write_all(br#"{"type":"approve"}"#).await.unwrap();
            bad.shutdown().await.unwrap();
            let mut response = Vec::new();
            bad.read_to_end(&mut response).await.unwrap();
            assert!(matches!(
                serde_json::from_slice::<Reply>(&response).unwrap(),
                Reply::Error(_)
            ));
            let mut partial = UnixStream::connect(&socket).await.unwrap();
            partial.write_all(b"{").await.unwrap(); // The server must time out this client.
            let snapshot = status(&cfg).await.unwrap();
            assert_eq!(snapshot.seq, Seq(0));
            assert!(snapshot.tasks.is_empty());
            assert!(snapshot.events.is_empty());
            assert!(!cancel.is_cancelled());
            drop(partial);
        };
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::select! {
                result = control.serve(&engine, &active, &cancel) => panic!("server stopped: {result:?}"),
                () = clients => {}
            }
        }).await.unwrap();
        drop(control);
        assert!(!endpoint.exists());
        assert!(!private_dir.exists());
        drop(engine);
        fs::remove_dir_all(root).unwrap();
    }
}
