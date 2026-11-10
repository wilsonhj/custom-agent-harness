# PR 13 R-11 containment review brief

## Decision and implementation

R-11 (`requirements.md`, design §6.2) requires every descendant confirmed gone
before worktree reset or lock release. The prior supervisor's polling and macOS
responsibility SPI cannot provide that guarantee: a child can disclaim responsibility,
leave the group and lose its parent before a scan. The coordinating agent selected
the minimal fail-closed correction under the user's authorization to fix verified
issues. This change does not build a privileged backend, create OS accounts, use a
VM, or weaken R-11.

`spawn_gated` returns typed `ContainmentUnavailable` (`containment_unavailable` on
the wire) before starting the shim, invoking the identity callback or executing the
plan. `containment_available()` is false for the only implemented backend, with no
environment, feature, debug or fake-runtime override. Agent, verifier and doctor
version launches share the gate. Doctor has an explicit failing containment check
and marks both runtime version checks skipped without executing them. A missing
egress grant or unusable sandbox can still deny earlier for its own reason.

`kill_identity` returns `KillFailed` for every same/unknown-boot legacy identity;
missing leaders, reused groups and empty scans are not evidence of containment.
It does not signal such processes, reset/adopt their workspaces or release locks.
Recovery retains the started outbox row and quarantine. A confirmed different,
nonempty kernel boot identity still proves all old descendants gone. A fresh
starting effect with no process record recovers to intended without inventing a
process proof.

## Tests and coverage limits

The new launch-before-callback, empty-scan recovery, engine quarantine and doctor
regressions each failed against the original code before the fix, then passed.
New private supervisor byte fixtures cover fragmented lines, unterminated tails,
line size and combined stdout/stderr budget; they expose no launch bypass.
The existing unrelated-group test now requires refusal while proving unrelated
processes remain alive. Adapter version mismatch and parsing tests still run.
CLI audit verification now uses approval events, preserving hash-chain/tamper
coverage without requiring execution. Pure policy, store, sandbox and workspace
checks remain enabled.

The 29 newly ignored Rust tests below cannot reach their tested behavior through
the production launch API without a strong containment backend. They remain
compiled, with an explicit R-11 ignore reason. Re-enabling execution requires a
real backend and durable recovery evidence, not removal of the guard. These
ignores lose end-to-end checks of agent/verification/review/repair, cancellation,
shim IPC, process teardown and crash replay. Private byte tests do not substitute
for that missing process-level coverage. The 3 CLI skips lose successful flow,
terminal acceptance after review and runtime-version execution checks.

### `crates/exec/tests/supervisor.rs` (12)

- `shim_runs_nothing_if_identity_callback_fails`
- `deadline_kills_group_and_reports_deadline_exceeded`
- `detached_grandchild_is_killed_before_tree_gone`
- `detached_grandchild_escaping_as_the_plan_exits_is_killed_before_tree_gone`
- `plan_inherits_neither_the_gate_nor_the_status_fd`
- `plan_that_cannot_start_reports_exec_failed_and_tree_gone`
- `plan_that_kills_the_anchor_yields_kill_failed`
- `output_cap_triggers_kill`
- `oversized_line_is_a_failure_not_a_panic`
- `stderr_flood_does_not_deadlock`
- `cancel_token_yields_cancelled_exit`
- `kill_identity_ends_a_run_from_its_identity_alone`

### `crates/engine/tests/flow.rs` (11)

- `clean_flow_stops_at_awaiting_human_with_one_checkpoint`
- `blocking_finding_runs_two_repair_rounds_then_stops`
- `failing_verify_command_triggers_repair_without_review`
- `agent_that_writes_a_token_fails_with_secret_in_diff`
- `accept_moves_to_verified_and_creates_branch_harness_task_id`
- `claude_run_without_confirmed_own_sandbox_off_is_policy_denied`
- `cancel_kills_the_running_agent_and_cancels_the_task`
- `verification_runs_in_a_writable_scratch_checkout`
- `verification_home_is_fresh_and_reviewer_cannot_read_implementer_home`
- `agent_output_equal_to_a_later_evidence_artifact_does_not_block_the_task`
- `egress_decisions_are_stored_for_the_agent_run`

### `crates/engine/tests/recovery.rs` (5)

- `crash_in_starting_reruns_effect_and_nothing_ran_twice`
- `crash_during_agent_run_gives_lost_on_restart_and_reset_worktree`
- `crash_during_verify_retries_once`
- `crash_during_checkpoint_yields_same_commit_sha`
- `lock_is_not_released_while_orphan_is_alive`

### `crates/engine/tests/egress_policy.rs` (1)

- `revoking_the_grant_during_version_preparation_prevents_the_agent_go_byte`

### CLI skips (3)

- `flow.test.mjs`: new, approve-spec, run stops for a human at review_clean with one checkpoint.
- `decide.test.mjs`: accept with a terminal and yes verifies the task and creates branch harness/<id>.
- `doctor.test.mjs`: exits non-zero and names version_mismatch when claude reports 9.9.9.

### Pre-existing Rust ignores (5, unchanged)

All are in `crates/exec/tests/workspace.rs`: `bug5_prepare_rejects_a_tools_directory_that_contains_the_repository`
(pilot-only toolsDirectory), `bug6a_runtime_paths_entry_escaping_via_dotdot_or_a_symlink_is_rejected`
(pilot-only runtimePaths), `bug6b_a_verifier_reviewer_argv_containing_workspace_is_rejected`
(pilot-only argv templating), `bug6c_a_decision_statement_with_a_duplicate_decision_key_is_rejected`
(pilot signed JSON versus Rust store commands), and `bug7_an_ignored_worker_artifact_fails_the_run_and_the_error_names_the_path`
(replaced by the Rust exclusion/reset behavior test).

## External review focus

Check that all production agent/verification launch sites still call `spawn_gated`,
that no public test feature or debug path can bypass its guard, and that same-boot
legacy recovery retains both the outbox and workspace lock without reset/cleanup.
Do not interpret passing remaining tests as a successful end-to-end workflow or
R-11 containment implementation. Strong containment is absent; execution is disabled.
No live model, credentials, account provisioning, process-escape proof-of-concept,
GitHub write, approval-ledger update or release occurred in this change.

The handoff records exact command results. Next action: merge the independent
functional fixes, rerun their combined tests and obtain external review; retain
the disabled execution state until a separately designed backend proves R-11.
