# Design: local-evidence-workflow

## Plan

1. Research Node process/fs contracts, bubblewrap namespaces and SSH detached verification.
2. Add small modules for sandboxed processes, artifacts, local workflow and CLI.
3. Test successful and adversarial flows using temporary fixture repositories; separately
   exercise the real sandbox or report a denied capability. Add CI requiring real isolation.
4. Review the final diff; update this packet and publish a reviewable PR.

## Architecture

One trusted controller, no service infrastructure or npm dependencies. Node 24 ESM.
The controller keeps config, journal and evidence outside agent mounts. A Git worktree is
checkout isolation only; bubblewrap is the worker's actual execution boundary. Worker .git
is hidden behind an immutable empty file; Git administration and the source checkout are
not mounted. Verifier and reviewer get a read-only candidate. Runtime directories and a
separate read-only tool directory are operator supplied and validated. Every job gets a
new network/PID/user namespace, a private /tmp, a cleared environment and bounded output.

The controller runs trusted Git with hooks/fsmonitor/global configuration disabled. The
initial repo and its local Git configuration must be operator trusted. A worker cannot
change the feature packet. It may propose any other patch; independent review decides scope.
The controller stages and records a candidate commit before verification. The base checkout
is never modified. A run lease serializes all writes. Content-addressed objects are written
exclusively and fsynced; an atomic fsynced manifest references immutable event/evidence
objects. Existing object bytes are rehashed before use. No hash chain is claimed to be an
external timestamp or protection from a malicious controller.

Run states are local execution facts: prepared, running, awaiting-decision, failed,
decision-recorded. They are NOT TaskState and do not call or weaken canTransition.
Interrupted running jobs require a new run; never guess whether an operation completed.

Human decisions are detached SSH signatures under namespace local-review-v1. The signed
JSON binds run ID, base/candidate, evidence manifest digest and accept/reject. Verification
uses an external pinned allowed-signers file and the current verification time. The CLI has
no signing command and no human identity default. Durable handoff includes exact next action.
No live human approval is manufactured during implementation or tests.

## Research

Context7 retrieved primary sources on 2026-10-28:
- https://github.com/nodejs/node/blob/main/doc/api/child_process.md — shell-free argv,
  detached POSIX processes, streaming output. Group kill on timeout; bounded stdout/stderr.
- https://github.com/nodejs/node/blob/main/doc/api/fs.md — exclusive open and fsync.
- https://github.com/containers/bubblewrap/blob/main/bwrap.xml — bind/ro-bind, new-session,
  die-with-parent. Explicit required user namespace, unshare-all, dropped capabilities.
- https://github.com/openssh/openssh-portable/blob/master/ssh-keygen.1 — detached signature
  verification with allowed signers and namespaces (also verified in prior review).

## API and schema

CLI: prepare, run, status, request-decision, record-decision, doctor. JSON output; errors exit 1.
Config schemaVersion 1 contains toolsDirectory, runtimePaths, worker/verifier/reviewer argv,
deadlineMs, maxOutputBytes and allowedSigners. All executable paths are absolute, outside
candidate and run state; config/trust files are owned by root or controller and not writable
by group/others. Only registered command arrays execute; no shell command strings.
Arguments support {workspace}; context is also supplied on stdin as JSON.

## Migration plan

No migration. New local-only versioned artifacts. No changes to existing policy or ledger.

## Rollout and rollback

Run fixture tests, then doctor and the real sandbox acceptance test on a supported Linux
host. A genuine project run stops at awaiting-decision until a human signs. Delete an
abandoned run's worktree with git worktree remove; retain its evidence for inspection.
Revert this PR to remove the prototype. Nothing is deployed and no cloud resources exist.
