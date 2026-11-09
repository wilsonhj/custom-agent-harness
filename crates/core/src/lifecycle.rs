//! Task lifecycle: the 16 states and transition table of `harness/policy/transitions.ts`,
//! with the TS approval record replaced by a `HumanDecision` bound to an exact packet digest
//! or commit (design §5.3, §6.5).

use serde::{Deserialize, Serialize};

use crate::ids::{ArtifactId, CommitSha, Seq};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    // Successful path
    Draft,
    Planned,
    Approved,
    Implementing,
    Reviewing,
    Verified,
    PreviewVerified,
    MigrationGated,
    Released,
    Observed,
    Closed,
    // Side states
    Blocked,
    Failed,
    Cancelled,
    Repair,
    Superseded,
}

use TaskState::*;

/// `ALLOWED_TRANSITIONS` of `harness/policy/transitions.ts`, verbatim and in the same order.
/// Resume from `Blocked` is not listed; `can_transition` allows it only back to
/// `TransitionContext::blocked_from`.
pub const ALLOWED: &[(TaskState, &[TaskState])] = &[
    (Draft, &[Planned, Blocked, Cancelled]),
    (Planned, &[Approved, Draft, Blocked, Cancelled, Superseded]),
    (Approved, &[Implementing, Blocked, Cancelled, Superseded]),
    (
        Implementing,
        &[Reviewing, Blocked, Failed, Cancelled, Superseded],
    ),
    (Reviewing, &[Verified, Repair, Blocked, Failed, Cancelled]),
    (Repair, &[Reviewing, Blocked, Failed, Cancelled, Superseded]),
    (
        Verified,
        &[PreviewVerified, Repair, Blocked, Failed, Cancelled],
    ),
    (
        PreviewVerified,
        &[MigrationGated, Repair, Blocked, Failed, Cancelled],
    ),
    (
        MigrationGated,
        &[Released, Repair, Blocked, Failed, Cancelled],
    ),
    (Released, &[Observed, Blocked, Failed]),
    (Observed, &[Closed, Blocked, Failed]),
    (Blocked, &[Cancelled, Superseded]),
    (Failed, &[Repair, Cancelled, Superseded]),
    (Closed, &[]),
    (Cancelled, &[]),
    (Superseded, &[]),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    ApproveSpec,
    Accept,
    Reject,
    SendBack,
}

/// What a decision is bound to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Subject {
    PacketDigest(ArtifactId),
    Commit(CommitSha),
}

/// A human's decision, read back from a recorded decision event. Fields are private and there
/// is no `Deserialize`, so the only way to hold one is `from_event`, which only the store
/// calls with an event the human entered at the CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HumanDecision {
    kind: DecisionKind,
    subject: Subject,
    principal: String,
    seq: Seq,
}

impl HumanDecision {
    pub fn from_event(kind: DecisionKind, subject: Subject, principal: String, seq: Seq) -> Self {
        Self {
            kind,
            subject,
            principal,
            seq,
        }
    }

    pub fn kind(&self) -> DecisionKind {
        self.kind
    }

    pub fn subject(&self) -> &Subject {
        &self.subject
    }

    pub fn principal(&self) -> &str {
        &self.principal
    }

    pub fn seq(&self) -> Seq {
        self.seq
    }
}

/// Facts the engine supplies from its own state, never from an agent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransitionContext {
    /// The task's packet digest (subject of `ApproveSpec`).
    pub packet: Option<ArtifactId>,
    /// The candidate commit under review (subject of `Accept`).
    pub candidate: Option<CommitSha>,
    /// The state the task entered `Blocked` from (the TS `BlockedRecord.from_state`).
    pub blocked_from: Option<TaskState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum Denied {
    #[error("transition is not in the table")]
    NotInTable,
    #[error("transition requires a human decision")]
    ApprovalRequired,
    #[error("decision is bound to a different digest or commit")]
    WrongSubject,
    #[error("decision kind does not authorise this transition")]
    WrongKind,
    #[error("state is unreachable in this design (no release path)")]
    Unreachable,
}

/// Fail closed: anything not positively allowed is denied. Every arm names its target states,
/// so adding a `TaskState` variant does not compile until it is placed here.
pub fn can_transition(
    from: TaskState,
    to: TaskState,
    decision: Option<&HumanDecision>,
    ctx: &TransitionContext,
) -> Result<(), Denied> {
    match (from, to) {
        // Kept for parity with the TS table; this design has no release path.
        (_, PreviewVerified | MigrationGated | Released | Observed | Closed) => {
            Err(Denied::Unreachable)
        }
        // Stopping work never needs a decision. `Blocked -> Failed | Blocked` is a TS resume to
        // a non-resumable state and is denied as unlisted.
        (_, Failed | Cancelled | Blocked) => listed(from, to),
        (Planned, Approved) => gate(decision, DecisionKind::ApproveSpec, |s| match s {
            Subject::PacketDigest(d) => ctx.packet.as_ref() == Some(d),
            Subject::Commit(_) => false,
        }),
        (Reviewing, Verified) => gate(decision, DecisionKind::Accept, |s| match s {
            Subject::Commit(c) => ctx.candidate.as_ref() == Some(c),
            Subject::PacketDigest(_) => false,
        }),
        // Engine-driven steps of the implement/review/repair loop.
        (Approved, Implementing)
        | (Implementing, Reviewing)
        | (Reviewing, Repair)
        | (Repair, Reviewing) => Ok(()),
        // Resume (TS rule): only back to the state the task was blocked from, and then only
        // with an approval. No decision kind in this slice authorises a resume.
        (Blocked, Draft | Planned | Approved | Implementing | Reviewing | Verified | Repair) => {
            if ctx.blocked_from != Some(to) {
                Err(Denied::NotInTable)
            } else if decision.is_none() {
                Err(Denied::ApprovalRequired)
            } else {
                Err(Denied::WrongKind)
            }
        }
        // Remaining listed edges (for example `Draft -> Planned`, `Failed -> Repair`,
        // `-> Superseded`) need a decision kind this slice does not have.
        (
            _,
            Draft | Planned | Approved | Implementing | Reviewing | Verified | Repair | Superseded,
        ) => {
            listed(from, to)?;
            match decision {
                None => Err(Denied::ApprovalRequired),
                Some(_) => Err(Denied::WrongKind),
            }
        }
    }
}

fn listed(from: TaskState, to: TaskState) -> Result<(), Denied> {
    let ok = ALLOWED
        .iter()
        .any(|(f, tos)| *f == from && tos.contains(&to));
    if ok { Ok(()) } else { Err(Denied::NotInTable) }
}

fn gate(
    decision: Option<&HumanDecision>,
    kind: DecisionKind,
    subject_matches: impl FnOnce(&Subject) -> bool,
) -> Result<(), Denied> {
    let Some(d) = decision else {
        return Err(Denied::ApprovalRequired);
    };
    if d.kind != kind {
        Err(Denied::WrongKind)
    } else if !subject_matches(&d.subject) {
        Err(Denied::WrongSubject)
    } else {
        Ok(())
    }
}
