//! `decide(task, command) -> {events, effects}` and `apply(task, event) -> task`.
//!
//! The flow (plan Task 4): create (`Draft -> Planned`), approve the packet digest
//! (`Planned -> Approved`), run (`Approved -> Implementing`, implementer run), checkpoint,
//! one verify per command, then one reviewer run per diff chunk; then a repair round, or a
//! stop for the human. Work is done in batches: the implementer run, the checkpoint, the
//! verifies, the reviews. Outcomes inside a batch are recorded as they arrive; the next step
//! is decided when the batch's last effect finishes.
//!
//! Every repair decision is taken in `Reviewing`: when verification fails the task still
//! moves `Implementing | Repair -> Reviewing` (both ungated) and then, without review, to
//! `Repair` or to a stop. `AwaitingHuman{RepairLimitReached | ReviewClean}` therefore always
//! sits in `Reviewing`, where `Accept`, `Reject` and `SendBack` are table transitions.
//!
//! A batch that ends in `LimitExhausted` or `LostOnRestart` stops for the human without
//! touching the round. `Run` then re-issues exactly the effects that stopped, with new ids;
//! the rest of the batch keeps its results.

use serde::{Deserialize, Serialize};

use crate::finding::{Finding, Review};
use crate::ids::{ArtifactId, CommitSha, EffectId, IdempotencyKey, Seq, Timestamp};
use crate::lifecycle::{
    DecisionKind, Denied, HumanDecision, Subject, TaskState, TransitionContext, can_transition,
};
use crate::outcome::{FailureCode, RunOutcome, TerminationReason};
use crate::policy::{Pipeline, VERIFY_MAX_ATTEMPTS};
use crate::stage::{HumanGate, StageRunState};
use crate::types::{Actor, Envelope, RepoRef, Role, RuntimeKind};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub n: u32,
    pub commit: CommitSha,
}

/// Why a batch stops instead of advancing. When several effects of one batch stop, the most
/// severe wins: `Cancelled` > `Failed` > `LostOnRestart` > `LimitExhausted`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Stop {
    Gate { gate: HumanGate },
    Failed { code: FailureCode },
    Cancelled,
}

impl Stop {
    fn rank(self) -> u8 {
        match self {
            Stop::Cancelled => 3,
            Stop::Failed { .. } => 2,
            Stop::Gate {
                gate: HumanGate::LostOnRestart,
            } => 1,
            Stop::Gate {
                gate:
                    HumanGate::LimitExhausted | HumanGate::ReviewClean | HumanGate::RepairLimitReached,
            } => 0,
        }
    }

    fn worst(a: Option<Stop>, b: Option<Stop>) -> Option<Stop> {
        match (a, b) {
            (Some(x), Some(y)) => Some(if y.rank() > x.rank() { y } else { x }),
            (x, None) => x,
            (None, y) => y,
        }
    }
}

