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
    assert_eq!(r.session.as_deref(), Some("thr_abc"));
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
