//! harness-store: receipts, the hash chain, artifacts, crash consistency, the engine lock.

#![allow(clippy::unwrap_used)]

use std::os::unix::process::ExitStatusExt as _;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

use harness_core::artifact::{MediaType, Provenance};
use harness_core::decide::{
    Command, Decision, EffectOutcome, Event, Facts, Rejection, Task, decide,
};
use harness_core::finding::Severity;
use harness_core::ids::{ArtifactId, CommitSha, EffectId, IdempotencyKey, Seq, TaskId, Timestamp};
use harness_core::lifecycle::{DecisionKind, Subject, TaskState};
use harness_core::outcome::RunOutcome;
use harness_core::policy::{Pipeline, StageRoute};
use harness_core::types::{Actor, Budget, Envelope, RepoRef, Role, RuntimeKind, WIRE_VERSION};
use harness_store::{ChainBroken, OutboxState, Store, StoreError};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let p = std::env::temp_dir().join(format!(
            "eng-store-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&p);
        TempDir(p)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn pipeline() -> Pipeline {
    Pipeline {
        implement: StageRoute {
            runtime: RuntimeKind::Claude,
            role: Role::Implementer,
        },
        review: StageRoute {
            runtime: RuntimeKind::Codex,
            role: Role::Reviewer,
        },
        verify_commands: vec![vec!["true".into()]],
        budget: Budget {
            max_repair_rounds: 2,
            run_deadline_ms: 60_000,
            max_turns: 10,
        },
        severity_threshold: Severity::High,
        review_chunk_max_lines: 400,
    }
}

const TASK: &str = "task-1";

fn packet() -> ArtifactId {
    ArtifactId::of(b"packet")
}

fn env(id: &str, actor: Actor, body: Command) -> Envelope<Command> {
    Envelope {
        v: WIRE_VERSION,
        id: id.into(),
        task: TaskId::new(TASK),
        at: Timestamp(1_700_000_000_000_000),
        actor,
        body,
    }
}

fn human() -> Actor {
    Actor::Human {
        principal: "owner".into(),
    }
}

fn create(id: &str, key: &str) -> Envelope<Command> {
    env(
        id,
        human(),
        Command::CreateTask {
            repo: RepoRef {
                path: "/repo".into(),
                git_common_dir: "/repo/.git".into(),
            },
            base_commit: CommitSha::parse(&"a".repeat(40)).unwrap_or_else(|e| panic!("{e}")),
            packet: packet(),
            pipeline: ArtifactId::of(b"pipeline"),
            key: IdempotencyKey::new(key),
        },
    )
}

fn approve(id: &str) -> Envelope<Command> {
    env(
        id,
        human(),
        Command::RecordHumanDecision {
            kind: DecisionKind::ApproveSpec,
            subject: Subject::PacketDigest(packet()),
            key: IdempotencyKey::new(format!("k-{id}")),
        },
    )
}

fn run(id: &str, key: &str) -> Envelope<Command> {
    env(
        id,
        human(),
        Command::Run {
            key: IdempotencyKey::new(key),
        },
    )
}

fn submit(store: &Store, cmd: Envelope<Command>) -> Result<harness_store::Receipt, StoreError> {
    let p = pipeline();
    let ids: Vec<EffectId> = (1..=8).map(|i| EffectId::new(format!("eff-{i}"))).collect();
    let decide_fn = |t: Option<&Task>, c: &Envelope<Command>| -> Result<Decision, Rejection> {
        decide(
            t,
            c,
            &Facts {
                pipeline: &p,
                review: None,
                diff_chunks: None,
                new_effect_ids: &ids,
            },
        )
    };
    store.submit(cmd, &decide_fn)
}

fn ok_submit(store: &Store, cmd: Envelope<Command>) -> harness_store::Receipt {
    let r = submit(store, cmd).unwrap_or_else(|e| panic!("submit: {e}"));
    assert!(r.accepted, "rejected: {:?}", r.rejection);
    r
}

fn open(dir: &Path) -> Store {
    Store::open(dir).unwrap_or_else(|e| panic!("open: {e}"))
}

fn raw(dir: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open(dir.join("state.db")).unwrap_or_else(|e| panic!("{e}"))
}

fn count(dir: &Path, sql: &str) -> i64 {
    raw(dir)
        .query_row(sql, [], |r| r.get(0))
        .unwrap_or_else(|e| panic!("{sql}: {e}"))
}

#[test]
fn retried_command_returns_same_receipt() {
    let d = TempDir::new("retry");
    let store = open(d.path());
    let first = ok_submit(&store, create("c1", "create"));
    // A retry may carry a new envelope id; the key decides.
    let again = ok_submit(&store, create("c1-retry", "create"));
    assert_eq!(first, again);
    assert_eq!(first.first_seq, Some(Seq(1)));
    assert_eq!(
        store.events_after(Seq(0), 100).map(|e| e.len()).ok(),
        Some(2)
    );
    drop(store);
    assert_eq!(count(d.path(), "SELECT COUNT(*) FROM commands"), 1);
}

#[test]
fn same_key_different_body_is_rejected() {
    let d = TempDir::new("conflict");
    let store = open(d.path());
    ok_submit(&store, create("c1", "k"));
    ok_submit(&store, approve("c2"));
    // `Run` with the key of `CreateTask`.
    match submit(&store, run("c3", "k")) {
        Err(StoreError::IdempotencyConflict { .. }) => {}
        other => panic!("expected IdempotencyConflict, got {other:?}"),
    }
    // Nothing was written for it.
    let task = store.task(&TaskId::new(TASK)).ok().flatten();
    assert_eq!(task.map(|t| t.state), Some(TaskState::Approved));
}

#[test]
fn events_are_gapless_and_chain_verifies() {
    let d = TempDir::new("chain");
    let store = open(d.path());
    ok_submit(&store, create("c1", "create"));
    ok_submit(&store, approve("c2"));
    let r = ok_submit(&store, run("c3", "run"));
    let events = store
        .events_after(Seq(0), 1000)
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(events.len() >= 5);
    let mut prev = [0u8; 32];
    for (i, e) in events.iter().enumerate() {
        assert_eq!(e.seq, Seq(i as u64 + 1), "gapless");
        assert_eq!(e.task_seq, i as u64 + 1);
        assert_eq!(e.prev_hash, prev, "linked");
        prev = e.hash;
    }
    assert_eq!(r.last_seq, Some(Seq(events.len() as u64)));
    assert_eq!(store.verify_chain(), Ok(Seq(events.len() as u64)));
    // The intended implementer run is in the outbox, at its event's seq.
    let pending = store.pending_effects().unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].state, OutboxState::Intended);
    let intended = events
        .iter()
        .find(|e| matches!(e.event.body, Event::EffectIntended { .. }))
        .map(|e| e.seq);
    assert_eq!(Some(pending[0].intended_seq), intended);
    // Paging.
    let tail = store
        .events_after(Seq(2), 2)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        tail.iter().map(|e| e.seq).collect::<Vec<_>>(),
        [Seq(3), Seq(4)]
    );
}

