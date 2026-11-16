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

/// Codex versions this adapter has been checked against (`codex --version` prints
/// `codex-cli <version>`). Codex updates itself, so this is a list of tested versions rather
/// than one exact pin; adding a version means re-running the checks recorded in `plan`.
const TESTED_VERSIONS: &[&str] = &["0.166.0", "0.169.0"];

/// `TESTED_VERSIONS` for display. `Capabilities::pinned_version` is a `&'static str`, and the
/// doctor's mismatch message prints it next to the installed version. A unit test keeps the
/// two in step.
const TESTED_VERSIONS_DISPLAY: &str = "0.166.0, 0.169.0";

static CAPS: Capabilities = Capabilities {
    runtime: RuntimeKind::Codex,
    model_family: ModelFamily::OpenAi,
    pinned_version: TESTED_VERSIONS_DISPLAY,
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
    /// A `turn.failed` was seen. A bare `error` line never sets this: Codex also prints one
    /// for errors it is about to retry (`will_retry` is dropped from the JSONL line).
    turn_failed: bool,
    /// The `turn.failed` message, which Codex builds from the turn error.
    failure_message: Option<String>,
    /// The most recent `error` line; only used to classify a run that otherwise failed.
    last_error: Option<String>,
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

    /// Accepts exactly `codex-cli <v>` for a `v` in `TESTED_VERSIONS` (surrounding
    /// whitespace ignored).
    fn check_version(&self, stdout: &str) -> Result<(), FailureCode> {
        let mut it = stdout.split_whitespace();
        match (it.next(), it.next(), it.next()) {
            (Some("codex-cli"), Some(v), None) if TESTED_VERSIONS.contains(&v) => Ok(()),
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
            // No repository skill descriptions (`.agents/skills/*/SKILL.md`) in the prompt
            // either. On 0.166.0 its effect was checked offline with `codex debug
            // prompt-input`. The sandbox also denies reads of `.agents`, so this is defence in
            // depth; Task 11 rechecks it on a tested version.
            s("-c"),
            s("skills.include_instructions=false"),
            // The harness, not Codex, controls the version. Only the TUI and `codex doctor`
            // read this key (openai/codex b016cd1847: tui/src/updates.rs, cli/src/doctor/
            // updates.rs), not `codex exec`, so this is defence in depth.
            s("-c"),
            s("check_for_update_on_startup=false"),
            // Verified on 0.169.0 (2026-11-15), offline and without starting a session:
            // `codex exec --strict-config` rejects an unknown `-c` key ("unknown
            // configuration field ... in -c/--config override"; controls
            // `definitely_not_a_codex_key` and `skills.not_a_real_subkey`). With the three
            // keys above and a missing `--output-schema` file it got past config loading and
            // exited 1 with "Failed to read output schema file", before any thread started.
            // So 0.169.0 recognises all three keys and reads `--output-schema`. Not verified
            // on 0.169.0: that the keys change the prompt (AGENTS.md / skills left out).
            // `-s danger-full-access`, `--skip-git-repo-check`, `-C`, `--json` and `-`
            // appear in `codex exec --help` for 0.169.0.
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
            Some("turn.failed") => {
                c.turn_failed = true;
                c.failure_message = ev
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
            Some("error") => {
                // Possibly transient: Codex retries some errors and the turn still completes.
                if let Some(m) = ev.get("message").and_then(Value::as_str) {
                    c.last_error = Some(m.to_owned());
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
        // Failure comes only from `turn.failed` or the exit code (a missing `turn.completed`
        // is handled below). The class comes from the `turn.failed` message, else the last
        // `error` line; earlier `error` lines (retries) are never consulted.
        let run_failed = c.turn_failed || exit_code != Some(0);
        let reason = c.failure_message.as_deref().or(c.last_error.as_deref());
        if run_failed && reason.is_some_and(message_signals_limit) {
            out.outcome = RunOutcome::LimitExhausted { reset_at: None };
        } else if invalid {
            out.outcome = failed(FailureCode::InvalidAgentOutput);
        } else if run_failed {
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

#[cfg(test)]
mod tests {
    use super::{TESTED_VERSIONS, TESTED_VERSIONS_DISPLAY};

    #[test]
    fn tested_versions_display_matches_the_list() {
        assert_eq!(TESTED_VERSIONS.join(", "), TESTED_VERSIONS_DISPLAY);
    }
}
