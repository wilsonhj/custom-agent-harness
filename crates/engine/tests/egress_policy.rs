//! Hosted task execution requires an operator grant, independent of repository content.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use harness_core::decide::{EffectOutcome, Event};
use harness_core::outcome::{FailureCode, RunOutcome};
use harness_engine::Engine;
use std::os::unix::fs::PermissionsExt as _;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn missing_operator_policy_denies_before_workspace_or_runtime_preparation() {
    let f = fixture();
    std::fs::remove_file(f.state.join("egress-policy.json")).unwrap();
    let mut cfg = config(&f, "clean", "clean");
    // A missing shim would produce SpawnError if the version launch were reached.
    cfg.shim_bin = f.tmp.path().join("no-shim");
    let engine = Engine::open(cfg).await.unwrap();
    let id = start_task(&engine, &f, &default_pipeline()).await;
    engine
        .run_until_idle(&id, CancellationToken::new())
        .await
        .unwrap();

    let outcome = events(&engine).into_iter().find_map(|event| match event {
        Event::EffectFinished {
            outcome: EffectOutcome::AgentRun { outcome, .. },
            ..
        } => Some(outcome),
        _ => None,
    });
    assert_eq!(
        outcome,
        Some(RunOutcome::Failed {
            code: FailureCode::PolicyDenied
        })
    );
    assert!(!f.state.join("worktrees").join(id.as_str()).exists());
    assert!(
        !f.state
            .join("tasks")
            .join(id.as_str())
            .join("homes")
            .exists()
    );
}

#[tokio::test]
async fn invalid_or_unapproved_policy_denies_before_runtime_preparation() {
    for change in [
        serde_json::json!({"egress_class": "local-only"}),
        serde_json::json!({"egress_class": "baa-covered-providers"}),
        serde_json::json!({"egress_class": "unknown"}),
        serde_json::json!({"allowed_runtimes": ["codex"]}),
        serde_json::json!({"project_root": "/another/project"}),
        serde_json::json!({"reviewed_by": ""}),
        serde_json::json!({"self_approved": true}),
    ] {
        let f = fixture();
        let path = f.state.join("egress-policy.json");
        let mut policy: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        for (key, value) in change.as_object().unwrap() {
            policy[key] = value.clone();
        }
        std::fs::write(&path, serde_json::to_vec(&policy).unwrap()).unwrap();
        let mut cfg = config(&f, "clean", "clean");
        cfg.shim_bin = f.tmp.path().join("no-shim");
        let engine = Engine::open(cfg).await.unwrap();
        let id = start_task(&engine, &f, &default_pipeline()).await;
        engine
            .run_until_idle(&id, CancellationToken::new())
            .await
            .unwrap();
        assert!(
            events(&engine).into_iter().any(|event| matches!(
                event,
                Event::EffectFinished {
                    outcome: EffectOutcome::AgentRun {
                        outcome: RunOutcome::Failed {
                            code: FailureCode::PolicyDenied
                        },
                        ..
                    },
                    ..
                }
            )),
            "policy {change}"
        );
        assert!(!f.state.join("worktrees").join(id.as_str()).exists());
    }
}

#[tokio::test]
async fn approved_policy_snapshot_is_bound_to_the_effect_before_preparation() {
    let f = fixture();
    let expected = std::fs::read(f.state.join("egress-policy.json")).unwrap();
    let mut cfg = config(&f, "clean", "clean");
    cfg.shim_bin = f.tmp.path().join("no-shim");
    let engine = Engine::open(cfg).await.unwrap();
    let id = start_task(&engine, &f, &default_pipeline()).await;
    engine
        .run_until_idle(&id, CancellationToken::new())
        .await
        .unwrap();
    let effect = events(&engine)
        .into_iter()
        .find_map(|event| match event {
            Event::EffectIntended { spec } => Some(spec.effect().clone()),
            _ => None,
        })
        .unwrap();
    let artifact = engine.egress_authorization(&effect).unwrap().unwrap();
    let record: serde_json::Value =
        serde_json::from_slice(&engine.read_artifact(&artifact).unwrap()).unwrap();
    assert_eq!(record["effect"], effect.as_str());
    assert_eq!(record["runtime"], "claude");
    assert_eq!(
        record["policy_snapshot"],
        String::from_utf8(expected.clone()).unwrap()
    );
    assert_eq!(
        record["policy_digest"],
        harness_core::ids::ArtifactId::of(&expected).as_str()
    );
    assert!(
        engine
            .artifact_refs(&artifact)
            .unwrap()
            .iter()
            .any(|reference| {
                reference.label == harness_core::artifact::Provenance::HarnessEvidence
                    && reference.produced_by.as_ref() == Some(&effect)
            })
    );
}

#[tokio::test]
#[ignore = "R-11: strong descendant containment backend unavailable; production execution is disabled"]
async fn revoking_the_grant_during_version_preparation_prevents_the_agent_go_byte() {
    let f = fixture();
    let mut cfg = config(&f, "clean", "clean");
    let fake = f.tmp.path().join("bin/revocation-version");
    std::fs::write(
        &fake,
        "#!/bin/sh\nif [ \"$1\" = --version ]; then\n  : > \"$HOME/version-ready\"\n  while [ ! -f \"$HOME/version-release\" ]; do /bin/sleep 0.01; done\n  echo '2.1.318 (Claude Code)'\n  exit 0\nfi\n: > \"$HOME/agent-started\"\nexit 1\n",
    ).unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    cfg.claude_bin = fake;
    let engine = Engine::open(cfg).await.unwrap();
    let id = start_task(&engine, &f, &default_pipeline()).await;
    let home = f
        .state
        .join("tasks")
        .join(id.as_str())
        .join("homes/implementer");
    let revoke_home = home.clone();
    let policy = f.state.join("egress-policy.json");
    let revoke = tokio::spawn(async move {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while !revoke_home.join("version-ready").exists() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        std::fs::remove_file(policy).unwrap();
        std::fs::write(revoke_home.join("version-release"), b"").unwrap();
    });
    engine
        .run_until_idle(&id, CancellationToken::new())
        .await
        .unwrap();
    revoke.await.unwrap();
    assert!(!home.join("agent-started").exists());
    assert!(events(&engine).into_iter().any(|event| matches!(
        event,
        Event::EffectFinished {
            outcome: EffectOutcome::AgentRun {
                outcome: RunOutcome::Failed {
                    code: FailureCode::PolicyDenied
                },
                ..
            },
            ..
        }
    )));
}

#[tokio::test]
async fn doctor_reports_containment_unavailable_without_executing_runtimes() {
    let f = fixture();
    let mut cfg = config(&f, "clean", "clean");
    let marker = f.tmp.path().join("doctor-runtime-ran");
    let fake = f.tmp.path().join("bin/doctor-marker");
    std::fs::write(
        &fake,
        format!(
            "#!/bin/sh\ntouch '{}'\necho '2.1.318 (Claude Code)'\n",
            marker.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    cfg.claude_bin = fake.clone();
    cfg.codex_bin = fake;
    let checks = harness_engine::doctor(&cfg).await;
    let containment = checks
        .iter()
        .find(|c| c.name == "containment")
        .expect("containment check");
    assert!(!containment.ok);
    assert!(containment.detail.starts_with("containment_unavailable: "));
    for name in ["claude_version", "codex_version"] {
        let check = checks.iter().find(|c| c.name == name).unwrap();
        assert!(!check.ok);
        assert!(check.detail.starts_with("containment_unavailable: skipped"));
    }
    assert!(!marker.exists());
}
