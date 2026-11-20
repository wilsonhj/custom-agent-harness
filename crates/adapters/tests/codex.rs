#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use harness_adapters::{AdapterPlan, AgentRunResult, AgentRunSpec, ParseState, adapter_for};
use harness_core::outcome::{FailureCode, RunOutcome};
use harness_core::types::{Role, RuntimeKind};

fn spec(role: Role, schema: Option<&str>) -> AgentRunSpec {
    AgentRunSpec {
        role,
        prompt: "review it".into(),
        worktree: PathBuf::from("/wt"),
        config_dir: PathBuf::from("/cfg"),
        mcp_config: PathBuf::from("/cfg/mcp.json"),
        review_schema: schema.map(PathBuf::from),
        max_turns: 7,
    }
}

fn plan(role: Role) -> AdapterPlan {
    adapter_for(RuntimeKind::Codex)
        .plan(&spec(role, Some("/cfg/schema.json")))
        .unwrap()
}

fn run(name: &str, role: Role, exit: Option<i32>) -> AgentRunResult {
    let a = adapter_for(RuntimeKind::Codex);
    let p = plan(role);
    let mut st = ParseState::new(role, &p.argv);
    let path = format!("{}/fixtures/codex/{name}", env!("CARGO_MANIFEST_DIR"));
    for line in std::fs::read(path).unwrap().split(|b| *b == b'\n') {
        a.parse_line(&mut st, line);
    }
    a.finish(st, exit)
}

/// Thread id used by every Codex fixture. Real ids are UUIDs (a live 0.169.0 run printed
/// `01a1ddd6-3014-70e2-af57-4ff00c73d791`); this one is synthetic in the same format.
const THREAD_ID: &str = "019a8f3c-5b2e-7d41-9c6a-2f0e8b4d1a77";

fn run_lines(role: Role, lines: &[&str], exit: Option<i32>) -> AgentRunResult {
    let a = adapter_for(RuntimeKind::Codex);
    let mut st = ParseState::new(role, &plan(role).argv);
    for line in lines {
        a.parse_line(&mut st, line.as_bytes());
    }
    a.finish(st, exit)
}

fn value_after<'a>(argv: &'a [String], flag: &str) -> &'a str {
    let i = argv.iter().position(|a| a == flag).unwrap();
    &argv[i + 1]
}

#[test]
fn codex_both_roles_disable_agents_md_discovery() {
    for role in [Role::Implementer, Role::Reviewer] {
        let p = plan(role);
        let n = p
            .argv
            .windows(2)
            .filter(|w| w[0] == "-c" && w[1] == "project_doc_max_bytes=0")
            .count();
        assert_eq!(n, 1, "{role:?}: {:?}", p.argv);
        let skills = p
            .argv
            .windows(2)
            .filter(|w| w[0] == "-c" && w[1] == "skills.include_instructions=false")
            .count();
        assert_eq!(skills, 1, "{role:?}: {:?}", p.argv);
        // Options, not part of the prompt: they precede the stdin marker.
        assert_eq!(p.argv.last().map(String::as_str), Some("-"));
    }
}

#[test]
fn codex_reviewer_passes_output_schema_and_disables_own_sandbox() {
    let p = plan(Role::Reviewer);
    assert_eq!(&p.argv[..3], ["codex", "exec", "--json"]);
    assert_eq!(value_after(&p.argv, "-s"), "danger-full-access");
    assert!(p.argv.iter().any(|a| a == "--skip-git-repo-check"));
    assert_eq!(value_after(&p.argv, "-C"), "/wt");
    assert_eq!(value_after(&p.argv, "--output-schema"), "/cfg/schema.json");
    assert!(p.env.contains(&("CODEX_HOME".into(), "/cfg".into())));
    assert_eq!(p.stdin, b"review it");
    assert_eq!(
        p.egress_hosts,
        vec!["chatgpt.com".to_string(), "api.openai.com".to_string()]
    );
    let imp = plan(Role::Implementer);
    assert!(!imp.argv.iter().any(|a| a == "--output-schema"));
    // Reviewer without a schema path is refused.
    let err = adapter_for(RuntimeKind::Codex)
        .plan(&spec(Role::Reviewer, None))
        .unwrap_err();
    assert_eq!(err, FailureCode::PolicyDenied);
}

