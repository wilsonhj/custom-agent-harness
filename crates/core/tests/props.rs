//! Property tests: random command and outcome sequences folded through `decide` + `apply`.
//!
//! Every sequence starts with create, approve and run so most cases reach review and repair;
//! the rest is random commands, outcomes (including limits, lost runs, failures, wrong
//! subjects, stale effects and too few effect ids) and human decisions. Rejected commands
//! change nothing, as in the store.

use harness_core::decide::{
    Command, EffectOutcome, EffectSpec, Event, Facts, Rejection, Task, apply, decide,
};
use harness_core::finding::{Finding, FindingCategory, Review, Severity, Verdict};
use harness_core::ids::{ArtifactId, CommitSha, EffectId, IdempotencyKey, TaskId, Timestamp};
use harness_core::lifecycle::{DecisionKind, Subject, TaskState};
use harness_core::outcome::{FailureCode, RunOutcome};
use harness_core::policy::Pipeline;
use harness_core::stage::{HumanGate, StageRunState};
use harness_core::types::{Actor, Envelope, RepoRef, Role, RuntimeKind, WIRE_VERSION};
use proptest::prelude::*;

const PIPELINE: &str = r#"
verify_commands = [["make", "test"], ["make", "lint"]]
run_deadline_ms = 1000
max_turns = 10

[implement]
runtime = "claude"

[review]
runtime = "codex"
"#;

#[derive(Debug, Clone)]
struct Step {
    op: u8,
    a: u8,
    b: u8,
    flag: bool,
}

fn steps() -> impl Strategy<Value = Vec<Step>> {
    prop::collection::vec(
        (any::<u8>(), any::<u8>(), any::<u8>(), any::<bool>()).prop_map(|(op, a, b, flag)| Step {
            op,
            a,
            b,
            flag,
        }),
        1..120,
    )
}

struct Sim {
    pipeline: Pipeline,
    task: Option<Task>,
    history: Vec<Envelope<Event>>,
    counter: u64,
}

fn sha(n: u64) -> Result<CommitSha, String> {
    CommitSha::parse(&format!("{n:040x}")).map_err(|e| e.to_string())
}

fn art(s: &str) -> ArtifactId {
    ArtifactId::of(s.as_bytes())
}

fn packet() -> ArtifactId {
    art("packet")
}

fn human() -> Actor {
    Actor::Human {
        principal: "owner".into(),
    }
}

impl Sim {
    fn new() -> Result<Self, String> {
        Ok(Self {
            pipeline: Pipeline::parse_toml(PIPELINE).map_err(|e| e.to_string())?,
            task: None,
            history: Vec::new(),
            counter: 0,
        })
    }

    fn next(&mut self) -> u64 {
        self.counter += 1;
        self.counter
    }

    fn envelope<T>(&mut self, actor: Actor, body: T) -> Envelope<T> {
        let n = self.next();
        Envelope {
            v: WIRE_VERSION,
            id: format!("m{n}"),
            task: TaskId::new("t1"),
            at: Timestamp(i64::try_from(n).unwrap_or(i64::MAX)),
            actor,
            body,
        }
    }

    /// Submits like the engine: on `NeedEffectIds` it retries once with enough ids.
    /// Returns `Err` only for a corrupt fold (property P4).
    fn submit(
        &mut self,
        actor: Actor,
        cmd: Command,
        review: Option<&Review>,
        diff_chunks: Option<u32>,
        supply: usize,
    ) -> Result<(), String> {
        let env = self.envelope(actor, cmd);
        let mut supply = supply;
        let decision = loop {
            let base = self.next();
            let ids: Vec<EffectId> = (0..supply)
                .map(|i| EffectId::new(format!("e{base}-{i}")))
                .collect();
            let facts = Facts {
                pipeline: &self.pipeline,
                review,
                diff_chunks,
                new_effect_ids: &ids,
            };
            match decide(self.task.as_ref(), &env, &facts) {
                Ok(d) => break d,
                Err(Rejection::NeedEffectIds { needed }) if needed > supply => supply = needed,
                Err(_) => return Ok(()),
            }
        };
        if decision.effects.len() > supply {
            return Err("decision used more effect ids than supplied".into());
        }
        for ev in decision.events {
            let e = self.envelope(Actor::Engine, ev);
            match apply(self.task.take(), &e) {
                Ok(t) => self.task = Some(t),
                Err(c) => return Err(format!("apply returned Corrupt({c:?}) for {:?}", e.body)),
            }
            self.history.push(e);
        }
        Ok(())
    }

