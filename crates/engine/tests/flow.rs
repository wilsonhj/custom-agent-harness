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
        .flat_map(|a| engine.artifact_refs(a).unwrap())
        .map(|a| a.label)
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
            let labels: Vec<Provenance> = engine
                .artifact_refs(transcript)
                .unwrap()
                .into_iter()
                .map(|a| a.label)
                .collect();
            assert_eq!(labels, [Provenance::RuntimeClaim]);
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
    let store = reopen_store(&f.state);
    assert!(store.pending_effects().unwrap().is_empty());
    assert_eq!(store.workspace(&id).unwrap().unwrap().holder, None);
}

fn verification_outputs(engine: &Engine) -> Vec<String> {
    events(engine)
        .iter()
        .filter_map(|e| match e {
            Event::VerificationFinished { evidence, .. } => {
                Some(String::from_utf8(engine.read_artifact(evidence).unwrap()).unwrap())
            }
            _ => None,
        })
        .collect()
}

/// Item 2: verification checks out the candidate into its own writable scratch directory; the
/// task worktree is neither written nor readable, and nothing outside the scratch is writable.
#[tokio::test]
async fn verification_runs_in_a_writable_scratch_checkout() {
    let f = fixture();
    let outside = f.tmp.path().join("outside.txt");
    let script = format!(
        r#"set -e
test -f feature.txt
mkdir -p target && echo built > target/x && test -f target/x
t=${{PWD%/verify/*}}; id=${{t##*/}}; s=${{t%/tasks/*}}; wt=$s/worktrees/$id
echo "wt=$wt"
if (echo z > "$wt/escape.txt") 2>/dev/null; then echo wrote-worktree; exit 1; fi
if (echo y > '{}') 2>/dev/null; then echo wrote-outside; exit 1; fi
echo verified"#,
        outside.display()
    );
    let engine = Engine::open(config(&f, "clean", "clean")).await.unwrap();
    let id = start_task(&engine, &f, &pipeline(&["sh", "-c", &script])).await;
    let t = run(&engine, &id).await;
    let outputs = verification_outputs(&engine);
    assert_eq!(
        t.run,
        StageRunState::AwaitingHuman {
            gate: HumanGate::ReviewClean
        },
        "{outputs:?}"
    );
    let wt = worktree(&f, &id);
    assert!(
        outputs[0].contains(&format!("wt={}\n", wt.display())),
        "{outputs:?}"
    );
    assert!(outputs[0].contains("verified\n"), "{outputs:?}");
    assert!(!wt.join("target").exists(), "the task worktree got target/");
    assert!(!wt.join("escape.txt").exists());
    assert!(!outside.exists());
    // The scratch checkout is gone after the run.
    let scratch = f.state.join("tasks").join(id.as_str()).join("verify");
    let left: Vec<_> = std::fs::read_dir(&scratch)
        .map(|d| d.map(|e| e.unwrap().path()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "{left:?}");
    // No worktree registration is left behind in the repository either.
    let listed = tgit(&f.repo, &["worktree", "list", "--porcelain"]);
    assert_eq!(listed.matches("worktree ").count(), 2, "{listed}");
}

/// Item 5: verification gets a fresh, empty, private HOME that no agent run could write, and
/// the reviewer cannot read the implementer's home.
#[tokio::test]
async fn verification_home_is_fresh_and_reviewer_cannot_read_implementer_home() {
    let f = fixture();
    let script = r#"set -e
test -f feature.txt
echo "home=$HOME"
echo "tmp=$TMPDIR"
echo "mode=$(stat -f %Lp "$HOME")"
echo "entries=$(ls -A "$HOME" | wc -l | tr -d ' ')"
test ! -e "$HOME/.cargo/config.toml"
echo verified"#;
    let engine = Engine::open(config(&f, "plant_home", "probe_home"))
        .await
        .unwrap();
    let id = start_task(&engine, &f, &pipeline(&["sh", "-c", script])).await;
    let t = run(&engine, &id).await;
    let outputs = verification_outputs(&engine);
    assert_eq!(
        t.run,
        StageRunState::AwaitingHuman {
            gate: HumanGate::ReviewClean
        },
        "{outputs:?}"
    );
    let out = &outputs[0];
    let task_dir = f.state.join("tasks").join(id.as_str());
    let home = out
        .lines()
        .find_map(|l| l.strip_prefix("home="))
        .unwrap()
        .to_owned();
    assert!(
        home.starts_with(&format!("{}/verify/", task_dir.display())),
        "{home}"
    );
    assert!(out.contains(&format!("tmp={home}\n")), "{out}");
    assert!(out.contains("mode=700\n"), "{out}");
    assert!(out.contains("entries=0\n"), "{out}");
    assert!(out.contains("verified\n"), "{out}");
    // The implementer really planted the file in its own home.
    let planted = task_dir.join("homes/implementer/.cargo/config.toml");
    assert!(planted.exists(), "{}", planted.display());
    assert!(!std::path::Path::new(&home).exists(), "verify home deleted");
}

/// Item 3: an agent may print exactly the bytes of a later evidence artifact (here the
/// checkpoint diff, which it can predict). That must not block the task.
#[tokio::test]
async fn agent_output_equal_to_a_later_evidence_artifact_does_not_block_the_task() {
    // First, learn the checkpoint diff the clean scenario produces.
    let first = fixture();
    let diff = {
        let engine = Engine::open(config(&first, "clean", "clean"))
            .await
            .unwrap();
        let id = start_task(&engine, &first, &default_pipeline()).await;
        run(&engine, &id).await;
        let diff = events(&engine)
            .iter()
            .find_map(|e| match e {
                Event::EffectFinished {
                    outcome: EffectOutcome::Checkpoint { diff: Some(d), .. },
                    ..
                } => Some(d.clone()),
                _ => None,
            })
            .unwrap();
        engine.read_artifact(&diff).unwrap()
    };
    let f = fixture();
    let echo = f.tmp.path().join("echo.patch");
    std::fs::write(&echo, &diff).unwrap();
    let mut cfg = config(&f, "clean", "clean");
    cfg.claude_bin = wrapper_env(
        &f,
        "fake-claude",
        "echo_file",
        &[("FAKE_ECHO_FILE", echo.to_str().unwrap())],
    );
    let engine = Engine::open(cfg).await.unwrap();
    let id = start_task(&engine, &f, &default_pipeline()).await;
    let t = engine
        .run_until_idle(&id, CancellationToken::new())
        .await
        .expect("an agent's output must not block the task");
    assert_eq!(
        t.run,
        StageRunState::AwaitingHuman {
            gate: HumanGate::ReviewClean
        }
    );
    let evs = events(&engine);
    let cp_diff = evs
        .iter()
        .find_map(|e| match e {
            Event::EffectFinished {
                outcome: EffectOutcome::Checkpoint { diff: Some(d), .. },
                ..
            } => Some(d.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(engine.read_artifact(&cp_diff).unwrap(), diff);
    // The agent's copy is a claim of the implementer run; the evidence is the checkpoint's.
    let implementer = evs
        .iter()
        .find_map(|e| match e {
            Event::EffectIntended {
                spec:
                    EffectSpec::AgentRun {
                        effect,
                        role: Role::Implementer,
                        ..
                    },
            } => Some(effect.clone()),
            _ => None,
        })
        .unwrap();
    let checkpoint = evs
        .iter()
        .find_map(|e| match e {
            Event::EffectIntended {
                spec: EffectSpec::Checkpoint { effect, .. },
            } => Some(effect.clone()),
            _ => None,
        })
        .unwrap();
    drop(engine);
    let store = reopen_store(&f.state);
    assert!(
        store
            .has_label(&cp_diff, Provenance::RuntimeClaim, Some(&implementer))
            .unwrap()
    );
    assert!(
        !store
            .has_label(&cp_diff, Provenance::HarnessEvidence, Some(&implementer))
            .unwrap(),
        "the agent's output was accepted as evidence"
    );
    assert!(
        store
            .has_label(&cp_diff, Provenance::HarnessEvidence, Some(&checkpoint))
            .unwrap()
    );
}

/// Item 4: a task id becomes a path component; unsafe ids never reach the store.
#[tokio::test]
async fn create_task_with_an_unsafe_id_is_rejected_and_stores_nothing() {
    let f = fixture();
    let engine = Engine::open(config(&f, "clean", "clean")).await.unwrap();
    let packet = engine
        .put_input(b"eng-packet-v1\n", harness_core::artifact::MediaType::Text)
        .unwrap();
    let pipeline_id = engine
        .put_input(
            default_pipeline().as_bytes(),
            harness_core::artifact::MediaType::Text,
        )
        .unwrap();
    for bad in ["../x", "a/b", "/abs", ".", ""] {
        let task = harness_core::ids::TaskId::new(bad);
        let r = engine
            .submit(envelope(
                &task,
                human(),
                Command::CreateTask {
                    repo: harness_core::types::RepoRef {
                        path: f.repo.to_string_lossy().into_owned(),
                        git_common_dir: f.repo.join(".git").to_string_lossy().into_owned(),
                    },
                    base_commit: f.base.clone(),
                    packet: packet.clone(),
                    pipeline: pipeline_id.clone(),
                    key: IdempotencyKey::new(format!("create-{bad}")),
                },
            ))
            .await
            .unwrap();
        assert!(!r.accepted, "{bad:?} was accepted");
        assert!(engine.task(&task).unwrap().is_none(), "{bad:?}");
    }
    assert!(events(&engine).is_empty());
}

/// Item 4: a symlink planted at the task's worktree path is never used as the worktree.
#[tokio::test]
async fn a_symlink_planted_at_the_worktree_path_is_refused() {
    let f = fixture();
    let engine = Engine::open(config(&f, "clean", "clean")).await.unwrap();
    let id = start_task(&engine, &f, &default_pipeline()).await;
    let root = f.state.join("worktrees");
    std::fs::create_dir_all(&root).unwrap();
    std::os::unix::fs::symlink(&f.repo, root.join(id.as_str())).unwrap();
    let r = engine.run_until_idle(&id, CancellationToken::new()).await;
    assert!(r.is_err(), "{r:?}");
    assert_eq!(task(&engine, &id).state, TaskState::Failed);
    assert!(
        !f.repo.join("ran.txt").exists(),
        "the agent ran in the repository"
    );
    assert_eq!(tgit(&f.repo, &["status", "--porcelain"]), "");
}
