import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import { after, describe, it } from 'node:test';
import { approvedTask, git, newTask, runToGate, setup } from './support.mjs';

describe('flow', () => {
  const ctxs = [];
  const mk = (tag, o) => {
    const c = setup(tag, o);
    ctxs.push(c);
    return c;
  };
  after(() => ctxs.forEach((c) => c.cleanup()));

  it('new, approve-spec, run stops for a human at review_clean with one checkpoint', async () => {
    const c = mk('flow');
    const t = await runToGate(c);
    assert.equal(t.run.status, 4, `${t.run.stdout}${t.run.stderr}`);
    assert.equal(t.run.json.ok, true);
    assert.equal(t.run.json.task, t.id);
    assert.equal(t.run.json.gate, 'review_clean');

    const s = c.eng(['status', t.id], { json: true });
    assert.equal(s.status, 0);
    assert.equal(s.json.task, t.id);
    assert.equal(s.json.state, 'reviewing');
    assert.equal(s.json.gate, 'review_clean');
    assert.equal(s.json.checkpoints.length, 1);
    const cp = s.json.checkpoints[0].commit;
    assert.equal(git(c.repo, 'rev-parse', `refs/harness/tasks/${t.id}/cp/1`), cp);
    assert.equal(git(c.repo, 'show', `${cp}:feature.txt`), 'implemented');
    // No branch moved.
    assert.equal(git(c.repo, 'rev-parse', 'main'), c.base);
    assert.equal(git(c.repo, 'for-each-ref', '--format=%(refname)', 'refs/heads'), 'refs/heads/main');
  });

  it('status without a task lists the tasks', () => {
    const c = mk('list');
    const t = newTask(c);
    const s = c.eng(['status'], { json: true });
    assert.equal(s.status, 0);
    assert.deepEqual(s.json.tasks.map((x) => x.task), [t.id]);
  });

  it('run before approve-spec is rejected with exit 3', () => {
    const c = mk('noapprove');
    const t = newTask(c);
    const r = c.eng(['run', t.id], { json: true });
    assert.equal(r.status, 3, `${r.stdout}${r.stderr}`);
    assert.equal(r.json.ok, false);
    assert.match(r.json.error, /^rejected:/);
    assert.equal(c.eng(['status', t.id], { json: true }).json.checkpoints.length, 0);
  });

  it('with the sandbox unavailable run fails with sandbox_unavailable and starts no agent', async () => {
    const c = mk('nosandbox');
    const t = await approvedTask(c);
    const r = c.eng(['run', t.id], { json: true, env: { ENG_FORCE_SANDBOX_UNAVAILABLE: '1' } });
    assert.equal(r.status, 5, `${r.stdout}${r.stderr}`);
    assert.equal(r.json.ok, false);
    assert.equal(r.json.error, 'sandbox_unavailable');
    assert.equal(fs.existsSync(c.marker), false, 'an agent process was started');
  });
});
