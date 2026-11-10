//! Restart recovery: a child engine process is SIGKILLed at a named failpoint
//! (`ENG_FAILPOINT`, `failpoints` feature), then the engine is reopened and the restart table
//! of design §5.4 is checked.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::os::unix::process::ExitStatusExt as _;
use std::path::PathBuf;
use std::process::{Command as Process, Stdio};

use common::*;
use harness_core::decide::{EffectSpec, Event};
use harness_core::ids::{EffectId, TaskId};
use harness_core::lifecycle::TaskState;
use harness_core::stage::{HumanGate, StageRunState};
use harness_core::types::Role;
use harness_engine::{Engine, EngineConfig};
use harness_store::{OutboxRow, OutboxState, Store};
use tokio_util::sync::CancellationToken;

const CHILD_STATE: &str = "ENG_TEST_CHILD_STATE";

/// Not a test of its own: the body of the child engine process the tests below start. Without
/// the environment it does nothing.
#[tokio::test]
async fn engine_child() {
    let Ok(state) = std::env::var(CHILD_STATE) else {
        return;
    };
    let var = |k: &str| std::env::var(k).unwrap();
    let cfg = EngineConfig {
        state_dir: PathBuf::from(state),
        claude_bin: PathBuf::from(var("ENG_TEST_CHILD_CLAUDE")),
        codex_bin: PathBuf::from(var("ENG_TEST_CHILD_CODEX")),
        node_bin: None,
        codex_uses_node: false,
        shim_bin: shim(),
        extra_read_dirs: std::env::split_paths(&var("ENG_TEST_CHILD_EXTRA")).collect(),
    };
    let engine = Engine::open(cfg).await.unwrap();
    let task = TaskId::new(var("ENG_TEST_CHILD_TASK"));
    let _ = engine.run_until_idle(&task, CancellationToken::new()).await;
    // The failpoint should have killed this process before it gets here.
    std::process::exit(3);
}

/// Runs the task in a child engine that SIGKILLs itself at `point`.
fn crash_child(cfg: &EngineConfig, task: &TaskId, point: &str) {
    let exe = std::env::current_exe().unwrap();
    let status = Process::new(exe)
        .args([
            "engine_child",
            "--exact",
            "--nocapture",
            "--test-threads",
            "1",
        ])
        .env(CHILD_STATE, &cfg.state_dir)
        .env("ENG_TEST_CHILD_CLAUDE", &cfg.claude_bin)
        .env("ENG_TEST_CHILD_CODEX", &cfg.codex_bin)
        .env(
            "ENG_TEST_CHILD_EXTRA",
            std::env::join_paths(&cfg.extra_read_dirs).unwrap(),
        )
        .env("ENG_TEST_CHILD_TASK", task.as_str())
        .env("ENG_FAILPOINT", point)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert_eq!(status.signal(), Some(9), "child was not killed at {point}");
}

/// A created, approved and started task whose engine has been closed again.
async fn prepared(cfg: &EngineConfig, f: &Fixture, pipeline_toml: &str) -> TaskId {
    let engine = Engine::open(cfg.clone()).await.unwrap();
    start_task(&engine, f, pipeline_toml).await
}

fn with_store<T>(f: &Fixture, read: impl FnOnce(&Store) -> T) -> T {
    let store = Store::open(&f.state).unwrap();
    read(&store)
}

fn outbox(f: &Fixture, effect: &EffectId) -> OutboxRow {
    with_store(f, |s| s.effect(effect).unwrap().unwrap())
}

/// Ids of intended effects matching `pick`, in intent order.
fn intended(engine: &Engine, pick: impl Fn(&EffectSpec) -> bool) -> Vec<EffectId> {
    events(engine)
        .into_iter()
        .filter_map(|e| match e {
            Event::EffectIntended { spec } if pick(&spec) => Some(spec.effect().clone()),
            _ => None,
        })
        .collect()
}

