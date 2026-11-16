//! Restart recovery (design §5.4, research 5 §4.3). Runs inside `Engine::open`, before any
//! command is accepted.
//!
//! | Outbox state | Agent run | Verification | Checkpoint |
//! |---|---|---|---|
//! | `intended` | run normally | run normally | run normally |
//! | `starting` | nothing ran (the shim saw EOF): back to `intended`, attempt uncounted | same | same |
//! | `started` | kill the recorded tree (or take the reap the store recorded when the run's lock was released), then `Failed{LostOnRestart}`, worktree reset to the run's base | kill, then re-run if attempts remain, else `Failed{LostOnRestart}` | re-run (same author time and compare-and-swap ref give the same commit) |
//!
//! A tree that cannot be confirmed dead leaves the row `started` and the worktree locked; the
//! task then waits for a human (`run_until_idle` reports `WorkspaceLocked`).

use harness_core::artifact::Provenance;
use harness_core::decide::{EffectOutcome, EffectSpec};
use harness_core::lifecycle::TaskState;
use harness_core::outcome::{FailureCode, RunOutcome};
use harness_exec::TreeGone;
use harness_exec::supervisor::{kill_identity, reaped_on_record};
use harness_exec::workspace;
use harness_store::{OutboxRow, OutboxState, ProcessRecord, WorkspaceRow};

use crate::worker::to_identity;
use crate::{Engine, EngineError, failpoint};

/// The supervisor's proof that `process`'s tree is gone, as the store recorded it when the
/// worktree lock was released (issue #24): only for a released worktree whose reap record is
/// set and whose last recorded process is exactly `process` (every identity field, checked by
/// `reaped_on_record`). Anything else is `None`, and the caller falls back to `kill_identity`.
pub(crate) fn recorded_proof(
    ws: Option<&WorkspaceRow>,
    process: &ProcessRecord,
) -> Option<TreeGone> {
    let ws = ws?;
    if !ws.reaped || ws.holder.is_some() {
        return None;
    }
    reaped_on_record(
        &to_identity(ws.last_process.as_ref()?),
        &to_identity(process),
    )
}

impl Engine {
    pub(crate) async fn recover(&self) -> Result<(), EngineError> {
        for row in self.store.pending_effects()? {
            match row.state {
                OutboxState::Intended => {}
                OutboxState::Starting => {
                    self.store
                        .mark_effect(&row.effect, OutboxState::Intended, None)?;
                }
                OutboxState::Started => self.recover_started(row).await?,
                OutboxState::Done | OutboxState::Abandoned => {}
            }
        }
        // Best effort: an accepted task whose branch write was cut off by a crash. A
        // repository that moved must not stop the engine from opening.
        for id in self.store.task_ids()? {
            if let Some(task) = self.store.task(&id)?
                && task.state == TaskState::Verified
            {
                let _ = self.ensure_branch(&task).await;
            }
        }
        Ok(())
    }

    async fn recover_started(&self, row: OutboxRow) -> Result<(), EngineError> {
        let proof = match &row.process {
            // The run's tree was proven gone and its lock released before the engine died.
            Some(p)
                if let Some(proof) =
                    recorded_proof(self.store.workspace(&row.task)?.as_ref(), p) =>
            {
                Some(proof)
            }
            Some(p) => {
                failpoint("recovery:before_kill");
                match kill_identity(&to_identity(p)).await {
                    Ok(proof) => Some(proof),
                    // Never adopt or release: the tree may still write in the worktree.
                    Err(_) => return Ok(()),
                }
            }
            None => None,
        };
        if let Some(proof) = &proof {
            self.release(&row.task, proof)?;
        }
        match &row.spec {
            EffectSpec::AgentRun { base, .. } => {
                // Best effort: the next run of this task resets the worktree again first.
                if let (Some(proof), Some(task)) = (&proof, self.store.task(&row.task)?)
                    && let Ok(ws) = self.workspace(&task).await
                {
                    let _ = workspace::reset_to(&ws, base, proof).await;
                }
                if let Some(proof) = proof {
                    self.keep_proof(&row.task, proof);
                }
                let outcome = EffectOutcome::AgentRun {
                    outcome: RunOutcome::Failed {
                        code: FailureCode::LostOnRestart,
                    },
                    transcript: self.note(
                        &row.effect,
                        Provenance::RuntimeClaim,
                        "lost_on_restart",
                    )?,
                    review: None,
                };
                self.record_outcome(&row.effect, &row.task, outcome).await?;
            }
            EffectSpec::Verify { .. } => {
                if let Some(proof) = proof {
                    self.keep_proof(&row.task, proof);
                    // Best effort: the next attempt removes a leftover scratch area first.
                    if let Some(task) = self.store.task(&row.task)? {
                        let _ = self.remove_verify_scratch(&task, &row.effect).await;
                    }
                }
                if row.attempt < row.max_attempts {
                    self.store
                        .mark_effect(&row.effect, OutboxState::Intended, None)?;
                } else {
                    let outcome = EffectOutcome::Verify {
                        outcome: RunOutcome::Failed {
                            code: FailureCode::LostOnRestart,
                        },
                        evidence: self.note(
                            &row.effect,
                            Provenance::HarnessEvidence,
                            "verification_lost_on_restart",
                        )?,
                    };
                    self.record_outcome(&row.effect, &row.task, outcome).await?;
                }
            }
            EffectSpec::Checkpoint { .. } => {
                if let Some(proof) = proof {
                    self.keep_proof(&row.task, proof);
                }
                self.store
                    .mark_effect(&row.effect, OutboxState::Intended, None)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use harness_core::ids::{EffectId, TaskId};
    use harness_store::{ProcessRecord, WorkspaceRow};

    use super::recorded_proof;

    fn process(pid: i32) -> ProcessRecord {
        ProcessRecord {
            pgid: pid,
            pid,
            start_us: 1,
            boot_id: "boot".into(),
        }
    }

    fn reaped_row() -> WorkspaceRow {
        WorkspaceRow {
            task: TaskId::new("t"),
            path: PathBuf::from("/wt"),
            holder: None,
            last_process: Some(process(10)),
            reaped: true,
        }
    }

    #[test]
    fn a_released_reaped_record_of_this_tree_is_proof() {
        assert!(recorded_proof(Some(&reaped_row()), &process(10)).is_some());
    }

    #[test]
    fn anything_short_of_that_record_is_not_proof() {
        assert!(recorded_proof(None, &process(10)).is_none(), "no workspace");
        let unreaped = WorkspaceRow {
            reaped: false,
            ..reaped_row()
        };
        assert!(
            recorded_proof(Some(&unreaped), &process(10)).is_none(),
            "not reaped"
        );
        let held = WorkspaceRow {
            holder: Some(EffectId::new("e")),
            ..reaped_row()
        };
        assert!(
            recorded_proof(Some(&held), &process(10)).is_none(),
            "lock held"
        );
        assert!(
            recorded_proof(Some(&reaped_row()), &process(11)).is_none(),
            "another tree"
        );
        let no_process = WorkspaceRow {
            last_process: None,
            ..reaped_row()
        };
        assert!(recorded_proof(Some(&no_process), &process(10)).is_none());
    }
}
