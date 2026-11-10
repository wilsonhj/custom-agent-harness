//! End-to-end flows through the real sandbox, supervisor and host Git, with fake agents.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use harness_core::artifact::Provenance;
use harness_core::decide::{Command, EffectOutcome, EffectSpec, Event};
use harness_core::ids::{CommitSha, IdempotencyKey};
use harness_core::lifecycle::{DecisionKind, Subject, TaskState};
use harness_core::outcome::FailureCode;
use harness_core::stage::{HumanGate, StageRunState};
use harness_core::types::Role;
use harness_engine::{Engine, EngineConfig};
use tokio_util::sync::CancellationToken;

async fn run(engine: &Engine, task: &harness_core::ids::TaskId) -> harness_core::decide::Task {
    engine
        .run_until_idle(task, CancellationToken::new())
        .await
        .unwrap()
}

#[tokio::test]
async fn clean_flow_stops_at_awaiting_human_with_one_checkpoint() {
    let f = fixture();
    // Launch both fakes by their own paths: Claude as an executable script, Codex as
    // `node <entry>` (the real Codex launch shape).
    let cfg = EngineConfig {
        state_dir: f.state.clone(),
        claude_bin: fixtures().join("fake-claude.mjs"),
        codex_bin: fixtures().join("fake-codex.mjs"),
        node_bin: Some(node()),
        shim_bin: shim(),
        extra_read_dirs: vec![],
    };
    let engine = Engine::open(cfg).await.unwrap();
    let id = start_task(&engine, &f, &default_pipeline()).await;
    let t = run(&engine, &id).await;

    assert_eq!(t.state, TaskState::Reviewing);
    assert_eq!(
        t.run,
        StageRunState::AwaitingHuman {
            gate: HumanGate::ReviewClean
        }
    );
    assert_eq!(t.round, 0);
    assert_eq!(t.checkpoints.len(), 1);
    let cp = &t.checkpoints[0].commit;
    assert_eq!(
        try_tgit(&f.repo, &["rev-parse", &cp_ref(&id, 1)]).as_deref(),
        Some(cp.as_str())
    );
    // No branch moved; only the hidden ref was written.
    assert_eq!(tgit(&f.repo, &["rev-parse", "main"]), f.base.as_str());
    assert_eq!(
        tgit(
            &f.repo,
            &["for-each-ref", "--format=%(refname)", "refs/heads"]
        ),
        "refs/heads/main"
    );
    // The checkpoint holds what the agent wrote, and the worktree is exactly that commit.
    assert_eq!(
        tgit(&f.repo, &["show", &format!("{cp}:feature.txt")]),
        "implemented"
    );
    let wt = worktree(&f, &id);
    assert_eq!(tgit(&wt, &["rev-parse", "HEAD"]), cp.as_str());
    assert_eq!(tgit(&wt, &["status", "--porcelain"]), "");

    // Evidence attached to the stop is only harness evidence and review findings.
    let evs = events(&engine);
    let evidence = evs
        .iter()
        .find_map(|e| match e {
            Event::AwaitingHuman { evidence, .. } => Some(evidence.clone()),
            _ => None,
        })
        .unwrap();
    let labels: Vec<Provenance> = evidence
        .iter()
        .map(|a| engine.artifact(a).unwrap().unwrap().label)
        .collect();
    assert!(labels.contains(&Provenance::HarnessEvidence));
    assert!(labels.contains(&Provenance::ReviewFindings));
    assert!(labels.iter().all(|l| l.is_evidence()), "{labels:?}");
    // Transcripts are claims.
    for e in &evs {
        if let Event::EffectFinished {
            outcome: EffectOutcome::AgentRun { transcript, .. },
            ..
        } = e
        {
            assert_eq!(
                engine.artifact(transcript).unwrap().unwrap().label,
                Provenance::RuntimeClaim
            );
        }
    }
    assert!(engine.verify_chain().unwrap().0 as usize >= evs.len());
}