    fn key(&mut self) -> IdempotencyKey {
        IdempotencyKey::new(format!("k{}", self.next()))
    }

    fn gate_subject(&self, correct: bool) -> Result<Subject, String> {
        if !correct {
            return Ok(Subject::Commit(sha(0xdead)?));
        }
        Ok(
            match self.task.as_ref().and_then(|t| t.checkpoints.last()) {
                Some(c) => Subject::Commit(c.commit.clone()),
                None => Subject::PacketDigest(packet()),
            },
        )
    }

    fn step(&mut self, s: &Step) -> Result<(), String> {
        let supply = usize::from(s.a % 5);
        let chunks = if s.a == 255 {
            None
        } else {
            Some(u32::from(s.a % 4))
        };
        match s.op % 16 {
            0 => self.create(),
            1 => {
                let subject = if s.flag {
                    Subject::PacketDigest(packet())
                } else {
                    Subject::PacketDigest(art("other"))
                };
                self.decision(DecisionKind::ApproveSpec, subject)
            }
            2 | 3 => self.run(supply),
            4..=10 => self.outcome(s, supply, chunks),
            11..=13 => {
                let kind = match s.a % 4 {
                    0 => DecisionKind::ApproveSpec,
                    1 => DecisionKind::Accept,
                    2 => DecisionKind::Reject,
                    _ => DecisionKind::SendBack,
                };
                let subject = self.gate_subject(s.flag)?;
                self.decision(kind, subject)
            }
            14 if s.a < 2 => {
                let key = self.key();
                self.submit(human(), Command::Cancel { key }, None, None, 0)
            }
            14 => self.run(supply),
            _ => {
                let effect = EffectId::new(format!("stale{}", s.a));
                let cmd = Command::RecordEffectOutcome {
                    effect: effect.clone(),
                    outcome: EffectOutcome::AgentRun {
                        outcome: RunOutcome::Succeeded,
                        transcript: art("t"),
                        review: None,
                    },
                };
                self.submit(Actor::Effect { effect }, cmd, None, chunks, supply)
            }
        }
    }

    fn create(&mut self) -> Result<(), String> {
        let key = self.key();
        let cmd = Command::CreateTask {
            repo: RepoRef {
                path: "/repo".into(),
                git_common_dir: "/repo/.git".into(),
            },
            base_commit: sha(1)?,
            packet: packet(),
            pipeline: art("pipeline"),
            key,
        };
        self.submit(Actor::Engine, cmd, None, None, 0)
    }

    fn decision(&mut self, kind: DecisionKind, subject: Subject) -> Result<(), String> {
        let key = self.key();
        let cmd = Command::RecordHumanDecision { kind, subject, key };
        self.submit(human(), cmd, None, None, 4)
    }

    fn run(&mut self, supply: usize) -> Result<(), String> {
        let key = self.key();
        self.submit(human(), Command::Run { key }, None, None, supply)
    }

    fn outcome(&mut self, s: &Step, supply: usize, chunks: Option<u32>) -> Result<(), String> {
        let in_flight = match &self.task {
            Some(t) if !t.progress.in_flight.is_empty() => t.progress.in_flight.clone(),
            _ => return Ok(()),
        };
        let spec = in_flight[usize::from(s.a) % in_flight.len()].clone();
        let run = match s.b % 40 {
            0..=33 => RunOutcome::Succeeded,
            34 => RunOutcome::Failed {
                code: FailureCode::ExitNonzero,
            },
            35 => RunOutcome::LimitExhausted { reset_at: None },
            36 => RunOutcome::Failed {
                code: FailureCode::LostOnRestart,
            },
            37 => RunOutcome::Failed {
                code: FailureCode::SecretInDiff,
            },
            38 => RunOutcome::Cancelled,
            _ => RunOutcome::Failed {
                code: FailureCode::DeadlineExceeded,
            },
        };
        let n = self.next();
        let mut review = None;
        let outcome = match &spec {
            EffectSpec::AgentRun { role, base, .. } => {
                let mut review_artifact = None;
                if *role == Role::Reviewer && run == RunOutcome::Succeeded && s.b != 13 {
                    review_artifact = Some(art(&format!("review{n}")));
                    review = Some(make_review(s.flag, base));
                }
                EffectOutcome::AgentRun {
                    outcome: run,
                    transcript: art(&format!("transcript{n}")),
                    review: review_artifact,
                }
            }
            EffectSpec::Verify { .. } => EffectOutcome::Verify {
                outcome: if s.flag || run != RunOutcome::Succeeded {
                    run
                } else {
                    RunOutcome::Failed {
                        code: FailureCode::ExitNonzero,
                    }
                },
                evidence: art(&format!("verify{n}")),
            },
            EffectSpec::Checkpoint { .. } => EffectOutcome::Checkpoint {
                outcome: run,
                commit: if run == RunOutcome::Succeeded && s.b != 13 {
                    Some(sha(1000 + n)?)
                } else {
                    None
                },
                diff: Some(art(&format!("diff{n}"))),
            },
        };
        let effect = spec.effect().clone();
        let cmd = Command::RecordEffectOutcome {
            effect: effect.clone(),
            outcome,
        };
        self.submit(
            Actor::Effect { effect },
            cmd,
            review.as_ref(),
            chunks,
            supply,
        )
    }

