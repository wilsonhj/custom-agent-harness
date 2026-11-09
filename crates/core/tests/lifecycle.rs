//! Task lifecycle against the TypeScript oracle (`conformance/golden/transitions.json`).
//!
//! Golden mapping. `scripts/export-transitions.mjs` records what the TS `canTransition`
//! returns. The TS approval policy is unimplemented, so it denies every transition except
//! entering `Blocked`. The Rust design replaces that policy with the `HumanDecision` gate, so
//! the matrix check is: for every golden row with `approval = false`, if `(from, to)` is one of
//! Task 3's ungated transitions, `can_transition(from, to, None, ctx)` must be `Ok`; otherwise
//! `is_ok()` must equal the golden `allowed`.
//!
//! The golden `blocked_record` flag maps onto `ctx.blocked_from` the way the export script
//! builds its context: `false` means no record (`None`); `true` means a record that entered
//! `Blocked` from `to` when `to` is in TS `RESUMABLE_STATES`, otherwise from `Draft` (the
//! mismatching case). The script pins that record to the same commit it passes as
//! `currentCommitSha`, so the commit check always passes there and has no Rust counterpart.
//! Rows with `approval = true` exercise the TS approval-record shape, which Rust does not
//! have; the decision gate is tested directly below.

use std::collections::{BTreeMap, BTreeSet};

use harness_core::ids::{ArtifactId, CommitSha, EffectId, Seq};
use harness_core::lifecycle::{
    ALLOWED, DecisionKind, Denied, HumanDecision, Subject, TaskState, TransitionContext,
    can_transition,
};
use harness_core::outcome::FailureCode;
use harness_core::stage::{HumanGate, StageRunState};
use serde::Deserialize;

use TaskState::*;

const GOLDEN: &str = include_str!("../../../conformance/golden/transitions.json");

const ALL: [TaskState; 16] = [
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
    Blocked,
    Failed,
    Cancelled,
    Repair,
    Superseded,
];

/// `RESUMABLE_STATES` in `harness/policy/transitions.ts`.
const RESUMABLE: [TaskState; 11] = [
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
    Repair,
];

/// Task 3 "Rules": engine-driven edges that need no decision (plus any listed edge into
/// `Failed`, `Cancelled` or `Blocked`, see `is_ungated`).
const ENGINE_DRIVEN: [(TaskState, TaskState); 5] = [
    (Draft, Planned),
    (Approved, Implementing),
    (Implementing, Reviewing),
    (Reviewing, Repair),
    (Repair, Reviewing),
];

#[derive(Deserialize)]
struct Golden {
    states: Vec<String>,
    allowed: BTreeMap<String, Vec<String>>,
    matrix: Vec<Row>,
}

#[derive(Deserialize)]
struct Row {
    from: String,
    to: String,
    approval: bool,
    blocked_record: bool,
    allowed: bool,
}

fn golden() -> Golden {
    match serde_json::from_str(GOLDEN) {
        Ok(g) => g,
        Err(e) => panic!("golden transitions.json does not parse: {e}"),
    }
}

/// TS names are the Rust variant names.
fn name(s: TaskState) -> String {
    format!("{s:?}")
}

fn state(n: &str) -> TaskState {
    match ALL.into_iter().find(|s| name(*s) == n) {
        Some(s) => s,
        None => panic!("golden names unknown state {n:?}"),
    }
}

fn listed(from: TaskState, to: TaskState) -> bool {
    ALLOWED
        .iter()
        .any(|(f, tos)| *f == from && tos.contains(&to))
}

fn is_ungated(from: TaskState, to: TaskState) -> bool {
    ENGINE_DRIVEN.contains(&(from, to))
        || (matches!(to, Failed | Cancelled | Blocked) && listed(from, to))
}

fn digest(byte: u8) -> ArtifactId {
    ArtifactId::of(&[byte])
}

fn commit(c: char) -> CommitSha {
    let s: String = std::iter::repeat_n(c, 40).collect();
    match CommitSha::parse(&s) {
        Ok(c) => c,
        Err(e) => panic!("valid sha rejected: {e}"),
    }
}

fn decision(kind: DecisionKind, subject: Subject) -> HumanDecision {
    HumanDecision::from_event(kind, subject, "owner".into(), Seq(7))
}

fn ctx() -> TransitionContext {
    TransitionContext {
        packet: Some(digest(1)),
        candidate: Some(commit('a')),
        blocked_from: None,
    }
}