/// What `decide` must remember between outcomes. Folded by `apply` from events only.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    /// Specs of intended effects with no recorded outcome; ids equal `run.effects` while the
    /// run is `EffectsInFlight` (they stay here after a cancel, for late outcomes).
    pub in_flight: Vec<EffectSpec>,
    /// This round's evidence: verify outputs and review-findings artifacts, in arrival order.
    pub evidence: Vec<ArtifactId>,
    /// Some verify of this round did not pass.
    pub verify_failed: bool,
    /// Blocking findings recorded this round.
    pub blocking: u32,
    /// The current batch's stop so far.
    pub stop: Option<Stop>,
    /// Effects of the current batch that stopped at a human gate; `Run` re-issues them.
    pub retry: Vec<EffectSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: crate::ids::TaskId,
    pub repo: RepoRef,
    pub packet: ArtifactId,
    pub pipeline: ArtifactId,
    pub base_commit: CommitSha,
    pub state: TaskState,
    pub run: StageRunState,
    pub round: u8,
    pub checkpoints: Vec<Checkpoint>,
    /// Number of events applied.
    pub version: u64,
    pub progress: Progress,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    CreateTask {
        repo: RepoRef,
        base_commit: CommitSha,
        packet: ArtifactId,
        pipeline: ArtifactId,
        key: IdempotencyKey,
    },
    RecordHumanDecision {
        kind: DecisionKind,
        subject: Subject,
        key: IdempotencyKey,
    },
    /// Starts an approved task, or re-issues the effects that stopped at a
    /// `LimitExhausted` / `LostOnRestart` gate.
    Run {
        key: IdempotencyKey,
    },
    RecordEffectOutcome {
        effect: EffectId,
        outcome: EffectOutcome,
    },
    Cancel {
        key: IdempotencyKey,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EffectSpec {
    /// `base` is the commit the worktree is at: for the implementer, the last checkpoint (or
    /// the task's base commit); for a reviewer, the candidate. A reviewer reviews chunk
    /// `chunk` of `chunk_diff(diff(task.base_commit..base), review_chunk_max_lines)`.
    /// `inputs`: the packet, then, for a repair run, the previous round's evidence.
    AgentRun {
        effect: EffectId,
        role: Role,
        runtime: RuntimeKind,
        base: CommitSha,
        inputs: Vec<ArtifactId>,
        chunk: Option<u32>,
        deadline_ms: u64,
    },
    Verify {
        effect: EffectId,
        command_index: u32,
        candidate: CommitSha,
        deadline_ms: u64,
        max_attempts: u8,
    },
    Checkpoint {
        effect: EffectId,
        expected_parent: CommitSha,
        author_time: Timestamp,
    },
}

impl EffectSpec {
    pub fn effect(&self) -> &EffectId {
        match self {
            EffectSpec::AgentRun { effect, .. }
            | EffectSpec::Verify { effect, .. }
            | EffectSpec::Checkpoint { effect, .. } => effect,
        }
    }

    fn with_effect(&self, id: EffectId) -> EffectSpec {
        let mut spec = self.clone();
        match &mut spec {
            EffectSpec::AgentRun { effect, .. }
            | EffectSpec::Verify { effect, .. }
            | EffectSpec::Checkpoint { effect, .. } => *effect = id,
        }
        spec
    }
}

/// Artifacts arrive by reference; the engine stored them with their labels first.
/// `AgentRun.review` is the reviewer's `ReviewFindings` artifact (the parsed, validated
/// `Review` arrives in `Facts.review`); `Verify.evidence` is `HarnessEvidence`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EffectOutcome {
    AgentRun {
        outcome: RunOutcome,
        transcript: ArtifactId,
        review: Option<ArtifactId>,
    },
    Verify {
        outcome: RunOutcome,
        evidence: ArtifactId,
    },
    Checkpoint {
        outcome: RunOutcome,
        commit: Option<CommitSha>,
        diff: Option<ArtifactId>,
    },
}