    fn events(&self) -> impl Iterator<Item = &Event> {
        self.history.iter().map(|e| &e.body)
    }
}

fn make_review(blocking: bool, commit: &CommitSha) -> Review {
    let severity = if blocking {
        Severity::Critical
    } else {
        Severity::Low
    };
    Review {
        verdict: if blocking {
            Verdict::ChangesRequested
        } else {
            Verdict::Pass
        },
        summary: "s".into(),
        findings: vec![Finding {
            id: "F1".into(),
            severity,
            blocking,
            category: FindingCategory::Tests,
            location: None,
            claim: "c".into(),
            failure_scenario: "f".into(),
            recommendation: "r".into(),
            reviewed_commit: commit.clone(),
            reviewer: RuntimeKind::Codex,
        }],
    }
}

/// Runs the prefix (create, approve, run) and then `steps`, calling `check` after each.
fn simulate(steps: &[Step], check: fn(&Sim) -> Result<(), String>) -> Result<(), String> {
    let mut sim = Sim::new()?;
    sim.create()?;
    sim.decision(DecisionKind::ApproveSpec, Subject::PacketDigest(packet()))?;
    sim.run(1)?;
    check(&sim)?;
    for s in steps {
        sim.step(s)?;
        check(&sim)?;
    }
    Ok(())
}

fn last_checkpoint(t: &Task) -> Option<&CommitSha> {
    t.checkpoints.last().map(|c| &c.commit)
}

/// P1: Verified => an Accept decision for the last checkpoint's commit is in the history.
fn p1(sim: &Sim) -> Result<(), String> {
    let Some(t) = &sim.task else { return Ok(()) };
    if t.state != TaskState::Verified {
        return Ok(());
    }
    let Some(c) = last_checkpoint(t) else {
        return Err("Verified without a checkpoint".into());
    };
    let accepted = sim.events().any(|e| {
        matches!(e, Event::HumanDecisionRecorded {
            kind: DecisionKind::Accept,
            subject: Subject::Commit(s),
        } if s == c)
    });
    if accepted {
        Ok(())
    } else {
        Err(format!("Verified without Accept for {c}"))
    }
}

/// P2: AwaitingHuman{ReviewClean} => a passing verification exists for the last checkpoint.
fn p2(sim: &Sim) -> Result<(), String> {
    let Some(t) = &sim.task else { return Ok(()) };
    if t.run
        != (StageRunState::AwaitingHuman {
            gate: HumanGate::ReviewClean,
        })
    {
        return Ok(());
    }
    let Some(c) = last_checkpoint(t) else {
        return Err("ReviewClean without a checkpoint".into());
    };
    let verify_effects: Vec<&EffectId> = sim
        .events()
        .filter_map(|e| match e {
            Event::EffectIntended {
                spec:
                    EffectSpec::Verify {
                        effect, candidate, ..
                    },
            } if candidate == c => Some(effect),
            _ => None,
        })
        .collect();
    let passed = sim.events().any(|e| {
        matches!(e, Event::VerificationFinished { effect, passed: true, .. }
            if verify_effects.contains(&effect))
    });
    if passed {
        Ok(())
    } else {
        Err(format!("ReviewClean without a passing verification of {c}"))
    }
}

/// P3: repair rounds never exceed the budget plus the human's send-backs.
fn p3(sim: &Sim) -> Result<(), String> {
    let rounds = sim
        .events()
        .filter(|e| matches!(e, Event::RepairRoundStarted { .. }))
        .count();
    let send_backs = sim
        .events()
        .filter(|e| {
            matches!(
                e,
                Event::HumanDecisionRecorded {
                    kind: DecisionKind::SendBack,
                    ..
                }
            )
        })
        .count();
    let max = usize::from(sim.pipeline.budget.max_repair_rounds);
    if rounds <= max + send_backs {
        Ok(())
    } else {
        Err(format!("{rounds} repair rounds > {max} + {send_backs}"))
    }
}