#[test]
fn update_or_delete_of_events_is_refused_by_trigger() {
    let d = TempDir::new("trigger");
    let store = open(d.path());
    ok_submit(&store, create("c1", "create"));
    let conn = raw(d.path());
    let upd = conn.execute("UPDATE events SET body = '{}' WHERE seq = 1", []);
    assert!(
        upd.as_ref()
            .is_err_and(|e| e.to_string().contains("append-only")),
        "{upd:?}"
    );
    let del = conn.execute("DELETE FROM events", []);
    assert!(
        del.as_ref()
            .is_err_and(|e| e.to_string().contains("append-only")),
        "{del:?}"
    );
    drop(conn);
    assert_eq!(store.verify_chain(), Ok(Seq(2)));
}

#[test]
fn tampered_event_body_breaks_chain_at_that_seq() {
    let d = TempDir::new("tamper");
    let store = open(d.path());
    ok_submit(&store, create("c1", "create"));
    ok_submit(&store, approve("c2"));
    let conn = raw(d.path());
    conn.execute_batch("DROP TRIGGER events_no_update")
        .unwrap_or_else(|e| panic!("{e}"));
    let body: String = conn
        .query_row("SELECT body FROM events WHERE seq = 3", [], |r| r.get(0))
        .unwrap_or_else(|e| panic!("{e}"));
    let tampered = body.replace("approve_spec", "accept");
    assert_ne!(body, tampered);
    conn.execute(
        "UPDATE events SET body = ?1 WHERE seq = 3",
        [tampered.as_str()],
    )
    .unwrap_or_else(|e| panic!("{e}"));
    drop(conn);
    match store.verify_chain() {
        Err(ChainBroken::At { seq, .. }) => assert_eq!(seq, Seq(3)),
        other => panic!("expected a break at 3, got {other:?}"),
    }
}

