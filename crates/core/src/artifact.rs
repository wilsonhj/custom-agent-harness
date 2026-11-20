//! Content-addressed artifacts and their provenance labels.

use serde::{Deserialize, Serialize};

use crate::ids::{ArtifactId, EffectId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaType {
    Json,
    Jsonl,
    Patch,
    Markdown,
    Text,
    Binary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    Input,
    RuntimeClaim,
    HarnessEvidence,
    ReviewFindings,
    HumanStatement,
}

impl Provenance {
    /// Agent output is a claim; only these two labels may be cited as evidence.
    pub fn is_evidence(self) -> bool {
        matches!(
            self,
            Provenance::HarnessEvidence | Provenance::ReviewFindings
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Artifact {
    pub id: ArtifactId,
    pub size: u64,
    pub media_type: MediaType,
    pub label: Provenance,
    pub produced_by: Option<EffectId>,
}
