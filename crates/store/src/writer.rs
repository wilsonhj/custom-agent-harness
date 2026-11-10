//! Everything that runs on the writer thread.

use std::collections::HashMap;
use std::path::PathBuf;

use harness_core::artifact::{Artifact, MediaType, Provenance};
use harness_core::decide::{
    Command, Decision, EffectOutcome, EffectSpec, Event, Rejection, Task, apply,
};
use harness_core::ids::{ArtifactId, CommandId, EffectId, Seq, TaskId};
use harness_core::types::{Envelope, WIRE_VERSION};
use rusqlite::{Connection, OptionalExtension as _, Row, TransactionBehavior, params};
use serde_json::Value;

use crate::events::{GENESIS, canonical_json, chain_hash, hash32, sha256};
use crate::{
    EventRecord, OutboxRow, OutboxState, ProcessRecord, Receipt, StoreError, WorkspaceRow,
    wire_name,
};

/// Messages from the writer thread to the caller of `Store::submit`.
pub(crate) enum ToCaller {
    /// "Decide on this task." The writer waits for the answer inside its transaction.
    Ask(Option<Task>),
    Done(Result<Receipt, StoreError>),
}

/// Why [`crate::Store::verify_chain`] failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChainBroken {
    /// The event at `seq` does not continue the chain (wrong hash, wrong `prev_hash`, a gap).
    #[error("event chain broken at seq {}: {reason}", seq.0)]
    At { seq: Seq, reason: String },
    /// The log could not be read at all.
    #[error("event log unreadable: {0}")]
    Unreadable(String),
}

/// Test-only crash points (`failpoints` feature): when `ENG_FAILPOINT` names this point, the
/// process sends itself `SIGKILL` here. Without the feature this is a no-op.
#[cfg(feature = "failpoints")]
pub(crate) fn failpoint(name: &str) {
    if std::env::var("ENG_FAILPOINT").as_deref() == Ok(name) {
        let _ = std::process::Command::new("/bin/kill")
            .args(["-KILL", &std::process::id().to_string()])
            .status();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}

#[cfg(not(feature = "failpoints"))]
#[inline(always)]
pub(crate) fn failpoint(_name: &str) {}

fn corrupt(what: impl std::fmt::Display) -> StoreError {
    StoreError::Corrupt(what.to_string())
}

fn idempotency_key(cmd: &Command) -> String {
    match cmd {
        Command::CreateTask { key, .. }
        | Command::RecordHumanDecision { key, .. }
        | Command::Run { key }
        | Command::Cancel { key } => format!("key:{}", key.as_str()),
        Command::RecordEffectOutcome { effect, .. } => format!("effect:{}", effect.as_str()),
    }
}

fn effect_kind(spec: &EffectSpec) -> &'static str {
    match spec {
        EffectSpec::AgentRun { .. } => "agent_run",
        EffectSpec::Verify { .. } => "verify",
        EffectSpec::Checkpoint { .. } => "checkpoint",
    }
}

fn max_attempts(spec: &EffectSpec) -> u32 {
    match spec {
        EffectSpec::Verify { max_attempts, .. } => u32::from(*max_attempts).max(1),
        EffectSpec::AgentRun { .. } | EffectSpec::Checkpoint { .. } => 1,
    }
}

fn seq_i64(seq: u64) -> Result<i64, StoreError> {
    i64::try_from(seq).map_err(|_| corrupt("sequence overflow"))
}

fn opt_seq(v: Option<i64>) -> Option<Seq> {
    v.and_then(|n| u64::try_from(n).ok()).map(Seq)
}

pub(crate) fn load_task(conn: &Connection, id: &TaskId) -> Result<Option<Task>, StoreError> {
    let snapshot: Option<String> = conn
        .query_row(
            "SELECT snapshot FROM tasks WHERE id = ?1",
            [id.as_str()],
            |r| r.get(0),
        )
        .optional()?;
    snapshot
        .map(|s| serde_json::from_str(&s).map_err(StoreError::from))
        .transpose()
}

pub(crate) fn task_ids(conn: &mut Connection) -> Result<Vec<TaskId>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT t.id FROM tasks t ORDER BY (SELECT MIN(seq) FROM events e WHERE e.task_id = t.id)",
    )?;
    let ids = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids.into_iter().map(TaskId::new).collect())
}