#[test]
fn corrupted_artifact_is_detected_on_read() {
    let d = TempDir::new("artifact");
    let store = open(d.path());
    let id = store
        .put_artifact(
            b"evidence bytes",
            MediaType::Text,
            Provenance::HarnessEvidence,
            None,
        )
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(id, ArtifactId::of(b"evidence bytes"));
    assert_eq!(
        store.get_artifact(&id).ok().as_deref(),
        Some(&b"evidence bytes"[..])
    );
    let meta = store.artifact_refs(&id).unwrap_or_default();
    assert_eq!(
        meta.iter().map(|a| (a.size, a.label)).collect::<Vec<_>>(),
        [(14, Provenance::HarnessEvidence)]
    );
    // Flip the stored bytes behind the store's back.
    let hex = id.as_str();
    let file = d.path().join("objects").join(&hex[..2]).join(hex);
    std::fs::write(&file, b"evidence bytez").unwrap_or_else(|e| panic!("{e}"));
    match store.get_artifact(&id) {
        Err(StoreError::CorruptArtifact(bad)) => assert_eq!(bad, id),
        other => panic!("expected CorruptArtifact, got {other:?}"),
    }
    // Storing the bytes again repairs the object.
    store
        .put_artifact(
            b"evidence bytes",
            MediaType::Text,
            Provenance::HarnessEvidence,
            None,
        )
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(store.get_artifact(&id).is_ok());
    // Unknown ids are not found.
    assert!(matches!(
        store.get_artifact(&ArtifactId::of(b"nope")),
        Err(StoreError::NotFound(_))
    ));
}

/// `submit` with effect ids `<prefix>-1..` and the given `diff_chunks` fact.
fn submit_facts(
    store: &Store,
    cmd: Envelope<Command>,
    prefix: &str,
    diff_chunks: Option<u32>,
) -> Result<harness_store::Receipt, StoreError> {
    let p = pipeline();
    let ids: Vec<EffectId> = (1..=8)
        .map(|i| EffectId::new(format!("{prefix}-{i}")))
        .collect();
    let decide_fn = |t: Option<&Task>, c: &Envelope<Command>| -> Result<Decision, Rejection> {
        decide(
            t,
            c,
            &Facts {
                pipeline: &p,
                review: None,
                diff_chunks,
                new_effect_ids: &ids,
            },
        )
    };
    store.submit(cmd, &decide_fn)
}

fn outcome(effect: &str, outcome: EffectOutcome) -> Envelope<Command> {
    env(
        &format!(
            "outcome-{effect}-{}",
            ArtifactId::of(format!("{outcome:?}").as_bytes())
        ),
        Actor::Effect {
            effect: EffectId::new(effect),
        },
        Command::RecordEffectOutcome {
            effect: EffectId::new(effect),
            outcome,
        },
    )
}