#[test]
fn codex_reviewer_output_parses_into_raw_review() {
    let r = run("review_ok.jsonl", Role::Reviewer, Some(0));
    assert_eq!(r.outcome, RunOutcome::Succeeded);
    assert_eq!(r.review.unwrap().findings.len(), 1);
    assert_eq!(r.session.as_deref(), Some(THREAD_ID));
    let u = r.usage.unwrap();
    assert_eq!(
        (u.input_tokens, u.output_tokens, u.turns),
        (Some(200), Some(80), Some(1))
    );
    assert!(r.own_sandbox_confirmed_off);
}

#[test]
fn codex_non_schema_output_is_invalid_agent_output() {
    let r = run("review_not_schema.jsonl", Role::Reviewer, Some(0));
    assert_eq!(
        r.outcome,
        RunOutcome::Failed {
            code: FailureCode::InvalidAgentOutput
        }
    );
    assert!(r.review.is_none());
}

#[test]
fn codex_limit_message_is_limit_exhausted() {
    let r = run("limit_message.jsonl", Role::Reviewer, Some(1));
    assert_eq!(r.outcome, RunOutcome::LimitExhausted { reset_at: None });
}

#[test]
fn codex_nonzero_exit_fails_even_with_completed_turn() {
    let r = run("review_ok.jsonl", Role::Reviewer, Some(2));
    assert_eq!(
        r.outcome,
        RunOutcome::Failed {
            code: FailureCode::ExitNonzero
        }
    );
}

#[test]
fn codex_garbage_line_is_invalid_agent_output() {
    let a = adapter_for(RuntimeKind::Codex);
    let mut st = ParseState::new(Role::Implementer, &plan(Role::Implementer).argv);
    a.parse_line(&mut st, b"{\"type\":\"turn.started\"}");
    a.parse_line(&mut st, b"\xff\xfe");
    assert_eq!(
        a.finish(st, Some(0)).outcome,
        RunOutcome::Failed {
            code: FailureCode::InvalidAgentOutput
        }
    );
}

#[test]
fn version_mismatch_is_detected() {
    let a = adapter_for(RuntimeKind::Codex);
    assert_eq!(
        a.version_argv(),
        vec!["codex".to_string(), "--version".to_string()]
    );
    assert_eq!(a.check_version("codex-cli 0.166.0\n"), Ok(()));
    assert_eq!(
        a.check_version("codex-cli 0.167.0"),
        Err(FailureCode::VersionMismatch)
    );
    assert_eq!(
        a.check_version("2.1.318 (Claude Code)"),
        Err(FailureCode::VersionMismatch)
    );
}

// --- Issue #21: transient (retried) `error` lines are not failures ---

#[test]
fn codex_retry_errors_then_completed_turn_succeeds() {
    // Codex prints `{"type":"error"}` for errors it is about to retry ("Reconnecting... n/5"),
    // including a 429 that mentions a rate limit, and the turn still completes with exit 0.
    let r = run("retry_then_ok.jsonl", Role::Reviewer, Some(0));
    assert_eq!(r.outcome, RunOutcome::Succeeded);
    assert_eq!(r.review.unwrap().findings.len(), 1);
    assert_eq!(r.session.as_deref(), Some(THREAD_ID));
    let u = r.usage.unwrap();
    assert_eq!(
        (u.input_tokens, u.output_tokens, u.turns),
        (Some(4100), Some(310), Some(1))
    );
}

#[test]
fn codex_limit_retry_then_unrelated_failure_is_not_limit_exhausted() {
    let r = run(
        "limit_retry_then_unrelated_failure.jsonl",
        Role::Implementer,
        Some(1),
    );
    assert_eq!(
        r.outcome,
        RunOutcome::Failed {
            code: FailureCode::ExitNonzero
        }
    );
}

