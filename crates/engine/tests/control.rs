//! Live control uses the existing engine writer, never a second Store or worker.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use harness_core::decide::Command;
use harness_core::ids::{IdempotencyKey, TaskId};
use harness_engine::{Engine, EngineError, cancel_task, status};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::UnixStream;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn status_snapshot_sequence_matches_tasks_during_concurrent_submissions() {
    use std::sync::{Arc, Barrier};
    let f = fixture();
    let engine = Arc::new(Engine::open(config(&f, "clean", "clean")).await.unwrap());
    let id = start_task(&engine, &f, &default_pipeline()).await;
    let template = engine.task(&id).unwrap().unwrap();
    let barrier = Arc::new(Barrier::new(2));
    std::thread::scope(|scope| {
        let writer = Arc::clone(&engine);
        let writer_barrier = Arc::clone(&barrier);
        scope.spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            writer_barrier.wait();
            for i in 0..100 {
                let task = TaskId::new(format!("snapshot-{i}"));
                let command = envelope(
                    &task,
                    human(),
                    Command::CreateTask {
                        repo: template.repo.clone(),
                        base_commit: template.base_commit.clone(),
                        packet: template.packet.clone(),
                        pipeline: template.pipeline.clone(),
                        key: IdempotencyKey::new(format!("snapshot-{i}")),
                    },
                );
                assert!(runtime.block_on(writer.submit(command)).unwrap().accepted);
            }
        });
        barrier.wait();
        for _ in 0..100 {
            let snapshot = engine.status_snapshot().unwrap();
            assert_eq!(
                snapshot.tasks.iter().map(|t| t.version).sum::<u64>(),
                snapshot.seq.0,
                "task projections and the event sequence must describe the same instant"
            );
        }
    });
}

#[tokio::test]
async fn cancellation_api_refuses_other_commands_before_writing() {
    let f = fixture();
    let cfg = config(&f, "clean", "clean");
    let policy = f.state.join("egress-policy.json");
    let policy_before = std::fs::read(&policy).unwrap();
    let command = envelope(
        &TaskId::new("missing"),
        human(),
        Command::Run {
            key: IdempotencyKey::new("no-run"),
        },
    );
    assert!(matches!(
        cancel_task(&cfg, command).await,
        Err(EngineError::Control(_))
    ));
    assert_eq!(std::fs::read(&policy).unwrap(), policy_before);
    let entries: Vec<_> = std::fs::read_dir(&f.state)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(entries, [std::ffi::OsString::from("egress-policy.json")]);
    for name in [
        "state.db",
        "engine.lock",
        "run-control",
        "run-control.pending",
    ] {
        assert!(
            std::fs::symlink_metadata(f.state.join(name)).is_err(),
            "a rejected non-cancel command must not create {name}"
        );
    }
}

#[tokio::test]
async fn stale_control_endpoint_is_removed_after_reacquiring_writer_lock() {
    let f = fixture();
    let cfg = config(&f, "clean", "clean");
    let engine = Engine::open(cfg.clone()).await.unwrap();
    let dir = std::fs::canonicalize("/tmp")
        .unwrap()
        .join(format!("eng-control-{}", harness_engine::new_id()));
    std::fs::create_dir(&dir).unwrap();
    let socket = dir.join("socket");
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    drop(listener);
    std::fs::write(f.state.join("run-control"), socket.to_str().unwrap()).unwrap();
    drop(engine); // Simulates the writer's lifetime ending with a stale endpoint.
    let snapshot = status(&cfg).await.unwrap();
    assert!(snapshot.tasks.is_empty());
    assert!(!f.state.join("run-control").exists());
    assert!(!dir.exists());
}

#[tokio::test]
async fn malformed_and_partial_clients_do_not_block_running_task_control() {
    use std::os::unix::fs::PermissionsExt as _;
    let f = fixture();
    let mut cfg = config(&f, "hang", "clean");
    // Longer than macOS's sockaddr_un limit; the private endpoint pointer must still work.
    cfg.state_dir = f
        .tmp
        .path()
        .join("long-state-path-".repeat(8))
        .join("state");
    authorize_fake_agents(&f.repo, &cfg.state_dir);
    let engine = Engine::open(cfg.clone()).await.unwrap();
    let id = start_task(&engine, &f, &default_pipeline()).await;
    let cancel = CancellationToken::new();
    let exercise = async {
        let endpoint = cfg.state_dir.join("run-control");
        while !endpoint.exists() {
            tokio::task::yield_now().await;
        }
        let socket = std::fs::read_to_string(&endpoint).unwrap();
        let private_dir = std::path::Path::new(&socket).parent().unwrap();
        assert_eq!(
            std::fs::metadata(private_dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&endpoint).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let mut bad = UnixStream::connect(&socket).await.unwrap();
        bad.write_all(br#"{"type":"approve"}"#).await.unwrap();
        bad.shutdown().await.unwrap();
        let mut response = Vec::new();
        bad.read_to_end(&mut response).await.unwrap();
        assert!(
            serde_json::from_slice::<serde_json::Value>(&response)
                .unwrap()
                .get("Error")
                .is_some()
        );
        let mut partial = UnixStream::connect(&socket).await.unwrap();
        partial.write_all(b"{").await.unwrap(); // Never half-close this client's request.
        let snapshot = status(&cfg).await.unwrap();
        assert_eq!(snapshot.tasks[0].id, id);
        drop(partial);
        let reply = cancel_task(
            &cfg,
            envelope(
                &id,
                human(),
                Command::Cancel {
                    key: IdempotencyKey::new("cancel-control"),
                },
            ),
        )
        .await
        .unwrap();
        assert!(reply.accepted);
        socket
    };
    let (result, socket) = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        tokio::join!(engine.run_with_control(&id, cancel.clone()), exercise)
    })
    .await
    .unwrap();
    assert_eq!(
        result.unwrap().state,
        harness_core::lifecycle::TaskState::Cancelled
    );
    assert!(!cfg.state_dir.join("run-control").exists());
    assert!(!std::path::Path::new(&socket).exists());
    drop(engine);
    assert!(status(&cfg).await.is_ok());
}
