// Task state machine and approval-record shape.
// Ledger design: docs/architecture/approval-ledger.md
// Migration gate (PreviewVerified -> MigrationGated -> Released): docs/architecture/release-gate.md

export type TaskState =
  // Successful path
  | "Draft"
  | "Planned"
  | "Approved"
  | "Implementing"
  | "Reviewing"
  | "Verified"
  | "PreviewVerified"
  | "MigrationGated"
  | "Released"
  | "Observed"
  | "Closed"
  // Side states
  | "Blocked"
  | "Failed"
  | "Cancelled"
  | "Repair"
  | "Superseded";

// States a task may be blocked from and resume to (Blocked -> same state, same commit).
// Released and Observed are included so a released task can be escalated (README
// "Observation": close or escalate) and later resume where it was.
export const RESUMABLE_STATES = [
  "Draft",
  "Planned",
  "Approved",
  "Implementing",
  "Reviewing",
  "Verified",
  "PreviewVerified",
  "MigrationGated",
  "Released",
  "Observed",
  "Repair",
] as const satisfies readonly TaskState[];

export type ResumableState = (typeof RESUMABLE_STATES)[number];

export const ALLOWED_TRANSITIONS: Readonly<Record<TaskState, readonly TaskState[]>> = {
  Draft: ["Planned", "Blocked", "Cancelled"],
  Planned: ["Approved", "Draft", "Blocked", "Cancelled", "Superseded"],
  Approved: ["Implementing", "Blocked", "Cancelled", "Superseded"],
  Implementing: ["Reviewing", "Blocked", "Failed", "Cancelled", "Superseded"],
  Reviewing: ["Verified", "Repair", "Blocked", "Failed", "Cancelled"],
  Repair: ["Reviewing", "Blocked", "Failed", "Cancelled", "Superseded"],
  // Failed is reachable from every state that can precede a release failure
  // (docs/architecture/release-gate.md "Failure handling").
  Verified: ["PreviewVerified", "Repair", "Blocked", "Failed", "Cancelled"],
  PreviewVerified: ["MigrationGated", "Repair", "Blocked", "Failed", "Cancelled"],
  MigrationGated: ["Released", "Repair", "Blocked", "Failed", "Cancelled"],
  Released: ["Observed", "Blocked", "Failed"],
  Observed: ["Closed", "Blocked", "Failed"],
  // Resume is not listed here: canTransition allows Blocked -> BlockedRecord.from_state only,
  // and only at the same commit. Listing RESUMABLE_STATES would let
  // Implementing -> Blocked -> MigrationGated skip review, preview and the migration gate.
  Blocked: ["Cancelled", "Superseded"],
  Failed: ["Repair", "Cancelled", "Superseded"],
  // Terminal
  Closed: [],
  Cancelled: [],
  Superseded: [],
};

// An evidence file in the content-addressed store, bound by digest so it cannot be
// swapped or regenerated after approval.
export interface EvidenceRef {
  name: string; // e.g. "tests/out.json"
  digest: { sha256: string };
}

// One line of the append-only ledger (docs/architecture/approval-ledger.md).
export interface ApprovalRecord {
  id: string;
  task_id: string;
  from_state: TaskState;
  to_state: TaskState;
  actor: "human" | "policy";
  approver: string;
  target: {
    commit_sha: string;
    deployment_target?: string; // "<provider>/<project>/<environment>"
  };
  evidence_refs: EvidenceRef[];
  policy_digest: string; // SHA-256 hex of the policy version that evaluated this transition
  timestamp: string; // ISO 8601 UTC
  prev_record_hash: string; // SHA-256 hex of the previous line; 64 zeros for the first record
  signature: string; // over the canonical bytes without this field; see ledger "Approver identity"
}

// The ledger record that entered Blocked (docs/architecture/approval-ledger.md, "Reading the
// ledger"). `from_state` is the state to resume to; `target.commit_sha` pins the commit.
export interface BlockedRecord extends ApprovalRecord {
  from_state: ResumableState;
  to_state: "Blocked";
}

// Facts the control plane supplies from the verified ledger and the worktree. They must
// never be copied from an agent's request.
export interface TransitionContext {
  // Required to resume from Blocked: the ledger record that entered Blocked.
  blockedRecord?: BlockedRecord;
  // Required to resume from Blocked: the commit the task would resume at.
  currentCommitSha?: string;
}

export interface TransitionResult {
  ok: boolean;
  reason?: string;
}