fn receipt_from_row(r: &Row<'_>) -> rusqlite::Result<(Receipt, Vec<u8>)> {
    let status: String = r.get("status")?;
    Ok((
        Receipt {
            command: CommandId::new(r.get::<_, String>("id")?),
            accepted: status == "accepted",
            rejection: r.get("rejection")?,
            first_seq: opt_seq(r.get("first_seq")?),
            last_seq: opt_seq(r.get("last_seq")?),
        },
        r.get("body_hash")?,
    ))
}

/// The whole of `Store::submit` on the writer thread.
pub(crate) fn submit(
    conn: &mut Connection,
    cmd: &Envelope<Command>,
    ask: &dyn Fn(Option<Task>) -> Option<Result<Decision, Rejection>>,
) -> Result<Receipt, StoreError> {
    let key = idempotency_key(&cmd.body);
    let body = canonical_json(&serde_json::json!({
        "actor": serde_json::to_value(&cmd.actor)?,
        "body": serde_json::to_value(&cmd.body)?,
    }));
    let body_hash = sha256(body.as_bytes());

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    failpoint("submit:begin");
    let earlier = tx
        .query_row(
            "SELECT * FROM commands WHERE task_id = ?1 AND idempotency_key = ?2",
            params![cmd.task.as_str(), key],
            receipt_from_row,
        )
        .optional()?;
    if let Some((receipt, hash)) = earlier {
        if hash == body_hash {
            return Ok(receipt);
        }
        return Err(StoreError::IdempotencyConflict { key });
    }
    failpoint("submit:after_receipt_check");

    let mut task = load_task(&tx, &cmd.task)?;
    let decision = match ask(task.clone()) {
        Some(Ok(d)) => d,
        Some(Err(rejection)) => {
            return Ok(Receipt {
                command: CommandId::new(cmd.id.clone()),
                accepted: false,
                rejection: Some(rejection.to_string()),
                first_seq: None,
                last_seq: None,
            });
        }
        None => return Err(StoreError::WriterGone),
    };
    check_references(&tx, &cmd.task, &decision.events)?;

    let (head_seq, head_hash): (i64, Option<Vec<u8>>) = tx.query_row(
        "SELECT COALESCE(MAX(seq), 0), (SELECT hash FROM events ORDER BY seq DESC LIMIT 1) FROM events",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let head_seq = u64::try_from(head_seq).map_err(|_| corrupt("negative seq"))?;
    let mut prev = match head_hash {
        Some(h) => hash32(&h).ok_or_else(|| corrupt("hash column is not 32 bytes"))?,
        None => GENESIS,
    };
    let task_seq: i64 = tx.query_row(
        "SELECT COALESCE(MAX(task_seq), 0) FROM events WHERE task_id = ?1",
        [cmd.task.as_str()],
        |r| r.get(0),
    )?;
    let n = decision.events.len() as u64;
    let (first, last) = if n == 0 {
        (None, None)
    } else {
        (Some(head_seq + 1), Some(head_seq + n))
    };
    tx.execute(
        "INSERT INTO commands (id, task_id, idempotency_key, body_hash, body, received_at,
                               status, rejection, first_seq, last_seq)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'accepted', NULL, ?7, ?8)",
        params![
            cmd.id,
            cmd.task.as_str(),
            key,
            body_hash.to_vec(),
            body,
            cmd.at.0,
            first.map(seq_i64).transpose()?,
            last.map(seq_i64).transpose()?,
        ],
    )?;
    failpoint("submit:after_receipt_insert");

    let mut intended_at: HashMap<EffectId, u64> = HashMap::new();
    for (i, event) in decision.events.into_iter().enumerate() {
        let seq = head_seq + 1 + i as u64;
        if let Event::EffectIntended { spec } = &event {
            intended_at.insert(spec.effect().clone(), seq);
        }
        let envelope = Envelope {
            v: WIRE_VERSION,
            id: format!("{}/{}", cmd.id, i + 1),
            task: cmd.task.clone(),
            at: cmd.at,
            actor: cmd.actor.clone(),
            body: event,
        };
        task = Some(apply(task, &envelope).map_err(|e| corrupt(format!("apply: {e}")))?);
        let value = serde_json::to_value(&envelope)?;
        let ty = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned();
        let body = canonical_json(&value);
        let hash = chain_hash(&prev, seq, &body);
        tx.execute(
            "INSERT INTO events (seq, task_id, task_seq, command_id, v, type, body, at,
                                 prev_hash, hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                seq_i64(seq)?,
                cmd.task.as_str(),
                task_seq + 1 + i as i64,
                cmd.id,
                WIRE_VERSION,
                ty,
                body,
                cmd.at.0,
                prev.to_vec(),
                hash.to_vec(),
            ],
        )?;
        prev = hash;
    }
    failpoint("submit:after_events");

    for spec in &decision.effects {
        let effect = spec.effect();
        let intended_seq = intended_at
            .get(effect)
            .copied()
            .ok_or_else(|| corrupt(format!("effect {effect} has no EffectIntended event")))?;
        tx.execute(
            "INSERT INTO outbox (effect_id, task_id, kind, spec, state, attempt, max_attempts,
                                 process, intended_seq, outcome_seq)
             VALUES (?1, ?2, ?3, ?4, 'intended', 0, ?5, NULL, ?6, NULL)",
            params![
                effect.as_str(),
                cmd.task.as_str(),
                effect_kind(spec),
                canonical_json(&serde_json::to_value(spec)?),
                max_attempts(spec),
                seq_i64(intended_seq)?,
            ],
        )?;
    }
    if let (Command::RecordEffectOutcome { effect, .. }, Some(last)) = (&cmd.body, last) {
        tx.execute(
            "UPDATE outbox SET state = 'done', outcome_seq = ?2 WHERE effect_id = ?1",
            params![effect.as_str(), seq_i64(last)?],
        )?;
    }
    failpoint("submit:after_outbox");

    if let (Some(task), Some(last)) = (&task, last) {
        tx.execute(
            "INSERT INTO tasks (id, state, version, snapshot, updated_seq)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET state = excluded.state, version = excluded.version,
               snapshot = excluded.snapshot, updated_seq = excluded.updated_seq",
            params![
                task.id.as_str(),
                wire_name!(task.state),
                i64::try_from(task.version).map_err(|_| corrupt("version overflow"))?,
                serde_json::to_string(task)?,
                seq_i64(last)?,
            ],
        )?;
    }
    failpoint("submit:after_projection");

    tx.commit()?;
    failpoint("submit:after_commit");
    Ok(Receipt {
        command: CommandId::new(cmd.id.clone()),
        accepted: true,
        rejection: None,
        first_seq: first.map(Seq),
        last_seq: last.map(Seq),
    })
}

