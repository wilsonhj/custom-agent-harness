//! Shared vocabulary and the wire envelope.

use serde::{Deserialize, Serialize};

use crate::ids::{EffectId, TaskId, Timestamp};

pub const WIRE_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Research,
    Spec,
    Implement,
    Review,
    Repair,
    Operate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind {
    Claude,
    Codex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelFamily {
    Anthropic,
    #[serde(rename = "openai")]
    OpenAi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Implementer,
    Reviewer,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Actor {
    Human { principal: String },
    Engine,
    Effect { effect: EffectId },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RepoRef {
    pub path: String,
    pub git_common_dir: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Budget {
    pub max_repair_rounds: u8,
    pub run_deadline_ms: u64,
    pub max_turns: u32,
}

/// Every command and event on the wire. `body` is flattened, so its `"type"`
/// tag sits next to `v`, `id`, `task`, `at` and `actor`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub v: u16,
    pub id: String,
    pub task: TaskId,
    pub at: Timestamp,
    pub actor: Actor,
    #[serde(flatten)]
    pub body: T,
}
