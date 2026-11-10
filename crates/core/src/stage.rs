//! Stage run state (design §5.3).

use serde::{Deserialize, Serialize};

use crate::ids::EffectId;
use crate::outcome::FailureCode;

/// Why a stage stopped to wait for a human.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanGate {
    ReviewClean,
    RepairLimitReached,
    LostOnRestart,
    LimitExhausted,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StageRunState {
    Pending,
    EffectsInFlight { effects: Vec<EffectId> },
    AwaitingHuman { gate: HumanGate },
    Succeeded,
    Failed { code: FailureCode },
    Cancelled,
}