pub(crate) fn index_artifact(
    conn: &mut Connection,
    id: &ArtifactId,
    size: u64,
    media: MediaType,
    label: Provenance,
    by: Option<&EffectId>,
) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO artifacts (id, label, produced_by, size, media_type, created_seq)
         VALUES (?1, ?2, ?3, ?4, ?5, (SELECT COALESCE(MAX(seq), 0) FROM events))
         ON CONFLICT(id, label, produced_by) DO NOTHING",
        params![
            id.as_str(),
            wire_name!(label),
            by.map_or("", EffectId::as_str),
            i64::try_from(size).map_err(|_| corrupt("artifact too large"))?,
            wire_name!(media),
        ],
    )?;
    failpoint("artifact:after_index");
    Ok(())
}

pub(crate) fn artifact_refs(
    conn: &Connection,
    id: &ArtifactId,
) -> Result<Vec<Artifact>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT size, media_type, label, produced_by FROM artifacts WHERE id = ?1
         ORDER BY created_seq, rowid",
    )?;
    let rows = stmt
        .query_map([id.as_str()], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(size, media, label, by)| {
            Ok(Artifact {
                id: id.clone(),
                size: u64::try_from(size).map_err(|_| corrupt("negative size"))?,
                media_type: serde_json::from_value(Value::String(media))?,
                label: serde_json::from_value(Value::String(label))?,
                produced_by: (!by.is_empty()).then(|| EffectId::new(by)),
            })
        })
        .collect()
}

