//! The open question: what a task is waiting for a human to answer, what the answer is bound
//! to, and which answers are legal (see GLOSSARY.md).
//!
//! `decide` asks this module before it acts on a human decision or a `Run` at a human gate,
//! and the CLI asks it before prompting. `lifecycle::can_transition` stays the fail-closed
//! backstop: an answer this table allows can still be denied there, never the reverse.

use crate::decide::{Rejection, Task};
use crate::lifecycle::{DecisionKind, Denied, Subject, TaskState};
use crate::stage::{HumanGate, StageRunState};

/// What a task is waiting for a human to answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpenQuestion {
    /// Approve the packet before any work starts.
    SpecApproval,
    /// A stage stopped at a human gate.
    Gate(HumanGate),
}

/// A human's answer to an open question.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Answer {
    Decide(DecisionKind),
    /// Retry the work the gate stopped (`Command::Run`).
    Run,
}

use Answer::{Decide, Run};
use DecisionKind::{Accept, ApproveSpec, Reject, SendBack};

impl OpenQuestion {
    /// The question `task` is waiting on, if any.
    pub fn of(task: &Task) -> Option<Self> {
        match (&task.run, task.state) {
            (StageRunState::AwaitingHuman { gate }, _) => Some(Self::Gate(*gate)),
            (_, TaskState::Planned) => Some(Self::SpecApproval),
            _ => None,
        }
    }

    /// Every legal answer. `Accept` is legal only when review came back clean: at the other
    /// gates a blocking finding may be open, a chunk unreviewed, or verify failing (#22).
    pub fn answers(self) -> &'static [Answer] {
        match self {
            Self::SpecApproval => &[Decide(ApproveSpec)],
            Self::Gate(HumanGate::ReviewClean) => {
                &[Decide(Accept), Decide(Reject), Decide(SendBack)]
            }
            Self::Gate(HumanGate::RepairLimitReached) => &[Decide(Reject), Decide(SendBack)],
            Self::Gate(HumanGate::LimitExhausted | HumanGate::LostOnRestart) => {
                &[Run, Decide(Reject)]
            }
        }
    }

    pub fn allows(self, answer: Answer) -> bool {
        self.answers().contains(&answer)
    }

    /// One line for the human who is about to answer.
    pub fn describe(self) -> &'static str {
        match self {
            Self::SpecApproval => "spec approval: the packet awaits approval",
            Self::Gate(HumanGate::ReviewClean) => "review is clean",
            Self::Gate(HumanGate::RepairLimitReached) => "repair limit reached",
            Self::Gate(HumanGate::LimitExhausted) => {
                "a runtime hit its usage limit; work is unfinished"
            }
            Self::Gate(HumanGate::LostOnRestart) => {
                "an effect was lost on restart; work is unfinished"
            }
        }
    }
}

/// What an answer to the open question must be bound to: the packet digest for spec approval
/// and before any checkpoint, otherwise the last checkpoint's commit.
pub fn subject(task: &Task, question: OpenQuestion) -> Subject {
    match (question, task.checkpoints.last()) {
        (OpenQuestion::SpecApproval, _) | (_, None) => Subject::PacketDigest(task.packet.clone()),
        (OpenQuestion::Gate(_), Some(cp)) => Subject::Commit(cp.commit.clone()),
    }
}

/// Checks `answer` against the open question: `WrongState` when none is open, `WrongKind`
/// when the question does not allow the answer (except `Run`, which stays `WrongState`), then
/// `WrongSubject` when a decision is bound to anything but [`subject`].
pub fn check(
    task: &Task,
    answer: Answer,
    bound: Option<&Subject>,
) -> Result<OpenQuestion, Rejection> {
    let question = OpenQuestion::of(task).ok_or(Rejection::WrongState)?;
    if !question.allows(answer) {
        return Err(match answer {
            Run => Rejection::WrongState,
            Decide(_) => Rejection::Denied(Denied::WrongKind),
        });
    }
    if let Some(s) = bound
        && *s != subject(task, question)
    {
        return Err(Rejection::Denied(Denied::WrongSubject));
    }
    Ok(question)
}
