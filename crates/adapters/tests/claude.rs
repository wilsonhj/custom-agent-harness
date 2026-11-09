#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use harness_adapters::{AdapterPlan, AgentRunResult, AgentRunSpec, ParseState, adapter_for};
use harness_core::outcome::{FailureCode, RunOutcome};
use harness_core::types::{ModelFamily, Role, RuntimeKind};

fn spec(role: Role) -> AgentRunSpec {
    AgentRunSpec {
        role,
        prompt: "do the thing".into(),
        worktree: PathBuf::from("/wt"),
        config_dir: PathBuf::from("/cfg"),
        mcp_config: PathBuf::from("/cfg/mcp.json"),
        review_schema: Some(PathBuf::from("/cfg/schema.json")),
        max_turns: 7,
    }
}

fn plan(role: Role) -> AdapterPlan {
    adapter_for(RuntimeKind::Claude).plan(&spec(role)).unwrap()
}

fn fixture(name: &str) -> Vec<u8> {
    let p = format!("{}/fixtures/claude/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(p).unwrap()
}

fn run(name: &str, role: Role, exit: Option<i32>) -> AgentRunResult {
    let a = adapter_for(RuntimeKind::Claude);
    let p = plan(role);
    let mut st = ParseState::new(role, &p.argv);
    for line in fixture(name).split(|b| *b == b'\n') {
        a.parse_line(&mut st, line);
    }
    a.finish(st, exit)
}

fn has(argv: &[String], flag: &str) -> bool {
    argv.iter().any(|a| a == flag)
}

fn value_after<'a>(argv: &'a [String], flag: &str) -> &'a str {
    let i = argv.iter().position(|a| a == flag).unwrap();
    &argv[i + 1]
}

#[test]
fn claude_implementer_argv_names_shell_and_never_forbidden_flags() {
    let p = plan(Role::Implementer);
    assert_eq!(p.argv[0], "claude");
    assert_eq!(
        value_after(&p.argv, "--tools"),
        "Bash,Read,Edit,Write,Grep,Glob"
    );
    assert!(has(&p.argv, "-p"));
    assert_eq!(value_after(&p.argv, "--output-format"), "stream-json");
    assert!(has(&p.argv, "--verbose") && has(&p.argv, "--restricted"));
    assert_eq!(value_after(&p.argv, "--setting-sources"), "user");
    assert!(has(&p.argv, "--strict-mcp-config"));
    assert_eq!(value_after(&p.argv, "--mcp-config"), "/cfg/mcp.json");
    assert!(has(&p.argv, "--disable-slash-commands"));
    assert_eq!(value_after(&p.argv, "--max-turns"), "7");
    let settings: serde_json::Value =
        serde_json::from_str(value_after(&p.argv, "--settings")).unwrap();
    assert_eq!(settings["sandbox"]["enabled"], false);
    assert!(!has(&p.argv, "--json-schema"));
    assert_eq!(p.stdin, b"do the thing");
    assert!(p.env.contains(&("CLAUDE_CONFIG_DIR".into(), "/cfg".into())));
    for (k, v) in [
        ("CLAUDE_CODE_DISABLE_CLAUDE_MDS", "1"),
        ("DISABLE_AUTOUPDATER", "1"),
        ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1"),
    ] {
        assert!(p.env.contains(&(k.into(), v.into())), "{k}");
    }
    assert_eq!(p.egress_hosts, vec!["api.anthropic.com".to_string()]);
}

#[test]
fn claude_reviewer_has_no_shell_tool() {
    let p = plan(Role::Reviewer);
    assert_eq!(value_after(&p.argv, "--tools"), "Read,Grep,Glob");
    assert!(!value_after(&p.argv, "--tools").contains("Bash"));
    let schema: serde_json::Value =
        serde_json::from_str(value_after(&p.argv, "--json-schema")).unwrap();
    assert!(schema.is_object());
}

#[test]
fn claude_ok_run_succeeds_with_session_and_usage() {
    let r = run("ok.jsonl", Role::Implementer, Some(0));
    assert_eq!(r.outcome, RunOutcome::Succeeded);
    assert_eq!(r.final_text.as_deref(), Some("Done."));
    assert_eq!(
        r.session.as_deref(),
        Some("11111111-1111-4111-8111-111111111111")
    );
    let u = r.usage.unwrap();
    assert_eq!(
        (u.input_tokens, u.output_tokens, u.turns),
        (Some(120), Some(45), Some(3))
    );
}