pub(crate) fn has_label(
    conn: &Connection,
    id: &ArtifactId,
    label: Provenance,
    by: Option<&EffectId>,
) -> Result<bool, StoreError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM artifacts WHERE id = ?1 AND label = ?2 AND produced_by = ?3)",
        params![
            id.as_str(),
            wire_name!(label),
            by.map_or("", EffectId::as_str)
        ],
        |r| r.get(0),
    )?)
}

pub(crate) fn labelled_in_task(
    conn: &Connection,
    id: &ArtifactId,
    label: Provenance,
    task: &TaskId,
) -> Result<bool, StoreError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM artifacts a JOIN outbox o ON o.effect_id = a.produced_by
                        WHERE a.id = ?1 AND a.label = ?2 AND o.task_id = ?3)",
        params![id.as_str(), wire_name!(label), task.as_str()],
        |r| r.get(0),
    )?)
}

/// Every artifact an event cites must carry the label it is cited as, produced by the effect
/// that produced it: a transcript is that run's `RuntimeClaim`, a review that run's
/// `ReviewFindings`, verification output and checkpoint diffs that effect's `HarnessEvidence`,
/// and evidence attached to a human stop an evidence reference produced by an effect of the
/// same task (each of which was itself checked when its effect finished).
fn check_references(conn: &Connection, task: &TaskId, events: &[Event]) -> Result<(), StoreError> {
    let need = |id: &ArtifactId, label: Provenance, by: &EffectId| -> Result<(), StoreError> {
        if has_label(conn, id, label, Some(by))? {
            Ok(())
        } else {
            Err(StoreError::Unlabelled {
                id: id.clone(),
                label,
                by: Some(by.clone()),
            })
        }
    };
    for event in events {
        match event {
            Event::EffectFinished { effect, outcome } => match outcome {
                EffectOutcome::AgentRun {
                    transcript, review, ..
                } => {
                    need(transcript, Provenance::RuntimeClaim, effect)?;
                    if let Some(review) = review {
                        need(review, Provenance::ReviewFindings, effect)?;
                    }
                }
                EffectOutcome::Verify { evidence, .. } => {
                    need(evidence, Provenance::HarnessEvidence, effect)?;
                }
                EffectOutcome::Checkpoint { diff, .. } => {
                    if let Some(diff) = diff {
                        need(diff, Provenance::HarnessEvidence, effect)?;
                    }
                }
            },
            Event::VerificationFinished {
                effect, evidence, ..
            } => need(evidence, Provenance::HarnessEvidence, effect)?,
            Event::FindingsRecorded { effect, review, .. } => {
                need(review, Provenance::ReviewFindings, effect)?;
            }
            Event::AwaitingHuman { evidence, .. } => {
                for id in evidence {
                    let ok = labelled_in_task(conn, id, Provenance::HarnessEvidence, task)?
                        || labelled_in_task(conn, id, Provenance::ReviewFindings, task)?;
                    if !ok {
                        return Err(StoreError::Unlabelled {
                            id: id.clone(),
                            label: Provenance::HarnessEvidence,
                            by: None,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub(crate) fn events_after(
    conn: &mut Connection,
    after: Seq,
    limit: u32,
) -> Result<Vec<EventRecord>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT seq, task_seq, prev_hash, hash, body FROM events WHERE seq > ?1
         ORDER BY seq LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![seq_i64(after.0)?, limit], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Vec<u8>>(2)?,
                r.get::<_, Vec<u8>>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(seq, task_seq, prev, hash, body)| {
            Ok(EventRecord {
                seq: Seq(u64::try_from(seq).map_err(|_| corrupt("negative seq"))?),
                task_seq: u64::try_from(task_seq).map_err(|_| corrupt("negative task_seq"))?,
                prev_hash: hash32(&prev).ok_or_else(|| corrupt("prev_hash size"))?,
                hash: hash32(&hash).ok_or_else(|| corrupt("hash size"))?,
                event: serde_json::from_str(&body)?,
            })
        })
        .collect()
}

pub(crate) fn verify_chain(conn: &mut Connection) -> Result<Seq, ChainBroken> {
    let unreadable = |e: rusqlite::Error| ChainBroken::Unreadable(e.to_string());
    let mut stmt = conn
        .prepare("SELECT seq, prev_hash, hash, body FROM events ORDER BY seq")
        .map_err(unreadable)?;
    let mut rows = stmt.query([]).map_err(unreadable)?;
    let mut prev = GENESIS;
    let mut expected: u64 = 1;
    while let Some(row) = rows.next().map_err(unreadable)? {
        let seq: i64 = row.get(0).map_err(unreadable)?;
        let at = Seq(u64::try_from(seq).unwrap_or(0));
        let broken = |reason: &str| ChainBroken::At {
            seq: at,
            reason: reason.to_owned(),
        };
        if at.0 != expected {
            return Err(broken("gap in the sequence"));
        }
        let stored_prev: Vec<u8> = row.get(1).map_err(unreadable)?;
        let stored_hash: Vec<u8> = row.get(2).map_err(unreadable)?;
        // A body that is not valid UTF-8 text cannot be the canonical JSON that was hashed.
        let body: String = row.get(3).map_err(|_| broken("body is not UTF-8 text"))?;
        if hash32(&stored_prev) != Some(prev) {
            return Err(broken("prev_hash does not match the previous event"));
        }
        let hash = chain_hash(&prev, at.0, &body);
        if hash32(&stored_hash) != Some(hash) {
            return Err(broken("hash does not match the body"));
        }
        prev = hash;
        expected += 1;
    }
    Ok(Seq(expected - 1))
}

fn process_to_json(p: &ProcessRecord) -> String {
    canonical_json(&serde_json::json!({
        "pgid": p.pgid, "pid": p.pid, "start_us": p.start_us, "boot_id": p.boot_id,
    }))
}

fn process_from_json(s: &str) -> Result<ProcessRecord, StoreError> {
    let v: Value = serde_json::from_str(s)?;
    let int = |k: &str| v.get(k).and_then(Value::as_i64);
    let (Some(pgid), Some(pid), Some(start_us), Some(boot_id)) = (
        int("pgid").and_then(|n| i32::try_from(n).ok()),
        int("pid").and_then(|n| i32::try_from(n).ok()),
        int("start_us"),
        v.get("boot_id").and_then(Value::as_str),
    ) else {
        return Err(corrupt("process record"));
    };
    Ok(ProcessRecord {
        pgid,
        pid,
        start_us,
        boot_id: boot_id.to_owned(),
    })
}

const OUTBOX_COLUMNS: &str = "effect_id, task_id, spec, state, attempt, max_attempts, process,
                              intended_seq, outcome_seq";

type RawOutbox = (
    String,
    String,
    String,
    String,
    i64,
    i64,
    Option<String>,
    i64,
    Option<i64>,
);

fn raw_outbox(r: &Row<'_>) -> rusqlite::Result<RawOutbox> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
        r.get(6)?,
        r.get(7)?,
        r.get(8)?,
    ))
}

fn outbox_row(raw: RawOutbox) -> Result<OutboxRow, StoreError> {
    let (effect, task, spec, state, attempt, max_attempts, process, intended, outcome) = raw;
    Ok(OutboxRow {
        effect: EffectId::new(effect),
        task: TaskId::new(task),
        spec: serde_json::from_str(&spec)?,
        state: OutboxState::parse(&state).ok_or_else(|| corrupt("outbox state"))?,
        attempt: u32::try_from(attempt).map_err(|_| corrupt("attempt"))?,
        max_attempts: u32::try_from(max_attempts).map_err(|_| corrupt("max_attempts"))?,
        process: process.as_deref().map(process_from_json).transpose()?,
        intended_seq: opt_seq(Some(intended)).ok_or_else(|| corrupt("intended_seq"))?,
        outcome_seq: opt_seq(outcome),
    })
}

pub(crate) fn pending_effects(conn: &mut Connection) -> Result<Vec<OutboxRow>, StoreError> {
    let sql = format!(
        "SELECT {OUTBOX_COLUMNS} FROM outbox
         WHERE state IN ('intended','starting','started') ORDER BY intended_seq, effect_id"
    );
    let mut stmt = conn.prepare(&sql)?;
    let raws = stmt
        .query_map([], raw_outbox)?
        .collect::<Result<Vec<_>, _>>()?;
    raws.into_iter().map(outbox_row).collect()
}

pub(crate) fn effect(
    conn: &mut Connection,
    id: &EffectId,
) -> Result<Option<OutboxRow>, StoreError> {
    let sql = format!("SELECT {OUTBOX_COLUMNS} FROM outbox WHERE effect_id = ?1");
    conn.query_row(&sql, [id.as_str()], raw_outbox)
        .optional()?
        .map(outbox_row)
        .transpose()
}

pub(crate) fn mark_effect(
    conn: &mut Connection,
    id: &EffectId,
    to: OutboxState,
    process: Option<&ProcessRecord>,
) -> Result<(), StoreError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current: Option<(String, String)> = tx
        .query_row(
            "SELECT state, task_id FROM outbox WHERE effect_id = ?1",
            [id.as_str()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((from, task)) = current else {
        return Err(StoreError::UnknownEffect(id.clone()));
    };
    let from_state = OutboxState::parse(&from).ok_or_else(|| corrupt("outbox state"))?;
    use OutboxState::*;
    let attempt_delta: i64 = match (from_state, to) {
        (Intended, Starting) => 1,
        (Starting, Intended) => -1,
        (Starting, Started) | (Started, Intended) => 0,
        (Intended | Starting | Started, Abandoned) => 0,
        _ => {
            return Err(StoreError::BadEffectTransition {
                effect: id.clone(),
                from,
                to: to.as_str(),
            });
        }
    };
    let process_json = process.map(process_to_json);
    tx.execute(
        "UPDATE outbox SET state = ?2, attempt = attempt + ?3,
                           process = COALESCE(?4, process)
         WHERE effect_id = ?1",
        params![id.as_str(), to.as_str(), attempt_delta, process_json],
    )?;
    if let (Started, Some(p)) = (to, process) {
        let holder: Option<Option<String>> = tx
            .query_row(
                "SELECT holder_effect FROM workspaces WHERE task_id = ?1",
                [task.as_str()],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(Some(holder)) = &holder
            && holder != id.as_str()
        {
            return Err(StoreError::WorkspaceHeld {
                task: TaskId::new(task),
                holder: EffectId::new(holder.clone()),
            });
        }
        tx.execute(
            "UPDATE workspaces SET holder_effect = ?2, pgid = ?3, leader_pid = ?4,
                                   leader_start_us = ?5, boot_id = ?6
             WHERE task_id = ?1",
            params![task, id.as_str(), p.pgid, p.pid, p.start_us, p.boot_id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub(crate) fn record_workspace(
    conn: &mut Connection,
    task: &TaskId,
    path: &str,
) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO workspaces (task_id, path) VALUES (?1, ?2) ON CONFLICT(task_id) DO NOTHING",
        params![task.as_str(), path],
    )?;
    Ok(())
}

pub(crate) fn workspace(
    conn: &mut Connection,
    task: &TaskId,
) -> Result<Option<WorkspaceRow>, StoreError> {
    type Raw = (
        String,
        Option<String>,
        Option<i32>,
        Option<i32>,
        Option<i64>,
        Option<String>,
    );
    let raw: Option<Raw> = conn
        .query_row(
            "SELECT path, holder_effect, pgid, leader_pid, leader_start_us, boot_id
             FROM workspaces WHERE task_id = ?1",
            [task.as_str()],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .optional()?;
    Ok(raw.map(|(path, holder, pgid, pid, start_us, boot_id)| {
        let last_process = match (pgid, pid, start_us, boot_id) {
            (Some(pgid), Some(pid), Some(start_us), Some(boot_id)) => Some(ProcessRecord {
                pgid,
                pid,
                start_us,
                boot_id,
            }),
            _ => None,
        };
        WorkspaceRow {
            task: task.clone(),
            path: PathBuf::from(path),
            holder: holder.map(EffectId::new),
            last_process,
        }
    }))
}

pub(crate) fn release_workspace(conn: &mut Connection, task: &TaskId) -> Result<(), StoreError> {
    conn.execute(
        "UPDATE workspaces SET holder_effect = NULL WHERE task_id = ?1",
        [task.as_str()],
    )?;
    Ok(())
}
