//! Claude Code adapter (`claude -p --output-format stream-json --verbose`).
//! Event shapes are PROVISIONAL; see `fixtures/README.md`. Unknown fields are ignored.

use harness_core::capabilities::{Capabilities, OwnSandbox, StructuredOutput};
use harness_core::finding::{RawReview, review_json_schema};
use harness_core::ids::Timestamp;
use harness_core::outcome::{FailureCode, RunOutcome};
use harness_core::types::{ModelFamily, Role, RuntimeKind};
use serde_json::Value;

use crate::{
    AdapterPlan, AgentRunResult, AgentRunSpec, ParseState, RuntimeAdapter, Usage, failed,
    parse_object, parse_review_text, path_str, secs_to_timestamp,
};

pub(crate) struct ClaudeAdapter;

const PINNED: &str = "2.1.318";

static CAPS: Capabilities = Capabilities {
    runtime: RuntimeKind::Claude,
    model_family: ModelFamily::Anthropic,
    pinned_version: PINNED,
    roles: &[Role::Implementer, Role::Reviewer],
    structured_output: StructuredOutput::JsonSchemaInline,
    session_resume: true,
    reports_usage: true,
    own_sandbox: OwnSandbox::MustDisable,
};

#[derive(Default)]
pub(crate) struct State {
    init_seen: bool,
    /// `init.sandbox` was present and explicitly off.
    init_sandbox_off: bool,
    session: Option<String>,
    limit: bool,
    limit_reset: Option<Timestamp>,
    result: Option<ResultEvent>,
}

struct ResultEvent {
    is_error: Option<bool>,
    text: Option<String>,
    structured: Option<Value>,
    usage: Option<Usage>,
}

fn explicit_sandbox_off(v: &Value) -> bool {
    match v {
        Value::Bool(b) => !*b,
        Value::Object(o) => o.get("enabled") == Some(&Value::Bool(false)),
        _ => false,
    }
}

fn text_signals_limit(text: &str) -> bool {
    let t = text.to_ascii_lowercase();
    ["usage limit", "session limit", "weekly limit", "rate limit"]
        .iter()
        .any(|k| t.contains(k))
}

