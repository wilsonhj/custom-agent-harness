// Run: node --experimental-strip-types --test tests/
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  ALLOWED_TRANSITIONS,
  RESUMABLE_STATES,
  canTransition,
  type ApprovalRecord,
  type BlockedRecord,
  type ResumableState,
  type TaskState,
} from "../harness/policy/transitions.ts";

const ALL_STATES: readonly TaskState[] = [
  "Draft", "Planned", "Approved", "Implementing", "Reviewing", "Verified", "PreviewVerified",
  "MigrationGated", "Released", "Observed", "Closed", "Blocked", "Failed", "Cancelled", "Repair",
  "Superseded",
];

// Independent statement of the intended graph (not derived from the code under test).
// Sources: docs/architecture/release-gate.md (path, Failure handling), README "State machine".
const EXPECTED_EDGES: Record<TaskState, readonly TaskState[]> = {
  Draft: ["Planned", "Blocked", "Cancelled"],
  Planned: ["Approved", "Draft", "Blocked", "Cancelled", "Superseded"],
  Approved: ["Implementing", "Blocked", "Cancelled", "Superseded"],
  Implementing: ["Reviewing", "Blocked", "Failed", "Cancelled", "Superseded"],
  Reviewing: ["Verified", "Repair", "Blocked", "Failed", "Cancelled"],
  Repair: ["Reviewing", "Blocked", "Failed", "Cancelled", "Superseded"],
  Verified: ["PreviewVerified", "Repair", "Blocked", "Failed", "Cancelled"],
  PreviewVerified: ["MigrationGated", "Repair", "Blocked", "Failed", "Cancelled"],
  MigrationGated: ["Released", "Repair", "Blocked", "Failed", "Cancelled"],
  Released: ["Observed", "Blocked", "Failed"],
  Observed: ["Closed", "Blocked", "Failed"],
  Blocked: ["Cancelled", "Superseded"],
  Failed: ["Repair", "Cancelled", "Superseded"],
  Closed: [],
  Cancelled: [],
  Superseded: [],
};

const SHA_A = "a".repeat(40);
const SHA_B = "b".repeat(40);

function record(from: TaskState, to: TaskState, sha = SHA_A, task = "t1"): ApprovalRecord {
  return {
    id: "01TEST",
    task_id: task,
    from_state: from,
    to_state: to,
    actor: "human",
    approver: "someone",
    target: { commit_sha: sha },
    evidence_refs: [],
    policy_digest: "0".repeat(64),
    timestamp: "2026-10-28T00:00:00Z",
    prev_record_hash: "0".repeat(64),
    signature: "sig",
  };
}

function blockedAt(from: ResumableState, sha = SHA_A, task = "t1"): BlockedRecord {
  return { ...record(from, "Blocked", sha, task), from_state: from, to_state: "Blocked" };
}

describe("edges", () => {
  it("ALLOWED_TRANSITIONS matches the intended graph exactly", () => {
    for (const s of ALL_STATES) {
      assert.deepEqual([...ALLOWED_TRANSITIONS[s]].sort(), [...EXPECTED_EDGES[s]].sort(), s);
    }
    assert.deepEqual(Object.keys(ALLOWED_TRANSITIONS).sort(), [...ALL_STATES].sort());
  });

  it("every state pair: structure decides the reason, no pair is ok except entering Blocked", () => {
    for (const from of ALL_STATES) {
      for (const to of ALL_STATES) {
        if (from === "Blocked" && !EXPECTED_EDGES.Blocked.includes(to)) continue; // resume: see below
        const r = canTransition(from, to, record(from, to));
        const edge = EXPECTED_EDGES[from].includes(to);
        if (!edge) {
          assert.equal(r.ok, false, `${from}->${to}`);
          assert.match(r.reason ?? "", /not allowed/, `${from}->${to}`);
        } else if (to === "Blocked") {
          assert.equal(r.ok, true, `${from}->${to}`);
        } else {
          assert.equal(r.ok, false, `${from}->${to}`);
          assert.equal(r.reason, "approval policy not implemented", `${from}->${to}`);
        }
      }
    }
  });

  it("audit fixes: Failed, Blocked and Superseded edges exist", () => {
    for (const s of ["Verified", "PreviewVerified", "MigrationGated"] as const) {
      assert.ok(ALLOWED_TRANSITIONS[s].includes("Failed"), `${s}->Failed`);
    }
    for (const s of ["Released", "Observed"] as const) {
      assert.ok(ALLOWED_TRANSITIONS[s].includes("Blocked"), `${s}->Blocked`);
      assert.equal(canTransition(s, "Blocked", undefined).ok, true);
    }
    for (const s of ["Implementing", "Repair"] as const) {
      assert.ok(ALLOWED_TRANSITIONS[s].includes("Superseded"), `${s}->Superseded`);
    }
  });
});