/// Labels belong to a reference, not to bytes: an agent that prints exactly the bytes of a
/// later verification output must not block the task, and its claim must not become
/// evidence.
#[test]
fn same_bytes_under_two_labels_are_two_references() {
    let d = TempDir::new("labels");
    let store = open(d.path());
    ok_submit(&store, create("c1", "create"));
    ok_submit(&store, approve("c2"));
    let r = submit_facts(&store, run("c3", "run"), "r", None).unwrap();
    assert!(r.accepted);
    // The implementer run r-1 prints bytes that the verification will print again later.
    let bytes = b"eng verification output\n";
    let id = store
        .put_artifact(
            bytes,
            MediaType::Jsonl,
            Provenance::RuntimeClaim,
            Some(&EffectId::new("r-1")),
        )
        .unwrap();
    let r = submit_facts(
        &store,
        outcome(
            "r-1",
            EffectOutcome::AgentRun {
                outcome: RunOutcome::Succeeded,
                transcript: id.clone(),
                review: None,
            },
        ),
        "i",
        None,
    )
    .unwrap();
    assert!(r.accepted, "{:?}", r.rejection);
    let r = submit_facts(
        &store,
        outcome(
            "i-1",
            EffectOutcome::Checkpoint {
                outcome: RunOutcome::Succeeded,
                commit: Some(CommitSha::parse(&"b".repeat(40)).unwrap()),
                diff: None,
            },
        ),
        "c",
        None,
    )
    .unwrap();
    assert!(r.accepted, "{:?}", r.rejection);
    let verify = EffectOutcome::Verify {
        outcome: RunOutcome::Succeeded,
        evidence: id.clone(),
    };
    // Cited as the verification's evidence while only the agent's claim carries the bytes.
    let refused = submit_facts(&store, outcome("c-1", verify.clone()), "v", Some(0));
    match refused {
        Err(StoreError::Unlabelled { id: bad, label, by }) => {
            assert_eq!(bad, id);
            assert_eq!(label, Provenance::HarnessEvidence);
            assert_eq!(by, Some(EffectId::new("c-1")));
        }
        other => panic!("expected Unlabelled, got {other:?}"),
    }
    assert_eq!(
        store
            .task(&TaskId::new(TASK))
            .unwrap()
            .unwrap()
            .progress
            .evidence,
        Vec::<ArtifactId>::new()
    );
    // The verification then stores the same bytes as its own evidence.
    let again = store
        .put_artifact(
            bytes,
            MediaType::Text,
            Provenance::HarnessEvidence,
            Some(&EffectId::new("c-1")),
        )
        .unwrap();
    assert_eq!(again, id);
    let r = submit_facts(&store, outcome("c-1", verify), "v", Some(0)).unwrap();
    assert!(r.accepted, "{:?}", r.rejection);
    let task = store.task(&TaskId::new(TASK)).unwrap().unwrap();
    assert_eq!(
        task.run,
        harness_core::stage::StageRunState::AwaitingHuman {
            gate: harness_core::stage::HumanGate::ReviewClean
        }
    );
    // One object, two references; the claim is still only a claim.
    let refs: Vec<(Provenance, Option<EffectId>)> = store
        .artifact_refs(&id)
        .unwrap()
        .into_iter()
        .map(|a| (a.label, a.produced_by))
        .collect();
    assert_eq!(
        refs,
        [
            (Provenance::RuntimeClaim, Some(EffectId::new("r-1"))),
            (Provenance::HarnessEvidence, Some(EffectId::new("c-1"))),
        ]
    );
    let r1 = EffectId::new("r-1");
    let c1 = EffectId::new("c-1");
    assert!(
        !store
            .has_label(&id, Provenance::HarnessEvidence, Some(&r1))
            .unwrap()
    );
    assert!(
        store
            .has_label(&id, Provenance::HarnessEvidence, Some(&c1))
            .unwrap()
    );
    assert!(
        store
            .has_label(&id, Provenance::RuntimeClaim, Some(&r1))
            .unwrap()
    );
    let objects = std::fs::read_dir(d.path().join("objects").join(&id.as_str()[..2]))
        .unwrap()
        .count();
    assert_eq!(objects, 1, "bytes are stored once");
}