#[test]
fn claude_exit_1_with_subtype_success_is_failed() {
    let r = run(
        "is_error_true_subtype_success.jsonl",
        Role::Implementer,
        Some(1),
    );
    assert_eq!(
        r.outcome,
        RunOutcome::Failed {
            code: FailureCode::ExitNonzero
        }
    );
    // is_error alone also fails the run, even with exit 0.
    let r = run(
        "is_error_true_subtype_success.jsonl",
        Role::Implementer,
        Some(0),
    );
    assert_eq!(
        r.outcome,
        RunOutcome::Failed {
            code: FailureCode::ExitNonzero
        }
    );
}

#[test]
fn claude_rate_limit_event_is_limit_exhausted() {
    let r = run("rate_limit.jsonl", Role::Implementer, Some(1));
    assert_eq!(r.outcome, RunOutcome::LimitExhausted { reset_at: None });
}

#[test]
fn malformed_stream_is_invalid_agent_output() {
    for f in ["truncated.jsonl", "invalid_utf8.jsonl"] {
        let r = run(f, Role::Implementer, Some(0));
        assert_eq!(
            r.outcome,
            RunOutcome::Failed {
                code: FailureCode::InvalidAgentOutput
            },
            "{f}"
        );
    }
}

#[test]
fn claude_reviewer_parses_structured_output() {
    let r = run("review_ok.jsonl", Role::Reviewer, Some(0));
    assert_eq!(r.outcome, RunOutcome::Succeeded);
    let review = r.review.unwrap();
    assert_eq!(review.findings.len(), 1);
    // The same transcript for an implementer carries no review requirement.
    let r = run("ok.jsonl", Role::Reviewer, Some(0));
    assert_eq!(
        r.outcome,
        RunOutcome::Failed {
            code: FailureCode::InvalidAgentOutput
        }
    );
}

#[test]
fn claude_sandbox_off_is_not_confirmed_without_an_init_field() {
    let r = run("ok.jsonl", Role::Implementer, Some(0));
    assert!(!r.own_sandbox_confirmed_off);
    let a = adapter_for(RuntimeKind::Claude);
    let mut st = ParseState::new(Role::Implementer, &plan(Role::Implementer).argv);
    a.parse_line(
        &mut st,
        br#"{"type":"system","subtype":"init","session_id":"s","sandbox":{"enabled":false}}"#,
    );
    a.parse_line(
        &mut st,
        br#"{"type":"result","is_error":false,"result":"x"}"#,
    );
    assert!(a.finish(st, Some(0)).own_sandbox_confirmed_off);
    let mut st = ParseState::new(Role::Implementer, &plan(Role::Implementer).argv);
    a.parse_line(
        &mut st,
        br#"{"type":"system","subtype":"init","session_id":"s","sandbox":{"enabled":true}}"#,
    );
    a.parse_line(
        &mut st,
        br#"{"type":"result","is_error":false,"result":"x"}"#,
    );
    assert!(!a.finish(st, Some(0)).own_sandbox_confirmed_off);
}

#[test]
fn version_mismatch_is_detected() {
    let a = adapter_for(RuntimeKind::Claude);
    assert_eq!(
        a.version_argv(),
        vec!["claude".to_string(), "--version".to_string()]
    );
    assert_eq!(a.check_version("2.1.318 (Claude Code)\n"), Ok(()));
    assert_eq!(
        a.check_version("2.1.319 (Claude Code)"),
        Err(FailureCode::VersionMismatch)
    );
    assert_eq!(
        a.check_version("garbage"),
        Err(FailureCode::VersionMismatch)
    );
}

#[test]
fn pipeline_families_differ() {
    let c = adapter_for(RuntimeKind::Claude).capabilities();
    let x = adapter_for(RuntimeKind::Codex).capabilities();
    assert_ne!(c.model_family, x.model_family);
    assert_eq!(c.model_family, ModelFamily::Anthropic);
}