describe("graph", () => {
  // Search over (state, blockedFrom) so resume edges follow the real rule: Blocked returns
  // only to the state it was entered from. `skip` removes a state to test bypass paths.
  function reachable(skip?: TaskState): Set<TaskState> {
    const seen = new Set<string>();
    const out = new Set<TaskState>();
    const queue: Array<[TaskState, TaskState | null]> = [["Draft", null]];
    while (queue.length > 0) {
      const [state, from] = queue.shift()!;
      const key = `${state}|${from}`;
      if (seen.has(key) || state === skip) continue;
      seen.add(key);
      out.add(state);
      for (const next of ALLOWED_TRANSITIONS[state]) {
        queue.push([next, next === "Blocked" ? state : null]);
      }
      if (state === "Blocked" && from !== null && RESUMABLE_STATES.includes(from as ResumableState)) {
        queue.push([from, null]);
      }
    }
    return out;
  }

  it("every state is reachable from Draft", () => {
    assert.deepEqual([...reachable()].sort(), [...ALL_STATES].sort());
  });

  it("only Closed, Cancelled and Superseded are dead ends", () => {
    const dead = ALL_STATES.filter((s) => ALLOWED_TRANSITIONS[s].length === 0);
    assert.deepEqual(dead.sort(), ["Cancelled", "Closed", "Superseded"]);
  });

  it("Released (and everything after it) is unreachable without MigrationGated", () => {
    const r = reachable("MigrationGated");
    assert.equal(r.has("Released"), false);
    assert.equal(r.has("Observed"), false);
    assert.equal(r.has("Closed"), false);
  });

  it("Released is reachable only from MigrationGated (directly or via Blocked resume)", () => {
    const into = ALL_STATES.filter((s) => ALLOWED_TRANSITIONS[s].includes("Released"));
    assert.deepEqual(into, ["MigrationGated"]);
  });

  it("Blocked is reachable from every non-terminal, non-Failed, non-Blocked state", () => {
    for (const s of ALL_STATES) {
      const expected = !["Closed", "Cancelled", "Superseded", "Failed", "Blocked"].includes(s);
      assert.equal(ALLOWED_TRANSITIONS[s].includes("Blocked"), expected, s);
    }
  });

  it("every resumable state can enter Blocked", () => {
    for (const s of RESUMABLE_STATES) assert.ok(ALLOWED_TRANSITIONS[s].includes("Blocked"), s);
  });
});

describe("resume from Blocked", () => {
  it("is denied without a blocked record, whatever the target", () => {
    for (const to of RESUMABLE_STATES) {
      const r = canTransition("Blocked", to, record("Blocked", to), { currentCommitSha: SHA_A });
      assert.equal(r.ok, false, to);
      assert.match(r.reason ?? "", /requires the ledger record/, to);
    }
  });

  it("with matching record and commit still ends fail closed (policy stub), not ok", () => {
    for (const s of RESUMABLE_STATES) {
      const r = canTransition("Blocked", s, record("Blocked", s), {
        blockedRecord: blockedAt(s),
        currentCommitSha: SHA_A,
      });
      assert.deepEqual(r, { ok: false, reason: "approval policy not implemented" }, s);
    }
  });

  it("missing approval on a matching resume says approval required", () => {
    const r = canTransition("Blocked", "Verified", undefined, {
      blockedRecord: blockedAt("Verified"),
      currentCommitSha: SHA_A,
    });
    assert.equal(r.ok, false);
    assert.match(r.reason ?? "", /requires an approval record/);
  });

  it("block at Verified on commit A, push B, resume is denied", () => {
    const r = canTransition("Blocked", "Verified", record("Blocked", "Verified"), {
      blockedRecord: blockedAt("Verified", SHA_A),
      currentCommitSha: SHA_B,
    });
    assert.equal(r.ok, false);
    assert.match(r.reason ?? "", /current commit differs/);
  });

  it("is denied when the current commit is missing or empty", () => {
    for (const currentCommitSha of [undefined, ""]) {
      const r = canTransition("Blocked", "Verified", record("Blocked", "Verified"), {
        blockedRecord: blockedAt("Verified"),
        currentCommitSha,
      });
      assert.equal(r.ok, false);
      assert.match(r.reason ?? "", /requires the current commit/);
    }
  });

  it("is denied for every state other than the one blocked from", () => {
    for (const blockedFrom of RESUMABLE_STATES) {
      for (const to of ALL_STATES) {
        if (to === blockedFrom || ALLOWED_TRANSITIONS.Blocked.includes(to)) continue;
        const r = canTransition("Blocked", to, record("Blocked", to), {
          blockedRecord: blockedAt(blockedFrom),
          currentCommitSha: SHA_A,
        });
        assert.equal(r.ok, false, `${blockedFrom} -> Blocked -> ${to}`);
        assert.notEqual(r.reason, "approval policy not implemented", `${blockedFrom} -> ${to}`);
      }
    }
  });

  it("cannot skip review, preview or the migration gate via Blocked", () => {
    const r = canTransition("Blocked", "MigrationGated", record("Blocked", "MigrationGated"), {
      blockedRecord: blockedAt("Implementing"),
      currentCommitSha: SHA_A,
    });
    assert.equal(r.ok, false);
    assert.match(r.reason ?? "", /may only resume to Implementing/);
  });

  it("rejects empty, malformed or non-Blocked records", () => {
    const base = blockedAt("Verified");
    const bad: Array<[string, unknown]> = [
      ["empty object", {}],
      ["null", null],
      ["empty commit", { ...base, target: { commit_sha: "" } }],
      ["missing target", { ...base, target: undefined }],
      ["empty id", { ...base, id: "" }],
      ["empty task_id", { ...base, task_id: "" }],
      ["unknown from_state", { ...base, from_state: "Bogus" }],
      ["non-resumable from_state", { ...base, from_state: "Closed" }],
      ["Blocked as from_state", { ...base, from_state: "Blocked" }],
      ["did not enter Blocked", { ...base, to_state: "Verified" }],
    ];
    for (const [name, rec] of bad) {
      const r = canTransition("Blocked", "Verified", record("Blocked", "Verified"), {
        blockedRecord: rec as BlockedRecord,
        currentCommitSha: SHA_A,
      });
      assert.equal(r.ok, false, name);
      assert.notEqual(r.reason, "approval policy not implemented", name);
    }
  });

  it("rejects an approval from a different task than the blocked record", () => {
    const r = canTransition("Blocked", "Verified", record("Blocked", "Verified", SHA_A, "other"), {
      blockedRecord: blockedAt("Verified", SHA_A, "t1"),
      currentCommitSha: SHA_A,
    });
    assert.equal(r.ok, false);
    assert.match(r.reason ?? "", /different tasks/);
  });

  it("Cancelled and Superseded from Blocked need no record", () => {
    for (const to of ["Cancelled", "Superseded"] as const) {
      const r = canTransition("Blocked", to, record("Blocked", to));
      assert.equal(r.reason, "approval policy not implemented", to);
    }
  });

  it("ignores a blocked record on non-resume transitions", () => {
    const r = canTransition("Reviewing", "Verified", record("Reviewing", "Verified"), {
      blockedRecord: blockedAt("Verified"),
      currentCommitSha: SHA_B,
    });
    assert.equal(r.reason, "approval policy not implemented");
  });
});

