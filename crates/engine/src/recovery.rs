//! Restart recovery (design §5.4, research 5 §4.3). Runs inside `Engine::open`, before any
//! command is accepted.
//!
//! | Outbox state | Agent run | Verification | Checkpoint |
//! |---|---|---|---|
//! | `intended` | run normally | run normally | run normally |
//! | `starting` | nothing ran (the shim saw EOF): back to `intended`, attempt uncounted | same | same |
//! | `started` | kill the recorded tree, then `Failed{LostOnRestart}`, worktree reset to the run's base | kill, then re-run if attempts remain, else `Failed{LostOnRestart}` | re-run (same author time and compare-and-swap ref give the same commit) |
//!
//! A tree that cannot be confirmed dead leaves the row `started` and the worktree locked; the
//! task then waits for a human (`run_until_idle` reports `WorkspaceLocked`).

use harness_core::artifact::Provenance;
use harness_core::decide::{EffectOutcome, EffectSpec};
use harness_core::lifecycle::TaskState;
use harness_core::outcome::{FailureCode, RunOutcome};
use harness_exec::supervisor::kill_identity;
use harness_exec::workspace;
use harness_store::{OutboxRow, OutboxState};

use crate::worker::to_identity;
use crate::{Engine, EngineError, failpoint};

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
