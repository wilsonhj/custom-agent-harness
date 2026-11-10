//! Pure runtime adapters: build commands, parse output. Never spawns a process.

use std::path::PathBuf;

use harness_core::capabilities::Capabilities;
use harness_core::finding::RawReview;
use harness_core::ids::Timestamp;
use harness_core::outcome::{FailureCode, RunOutcome};
use harness_core::types::{Role, RuntimeKind};

mod claude;
mod codex;

#[derive(Debug, Clone)]
pub struct AgentRunSpec {
    pub role: Role,
    pub prompt: String,
    pub worktree: PathBuf,
    pub config_dir: PathBuf,
    pub mcp_config: PathBuf,
    pub review_schema: Option<PathBuf>,
    pub max_turns: u32,
}

/// `argv[0]` is the bare binary name (`claude` / `codex`); the engine swaps in
/// the resolved path.
#[derive(Debug, Clone)]
pub struct AdapterPlan {
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
    pub stdin: Vec<u8>,
    pub egress_hosts: Vec<String>,
}

/// Accumulates what `parse_line` sees. Opaque to callers.
pub struct ParseState {
    role: Role,
    /// True if our own argv disabled the runtime's sandbox (known without output).
    argv_sandbox_off: bool,
    /// A line was not valid UTF-8 / JSON, or the stream violated its framing.
    invalid: bool,
    events: u32,
    claude: claude::State,
    codex: codex::State,
}

impl ParseState {
    /// `argv` is the `AdapterPlan::argv` the run was launched with.
    pub fn new(role: Role, argv: &[String]) -> ParseState {
        let argv_sandbox_off = argv
            .windows(2)
            .any(|w| (w[0] == "-s" || w[0] == "--sandbox") && w[1] == "danger-full-access");
        ParseState {
            role,
            argv_sandbox_off,
            invalid: false,
            events: 0,
            claude: claude::State::default(),
            codex: codex::State::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub turns: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct AgentRunResult {
    pub outcome: RunOutcome,
    pub final_text: Option<String>,
    pub review: Option<RawReview>,
    pub session: Option<String>,
    pub usage: Option<Usage>,
    pub own_sandbox_confirmed_off: bool,
}

pub trait RuntimeAdapter: Send + Sync {
    fn capabilities(&self) -> &'static Capabilities;
    fn version_argv(&self) -> Vec<String>;
    fn check_version(&self, stdout: &str) -> Result<(), FailureCode>;
    fn plan(&self, run: &AgentRunSpec) -> Result<AdapterPlan, FailureCode>;
    fn parse_line(&self, st: &mut ParseState, line: &[u8]);
    fn finish(&self, st: ParseState, exit_code: Option<i32>) -> AgentRunResult;
}

pub fn adapter_for(kind: RuntimeKind) -> &'static dyn RuntimeAdapter {
    match kind {
        RuntimeKind::Claude => &claude::ClaudeAdapter,
        RuntimeKind::Codex => &codex::CodexAdapter,
    }
}

/// Parse one stream line into a JSON object, marking the state invalid on failure.
fn parse_object(
    st: &mut ParseState,
    line: &[u8],
) -> Option<serde_json::Map<String, serde_json::Value>> {
    let Ok(text) = std::str::from_utf8(line) else {
        st.invalid = true;
        return None;
    };
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(serde_json::Value::Object(map)) => Some(map),
        _ => {
            st.invalid = true;
            None
        }
    }
}

/// Parse reviewer output text into a `RawReview`; tolerates surrounding whitespace only.
fn parse_review_text(text: &str) -> Option<RawReview> {
    serde_json::from_str(text.trim()).ok()
}

fn failed(code: FailureCode) -> RunOutcome {
    RunOutcome::Failed { code }
}

fn secs_to_timestamp(secs: f64) -> Timestamp {
    Timestamp((secs * 1_000_000.0) as i64)
}

fn path_str(p: &std::path::Path) -> Result<String, FailureCode> {
    p.to_str().map(str::to_owned).ok_or(FailureCode::SpawnError)
}