impl EffectOutcome {
    fn run(&self) -> &RunOutcome {
        match self {
            EffectOutcome::AgentRun { outcome, .. }
            | EffectOutcome::Verify { outcome, .. }
            | EffectOutcome::Checkpoint { outcome, .. } => outcome,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    TaskCreated {
        repo: RepoRef,
        base_commit: CommitSha,
        packet: ArtifactId,
        pipeline: ArtifactId,
    },
    /// The principal is the envelope's `Actor::Human`.
    HumanDecisionRecorded {
        kind: DecisionKind,
        subject: Subject,
    },
    TaskStateChanged {
        from: TaskState,
        to: TaskState,
    },
    EffectIntended {
        spec: EffectSpec,
    },
    EffectFinished {
        effect: EffectId,
        outcome: EffectOutcome,
    },
    CheckpointCreated {
        n: u32,
        commit: CommitSha,
    },
    VerificationFinished {
        effect: EffectId,
        passed: bool,
        evidence: ArtifactId,
    },
    FindingsRecorded {
        effect: EffectId,
        review: ArtifactId,
        findings: Vec<Finding>,
        blocking: u32,
    },
    RepairRoundStarted {
        round: u8,
    },
    /// `evidence` holds only artifact ids that came from `Verify` outcomes and reviewer
    /// `review` artifacts. Core cannot see labels: this is where the rule "only
    /// `is_evidence()` artifacts are cited" is enforced, and it holds only if the engine
    /// stores verify output as `HarnessEvidence` and validated reviews as `ReviewFindings`.
    AwaitingHuman {
        gate: HumanGate,
        evidence: Vec<ArtifactId>,
    },
    /// The stage failed with `code`; the task moves to `Failed` in the same decision.
    StageFailed {
        code: FailureCode,
    },
    TaskCancelled,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub events: Vec<Event>,
    pub effects: Vec<EffectSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Rejection {
    #[error("no such task")]
    UnknownTask,
    #[error("transition denied: {0}")]
    Denied(Denied),
    #[error("command is not valid in the task's current state")]
    WrongState,
    #[error("outcome for an effect that is not in flight")]
    StaleEffect,
    #[error("pipeline is invalid")]
    PipelineInvalid,
    /// `Facts.new_effect_ids` had fewer ids than the decision needs; retry with `needed`.
    #[error("decision needs {needed} new effect ids")]
    NeedEffectIds { needed: usize },
    /// The outcome contradicts the effect it answers: another kind of effect, a reviewer
    /// `Succeeded` with no `review` artifact, or a checkpoint `Succeeded` with no commit. Like
    /// `StaleEffect`, this is an engine bug, never a runtime's doing.
    #[error("outcome is inconsistent with its effect")]
    InvalidOutcome,
    /// `Facts` lacks something this decision needs (`"review"`, `"diff_chunks"`).
    #[error("facts lack {fact}")]
    MissingFact { fact: &'static str },
}

impl From<Denied> for Rejection {
    fn from(d: Denied) -> Self {
        Rejection::Denied(d)
    }
}

/// What the engine knows that core cannot compute.
pub struct Facts<'a> {
    pub pipeline: &'a Pipeline,
    /// The validated review behind a reviewer outcome's `review` artifact.
    pub review: Option<&'a Review>,
    /// `chunk_diff(diff, pipeline.review_chunk_max_lines).len()` for the candidate's diff
    /// against the task's base commit. Needed when a verify batch passes; `Some(0)` skips
    /// review and counts as clean.
    pub diff_chunks: Option<u32>,
    /// Fresh ids, used in order; core never generates ids.
    pub new_effect_ids: &'a [EffectId],
}

/// A log entry `apply` cannot fold: the log and the code disagree.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Corrupt {
    #[error("event for a task that was never created")]
    NoTask,
    #[error("task created twice")]
    AlreadyCreated,
    #[error("event belongs to another task")]
    WrongTask,
    #[error("state change from {expected:?} but the task is {actual:?}")]
    StateMismatch {
        expected: TaskState,
        actual: TaskState,
    },
    #[error("outcome for an effect that is not in flight")]
    UnknownEffect,
    #[error("checkpoint {n} is out of order")]
    CheckpointOutOfOrder { n: u32 },
    /// An event type this build does not know (a newer writer). Folding past it could give a
    /// wrong state, so it is refused.
    #[error("unknown event type")]
    UnknownEvent,
}

/// How a run ended, as one of the six termination reasons. `role` is `None` for harness
/// effects (verify commands, checkpoints). A deadline is a budget for an agent but a failing
/// check for a verify command.
pub fn classify(role: Option<Role>, outcome: &RunOutcome) -> TerminationReason {
    use FailureCode::*;
    match outcome {
        RunOutcome::Succeeded => TerminationReason::Succeeded,
        RunOutcome::Cancelled => TerminationReason::Cancelled,
        RunOutcome::LimitExhausted { .. } => TerminationReason::BudgetExhausted,
        RunOutcome::Failed { code } => match code {
            DeadlineExceeded => match role {
                Some(Role::Implementer | Role::Reviewer) => TerminationReason::BudgetExhausted,
                None => TerminationReason::TaskFailure,
            },
            ExitNonzero | OutputLimitExceeded | InvalidAgentOutput => {
                TerminationReason::TaskFailure
            }
            SpawnError
            | SandboxUnavailable
            | ContainmentUnavailable
            | VersionMismatch
            | ConfigChanged
            | LostOnRestart => TerminationReason::InfrastructureFailure,
            PacketChanged | CandidateChanged | GitMetadataTouched | SecretInDiff | PolicyDenied => {
                TerminationReason::PolicyViolation
            }
        },
    }
}

/// Whether an outcome stops its batch. A verify that fails as a task failure (the check
/// failed) does not stop: it sends the round to repair.
fn stop_of(spec: &EffectSpec, outcome: &EffectOutcome) -> Option<Stop> {
    let run = outcome.run();
    match run {
        RunOutcome::Succeeded => None,
        RunOutcome::Cancelled => Some(Stop::Cancelled),
        RunOutcome::LimitExhausted { .. } => Some(Stop::Gate {
            gate: HumanGate::LimitExhausted,
        }),
        RunOutcome::Failed {
            code: FailureCode::LostOnRestart,
        } => Some(Stop::Gate {
            gate: HumanGate::LostOnRestart,
        }),
        RunOutcome::Failed { code } => match spec {
            EffectSpec::Verify { .. } if classify(None, run) == TerminationReason::TaskFailure => {
                None
            }
            EffectSpec::Verify { .. }
            | EffectSpec::AgentRun { .. }
            | EffectSpec::Checkpoint { .. } => Some(Stop::Failed { code: *code }),
        },
    }
}

/// Whether a verify outcome means the check itself failed. An interrupted verify (lost on
/// restart, limit, cancel) says nothing about the check: its retry decides.
fn check_failed(outcome: &EffectOutcome) -> bool {
    matches!(
        outcome.run(),
        RunOutcome::Failed { code } if *code != FailureCode::LostOnRestart
    )
}

/// Builds a decision, handing out effect ids in order. Running out does not stop the build;
/// `finish` then reports how many were needed.
struct Out<'a> {
    state: TaskState,
    events: Vec<Event>,
    effects: Vec<EffectSpec>,
    supply: &'a [EffectId],
    used: usize,
}

impl<'a> Out<'a> {
    fn new(state: TaskState, supply: &'a [EffectId]) -> Self {
        Self {
            state,
            events: Vec::new(),
            effects: Vec::new(),
            supply,
            used: 0,
        }
    }

