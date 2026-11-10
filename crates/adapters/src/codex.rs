//! Codex CLI adapter (`codex exec --json`). Event shapes are PROVISIONAL; see
//! `fixtures/README.md`. Unknown fields are ignored.

use harness_core::capabilities::{Capabilities, OwnSandbox, StructuredOutput};
use harness_core::outcome::{FailureCode, RunOutcome};
use harness_core::types::{ModelFamily, Role, RuntimeKind};
use serde_json::Value;

use crate::{
    AdapterPlan, AgentRunResult, AgentRunSpec, ParseState, RuntimeAdapter, Usage, failed,
    parse_object, parse_review_text, path_str,
};

pub(crate) struct CodexAdapter;

const PINNED: &str = "0.166.0";

static CAPS: Capabilities = Capabilities {
    runtime: RuntimeKind::Codex,
    model_family: ModelFamily::OpenAi,
    pinned_version: PINNED,
    roles: &[Role::Implementer, Role::Reviewer],
    structured_output: StructuredOutput::JsonSchemaFile,
    session_resume: true,
    reports_usage: true,
    own_sandbox: OwnSandbox::MustDisable,
};

#[derive(Default)]
pub(crate) struct State {
    thread: Option<String>,
    turns_completed: u32,
    failed: bool,
    limit: bool,
    last_agent_text: Option<String>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

fn message_signals_limit(msg: &str) -> bool {
    let t = msg.to_ascii_lowercase();
    ["usage limit", "rate limit", "quota"]
        .iter()
        .any(|k| t.contains(k))
}

fn add(acc: &mut Option<u64>, n: Option<u64>) {
    if let Some(n) = n {
        *acc = Some(acc.unwrap_or(0).saturating_add(n));
    }
}

impl RuntimeAdapter for CodexAdapter {
    fn capabilities(&self) -> &'static Capabilities {
        &CAPS
    }

    fn version_argv(&self) -> Vec<String> {
        vec!["codex".into(), "--version".into()]
    }

    /// Accepts exactly `codex-cli 0.166.0` (surrounding whitespace ignored).
    fn check_version(&self, stdout: &str) -> Result<(), FailureCode> {
        let mut it = stdout.split_whitespace();
        match (it.next(), it.next(), it.next()) {
            (Some("codex-cli"), Some(PINNED), None) => Ok(()),
            _ => Err(FailureCode::VersionMismatch),
        }
    }

    fn plan(&self, run: &AgentRunSpec) -> Result<AdapterPlan, FailureCode> {
        let s = |x: &str| x.to_string();
        let mut argv = vec![
            s("codex"),
            s("exec"),
            s("--json"),
            s("-s"),
            s("danger-full-access"),
            s("--skip-git-repo-check"),
            s("-C"),
            path_str(&run.worktree)?,
            // No AGENTS.md / AGENTS.override.md discovery: repository conventions
            // reach the agent only through the harness prompt.
            s("-c"),
            s("project_doc_max_bytes=0"),
        ];
        if run.role == Role::Reviewer {
            let schema = run
                .review_schema
                .as_deref()
                .ok_or(FailureCode::PolicyDenied)?;
            argv.push(s("--output-schema"));
            argv.push(path_str(schema)?);
        }
        argv.push(s("-")); // prompt from stdin
        Ok(AdapterPlan {
            argv,
            env: vec![(s("CODEX_HOME"), path_str(&run.config_dir)?)],
            stdin: run.prompt.clone().into_bytes(),
            egress_hosts: vec![s("chatgpt.com"), s("api.openai.com")],
        })
    }

    fn parse_line(&self, st: &mut ParseState, line: &[u8]) {
        let Some(ev) = parse_object(st, line) else {
            return;
        };
        st.events += 1;
        let c = &mut st.codex;
        match ev.get("type").and_then(Value::as_str) {
            Some("thread.started") => {
                c.thread = ev
                    .get("thread_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
            Some("turn.completed") => {
                c.turns_completed += 1;
                if let Some(u) = ev.get("usage") {
                    add(
                        &mut c.input_tokens,
                        u.get("input_tokens").and_then(Value::as_u64),
                    );
                    add(
                        &mut c.output_tokens,
                        u.get("output_tokens").and_then(Value::as_u64),
                    );
                }
            }
            Some("turn.failed") | Some("error") => {
                c.failed = true;
                let msg = ev
                    .get("message")
                    .or_else(|| ev.get("error").and_then(|e| e.get("message")))
                    .and_then(Value::as_str);
                if msg.is_some_and(message_signals_limit) {
                    c.limit = true;
                }
            }
            Some("item.completed") => {
                if let Some(item) = ev.get("item")
                    && item.get("type").and_then(Value::as_str) == Some("agent_message")
                    && let Some(t) = item.get("text").and_then(Value::as_str)
                {
                    c.last_agent_text = Some(t.to_owned());
                }
            }
            _ => {}
        }
    }

    fn finish(&self, st: ParseState, exit_code: Option<i32>) -> AgentRunResult {
        let ParseState {
            role,
            invalid,
            argv_sandbox_off,
            codex: c,
            ..
        } = st;
        let usage = (c.turns_completed > 0 || c.input_tokens.is_some()).then_some(Usage {
            input_tokens: c.input_tokens,
            output_tokens: c.output_tokens,
            turns: Some(c.turns_completed),
        });
        let mut out = AgentRunResult {
            outcome: RunOutcome::Succeeded,
            final_text: c.last_agent_text.clone(),
            review: None,
            session: c.thread.clone(),
            usage,
            own_sandbox_confirmed_off: argv_sandbox_off,
        };
        if c.limit && (c.failed || exit_code != Some(0)) {
            out.outcome = RunOutcome::LimitExhausted { reset_at: None };
        } else if invalid {
            out.outcome = failed(FailureCode::InvalidAgentOutput);
        } else if c.failed || exit_code != Some(0) {
            out.outcome = failed(FailureCode::ExitNonzero);
        } else if c.turns_completed == 0 {
            out.outcome = failed(FailureCode::InvalidAgentOutput);
        } else if role == Role::Reviewer {
            match c.last_agent_text.as_deref().and_then(parse_review_text) {
                Some(r) => out.review = Some(r),
                None => out.outcome = failed(FailureCode::InvalidAgentOutput),
            }
        }
        out
    }
}