#[tokio::test]
async fn blocking_finding_runs_two_repair_rounds_then_stops() {
    let f = fixture();
    let engine = Engine::open(config(&f, "clean", "blocking")).await.unwrap();
    let id = start_task(&engine, &f, &default_pipeline()).await;
    let t = run(&engine, &id).await;

    assert_eq!(
        t.run,
        StageRunState::AwaitingHuman {
            gate: HumanGate::RepairLimitReached
        }
    );
    assert_eq!(t.state, TaskState::Reviewing);
    assert_eq!(t.round, 2);
    assert_eq!(t.checkpoints.len(), 3);
    let evs = events(&engine);
    let rounds: Vec<u8> = evs
        .iter()
        .filter_map(|e| match e {
            Event::RepairRoundStarted { round } => Some(*round),
            _ => None,
        })
        .collect();
    assert_eq!(rounds, [1, 2]);
    let blocking: Vec<u32> = evs
        .iter()
        .filter_map(|e| match e {
            Event::FindingsRecorded { blocking, .. } => Some(*blocking),
            _ => None,
        })
        .collect();
    assert_eq!(blocking, [1, 1, 1]);
    // Each repair run got the findings: the fake writes "repaired" when it sees them.
    let last = &t.checkpoints[2].commit;
    assert_eq!(
        tgit(&f.repo, &["show", &format!("{last}:feature.txt")]),
        "repaired"
    );
    assert_eq!(
        tgit(&f.repo, &["show", &format!("{last}:ran.txt")]),
        "run\nrun\nrun"
    );
}

#[tokio::test]
async fn failing_verify_command_triggers_repair_without_review() {
    let f = fixture();
    let engine = Engine::open(config(&f, "clean", "clean")).await.unwrap();
    let id = start_task(
        &engine,
        &f,
        &pipeline(&["sh", "-c", "echo not yet; exit 1"]),
    )
    .await;
    let t = run(&engine, &id).await;

    assert_eq!(
        t.run,
        StageRunState::AwaitingHuman {
            gate: HumanGate::RepairLimitReached
        }
    );
    assert_eq!(t.round, 2);
    assert_eq!(t.checkpoints.len(), 3);
    let evs = events(&engine);
    let reviewer_runs = evs
        .iter()
        .filter(|e| {
            matches!(
                e,
                Event::EffectIntended {
                    spec: EffectSpec::AgentRun {
                        role: Role::Reviewer,
                        ..
                    }
                }
            )
        })
        .count();
    assert_eq!(reviewer_runs, 0, "a failed verification skips review");
    let verdicts: Vec<bool> = evs
        .iter()
        .filter_map(|e| match e {
            Event::VerificationFinished { passed, .. } => Some(*passed),
            _ => None,
        })
        .collect();
    assert_eq!(verdicts, [false, false, false]);
    // The output reached the harness evidence.
    let evidence = evs
        .iter()
        .find_map(|e| match e {
            Event::VerificationFinished { evidence, .. } => Some(evidence.clone()),
            _ => None,
        })
        .unwrap();
    let text = String::from_utf8(engine.read_artifact(&evidence).unwrap()).unwrap();
    assert!(text.contains("not yet"), "{text}");
}

#[tokio::test]
async fn agent_that_writes_a_token_fails_with_secret_in_diff() {
    let f = fixture();
    let engine = Engine::open(config(&f, "write_token", "clean"))
        .await
        .unwrap();
    let id = start_task(&engine, &f, &default_pipeline()).await;
    let t = run(&engine, &id).await;

    assert_eq!(t.state, TaskState::Failed);
    assert_eq!(
        t.run,
        StageRunState::Failed {
            code: FailureCode::SecretInDiff
        }
    );
    assert!(t.checkpoints.is_empty());
    assert_eq!(
        try_tgit(&f.repo, &["rev-parse", "--verify", "-q", &cp_ref(&id, 1)]),
        None
    );
    // The worktree went back to the previous checkpoint (here: the base commit).
    let wt = worktree(&f, &id);
    assert!(!wt.join("config.txt").exists());
    assert!(!wt.join("feature.txt").exists());
    assert_eq!(tgit(&wt, &["rev-parse", "HEAD"]), f.base.as_str());
}