#[test]
fn codex_last_error_classifies_a_failed_run_without_turn_failed() {
    // Nonzero exit and no `turn.failed`: the last buffered `error` decides the class.
    let r = run_lines(
        Role::Implementer,
        &[
            r#"{"type":"turn.started"}"#,
            r#"{"type":"error","message":"Reconnecting... 2/5 (stream disconnected before completion)"}"#,
            r#"{"type":"error","message":"You’ve hit your usage limit. Try again later."}"#,
        ],
        Some(1),
    );
    assert_eq!(r.outcome, RunOutcome::LimitExhausted { reset_at: None });

    // ...and only the last one: an earlier limit-mentioning retry does not count.
    let r = run_lines(
        Role::Implementer,
        &[
            r#"{"type":"turn.started"}"#,
            r#"{"type":"error","message":"Reconnecting... 2/5 (Rate limit reached for requests)"}"#,
            r#"{"type":"error","message":"stream disconnected before completion"}"#,
        ],
        Some(1),
    );
    assert_eq!(
        r.outcome,
        RunOutcome::Failed {
            code: FailureCode::ExitNonzero
        }
    );
}

#[test]
fn codex_turn_failed_message_wins_over_buffered_errors() {
    let r = run_lines(
        Role::Implementer,
        &[
            r#"{"type":"turn.started"}"#,
            r#"{"type":"error","message":"stream disconnected before completion"}"#,
            r#"{"type":"turn.failed","error":{"message":"You’ve hit your usage limit. Try again later."}}"#,
        ],
        Some(1),
    );
    assert_eq!(r.outcome, RunOutcome::LimitExhausted { reset_at: None });
}

#[test]
fn codex_errors_without_completed_turn_and_exit_zero_are_not_success() {
    let r = run_lines(
        Role::Implementer,
        &[
            r#"{"type":"turn.started"}"#,
            r#"{"type":"error","message":"Reconnecting... 2/5 (stream disconnected before completion)"}"#,
        ],
        Some(0),
    );
    assert_eq!(
        r.outcome,
        RunOutcome::Failed {
            code: FailureCode::InvalidAgentOutput
        }
    );
}

#[test]
fn codex_turn_failed_with_exit_zero_still_fails() {
    let r = run_lines(
        Role::Implementer,
        &[
            r#"{"type":"turn.started"}"#,
            r#"{"type":"turn.failed","error":{"message":"turn failed"}}"#,
        ],
        Some(0),
    );
    assert_eq!(
        r.outcome,
        RunOutcome::Failed {
            code: FailureCode::ExitNonzero
        }
    );
}

// --- Issue #20: tested-version list and update-check key ---

#[test]
fn codex_accepts_every_tested_version_and_names_them() {
    let a = adapter_for(RuntimeKind::Codex);
    for v in ["0.166.0", "0.169.0"] {
        assert_eq!(a.check_version(&format!("codex-cli {v}\n")), Ok(()), "{v}");
        // The doctor's mismatch message prints this field next to the installed version.
        assert!(
            a.capabilities().pinned_version.contains(v),
            "{v} missing from {:?}",
            a.capabilities().pinned_version
        );
    }
    for bad in [
        "codex-cli 0.170.0",
        "codex-cli 0.169",
        "codex 0.169.0",
        "codex-cli 0.169.0 x",
    ] {
        assert_eq!(
            a.check_version(bad),
            Err(FailureCode::VersionMismatch),
            "{bad}"
        );
    }
}

#[test]
fn codex_argv_disables_update_check() {
    for role in [Role::Implementer, Role::Reviewer] {
        let p = plan(role);
        let n = p
            .argv
            .windows(2)
            .filter(|w| w[0] == "-c" && w[1] == "check_for_update_on_startup=false")
            .count();
        assert_eq!(n, 1, "{role:?}: {:?}", p.argv);
        assert_eq!(p.argv.last().map(String::as_str), Some("-"));
    }
}