describe("approval shape", () => {
  it("rejects an approval whose from_state or to_state differs from the arguments", () => {
    const cases: Array<[TaskState, TaskState, ApprovalRecord]> = [
      ["Reviewing", "Verified", record("Reviewing", "Released")],
      ["Reviewing", "Verified", record("Planned", "Verified")],
      ["Reviewing", "Verified", record("Verified", "Reviewing")],
      ["Draft", "Blocked", record("Draft", "Cancelled")],
    ];
    for (const [from, to, approval] of cases) {
      const r = canTransition(from, to, approval);
      assert.equal(r.ok, false, `${from}->${to}`);
      assert.match(r.reason ?? "", /approval is for/);
    }
  });

  it("rejects a missing approval where one is needed", () => {
    const r = canTransition("Reviewing", "Verified", undefined);
    assert.equal(r.ok, false);
    assert.match(r.reason ?? "", /requires an approval record/);
  });

  it("rejects empty or malformed approvals", () => {
    const good = record("Reviewing", "Verified");
    const bad: Array<[string, unknown]> = [
      ["empty object", {}],
      ["null", null],
      ["empty id", { ...good, id: "" }],
      ["empty commit", { ...good, target: { commit_sha: "" } }],
      ["no target", { ...good, target: undefined }],
      ["unknown from_state", { ...good, from_state: "Bogus" }],
    ];
    for (const [name, a] of bad) {
      const r = canTransition("Reviewing", "Verified", a as ApprovalRecord);
      assert.equal(r.ok, false, name);
      assert.match(r.reason ?? "", /approval rejected/, name);
    }
  });

  it("entering Blocked needs no approval but a supplied one must still match", () => {
    assert.equal(canTransition("Reviewing", "Blocked", undefined).ok, true);
    assert.equal(canTransition("Reviewing", "Blocked", record("Reviewing", "Blocked")).ok, true);
    assert.equal(canTransition("Reviewing", "Blocked", record("Reviewing", "Verified")).ok, false);
  });

  it("denies unknown states", () => {
    assert.equal(canTransition("Bogus" as TaskState, "Draft", undefined).ok, false);
    assert.equal(canTransition("Draft", "Bogus" as TaskState, undefined).ok, false);
  });
});

describe("fail closed", () => {
  it("no transition other than entering Blocked is ever ok, even with a perfect approval", () => {
    for (const from of ALL_STATES) {
      for (const to of ALL_STATES) {
        for (const actor of ["human", "policy"] as const) {
          const approval: ApprovalRecord = { ...record(from, to), actor };
          const ctx =
            from === "Blocked" && RESUMABLE_STATES.includes(to as ResumableState)
              ? { blockedRecord: blockedAt(to as ResumableState), currentCommitSha: SHA_A }
              : {};
          const r = canTransition(from, to, approval, ctx);
          assert.equal(r.ok, to === "Blocked" && ALLOWED_TRANSITIONS[from].includes("Blocked"), `${from}->${to}`);
        }
      }
    }
  });
});