#[tokio::test]
async fn accept_moves_to_verified_and_creates_branch_harness_task_id() {
    let f = fixture();
    let engine = Engine::open(config(&f, "clean", "clean")).await.unwrap();
    let id = start_task(&engine, &f, &default_pipeline()).await;
    let t = run(&engine, &id).await;
    let cp = t.checkpoints[0].commit.clone();

    // A decision bound to another commit is refused.
    let other = CommitSha::parse(f.base.as_str()).unwrap();
    let wrong = engine
        .submit(envelope(
            &id,
            human(),
            Command::RecordHumanDecision {
                kind: DecisionKind::Accept,
                subject: Subject::Commit(other),
                key: IdempotencyKey::new("accept-wrong"),
            },
        ))
        .await
        .unwrap();
    assert!(!wrong.accepted);
    // The engine cannot accept on a human's behalf.
    let by_engine = engine
        .submit(envelope(
            &id,
            harness_core::types::Actor::Engine,
            Command::RecordHumanDecision {
                kind: DecisionKind::Accept,
                subject: Subject::Commit(cp.clone()),
                key: IdempotencyKey::new("accept-engine"),
            },
        ))
        .await
        .unwrap();
    assert!(!by_engine.accepted);
    assert_eq!(
        try_tgit(
            &f.repo,
            &[
                "rev-parse",
                "--verify",
                "-q",
                &format!("refs/heads/harness/{id}")
            ]
        ),
        None
    );

    submit_ok(
        &engine,
        envelope(
            &id,
            human(),
            Command::RecordHumanDecision {
                kind: DecisionKind::Accept,
                subject: Subject::Commit(cp.clone()),
                key: IdempotencyKey::new("accept"),
            },
        ),
    )
    .await;
    let t = task(&engine, &id);
    assert_eq!(t.state, TaskState::Verified);
    assert_eq!(t.run, StageRunState::Succeeded);
    assert_eq!(
        tgit(&f.repo, &["rev-parse", &format!("refs/heads/harness/{id}")]),
        cp.as_str()
    );
    assert_eq!(tgit(&f.repo, &["rev-parse", "main"]), f.base.as_str());
}

#[tokio::test]
async fn claude_run_without_confirmed_own_sandbox_off_is_policy_denied() {
    let f = fixture();
    let engine = Engine::open(config(&f, "sandbox_unconfirmed", "clean"))
        .await
        .unwrap();
    let id = start_task(&engine, &f, &default_pipeline()).await;
    let t = run(&engine, &id).await;
    assert_eq!(t.state, TaskState::Failed);
    assert_eq!(
        t.run,
        StageRunState::Failed {
            code: FailureCode::PolicyDenied
        }
    );
    assert!(t.checkpoints.is_empty());
}

#[tokio::test]
async fn second_engine_on_the_same_state_dir_refuses_to_open() {
    let f = fixture();
    let first = Engine::open(config(&f, "clean", "clean")).await.unwrap();
    let second = Engine::open(config(&f, "clean", "clean")).await;
    assert!(matches!(
        second,
        Err(harness_engine::EngineError::Store(
            harness_store::StoreError::Locked
        ))
    ));
    drop(first);
}

#[tokio::test]
async fn cancel_kills_the_running_agent_and_cancels_the_task() {
    let f = fixture();
    let engine = Engine::open(config(&f, "hang", "clean")).await.unwrap();
    let id = start_task(&engine, &f, &default_pipeline()).await;
    let cancel = CancellationToken::new();
    let trip = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        trip.cancel();
    });
    let t = engine.run_until_idle(&id, cancel).await.unwrap();
    assert_eq!(t.state, TaskState::Cancelled);
    assert_eq!(t.run, StageRunState::Cancelled);
    drop(engine);
    let store = harness_store::Store::open(&f.state).unwrap();
    assert!(store.pending_effects().unwrap().is_empty());
    assert_eq!(store.workspace(&id).unwrap().unwrap().holder, None);
}
