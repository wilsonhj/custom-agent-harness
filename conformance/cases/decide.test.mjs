import assert from 'node:assert/strict';
import { after, describe, it } from 'node:test';
import { git, newTask, runToGate, setup } from './support.mjs';

describe('decide', () => {
  const ctxs = [];
  const mk = (tag) => {
    const c = setup(tag);
    ctxs.push(c);
    return c;
  };
  after(() => ctxs.forEach((c) => c.cleanup()));

  it('refuses when stdin is not a terminal (exit 3) and records nothing', () => {
    const c = mk('notty');
    const t = newTask(c);
    const r = c.eng(['decide', t.id, 'approve-spec'], { json: true });
    assert.equal(r.status, 3, `${r.stdout}${r.stderr}`);
    assert.equal(r.json.ok, false);
    assert.equal(c.eng(['status', t.id], { json: true }).json.state, 'planned');
  });

  it('on a terminal, anything but the word yes records nothing', async () => {
    const c = mk('no');
    const t = newTask(c);
    const r = await c.engTty(['decide', t.id, 'approve-spec'], 'no\n');
    assert.equal(r.status, 3, r.stdout);
    assert.equal(c.eng(['status', t.id], { json: true }).json.state, 'planned');
  });

  it('accept with a terminal and yes verifies the task and creates branch harness/<id>', { skip: 'R-11: strong descendant containment backend unavailable; production execution is disabled' }, async () => {
    const c = mk('accept');
    const t = await runToGate(c);
    assert.equal(t.run.status, 4, `${t.run.stdout}${t.run.stderr}`);
    const cp = c.eng(['status', t.id], { json: true }).json.last_checkpoint;
    const d = await c.engTty(['decide', t.id, 'accept'], 'yes\n');
    assert.equal(d.status, 0, d.stdout);
    assert.ok(d.stdout.includes(cp), 'the prompt shows the commit being accepted');
    assert.equal(c.eng(['status', t.id], { json: true }).json.state, 'verified');
    assert.equal(git(c.repo, 'rev-parse', `refs/heads/harness/${t.id}`), cp);
    assert.equal(git(c.repo, 'rev-parse', 'main'), c.base);
  });
});
