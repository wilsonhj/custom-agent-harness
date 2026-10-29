import * as fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { invariant } from './artifacts.mjs';

// Do not trust Git's index stat cache or ignore rules as proof of tested bytes.
export function assertSnapshot(workspace, commit, git) {
  const algorithm = git(workspace, 'rev-parse', '--show-object-format');
  const expected = new Map();
  for (const entry of git(workspace, 'ls-tree', '-rz', '--full-tree', commit).split('\0').filter(Boolean)) {
    const tab = entry.indexOf('\t'), [mode, type, hash] = entry.slice(0, tab).split(' ');
    invariant(type === 'blob' && ['100644', '100755', '120000'].includes(mode), 'submodules and special Git entries are unsupported');
    expected.set(entry.slice(tab + 1), { mode, hash });
  }
  let total = 0, count = 0;
  function walk(at, prefix = '') {
    for (const name of fs.readdirSync(at)) {
      if (!prefix && name === '.git') continue;
      const rel = prefix ? `${prefix}/${name}` : name, file = path.join(at, name), stat = fs.lstatSync(file);
      if (stat.isDirectory()) { walk(file, rel); continue; }
      invariant(stat.isFile() || stat.isSymbolicLink(), 'special files are unsupported');
      invariant(++count <= 10000 && stat.size <= 16 * 1024 * 1024, 'snapshot file limit exceeded');
      const bytes = stat.isSymbolicLink() ? Buffer.from(fs.readlinkSync(file)) : fs.readFileSync(file);
      total += bytes.length;
      invariant(total <= 64 * 1024 * 1024, 'snapshot byte limit exceeded');
      const mode = stat.isSymbolicLink() ? '120000' : (stat.mode & 0o111) ? '100755' : '100644';
      const hash = createHash(algorithm).update(`blob ${bytes.length}\0`).update(bytes).digest('hex');
      const wanted = expected.get(rel);
      invariant(wanted && wanted.hash === hash && wanted.mode === mode, `candidate bytes differ from commit: ${rel}`);
      expected.delete(rel);
    }
  }
  walk(workspace);
  invariant(expected.size === 0, 'candidate is missing committed files');
}