impl RuntimeAdapter for ClaudeAdapter {
    fn capabilities(&self) -> &'static Capabilities {
        &CAPS
    }

    fn version_argv(&self) -> Vec<String> {
        vec!["claude".into(), "--version".into()]
    }

    /// Accepts exactly `2.1.318 (Claude Code)` (surrounding whitespace ignored).
    fn check_version(&self, stdout: &str) -> Result<(), FailureCode> {
        let mut it = stdout.split_whitespace();
        match (it.next(), it.next(), it.next(), it.next()) {
            (Some(PINNED), Some("(Claude"), Some("Code)"), None) => Ok(()),
            _ => Err(FailureCode::VersionMismatch),
        }
    }

    fn plan(&self, run: &AgentRunSpec) -> Result<AdapterPlan, FailureCode> {
        let s = |x: &str| x.to_string();
        let tools = match run.role {
            Role::Implementer => "Bash,Read,Edit,Write,Grep,Glob",
            Role::Reviewer => "Read,Grep,Glob",
        };
        let mut argv = vec![
            s("claude"),
            s("-p"),
            s("--output-format"),
            s("stream-json"),
            s("--verbose"),
            s("--restricted"),
            s("--setting-sources"),
            s("user"),
            s("--strict-mcp-config"),
            s("--mcp-config"),
            path_str(&run.mcp_config)?,
            s("--disable-slash-commands"),
            s("--max-turns"),
            run.max_turns.to_string(),
            s("--settings"),
            s(r#"{"sandbox":{"enabled":false}}"#),
            s("--tools"),
            s(tools),
            // `-p` cannot ask anyone: pre-approve exactly the role's tools and
            // auto-deny everything else. Never bypassPermissions; the sandbox,
            // not Claude, is the write boundary.
            s("--allowedTools"),
            s(tools),
            s("--permission-mode"),
            s("dontAsk"),
            s("--permission-prompts"),
            s("none"),
        ];
        if run.role == Role::Reviewer {
            // Claude takes the schema inline; the adapter cannot read `review_schema`.
            argv.push(s("--json-schema"));
            argv.push(review_json_schema().to_string());
        }
        Ok(AdapterPlan {
            argv,
            env: vec![
                (s("CLAUDE_CONFIG_DIR"), path_str(&run.config_dir)?),
                (s("CLAUDE_CODE_DISABLE_CLAUDE_MDS"), s("1")),
                (s("DISABLE_AUTOUPDATER"), s("1")),
                (s("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"), s("1")),
            ],
            stdin: run.prompt.clone().into_bytes(),
            egress_hosts: vec![s("api.anthropic.com")],
        })
    }

    fn parse_line(&self, st: &mut ParseState, line: &[u8]) {
        let Some(ev) = parse_object(st, line) else {
            return;
        };
        let ty = ev.get("type").and_then(Value::as_str);
        let sub = ev.get("subtype").and_then(Value::as_str);
        if st.events == 0 && !(ty == Some("system") && sub == Some("init")) {
            st.invalid = true;
        }
        st.events += 1;
        let c = &mut st.claude;
        match (ty, sub) {
            (Some("system"), Some("init")) => {
                c.init_seen = true;
                c.session = ev
                    .get("session_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                c.init_sandbox_off = ev.get("sandbox").is_some_and(explicit_sandbox_off);
            }
            (Some("system"), Some("api_retry")) => {
                if ev.get("error").and_then(Value::as_str) == Some("rate_limit") {
                    c.limit = true;
                }
            }
            (Some("rate_limit_event"), _) => {
                if let Some(info) = ev.get("rate_limit_info")
                    && info.get("status").and_then(Value::as_str) == Some("rejected")
                {
                    c.limit = true;
                    c.limit_reset = info
                        .get("resetsAt")
                        .and_then(Value::as_f64)
                        .map(secs_to_timestamp);
                }
            }
            (Some("result"), _) => {
                let text = ev.get("result").and_then(Value::as_str).map(str::to_owned);
                let is_error = ev.get("is_error").and_then(Value::as_bool);
                if is_error == Some(true) && text.as_deref().is_some_and(text_signals_limit) {
                    c.limit = true;
                }
                if let Some(id) = ev.get("session_id").and_then(Value::as_str) {
                    c.session = Some(id.to_owned());
                }
                let u = ev.get("usage");
                let tok = |k: &str| u.and_then(|u| u.get(k)).and_then(Value::as_u64);
                let turns = ev
                    .get("num_turns")
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok());
                let usage = (u.is_some() || turns.is_some()).then(|| Usage {
                    input_tokens: tok("input_tokens"),
                    output_tokens: tok("output_tokens"),
                    turns,
                });
                c.result = Some(ResultEvent {
                    is_error,
                    text,
                    structured: ev.get("structured_output").cloned(),
                    usage,
                });
            }
            _ => {}
        }
    }

    fn finish(&self, st: ParseState, exit_code: Option<i32>) -> AgentRunResult {
        let ParseState {
            role,
            invalid,
            claude: c,
            ..
        } = st;
        let (final_text, usage) = match &c.result {
            Some(r) => (r.text.clone(), r.usage),
            None => (None, None),
        };
        let mut out = AgentRunResult {
            outcome: RunOutcome::Succeeded,
            final_text,
            review: None,
            session: c.session.clone(),
            usage,
            own_sandbox_confirmed_off: c.init_seen && c.init_sandbox_off,
        };
        let clean_success =
            exit_code == Some(0) && c.result.as_ref().is_some_and(|r| r.is_error == Some(false));
        if c.limit && !clean_success {
            out.outcome = RunOutcome::LimitExhausted {
                reset_at: c.limit_reset,
            };
            return out;
        }
        if invalid || !c.init_seen {
            out.outcome = failed(FailureCode::InvalidAgentOutput);
            return out;
        }
        let Some(res) = c.result else {
            out.outcome = failed(match exit_code {
                Some(n) if n != 0 => FailureCode::ExitNonzero,
                _ => FailureCode::InvalidAgentOutput,
            });
            return out;
        };
        match res.is_error {
            None => out.outcome = failed(FailureCode::InvalidAgentOutput),
            Some(true) => out.outcome = failed(FailureCode::ExitNonzero),
            Some(false) if exit_code != Some(0) => out.outcome = failed(FailureCode::ExitNonzero),
            Some(false) => {
                if role == Role::Reviewer {
                    let review: Option<RawReview> = match &res.structured {
                        Some(v) => serde_json::from_value(v.clone()).ok(),
                        None => res.text.as_deref().and_then(parse_review_text),
                    };
                    match review {
                        Some(r) => out.review = Some(r),
                        None => out.outcome = failed(FailureCode::InvalidAgentOutput),
                    }
                }
            }
        }
        out
    }
}