/// P4: `apply` accepts every event `decide` produced (checked inside `submit`).
fn p4(_: &Sim) -> Result<(), String> {
    Ok(())
}

/// A verify lost on restart, a human `Run`, and a passing retry reach review, not repair
/// (P5). `lost` picks which of the two commands is lost; `chunks` is the diff size.
fn lost_verify_then_passing_retry(lost: usize, chunks: u32) -> Result<(), String> {
    let mut sim = Sim::new()?;
    sim.create()?;
    sim.decision(DecisionKind::ApproveSpec, Subject::PacketDigest(packet()))?;
    sim.run(4)?;
    let in_flight = |sim: &Sim| -> Result<Vec<EffectSpec>, String> {
        sim.task
            .as_ref()
            .map(|t| t.progress.in_flight.clone())
            .ok_or_else(|| "no task".to_string())
    };
    let finish = |sim: &mut Sim, spec: &EffectSpec, run: RunOutcome, chunks: Option<u32>| {
        let effect = spec.effect().clone();
        let outcome = match spec {
            EffectSpec::Verify { .. } => EffectOutcome::Verify {
                outcome: run,
                evidence: art(&format!("v-{}", effect.as_str())),
            },
            EffectSpec::Checkpoint { .. } => EffectOutcome::Checkpoint {
                outcome: run,
                commit: Some(sha(2)?),
                diff: Some(art("diff")),
            },
            EffectSpec::AgentRun { .. } => EffectOutcome::AgentRun {
                outcome: run,
                transcript: art("t"),
                review: None,
            },
        };
        let cmd = Command::RecordEffectOutcome {
            effect: effect.clone(),
            outcome,
        };
        sim.submit(Actor::Effect { effect }, cmd, None, chunks, 4)
    };
    let implementer = in_flight(&sim)?;
    finish(&mut sim, &implementer[0], RunOutcome::Succeeded, None)?;
    let checkpoint = in_flight(&sim)?;
    finish(&mut sim, &checkpoint[0], RunOutcome::Succeeded, None)?;
    let verifies = in_flight(&sim)?;
    for (i, spec) in verifies.iter().enumerate() {
        let run = if i == lost {
            RunOutcome::Failed {
                code: FailureCode::LostOnRestart,
            }
        } else {
            RunOutcome::Succeeded
        };
        finish(&mut sim, spec, run, Some(chunks))?;
    }
    sim.run(4)?;
    let retry = in_flight(&sim)?;
    if retry.len() != 1 {
        return Err(format!("expected one retried verify, got {}", retry.len()));
    }
    finish(&mut sim, &retry[0], RunOutcome::Succeeded, Some(chunks))?;
    let task = sim.task.as_ref().ok_or("no task")?;
    let repaired = sim
        .events()
        .any(|e| matches!(e, Event::RepairRoundStarted { .. }));
    if repaired || task.round != 0 {
        return Err("a lost verify with a passing retry started a repair round".into());
    }
    let reviewing = task.state == TaskState::Reviewing
        && !task.progress.in_flight.is_empty()
        && task.progress.in_flight.iter().all(|s| {
            matches!(
                s,
                EffectSpec::AgentRun {
                    role: Role::Reviewer,
                    ..
                }
            )
        });
    if reviewing {
        Ok(())
    } else {
        Err(format!("expected review, got state {:?}", task.state))
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn p1_verified_only_through_accept_of_last_checkpoint(steps in steps()) {
        if let Err(m) = simulate(&steps, p1) { prop_assert!(false, "{}", m); }
    }

    #[test]
    fn p2_review_clean_only_after_passing_verification(steps in steps()) {
        if let Err(m) = simulate(&steps, p2) { prop_assert!(false, "{}", m); }
    }

    #[test]
    fn p3_repair_rounds_bounded_by_budget_plus_send_backs(steps in steps()) {
        if let Err(m) = simulate(&steps, p3) { prop_assert!(false, "{}", m); }
    }

    #[test]
    fn p4_apply_never_reports_corrupt_for_decided_events(steps in steps()) {
        if let Err(m) = simulate(&steps, p4) { prop_assert!(false, "{}", m); }
    }

    #[test]
    fn p5_lost_verify_with_passing_retry_reaches_review(lost in 0usize..2, chunks in 1u32..4) {
        if let Err(m) = lost_verify_then_passing_retry(lost, chunks) { prop_assert!(false, "{}", m); }
    }
}
