//! The per-project pipeline: which runtime implements, which reviews, and which commands
//! verify. It is data, digested and frozen into the task at creation (design §5.3).

use serde::Deserialize;

use crate::finding::Severity;
use crate::types::{Budget, ModelFamily, Role, RuntimeKind};

pub const DEFAULT_MAX_REPAIR_ROUNDS: u8 = 2;
pub const DEFAULT_SEVERITY_THRESHOLD: Severity = Severity::High;
pub const DEFAULT_REVIEW_CHUNK_MAX_LINES: u32 = 400;
/// Attempts per verification command (a crash mid-command is retried once; design §5.4).
pub const VERIFY_MAX_ATTEMPTS: u8 = 2;

/// The model family behind a runtime. Core cannot depend on `harness-adapters`, where each
/// adapter's `Capabilities.model_family` lives, so the mapping is repeated here; the adapters
/// crate's tests check that Claude's capabilities say Anthropic and that Codex's differ.
pub const fn model_family(runtime: RuntimeKind) -> ModelFamily {
    match runtime {
        RuntimeKind::Claude => ModelFamily::Anthropic,
        RuntimeKind::Codex => ModelFamily::OpenAi,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StageRoute {
    pub runtime: RuntimeKind,
    pub role: Role,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pipeline {
    pub implement: StageRoute,
    pub review: StageRoute,
    /// Each entry is one argv, run as given (no shell).
    pub verify_commands: Vec<Vec<String>>,
    pub budget: Budget,
    pub severity_threshold: Severity,
    pub review_chunk_max_lines: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PipelineError {
    /// Not TOML, an unknown key, a missing required key or a value of the wrong type.
    #[error("pipeline TOML: {0}")]
    Toml(String),
    #[error("implement and review runtimes are both {family:?}; review needs another family")]
    SameFamily { family: ModelFamily },
    #[error("verify_commands is empty")]
    NoVerifyCommands,
    #[error("verify_commands[{index}] is an empty argv")]
    EmptyCommand { index: usize },
    #[error("a stage route has the wrong role")]
    WrongRole,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRoute {
    runtime: RuntimeKind,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPipeline {
    implement: RawRoute,
    review: RawRoute,
    verify_commands: Vec<Vec<String>>,
    #[serde(default = "default_rounds")]
    max_repair_rounds: u8,
    run_deadline_ms: u64,
    max_turns: u32,
    #[serde(default = "default_threshold")]
    severity_threshold: Severity,
    #[serde(default = "default_chunk")]
    review_chunk_max_lines: u32,
}

fn default_rounds() -> u8 {
    DEFAULT_MAX_REPAIR_ROUNDS
}

fn default_threshold() -> Severity {
    DEFAULT_SEVERITY_THRESHOLD
}

fn default_chunk() -> u32 {
    DEFAULT_REVIEW_CHUNK_MAX_LINES
}

impl Pipeline {
    /// Parses a project's `pipeline.toml`. Unknown keys are rejected. Top-level keys come
    /// before the two tables (TOML rule). Full example, with every default written out:
    ///
    /// ```toml
    /// # Required. One argv per command, run without a shell; at least one, none empty.
    /// verify_commands = [["cargo", "test", "--workspace"], ["cargo", "clippy"]]
    /// # Required. Wall-clock limit for one agent run or one verify command, in ms.
    /// run_deadline_ms = 1800000
    /// # Required. Turn limit for one agent run.
    /// max_turns = 200
    /// # Optional (default 2). Automatic repair rounds before stopping for the human.
    /// max_repair_rounds = 2
    /// # Optional (default "high"). critical | high | medium | low; findings at or above it block.
    /// severity_threshold = "high"
    /// # Optional (default 400). Changed lines per review chunk.
    /// review_chunk_max_lines = 400
    ///
    /// [implement]
    /// runtime = "claude"   # claude | codex
    ///
    /// [review]
    /// runtime = "codex"    # must be a different model family from implement
    /// ```
    ///
    /// The role of each route is fixed by its table (`implement` is the implementer, `review`
    /// the reviewer) and cannot be set.
    pub fn parse_toml(s: &str) -> Result<Pipeline, PipelineError> {
        let raw: RawPipeline = toml::from_str(s).map_err(|e| PipelineError::Toml(e.to_string()))?;
        let pipeline = Pipeline {
            implement: StageRoute {
                runtime: raw.implement.runtime,
                role: Role::Implementer,
            },
            review: StageRoute {
                runtime: raw.review.runtime,
                role: Role::Reviewer,
            },
            verify_commands: raw.verify_commands,
            budget: Budget {
                max_repair_rounds: raw.max_repair_rounds,
                run_deadline_ms: raw.run_deadline_ms,
                max_turns: raw.max_turns,
            },
            severity_threshold: raw.severity_threshold,
            review_chunk_max_lines: raw.review_chunk_max_lines,
        };
        pipeline.validate()?;
        Ok(pipeline)
    }

    /// The invariants `parse_toml` guarantees. The fields are public, so `decide` checks them
    /// again on every command.
    pub fn validate(&self) -> Result<(), PipelineError> {
        if self.implement.role != Role::Implementer || self.review.role != Role::Reviewer {
            return Err(PipelineError::WrongRole);
        }
        let family = model_family(self.implement.runtime);
        if family == model_family(self.review.runtime) {
            return Err(PipelineError::SameFamily { family });
        }
        if self.verify_commands.is_empty() {
            return Err(PipelineError::NoVerifyCommands);
        }
        if let Some(index) = self
            .verify_commands
            .iter()
            .position(|argv| argv.first().is_none_or(|program| program.is_empty()))
        {
            return Err(PipelineError::EmptyCommand { index });
        }
        Ok(())
    }
}
