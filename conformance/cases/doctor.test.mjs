import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { after, describe, it } from 'node:test';
import { setup } from './support.mjs';

describe('doctor', () => {
  const c = setup('doctor');
  after(() => c.cleanup());

  it('passes with agents at the pinned versions', () => {
    const r = c.eng(['doctor'], { json: true });
    assert.equal(r.status, 0, `${r.stdout}${r.stderr}`);
    assert.equal(r.json.ok, true);
    assert.deepEqual(r.json.checks.map((x) => x.name), [
      'sandbox',
      'descendant_tracking',
      'claude_version',
      'codex_version',
      'shim',
      'state_dir',
    ]);
  });

  it('exits non-zero and names version_mismatch when claude reports 9.9.9', () => {
    const fake = path.join(c.bin, 'claude-999');
    fs.writeFileSync(fake, '#!/bin/sh\necho "9.9.9 (Claude Code)"\n', { mode: 0o755 });
    const text = c.eng(['doctor'], { env: { ENG_CLAUDE_BIN: fake } });
    assert.notEqual(text.status, 0);
    assert.match(text.stdout, /version_mismatch/);
    const json = c.eng(['doctor'], { env: { ENG_CLAUDE_BIN: fake }, json: true });
    assert.equal(json.json.ok, false);
    assert.equal(json.json.error, 'version_mismatch');
    assert.match(json.json.checks.find((x) => x.name === 'claude_version').detail, /^version_mismatch: /);
  });

  it('fails when the state directory is not private', () => {
    const dir = path.join(c.tmp, 'open-state');
    fs.mkdirSync(dir, { mode: 0o755 });
    fs.chmodSync(dir, 0o755);
    const r = c.eng(['doctor'], { json: true, state: dir });
    assert.notEqual(r.status, 0);
    assert.match(r.json.checks.find((x) => x.name === 'state_dir').detail, /^state_dir_permissions: /);
  });
});