fn is_implementer(s: &EffectSpec) -> bool {
    matches!(
        s,
        EffectSpec::AgentRun {
            role: Role::Implementer,
            ..
        }
    )
}

fn kill_pid(pid: i32) {
    let _ = Process::new("/bin/kill")
        .args(["-KILL", &pid.to_string()])
        .status();
}

#[tokio::test]
async fn crash_in_starting_reruns_effect_and_nothing_ran_twice() {
    let f = fixture();
    let cfg = config(&f, "clean", "clean");
    let id = prepared(&cfg, &f, &default_pipeline()).await;
    crash_child(&cfg, &id, "spawn:before_started");

    let pending = with_store(&f, |s| s.pending_effects().unwrap());
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].state, OutboxState::Starting);
    assert!(pending[0].process.is_none());
    // The shim saw end of file and ran nothing.
    assert!(!worktree(&f, &id).join("ran.txt").exists());

    let engine = Engine::open(cfg).await.unwrap();
    let t = engine
        .run_until_idle(&id, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(
        t.run,
        StageRunState::AwaitingHuman {
            gate: HumanGate::ReviewClean
        }
    );
    let implementer = intended(&engine, is_implementer);
    assert_eq!(
        implementer.len(),
        1,
        "the same effect was re-run, not re-intended"
    );
    drop(engine);
    assert_eq!(
        outbox(&f, &implementer[0]).attempt,
        1,
        "the crashed start is not counted"
    );
    let cp = &t.checkpoints[0].commit;
    assert_eq!(tgit(&f.repo, &["show", &format!("{cp}:ran.txt")]), "run");
}

#[tokio::test]
async fn crash_during_agent_run_gives_lost_on_restart_and_reset_worktree() {
    let f = fixture();
    let cfg = config(&f, "hang", "clean");
    let id = prepared(&cfg, &f, &default_pipeline()).await;
    crash_child(&cfg, &id, "agent:first_line");

    let wt = worktree(&f, &id);
    assert!(
        wt.join("feature.txt").exists(),
        "the agent had written before the crash"
    );
    let pending = with_store(&f, |s| s.pending_effects().unwrap());
    assert_eq!(pending[0].state, OutboxState::Started);
    let orphan = pending[0].process.clone().unwrap();
    assert!(pid_alive(orphan.pid), "the agent outlived its engine");

    let engine = Engine::open(cfg).await.unwrap();
    let t = task(&engine, &id);
    assert_eq!(t.state, TaskState::Implementing);
    assert_eq!(
        t.run,
        StageRunState::AwaitingHuman {
            gate: HumanGate::LostOnRestart
        }
    );
    assert_eq!(t.round, 0);
    assert!(!pid_alive(orphan.pid), "recovery killed the orphan");
    assert!(!wt.join("feature.txt").exists());
    assert!(!wt.join("ran.txt").exists());
    assert_eq!(tgit(&wt, &["status", "--porcelain"]), "");
    let ws = with_engine_store_closed(engine, &f, |s| s.workspace(&id).unwrap().unwrap());
    assert_eq!(ws.holder, None);
}

/// Drops the engine (releasing the state lock) and reads the store.
fn with_engine_store_closed<T>(engine: Engine, f: &Fixture, read: impl FnOnce(&Store) -> T) -> T {
    drop(engine);
    with_store(f, read)
}