function nonEmpty(value: unknown): value is string {
  return typeof value === "string" && value.length > 0;
}

function isTaskState(value: unknown): value is TaskState {
  return typeof value === "string" && Object.hasOwn(ALLOWED_TRANSITIONS, value);
}

function isResumable(value: unknown): value is ResumableState {
  return typeof value === "string" && (RESUMABLE_STATES as readonly string[]).includes(value);
}

// Shape checks only; signatures and chain are verified by the ledger reader, not here.
function malformedRecord(record: unknown): string | undefined {
  if (typeof record !== "object" || record === null) return "record is not an object";
  const r = record as Partial<ApprovalRecord>;
  if (!nonEmpty(r.id)) return "record id is missing or empty";
  if (!nonEmpty(r.task_id)) return "record task_id is missing or empty";
  if (!isTaskState(r.from_state)) return "record from_state is missing or unknown";
  if (!isTaskState(r.to_state)) return "record to_state is missing or unknown";
  if (typeof r.target !== "object" || r.target === null || !nonEmpty(r.target.commit_sha)) {
    return "record target.commit_sha is missing or empty";
  }
  if (r.actor !== "human" && r.actor !== "policy") return "record actor must be human or policy";
  if (!nonEmpty(r.signature)) return "record signature is missing or empty";
  return undefined;
}

function deny(reason: string): TransitionResult {
  return { ok: false, reason };
}

// Fail closed: any check that cannot be positively satisfied denies.
export function canTransition(
  from: TaskState,
  to: TaskState,
  approval: ApprovalRecord | undefined,
  context: TransitionContext = {},
): TransitionResult {
  if (!isTaskState(from) || !isTaskState(to)) {
    return deny(`unknown state in transition ${String(from)} -> ${String(to)}`);
  }

  // Resuming from Blocked: the target comes from the ledger record that entered Blocked,
  // and the commit must be the one that was blocked. Cancelled/Superseded need neither.
  const resuming = from === "Blocked" && !ALLOWED_TRANSITIONS.Blocked.includes(to);
  const blocked = context.blockedRecord;
  if (resuming) {
    if (blocked === undefined) {
      return deny(`resuming Blocked -> ${to} requires the ledger record that entered Blocked`);
    }
    const bad = malformedRecord(blocked);
    if (bad !== undefined) return deny(`blocked record rejected: ${bad}`);
    if (blocked.to_state !== "Blocked") {
      return deny("blocked record rejected: it did not enter Blocked");
    }
    if (!isResumable(blocked.from_state)) {
      return deny(`blocked record rejected: cannot resume to ${String(blocked.from_state)}`);
    }
    if (to !== blocked.from_state) {
      return deny(`Blocked task may only resume to ${blocked.from_state}, not ${to}`);
    }
    if (!nonEmpty(context.currentCommitSha)) {
      return deny("resuming from Blocked requires the current commit");
    }
    if (context.currentCommitSha !== blocked.target.commit_sha) {
      return deny("current commit differs from the commit the task was blocked at");
    }
  } else if (!ALLOWED_TRANSITIONS[from].includes(to)) {
    return deny(`transition ${from} -> ${to} is not allowed`);
  }

  // An approval, when present, must be well-formed and describe exactly this transition.
  if (approval !== undefined) {
    const bad = malformedRecord(approval);
    if (bad !== undefined) return deny(`approval rejected: ${bad}`);
    if (approval.from_state !== from || approval.to_state !== to) {
      return deny(
        `approval is for ${approval.from_state} -> ${approval.to_state}, not ${from} -> ${to}`,
      );
    }
    if (resuming && blocked !== undefined && approval.task_id !== blocked.task_id) {
      return deny("approval and blocked record belong to different tasks");
    }
    if (resuming && approval.target.commit_sha !== context.currentCommitSha) {
      return deny("approval commit differs from the current commit");
    }
  }

  // Entering Blocked needs no approval, so stopping work is always possible.
  if (to === "Blocked") {
    return { ok: true };
  }

  if (approval === undefined) {
    return deny(`transition ${from} -> ${to} requires an approval record`);
  }

  // TODO(human): verify the signature, hash chain, and evidence digests, then apply the
  // release gate: MigrationGated -> Released needs a human record for this commit and
  // deployment_target; a policy actor may approve PreviewVerified -> MigrationGated only
  // on a classifier result of clean. Do not treat actor and target shape checks as that policy.

  // Until that policy exists, every other transition is denied (fail closed).
  return deny("approval policy not implemented");
}
