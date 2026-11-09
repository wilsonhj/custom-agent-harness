// Emits conformance/golden/transitions.json from the TypeScript policy (the oracle).
// Run with Node 24 type stripping; no flags needed.
import * as fs from 'node:fs';
import * as path from 'node:path';
import { ALLOWED_TRANSITIONS, RESUMABLE_STATES, canTransition } from '../harness/policy/transitions.ts';

const SHA = 'a'.repeat(40);
const states = Object.keys(ALLOWED_TRANSITIONS);
if (states.length !== 16) throw new Error(`expected 16 states, got ${states.length}`);

// Same record shape as tests/transitions.test.ts: well-formed, naming exactly this transition.
function record(from, to) {
  return {
    id: '01TEST',
    task_id: 't1',
    from_state: from,
    to_state: to,
    actor: 'human',
    approver: 'someone',
    target: { commit_sha: SHA },
    evidence_refs: [],
    policy_digest: '0'.repeat(64),
    timestamp: '2026-10-28T00:00:00Z',
    prev_record_hash: '0'.repeat(64),
    signature: 'sig',
  };
}

// blocked_record=true: the ledger record that entered Blocked, pinned to SHA and passed with
// currentCommitSha = SHA. Its from_state is `to` when `to` is resumable (the matching case),
// otherwise "Draft" (a mismatching case).
function context(to) {
  const resumeTo = RESUMABLE_STATES.includes(to) ? to : 'Draft';
  return { blockedRecord: record(resumeTo, 'Blocked'), currentCommitSha: SHA };
}

const cmp = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
const matrix = [];
for (const from of states) {
  for (const to of states) {
    for (const approval of [false, true]) {
      for (const blocked_record of [false, true]) {
        const result = canTransition(
          from,
          to,
          approval ? record(from, to) : undefined,
          blocked_record ? context(to) : {},
        );
        matrix.push({ from, to, approval, blocked_record, allowed: result.ok });
      }
    }
  }
}
matrix.sort(
  (a, b) =>
    cmp(a.from, b.from) || cmp(a.to, b.to) || cmp(a.approval, b.approval) || cmp(a.blocked_record, b.blocked_record),
);

const allowed = {};
for (const s of [...states].sort(cmp)) allowed[s] = [...ALLOWED_TRANSITIONS[s]].sort(cmp);

const out = { states: [...states].sort(cmp), allowed, matrix };
const file = path.resolve(path.dirname(new URL(import.meta.url).pathname), '../conformance/golden/transitions.json');
fs.writeFileSync(file, JSON.stringify(out, null, 2) + '\n');
console.log(`wrote ${path.relative(process.cwd(), file)}: ${out.states.length} states, ${matrix.length} rows`);
