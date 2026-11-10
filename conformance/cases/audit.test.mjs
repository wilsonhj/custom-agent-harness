import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import * as path from 'node:path';
import { after, describe, it } from 'node:test';
import { approvedTask, setup } from './support.mjs';

describe('audit', () => {
  const c = setup('audit');
  after(() => c.cleanup());

  it('verify passes after task approval, and fails after one event body is altered', async () => {
    await approvedTask(c);
    const ok = c.eng(['audit', 'verify'], { json: true });
    assert.equal(ok.status, 0, `${ok.stdout}${ok.stderr}`);
    assert.ok(ok.json.head_seq > 0);

    const db = path.join(c.state, 'state.db');
    const sql = spawnSync(
      '/usr/bin/sqlite3',
      [db, "DROP TRIGGER events_no_update; UPDATE events SET body = body || ' ' WHERE seq = 2;"],
      { encoding: 'utf8' },
    );
    assert.equal(sql.status, 0, sql.stderr);
    const bad = c.eng(['audit', 'verify'], { json: true });
    assert.equal(bad.status, 5, `${bad.stdout}${bad.stderr}`);
    assert.equal(bad.json.ok, false);
    assert.match(bad.json.error, /chain/);
  });
});