    fn id(&mut self) -> EffectId {
        let id = self.supply.get(self.used).cloned();
        self.used += 1;
        id.unwrap_or_else(|| EffectId::new(""))
    }

    fn event(&mut self, e: Event) {
        self.events.push(e);
    }

    fn intend(&mut self, spec: EffectSpec) {
        self.events
            .push(Event::EffectIntended { spec: spec.clone() });
        self.effects.push(spec);
    }

    fn to(
        &mut self,
        to: TaskState,
        decision: Option<&HumanDecision>,
        ctx: &TransitionContext,
    ) -> Result<(), Rejection> {
        can_transition(self.state, to, decision, ctx)?;
        self.events.push(Event::TaskStateChanged {
            from: self.state,
            to,
        });
        self.state = to;
        Ok(())
    }

    fn finish(self) -> Result<Decision, Rejection> {
        if self.used > self.supply.len() {
            return Err(Rejection::NeedEffectIds { needed: self.used });
        }
        Ok(Decision {
            events: self.events,
            effects: self.effects,
        })
    }
}

fn ctx(task: &Task) -> TransitionContext {
    TransitionContext {
        packet: Some(task.packet.clone()),
        candidate: task.checkpoints.last().map(|c| c.commit.clone()),
        blocked_from: None,
    }
}

/// The commit the next implementer run starts from.
fn head(task: &Task) -> CommitSha {
    task.checkpoints
        .last()
        .map_or_else(|| task.base_commit.clone(), |c| c.commit.clone())
}

fn awaiting(task: &Task) -> Result<HumanGate, Rejection> {
    match &task.run {
        StageRunState::AwaitingHuman { gate } => Ok(*gate),
        StageRunState::Pending
        | StageRunState::EffectsInFlight { .. }
        | StageRunState::Succeeded
        | StageRunState::Failed { .. }
        | StageRunState::Cancelled => Err(Rejection::WrongState),
    }
}

fn implementer_run(task: &Task, facts: &Facts, out: &mut Out, inputs: Vec<ArtifactId>) {
    let effect = out.id();
    out.intend(EffectSpec::AgentRun {
        effect,
        role: Role::Implementer,
        runtime: facts.pipeline.implement.runtime,
        base: head(task),
        inputs,
        chunk: None,
        deadline_ms: facts.pipeline.budget.run_deadline_ms,
    });
}

/// Starts round `task.round + 1` with `evidence` (the round just ended) as repair input.
fn repair_round(
    task: &Task,
    facts: &Facts,
    out: &mut Out,
    evidence: &[ArtifactId],
) -> Result<(), Rejection> {
    let round = task.round.checked_add(1).ok_or(Rejection::WrongState)?;
    out.event(Event::RepairRoundStarted { round });
    let mut inputs = vec![task.packet.clone()];
    inputs.extend_from_slice(evidence);
    implementer_run(task, facts, out, inputs);
    Ok(())
}

/// Called in `Reviewing` once verification (and review, if it ran) is complete.
fn repair_or_stop(
    task: &Task,
    facts: &Facts,
    out: &mut Out,
    needs_repair: bool,
    evidence: Vec<ArtifactId>,
) -> Result<(), Rejection> {
    if !needs_repair {
        out.event(Event::AwaitingHuman {
            gate: HumanGate::ReviewClean,
            evidence,
        });
    } else if task.round < facts.pipeline.budget.max_repair_rounds {
        out.to(TaskState::Repair, None, &ctx(task))?;
        repair_round(task, facts, out, &evidence)?;
    } else {
        out.event(Event::AwaitingHuman {
            gate: HumanGate::RepairLimitReached,
            evidence,
        });
    }
    Ok(())
}

pub fn decide(
    task: Option<&Task>,
    cmd: &Envelope<Command>,
    facts: &Facts,
) -> Result<Decision, Rejection> {
    facts
        .pipeline
        .validate()
        .map_err(|_| Rejection::PipelineInvalid)?;
    match (&cmd.body, task) {
        (
            Command::CreateTask {
                repo,
                base_commit,
                packet,
                pipeline,
                key: _,
            },
            None,
        ) => {
            let mut out = Out::new(TaskState::Draft, facts.new_effect_ids);
            out.event(Event::TaskCreated {
                repo: repo.clone(),
                base_commit: base_commit.clone(),
                packet: packet.clone(),
                pipeline: pipeline.clone(),
            });
            let ctx = TransitionContext {
                packet: Some(packet.clone()),
                ..TransitionContext::default()
            };
            out.to(TaskState::Planned, None, &ctx)?;
            out.finish()
        }
        (Command::CreateTask { .. }, Some(_)) => Err(Rejection::WrongState),
        (
            Command::RecordHumanDecision { .. }
            | Command::Run { .. }
            | Command::RecordEffectOutcome { .. }
            | Command::Cancel { .. },
            None,
        ) => Err(Rejection::UnknownTask),
        (
            Command::RecordHumanDecision {
                kind,
                subject,
                key: _,
            },
            Some(task),
        ) => human_decision(task, &cmd.actor, *kind, subject, facts),
        (Command::Run { key: _ }, Some(task)) => run(task, facts),
        (Command::RecordEffectOutcome { effect, outcome }, Some(task)) => {
            effect_outcome(task, cmd.at, effect, outcome, facts)
        }
        (Command::Cancel { key: _ }, Some(task)) => {
            let mut out = Out::new(task.state, facts.new_effect_ids);
            out.to(TaskState::Cancelled, None, &ctx(task))?;
            out.event(Event::TaskCancelled);
            out.finish()
        }
    }
}

fn human_decision(
    task: &Task,
    actor: &Actor,
    kind: DecisionKind,
    subject: &Subject,
    facts: &Facts,
) -> Result<Decision, Rejection> {
    let principal = match actor {
        Actor::Human { principal } => principal.clone(),
        Actor::Engine | Actor::Effect { .. } => {
            return Err(Rejection::Denied(Denied::ApprovalRequired));
        }
    };
    // Only for the gate check below; the store builds the durable `HumanDecision`, with the
    // event's real sequence number, when it reads the recorded event back.
    let decision = HumanDecision::from_event(kind, subject.clone(), principal, Seq(0));
    let ctx = ctx(task);
    let mut out = Out::new(task.state, facts.new_effect_ids);
    out.event(Event::HumanDecisionRecorded {
        kind,
        subject: subject.clone(),
    });
    // The subject of the open gate: the candidate commit, or the packet before any checkpoint.
    let gate_subject = match &ctx.candidate {
        Some(c) => Subject::Commit(c.clone()),
        None => Subject::PacketDigest(task.packet.clone()),
    };
    match kind {
        DecisionKind::ApproveSpec => out.to(TaskState::Approved, Some(&decision), &ctx)?,
        DecisionKind::Accept => {
            awaiting(task)?;
            out.to(TaskState::Verified, Some(&decision), &ctx)?;
        }
        DecisionKind::Reject => {
            awaiting(task)?;
            if *subject != gate_subject {
                return Err(Rejection::Denied(Denied::WrongSubject));
            }
            out.to(TaskState::Failed, None, &ctx)?;
        }
        DecisionKind::SendBack => {
            awaiting(task)?;
            if *subject != gate_subject {
                return Err(Rejection::Denied(Denied::WrongSubject));
            }
            out.to(TaskState::Repair, None, &ctx)?;
            repair_round(task, facts, &mut out, &task.progress.evidence)?;
        }
    }
    out.finish()
}

fn run(task: &Task, facts: &Facts) -> Result<Decision, Rejection> {
    let mut out = Out::new(task.state, facts.new_effect_ids);
    match &task.run {
        StageRunState::Pending => {
            out.to(TaskState::Implementing, None, &ctx(task))?;
            implementer_run(task, facts, &mut out, vec![task.packet.clone()]);
        }
        StageRunState::AwaitingHuman { gate } => match gate {
            HumanGate::LimitExhausted | HumanGate::LostOnRestart => {
                if task.progress.retry.is_empty() {
                    return Err(Rejection::WrongState);
                }
                for spec in &task.progress.retry {
                    let id = out.id();
                    out.intend(spec.with_effect(id));
                }
            }
            HumanGate::ReviewClean | HumanGate::RepairLimitReached => {
                return Err(Rejection::WrongState);
            }
        },
        StageRunState::EffectsInFlight { .. }
        | StageRunState::Succeeded
        | StageRunState::Failed { .. }
        | StageRunState::Cancelled => return Err(Rejection::WrongState),
    }
    out.finish()
}

fn effect_outcome(
    task: &Task,
    at: Timestamp,
    effect: &EffectId,
    outcome: &EffectOutcome,
    facts: &Facts,
) -> Result<Decision, Rejection> {
    let in_flight = &task.progress.in_flight;
    let Some(spec) = in_flight.iter().find(|s| s.effect() == effect) else {
        return Err(Rejection::StaleEffect);
    };
    let mut out = Out::new(task.state, facts.new_effect_ids);
    out.event(Event::EffectFinished {
        effect: effect.clone(),
        outcome: outcome.clone(),
    });
    match task.state {
        // A late outcome after a cancel is recorded and changes nothing else.
        TaskState::Cancelled => return out.finish(),
        TaskState::Implementing | TaskState::Reviewing | TaskState::Repair => {}
        TaskState::Draft
        | TaskState::Planned
        | TaskState::Approved
        | TaskState::Verified
        | TaskState::PreviewVerified
        | TaskState::MigrationGated
        | TaskState::Released
        | TaskState::Observed
        | TaskState::Closed
        | TaskState::Blocked
        | TaskState::Failed
        | TaskState::Superseded => return Err(Rejection::WrongState),
    }

    let succeeded = *outcome.run() == RunOutcome::Succeeded;
    let mut evidence = task.progress.evidence.clone();
    let mut verify_failed = task.progress.verify_failed;
    let mut blocking = task.progress.blocking;
    let mut checkpoint = None;
    match (spec, outcome) {
        (
            EffectSpec::AgentRun {
                role: Role::Implementer,
                ..
            },
            EffectOutcome::AgentRun { .. },
        ) => {}
        (
            EffectSpec::AgentRun {
                role: Role::Reviewer,
                ..
            },
            EffectOutcome::AgentRun { review, .. },
        ) => {
            if succeeded {
                let Some(artifact) = review else {
                    return Err(Rejection::InvalidOutcome);
                };
                let Some(r) = facts.review else {
                    return Err(Rejection::MissingFact { fact: "review" });
                };
                let n = r.findings.iter().filter(|f| f.blocking).count();
                let n = u32::try_from(n).unwrap_or(u32::MAX);
                blocking = blocking.saturating_add(n);
                evidence.push(artifact.clone());
                out.event(Event::FindingsRecorded {
                    effect: effect.clone(),
                    review: artifact.clone(),
                    findings: r.findings.clone(),
                    blocking: n,
                });
            }
        }
        (EffectSpec::Verify { .. }, EffectOutcome::Verify { evidence: e, .. }) => {
            verify_failed |= check_failed(outcome);
            evidence.push(e.clone());
            out.event(Event::VerificationFinished {
                effect: effect.clone(),
                passed: succeeded,
                evidence: e.clone(),
            });
        }
        (EffectSpec::Checkpoint { .. }, EffectOutcome::Checkpoint { commit, .. }) => {
            if succeeded {
                let Some(commit) = commit else {
                    return Err(Rejection::InvalidOutcome);
                };
                let n = u32::try_from(task.checkpoints.len())
                    .ok()
                    .and_then(|n| n.checked_add(1))
                    .ok_or(Rejection::WrongState)?;
                out.event(Event::CheckpointCreated {
                    n,
                    commit: commit.clone(),
                });
                checkpoint = Some(commit.clone());
            }
        }
        (
            EffectSpec::AgentRun { .. },
            EffectOutcome::Verify { .. } | EffectOutcome::Checkpoint { .. },
        )
        | (
            EffectSpec::Verify { .. },
            EffectOutcome::AgentRun { .. } | EffectOutcome::Checkpoint { .. },
        )
        | (
            EffectSpec::Checkpoint { .. },
            EffectOutcome::AgentRun { .. } | EffectOutcome::Verify { .. },
        ) => return Err(Rejection::InvalidOutcome),
    }

    if in_flight.len() > 1 {
        return out.finish();
    }

    // The batch is complete.
    let ctx = ctx(task);
    match Stop::worst(task.progress.stop, stop_of(spec, outcome)) {
        Some(Stop::Cancelled) => {
            out.to(TaskState::Cancelled, None, &ctx)?;
            out.event(Event::TaskCancelled);
        }
        Some(Stop::Failed { code }) => {
            out.to(TaskState::Failed, None, &ctx)?;
            out.event(Event::StageFailed { code });
        }
        Some(Stop::Gate { gate }) => out.event(Event::AwaitingHuman { gate, evidence }),
        None => match spec {
            EffectSpec::AgentRun {
                role: Role::Implementer,
                ..
            } => {
                let effect = out.id();
                out.intend(EffectSpec::Checkpoint {
                    effect,
                    expected_parent: head(task),
                    author_time: at,
                });
            }
            EffectSpec::Checkpoint { .. } => {
                let Some(candidate) = checkpoint else {
                    return Err(Rejection::InvalidOutcome);
                };
                for i in 0..facts.pipeline.verify_commands.len() {
                    let command_index = u32::try_from(i).map_err(|_| Rejection::PipelineInvalid)?;
                    let effect = out.id();
                    out.intend(EffectSpec::Verify {
                        effect,
                        command_index,
                        candidate: candidate.clone(),
                        deadline_ms: facts.pipeline.budget.run_deadline_ms,
                        max_attempts: VERIFY_MAX_ATTEMPTS,
                    });
                }
            }
            EffectSpec::Verify { candidate, .. } => {
                out.to(TaskState::Reviewing, None, &ctx)?;
                if verify_failed {
                    repair_or_stop(task, facts, &mut out, true, evidence)?;
                } else {
                    let chunks = facts.diff_chunks.ok_or(Rejection::MissingFact {
                        fact: "diff_chunks",
                    })?;
                    if chunks == 0 {
                        repair_or_stop(task, facts, &mut out, false, evidence)?;
                    }
                    for chunk in 0..chunks {
                        let effect = out.id();
                        out.intend(EffectSpec::AgentRun {
                            effect,
                            role: Role::Reviewer,
                            runtime: facts.pipeline.review.runtime,
                            base: candidate.clone(),
                            inputs: vec![task.packet.clone()],
                            chunk: Some(chunk),
                            deadline_ms: facts.pipeline.budget.run_deadline_ms,
                        });
                    }
                }
            }
            EffectSpec::AgentRun {
                role: Role::Reviewer,
                ..
            } => {
                repair_or_stop(
                    task,
                    facts,
                    &mut out,
                    verify_failed || blocking > 0,
                    evidence,
                )?;
            }
        },
    }
    out.finish()
}

/// Folds one event into the task. Run-state rules: `TaskCreated` and an answered gate
/// (`HumanDecisionRecorded`) leave it `Pending`; `EffectIntended` adds to `EffectsInFlight`
/// (starting a new batch when nothing was in flight); the last `EffectFinished` of a batch
/// returns it to `Pending`; `AwaitingHuman`, `StageFailed` and `TaskCancelled` set their
/// state; `TaskStateChanged` into `Verified` sets `Succeeded`.
pub fn apply(task: Option<Task>, event: &Envelope<Event>) -> Result<Task, Corrupt> {
    let Some(mut t) = task else {
        return match &event.body {
            Event::TaskCreated {
                repo,
                base_commit,
                packet,
                pipeline,
            } => Ok(Task {
                id: event.task.clone(),
                repo: repo.clone(),
                packet: packet.clone(),
                pipeline: pipeline.clone(),
                base_commit: base_commit.clone(),
                state: TaskState::Draft,
                run: StageRunState::Pending,
                round: 0,
                checkpoints: Vec::new(),
                version: 1,
                progress: Progress::default(),
            }),
            Event::Unknown => Err(Corrupt::UnknownEvent),
            Event::HumanDecisionRecorded { .. }
            | Event::TaskStateChanged { .. }
            | Event::EffectIntended { .. }
            | Event::EffectFinished { .. }
            | Event::CheckpointCreated { .. }
            | Event::VerificationFinished { .. }
            | Event::FindingsRecorded { .. }
            | Event::RepairRoundStarted { .. }
            | Event::AwaitingHuman { .. }
            | Event::StageFailed { .. }
            | Event::TaskCancelled => Err(Corrupt::NoTask),
        };
    };
    if t.id != event.task {
        return Err(Corrupt::WrongTask);
    }
    match &event.body {
        Event::TaskCreated { .. } => return Err(Corrupt::AlreadyCreated),
        Event::HumanDecisionRecorded { .. } => match &t.run {
            StageRunState::AwaitingHuman { .. } => t.run = StageRunState::Pending,
            StageRunState::Pending
            | StageRunState::EffectsInFlight { .. }
            | StageRunState::Succeeded
            | StageRunState::Failed { .. }
            | StageRunState::Cancelled => {}
        },
        Event::TaskStateChanged { from, to } => {
            if *from != t.state {
                return Err(Corrupt::StateMismatch {
                    expected: *from,
                    actual: t.state,
                });
            }
            t.state = *to;
            if *to == TaskState::Verified {
                t.run = StageRunState::Succeeded;
            }
        }
        Event::EffectIntended { spec } => {
            let id = spec.effect().clone();
            match &mut t.run {
                StageRunState::EffectsInFlight { effects } => effects.push(id),
                StageRunState::Pending
                | StageRunState::AwaitingHuman { .. }
                | StageRunState::Succeeded
                | StageRunState::Failed { .. }
                | StageRunState::Cancelled => {
                    t.run = StageRunState::EffectsInFlight { effects: vec![id] };
                    t.progress.stop = None;
                    t.progress.retry.clear();
                }
            }
            t.progress.in_flight.push(spec.clone());
        }
        Event::EffectFinished { effect, outcome } => {
            let pos = t
                .progress
                .in_flight
                .iter()
                .position(|s| s.effect() == effect)
                .ok_or(Corrupt::UnknownEffect)?;
            let spec = t.progress.in_flight.remove(pos);
            let batch_done = match &mut t.run {
                StageRunState::EffectsInFlight { effects } => {
                    effects.retain(|e| e != effect);
                    effects.is_empty()
                }
                StageRunState::Pending
                | StageRunState::AwaitingHuman { .. }
                | StageRunState::Succeeded
                | StageRunState::Failed { .. }
                | StageRunState::Cancelled => false,
            };
            if batch_done {
                t.run = StageRunState::Pending;
            }
            if matches!(spec, EffectSpec::Verify { .. }) {
                t.progress.verify_failed |= check_failed(outcome);
            }
            let stop = stop_of(&spec, outcome);
            if let Some(Stop::Gate { .. }) = stop {
                t.progress.retry.push(spec);
            }
            t.progress.stop = Stop::worst(t.progress.stop, stop);
        }
        Event::CheckpointCreated { n, commit } => {
            let expected = u32::try_from(t.checkpoints.len())
                .ok()
                .and_then(|len| len.checked_add(1));
            if expected != Some(*n) {
                return Err(Corrupt::CheckpointOutOfOrder { n: *n });
            }
            t.checkpoints.push(Checkpoint {
                n: *n,
                commit: commit.clone(),
            });
        }
        Event::VerificationFinished { evidence, .. } => {
            // `passed` is not folded: whether the check failed comes from the effect's
            // outcome (`EffectFinished`), so an interrupted verify cannot taint the round.
            t.progress.evidence.push(evidence.clone());
        }
        Event::FindingsRecorded {
            review, blocking, ..
        } => {
            t.progress.evidence.push(review.clone());
            t.progress.blocking = t.progress.blocking.saturating_add(*blocking);
        }
        Event::RepairRoundStarted { round } => {
            t.round = *round;
            t.progress.evidence.clear();
            t.progress.verify_failed = false;
            t.progress.blocking = 0;
        }
        Event::AwaitingHuman { gate, .. } => t.run = StageRunState::AwaitingHuman { gate: *gate },
        Event::StageFailed { code } => t.run = StageRunState::Failed { code: *code },
        Event::TaskCancelled => t.run = StageRunState::Cancelled,
        Event::Unknown => return Err(Corrupt::UnknownEvent),
    }
    t.version = t.version.saturating_add(1);
    Ok(t)
}
