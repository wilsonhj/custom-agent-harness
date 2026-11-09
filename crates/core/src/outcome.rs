//! How a run ended and why a task stopped.

use serde::{Deserialize, Serialize};

use crate::ids::Timestamp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureCode {
    ExitNonzero,
    DeadlineExceeded,
    OutputLimitExceeded,
    SpawnError,
    SandboxUnavailable,
    VersionMismatch,
    ConfigChanged,
    InvalidAgentOutput,
    PacketChanged,
    CandidateChanged,
    GitMetadataTouched,
    SecretInDiff,
    PolicyDenied,
    LostOnRestart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunOutcome {
    Succeeded,
    Failed { code: FailureCode },
    LimitExhausted { reset_at: Option<Timestamp> },
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminationReason {
    Succeeded,
    TaskFailure,
    BudgetExhausted,
    InfrastructureFailure,
    PolicyViolation,
    Cancelled,
}