#[tokio::test]
async fn crash_during_verify_retries_once() {
    let f = fixture();
    let cfg = config(&f, "clean", "clean");
    let verify = pipeline(&["sh", "-c", "echo checking; sleep 2; test -f feature.txt"]);
    let id = prepared(&cfg, &f, &verify).await;
    crash_child(&cfg, &id, "verify:first_line");

    let pending = with_store(&f, |s| s.pending_effects().unwrap());
    assert_eq!(pending.len(), 1);
    assert!(matches!(pending[0].spec, EffectSpec::Verify { .. }));
    assert_eq!(
        (pending[0].state, pending[0].attempt),
        (OutboxState::Started, 1)
    );
    let verify_effect = pending[0].effect.clone();
    // The crashed verification's scratch checkout is still there.
    let scratch = f
        .state
        .join("tasks")
        .join(id.as_str())
        .join("verify")
        .join(verify_effect.as_str());
    assert!(scratch.join("checkout").join("feature.txt").exists());

    let engine = Engine::open(cfg).await.unwrap();
    let t = engine
        .run_until_idle(&id, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(
        t.run,
        StageRunState::AwaitingHuman {
            gate: HumanGate::ReviewClean
        }
    );
    let passed: Vec<bool> = events(&engine)
        .into_iter()
        .filter_map(|e| match e {
            Event::VerificationFinished { passed, .. } => Some(passed),
            _ => None,
        })
        .collect();
    assert_eq!(passed, [true], "one verification result, from the retry");
    drop(engine);
    let row = outbox(&f, &verify_effect);
    assert_eq!((row.state, row.attempt), (OutboxState::Done, 2));
    // Recovery and the retry removed the scratch area and its worktree registration.
    assert!(!scratch.exists());
    let listed = tgit(&f.repo, &["worktree", "list", "--porcelain"]);
    assert_eq!(listed.matches("worktree ").count(), 2, "{listed}");
}

#[tokio::test]
async fn crash_during_checkpoint_yields_same_commit_sha() {
    let f = fixture();
    let cfg = config(&f, "clean", "clean");
    let id = prepared(&cfg, &f, &default_pipeline()).await;
    crash_child(&cfg, &id, "checkpoint:after_commit");

    let before = tgit(&f.repo, &["rev-parse", &cp_ref(&id, 1)]);
    let pending = with_store(&f, |s| s.pending_effects().unwrap());
    assert!(matches!(pending[0].spec, EffectSpec::Checkpoint { .. }));
    assert_eq!(pending[0].state, OutboxState::Started);

    let engine = Engine::open(cfg).await.unwrap();
    let t = engine
        .run_until_idle(&id, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(
        t.run,
        StageRunState::AwaitingHuman {
            gate: HumanGate::ReviewClean
        }
    );
    assert_eq!(t.checkpoints.len(), 1);
    assert_eq!(t.checkpoints[0].commit.as_str(), before);
    assert_eq!(tgit(&f.repo, &["rev-parse", &cp_ref(&id, 1)]), before);
}

#[tokio::test]
async fn lock_is_not_released_while_orphan_is_alive() {
    let f = fixture();
    let cfg = config(&f, "hang", "clean");
    let id = prepared(&cfg, &f, &default_pipeline()).await;
    crash_child(&cfg, &id, "agent:first_line");
    let (effect, orphan) = with_store(&f, |s| {
        let row = s.pending_effects().unwrap().remove(0);
        (row.effect, row.process.unwrap())
    });

    // Recovery itself is killed before it has killed the orphan.
    crash_child(&cfg, &id, "recovery:before_kill");
    let (ws, row) = with_store(&f, |s| {
        (
            s.workspace(&id).unwrap().unwrap(),
            s.effect(&effect).unwrap().unwrap(),
        )
    });
    assert!(pid_alive(orphan.pid));
    assert_eq!(
        ws.holder.as_ref(),
        Some(&effect),
        "lock held while the orphan lives"
    );
    assert_eq!(row.state, OutboxState::Started);

    let engine = Engine::open(cfg).await.unwrap();
    let alive_after = pid_alive(orphan.pid);
    if alive_after {
        kill_pid(orphan.pid);
    }
    assert!(!alive_after);
    let ws = with_engine_store_closed(engine, &f, |s| s.workspace(&id).unwrap().unwrap());
    assert_eq!(ws.holder, None, "released only once the tree is gone");
}
