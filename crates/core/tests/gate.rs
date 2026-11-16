//! The open-question table (#22): every question × every answer, and subject binding.

use harness_core::decide::{Checkpoint, Progress, Rejection, Task};
use harness_core::gate::{Answer, OpenQuestion, check, subject};
use harness_core::ids::{ArtifactId, CommitSha, TaskId};
use harness_core::lifecycle::{DecisionKind, Denied, Subject, TaskState};
use harness_core::stage::{HumanGate, StageRunState};
use harness_core::types::RepoRef;

const ANSWERS: [Answer; 5] = [
    Answer::Decide(DecisionKind::ApproveSpec),
    Answer::Decide(DecisionKind::Accept),
    Answer::Decide(DecisionKind::Reject),
    Answer::Decide(DecisionKind::SendBack),
    Answer::Run,
];

fn sha(n: u64) -> CommitSha {
    match CommitSha::parse(&format!("{n:040x}")) {
        Ok(c) => c,
        Err(e) => panic!("bad sha: {e:?}"),
    }
}

fn task(state: TaskState, run: StageRunState, checkpoints: u32) -> Task {
    Task {
        id: TaskId::new("t1"),
        repo: RepoRef {
            path: "/repo".into(),
            git_common_dir: "/repo/.git".into(),
        },
        packet: ArtifactId::of(b"packet"),
        pipeline: ArtifactId::of(b"pipeline"),
        base_commit: sha(1),
        state,
        run,
        round: 0,
        checkpoints: (1..=checkpoints)
            .map(|n| Checkpoint {
                n,
                commit: sha(u64::from(n) + 1),
            })
            .collect(),
        version: 0,
        progress: Progress::default(),
    }
}

fn awaiting(gate: HumanGate) -> Task {
    task(
        TaskState::Reviewing,
        StageRunState::AwaitingHuman { gate },
        2,
    )
}

#[test]
fn every_question_allows_exactly_its_answers() {
    use Answer::{Decide, Run};
    use DecisionKind::{Accept, ApproveSpec, Reject, SendBack};
    let table: [(OpenQuestion, &[Answer]); 5] = [
        (OpenQuestion::SpecApproval, &[Decide(ApproveSpec)]),
        (
            OpenQuestion::Gate(HumanGate::ReviewClean),
            &[Decide(Accept), Decide(Reject), Decide(SendBack)],
        ),
        (
            OpenQuestion::Gate(HumanGate::RepairLimitReached),
            &[Decide(Reject), Decide(SendBack)],
        ),
        (
            OpenQuestion::Gate(HumanGate::LimitExhausted),
            &[Run, Decide(Reject)],
        ),
        (
            OpenQuestion::Gate(HumanGate::LostOnRestart),
            &[Run, Decide(Reject)],
        ),
    ];
    for (q, legal) in table {
        for a in ANSWERS {
            assert_eq!(q.allows(a), legal.contains(&a), "{q:?} × {a:?}");
        }
    }
}

#[test]
fn accept_is_legal_only_at_review_clean() {
    for gate in [
        HumanGate::ReviewClean,
        HumanGate::RepairLimitReached,
        HumanGate::LimitExhausted,
        HumanGate::LostOnRestart,
    ] {
        let t = awaiting(gate);
        let r = check(
            &t,
            Answer::Decide(DecisionKind::Accept),
            Some(&Subject::Commit(sha(3))),
        );
        if gate == HumanGate::ReviewClean {
            assert_eq!(r, Ok(OpenQuestion::Gate(gate)));
        } else {
            assert_eq!(r, Err(Rejection::Denied(Denied::WrongKind)), "{gate:?}");
        }
    }
}

#[test]
fn illegal_run_is_wrong_state_and_illegal_decision_is_wrong_kind() {
    let t = awaiting(HumanGate::ReviewClean);
    assert_eq!(check(&t, Answer::Run, None), Err(Rejection::WrongState));
    let t = awaiting(HumanGate::LimitExhausted);
    assert_eq!(
        check(&t, Answer::Decide(DecisionKind::SendBack), None),
        Err(Rejection::Denied(Denied::WrongKind))
    );
}

#[test]
fn no_open_question_is_wrong_state() {
    let t = task(TaskState::Approved, StageRunState::Pending, 0);
    assert_eq!(OpenQuestion::of(&t), None);
    for a in ANSWERS {
        assert_eq!(check(&t, a, None), Err(Rejection::WrongState), "{a:?}");
    }
    let t = task(
        TaskState::Implementing,
        StageRunState::EffectsInFlight { effects: vec![] },
        0,
    );
    assert_eq!(OpenQuestion::of(&t), None);
}

#[test]
fn spec_approval_is_bound_to_the_packet() {
    let t = task(TaskState::Planned, StageRunState::Pending, 0);
    let q = OpenQuestion::of(&t);
    assert_eq!(q, Some(OpenQuestion::SpecApproval));
    let packet = Subject::PacketDigest(ArtifactId::of(b"packet"));
    assert_eq!(subject(&t, OpenQuestion::SpecApproval), packet);
    let approve = Answer::Decide(DecisionKind::ApproveSpec);
    assert!(check(&t, approve, Some(&packet)).is_ok());
    assert_eq!(
        check(
            &t,
            approve,
            Some(&Subject::PacketDigest(ArtifactId::of(b"other")))
        ),
        Err(Rejection::Denied(Denied::WrongSubject))
    );
}

#[test]
fn gate_answers_are_bound_to_the_last_checkpoint_or_the_packet() {
    let t = awaiting(HumanGate::ReviewClean);
    let q = OpenQuestion::Gate(HumanGate::ReviewClean);
    assert_eq!(subject(&t, q), Subject::Commit(sha(3)));
    let reject = Answer::Decide(DecisionKind::Reject);
    assert_eq!(
        check(&t, reject, Some(&Subject::Commit(sha(2)))),
        Err(Rejection::Denied(Denied::WrongSubject))
    );
    let early = task(
        TaskState::Implementing,
        StageRunState::AwaitingHuman {
            gate: HumanGate::LostOnRestart,
        },
        0,
    );
    let q = OpenQuestion::Gate(HumanGate::LostOnRestart);
    assert_eq!(
        subject(&early, q),
        Subject::PacketDigest(ArtifactId::of(b"packet"))
    );
}