#[test]
fn second_engine_refuses_to_start() {
    let d = TempDir::new("lock");
    let first = open(d.path());
    assert!(matches!(Store::open(d.path()), Err(StoreError::Locked)));
    drop(first);
    let again = Store::open(d.path());
    assert!(
        again.is_ok(),
        "lock is released when the first store closes"
    );
}

#[test]
fn state_dir_and_database_are_private() {
    use std::os::unix::fs::PermissionsExt as _;
    let d = TempDir::new("perms");
    let _store = open(d.path());
    let mode = |p: &Path| {
        std::fs::metadata(p)
            .map(|m| m.permissions().mode() & 0o777)
            .unwrap_or(0)
    };
    assert_eq!(mode(d.path()), 0o700);
    assert_eq!(mode(&d.path().join("state.db")), 0o600);
}

const CHILD_DIR: &str = "ENG_STORE_CRASH_CHILD_DIR";
const RUN_KEY: &str = "run-after-crash";
const PAYLOAD: &[u8] = b"artifact written by the crashing child";

/// Not a test of its own: the body of the child process `abort_after_each_statement...`
/// starts. Without the environment variable it does nothing.
#[test]
fn crash_child() {
    let Ok(dir) = std::env::var(CHILD_DIR) else {
        return;
    };
    let store = open(Path::new(&dir));
    store
        .put_artifact(PAYLOAD, MediaType::Text, Provenance::Input, None)
        .unwrap_or_else(|e| panic!("{e}"));
    ok_submit(&store, run("c-run", RUN_KEY));
    // The failpoint should have killed this process before it gets here.
    std::process::exit(3);
}

#[test]
fn abort_after_each_statement_then_reopen_is_consistent() {
    const POINTS: &[&str] = &[
        "artifact:after_file",
        "artifact:after_index",
        "submit:begin",
        "submit:after_receipt_check",
        "submit:after_receipt_insert",
        "submit:after_events",
        "submit:after_outbox",
        "submit:after_projection",
        "submit:after_commit",
    ];
    for point in POINTS {
        let d = TempDir::new("crash");
        {
            let store = open(d.path());
            ok_submit(&store, create("c1", "create"));
            ok_submit(&store, approve("c2"));
        }
        let exe = std::env::current_exe().unwrap_or_else(|e| panic!("{e}"));
        let status = Process::new(exe)
            .args([
                "crash_child",
                "--exact",
                "--nocapture",
                "--test-threads",
                "1",
            ])
            .env(CHILD_DIR, d.path())
            .env("ENG_FAILPOINT", point)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            status.signal(),
            Some(9),
            "{point}: child was not killed there"
        );

        let store = open(d.path());
        let head = store
            .verify_chain()
            .unwrap_or_else(|e| panic!("{point}: {e}"));
        let r1 = ok_submit(&store, run("c-run", RUN_KEY));
        let r2 = ok_submit(&store, run("c-run-again", RUN_KEY));
        assert_eq!(r1, r2, "{point}");
        let task = store.task(&TaskId::new(TASK)).ok().flatten();
        assert_eq!(
            task.map(|t| t.state),
            Some(TaskState::Implementing),
            "{point}"
        );
        let pending = store.pending_effects().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(pending.len(), 1, "{point}: exactly one intended effect");
        let after = store.verify_chain().unwrap_or_else(|e| panic!("{e}"));
        assert!(after >= head, "{point}");
        // The artifact is either fully there or absent; storing it again always works.
        store
            .put_artifact(PAYLOAD, MediaType::Text, Provenance::Input, None)
            .unwrap_or_else(|e| panic!("{point}: {e}"));
        assert_eq!(
            store.get_artifact(&ArtifactId::of(PAYLOAD)).ok().as_deref(),
            Some(PAYLOAD)
        );
        drop(store);
        let key = format!("key:{RUN_KEY}");
        assert_eq!(
            count(
                d.path(),
                &format!("SELECT COUNT(*) FROM commands WHERE idempotency_key = '{key}'")
            ),
            1,
            "{point}: one receipt"
        );
    }
}
