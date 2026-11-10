import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { spawn } from 'node:child_process';
import { after, describe, it } from 'node:test';
import { ENG_BIN, approvedTask, git, newTask, runToGate, setup } from './support.mjs';

async function waitUntil(check) {
  const deadline = Date.now() + 20_000;
  while (!check()) {
    if (Date.now() > deadline) throw new Error('timed out waiting for the running agent');
    await new Promise((r) => setTimeout(r, 20));
  }
}

describe('flow', () => {
  const ctxs = [];
  const mk = (tag, o) => {
    const c = setup(tag, o);
    ctxs.push(c);
    return c;
  };
  after(() => ctxs.forEach((c) => c.cleanup()));

  it('new, approve-spec, run stops for a human at review_clean with one checkpoint', { skip: 'R-11: strong descendant containment backend unavailable; production execution is disabled' }, async () => {
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

  it('approved run fails closed without strong containment and starts no agent', async () => {
    const c = mk('nocontainment');
    const t = await approvedTask(c);
    const r = c.eng(['run', t.id], { json: true });
    assert.equal(r.status, 5, `${r.stdout}${r.stderr}`);
    assert.equal(r.json.ok, false);
    assert.equal(r.json.error, 'containment_unavailable');
    assert.equal(fs.existsSync(c.marker), false, 'an agent process was started');
    assert.equal(c.eng(['status', t.id], { json: true }).json.checkpoints.length, 0);
  });

  it('status without a task lists the tasks', () => {
    const c = mk('list');
    const t = newTask(c);
    const s = c.eng(['status'], { json: true });
    assert.equal(s.status, 0);
    assert.deepEqual(s.json.tasks.map((x) => x.task), [t.id]);
  });

  it('native Codex keeps Node available to verification without an extra read directory', async () => {
    const c = mk('native-node');
    const codex = path.join(c.bin, 'codex-native');
    const fake = path.resolve(import.meta.dirname, '../fixtures/fake-codex.mjs');
    fs.writeFileSync(codex, `#!/bin/sh\nexec '${process.execPath}' '${fake}' "$@"\n`, { mode: 0o755 });
    c.env.ENG_CODEX_BIN = codex;
    c.env.ENG_EXTRA_READ_DIRS = path.dirname(fake);
    fs.writeFileSync(path.join(c.repo, '.eng/pipeline.toml'),
      'verify_commands = [["node", "-e", "process.exit(0)"]]\nrun_deadline_ms = 60000\nmax_turns = 5\n\n' +
      '[implement]\nruntime = "claude"\n\n[review]\nruntime = "codex"\n');
    const t = await runToGate(c);
    assert.equal(t.run.status, 4, `${t.run.stdout}${t.run.stderr}`);
    assert.equal(t.run.json.gate, 'review_clean');
    assert.equal(t.run.json.round, 0, 'verification must find Node on its clean PATH');
  });

  it('status and cancel reach the active runner while another writer stays excluded', async () => {
    const c = mk('active-control', { claudeScenario: 'hang' });
    const t = await approvedTask(c);
    const child = spawn(ENG_BIN, ['run', t.id, '--json', '--state-dir', c.state], {
      env: c.env, stdio: ['ignore', 'pipe', 'pipe'],
    });
    let output = '';
    child.stdout.on('data', (s) => { output += s; });
    child.stderr.on('data', (s) => { output += s; });
    const closed = new Promise((resolve) => child.once('close', resolve));
    try {
      const pidFile = path.join(c.state, 'worktrees', t.id, 'agent.pid');
      await waitUntil(() => fs.existsSync(pidFile));
      const pid = Number(fs.readFileSync(pidFile, 'utf8'));
      const status = c.eng(['status', t.id], { json: true });
      assert.equal(status.status, 0, status.stdout + status.stderr);
      assert.equal(status.json.run, 'effects_in_flight');
      const list = c.eng(['status'], { json: true });
      assert.equal(list.status, 0, list.stdout + list.stderr);
      assert.deepEqual(list.json.tasks.map((x) => x.task), [t.id]);
      const competing = c.eng(['run', t.id], { json: true });
      assert.notEqual(competing.status, 0, 'a second writer must stay excluded');
      const cancel = c.eng(['cancel', t.id], { json: true });
      assert.equal(cancel.status, 0, cancel.stdout + cancel.stderr);
      assert.equal(cancel.json.state, 'cancelled');
      assert.equal(await closed, 5, output);
      assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' });
      const final = c.eng(['status', t.id], { json: true });
      assert.equal(final.status, 0, final.stdout + final.stderr);
      assert.equal(final.json.run, 'cancelled');
      assert.equal(c.eng(['audit', 'verify'], { json: true }).status, 0);
    } finally {
      if (child.exitCode === null) child.kill('SIGINT');
      await closed;
    }
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