#[test]
fn allowed_table_equals_golden() {
    let g = golden();
    assert_eq!(ALLOWED.len(), 16, "one entry per state");
    let ours: BTreeMap<String, Vec<String>> = ALLOWED
        .iter()
        .map(|(from, tos)| {
            let mut tos: Vec<String> = tos.iter().map(|t| name(*t)).collect();
            tos.sort();
            (name(*from), tos)
        })
        .collect();
    assert_eq!(ours, g.allowed);
    let states: BTreeSet<String> = ALL.iter().map(|s| name(*s)).collect();
    assert_eq!(states, g.states.into_iter().collect::<BTreeSet<_>>());
}

#[test]
fn without_a_decision_matrix_matches_golden_for_ungated_and_denies_gated() {
    let mut checked = 0;
    for row in golden().matrix.iter().filter(|r| !r.approval) {
        let (from, to) = (state(&row.from), state(&row.to));
        let blocked_from = row
            .blocked_record
            .then(|| if RESUMABLE.contains(&to) { to } else { Draft });
        let ctx = TransitionContext {
            packet: None,
            candidate: None,
            blocked_from,
        };
        let got = can_transition(from, to, None, &ctx);
        if is_ungated(from, to) {
            assert_eq!(
                got,
                Ok(()),
                "{from:?} -> {to:?} (blocked_from {blocked_from:?})"
            );
        } else {
            assert_eq!(
                got.is_ok(),
                row.allowed,
                "{from:?} -> {to:?} (blocked_from {blocked_from:?}): {got:?}"
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 512);
}

#[test]
fn approve_spec_for_the_packet_approves() {
    let d = decision(DecisionKind::ApproveSpec, Subject::PacketDigest(digest(1)));
    assert_eq!(can_transition(Planned, Approved, Some(&d), &ctx()), Ok(()));
}

#[test]
fn approve_spec_for_other_digest_is_wrong_subject() {
    let d = decision(DecisionKind::ApproveSpec, Subject::PacketDigest(digest(2)));
    assert_eq!(
        can_transition(Planned, Approved, Some(&d), &ctx()),
        Err(Denied::WrongSubject)
    );
    let no_packet = TransitionContext {
        packet: None,
        ..ctx()
    };
    let d = decision(DecisionKind::ApproveSpec, Subject::PacketDigest(digest(1)));
    assert_eq!(
        can_transition(Planned, Approved, Some(&d), &no_packet),
        Err(Denied::WrongSubject)
    );
}

#[test]
fn accept_for_the_candidate_verifies() {
    let d = decision(DecisionKind::Accept, Subject::Commit(commit('a')));
    assert_eq!(
        can_transition(Reviewing, Verified, Some(&d), &ctx()),
        Ok(())
    );
}

#[test]
fn accept_for_other_commit_is_wrong_subject() {
    let d = decision(DecisionKind::Accept, Subject::Commit(commit('b')));
    assert_eq!(
        can_transition(Reviewing, Verified, Some(&d), &ctx()),
        Err(Denied::WrongSubject)
    );
    // The right kind naming a digest instead of a commit is also the wrong subject.
    let d = decision(DecisionKind::Accept, Subject::PacketDigest(digest(1)));
    assert_eq!(
        can_transition(Reviewing, Verified, Some(&d), &ctx()),
        Err(Denied::WrongSubject)
    );
}

#[test]
fn accept_cannot_approve_a_spec() {
    let d = decision(DecisionKind::Accept, Subject::PacketDigest(digest(1)));
    assert_eq!(
        can_transition(Planned, Approved, Some(&d), &ctx()),
        Err(Denied::WrongKind)
    );
    let d = decision(DecisionKind::ApproveSpec, Subject::Commit(commit('a')));
    assert_eq!(
        can_transition(Reviewing, Verified, Some(&d), &ctx()),
        Err(Denied::WrongKind)
    );
}

#[test]
fn gated_transitions_without_a_decision_require_approval() {
    for (from, to) in [(Planned, Approved), (Reviewing, Verified)] {
        assert_eq!(
            can_transition(from, to, None, &ctx()),
            Err(Denied::ApprovalRequired),
            "{from:?} -> {to:?}"
        );
    }
}

#[test]
fn unlisted_transition_is_not_in_table() {
    let d = decision(DecisionKind::Accept, Subject::Commit(commit('a')));
    assert_eq!(
        can_transition(Implementing, Verified, Some(&d), &ctx()),
        Err(Denied::NotInTable)
    );
    assert_eq!(
        can_transition(Draft, Approved, None, &ctx()),
        Err(Denied::NotInTable)
    );
}

#[test]
fn release_path_states_are_unreachable() {
    let accept = decision(DecisionKind::Accept, Subject::Commit(commit('a')));
    for to in [PreviewVerified, MigrationGated, Released, Observed, Closed] {
        for from in ALL {
            for d in [None, Some(&accept)] {
                let ctx = TransitionContext {
                    blocked_from: Some(to),
                    ..ctx()
                };
                assert_eq!(
                    can_transition(from, to, d, &ctx),
                    Err(Denied::Unreachable),
                    "{from:?} -> {to:?}"
                );
            }
        }
    }
}

#[test]
fn blocked_resumes_only_to_the_state_it_was_entered_from() {
    // TS: Blocked -> X (X not Cancelled/Superseded) is a resume; the record must say it entered
    // Blocked from exactly X, and then the transition still needs an approval.
    for to in [
        Draft,
        Planned,
        Approved,
        Implementing,
        Reviewing,
        Verified,
        Repair,
    ] {
        let none = TransitionContext {
            blocked_from: None,
            ..ctx()
        };
        assert_eq!(
            can_transition(Blocked, to, None, &none),
            Err(Denied::NotInTable),
            "{to:?}"
        );

        let other = if to == Draft { Planned } else { Draft };
        let mismatched = TransitionContext {
            blocked_from: Some(other),
            ..ctx()
        };
        assert_eq!(
            can_transition(Blocked, to, None, &mismatched),
            Err(Denied::NotInTable),
            "{to:?} from {other:?}"
        );

        let matching = TransitionContext {
            blocked_from: Some(to),
            ..ctx()
        };
        assert_eq!(
            can_transition(Blocked, to, None, &matching),
            Err(Denied::ApprovalRequired),
            "{to:?}"
        );
        // No decision kind in this slice authorises a resume, so it stays fail closed.
        for kind in [
            DecisionKind::ApproveSpec,
            DecisionKind::Accept,
            DecisionKind::Reject,
            DecisionKind::SendBack,
        ] {
            let d = decision(kind, Subject::Commit(commit('a')));
            assert_eq!(
                can_transition(Blocked, to, Some(&d), &matching),
                Err(Denied::WrongKind),
                "{to:?} {kind:?}"
            );
        }
    }
    // Leaving Blocked by cancelling needs neither a record nor a decision.
    assert_eq!(can_transition(Blocked, Cancelled, None, &ctx()), Ok(()));
    // A record naming a non-resumable state never resumes.
    let failed = TransitionContext {
        blocked_from: Some(Failed),
        ..ctx()
    };
    assert_eq!(
        can_transition(Blocked, Failed, None, &failed),
        Err(Denied::NotInTable)
    );
}

#[test]
fn entering_blocked_never_needs_a_decision() {
    for from in ALL {
        let expected = if listed(from, Blocked) {
            Ok(())
        } else {
            Err(Denied::NotInTable)
        };
        assert_eq!(
            can_transition(from, Blocked, None, &ctx()),
            expected,
            "{from:?}"
        );
    }
}

#[test]
fn decision_accessors_return_what_the_event_said() {
    let d = HumanDecision::from_event(
        DecisionKind::SendBack,
        Subject::Commit(commit('c')),
        "owner".into(),
        Seq(42),
    );
    assert_eq!(d.kind(), DecisionKind::SendBack);
    assert_eq!(d.subject(), &Subject::Commit(commit('c')));
}

#[test]
fn stage_run_state_wire_shape() {
    let s = StageRunState::EffectsInFlight {
        effects: vec![EffectId::new("e1")],
    };
    let json = match serde_json::to_value(&s) {
        Ok(v) => v,
        Err(e) => panic!("serialize: {e}"),
    };
    assert_eq!(
        json,
        serde_json::json!({ "type": "effects_in_flight", "effects": ["e1"] })
    );
    for s in [
        StageRunState::Pending,
        StageRunState::AwaitingHuman {
            gate: HumanGate::RepairLimitReached,
        },
        StageRunState::Failed {
            code: FailureCode::LostOnRestart,
        },
        StageRunState::Succeeded,
        StageRunState::Cancelled,
    ] {
        let text = match serde_json::to_string(&s) {
            Ok(t) => t,
            Err(e) => panic!("serialize: {e}"),
        };
        match serde_json::from_str::<StageRunState>(&text) {
            Ok(back) => assert_eq!(back, s),
            Err(e) => panic!("roundtrip {text}: {e}"),
        }
    }
}
