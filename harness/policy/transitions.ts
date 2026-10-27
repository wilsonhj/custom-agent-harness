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

// States from which work is still in progress. A task may enter Blocked from any of
// these, and may resume only to the one it entered from.
const ACTIVE: readonly TaskState[] = [
  "Draft",
  "Planned",
  "Approved",
  "Implementing",
  "Reviewing",
  "Verified",
  "PreviewVerified",
  "MigrationGated",
  "Repair",
];

export const ALLOWED_TRANSITIONS: Readonly<Record<TaskState, readonly TaskState[]>> = {
  Draft: ["Planned", "Blocked", "Cancelled"],
  Planned: ["Approved", "Draft", "Blocked", "Cancelled", "Superseded"],
  Approved: ["Implementing", "Blocked", "Cancelled", "Superseded"],
  Implementing: ["Reviewing", "Blocked", "Failed", "Cancelled"],
  Reviewing: ["Verified", "Repair", "Blocked", "Failed", "Cancelled"],
  Repair: ["Reviewing", "Blocked", "Failed", "Cancelled"],
  Verified: ["PreviewVerified", "Repair", "Blocked", "Cancelled"],
  PreviewVerified: ["MigrationGated", "Repair", "Blocked", "Cancelled"],
  MigrationGated: ["Released", "Repair", "Blocked", "Cancelled"],
  Released: ["Observed", "Failed"],
  Observed: ["Closed", "Failed"],
  // Resume is not listed here: canTransition allows Blocked -> blockedFrom only.
  // Listing ACTIVE would let Implementing -> Blocked -> MigrationGated skip review,
  // preview and the migration gate.
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

// `blockedFrom`: when `from` is Blocked, the `from_state` of the ledger record that
// entered Blocked. Without it a Blocked task can only be cancelled or superseded.
export function canTransition(
  from: TaskState,
  to: TaskState,
  approval: ApprovalRecord | undefined,
  blockedFrom?: TaskState,
): { ok: boolean; reason?: string } {
  const resuming =
    from === "Blocked" && blockedFrom !== undefined && ACTIVE.includes(blockedFrom) && to === blockedFrom;
  if (!resuming && !ALLOWED_TRANSITIONS[from].includes(to)) {
    return { ok: false, reason: `transition ${from} -> ${to} is not allowed` };
  }

  // Entering Blocked needs no approval, so stopping work is always possible.
  if (to === "Blocked") {
    return { ok: true };
  }

  // TODO(human): decide which transitions require a human approver vs. a policy actor,
  // and reject when approval.actor/target don't match. (~5-10 lines)

  // Until that policy exists, every other transition is denied (fail closed).
  return { ok: false, reason: "approval policy not implemented" };
}
