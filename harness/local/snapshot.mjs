import * as fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { invariant } from './artifacts.mjs';

const MAX_FILES = 10000, MAX_FILE_BYTES = 16 * 1024 * 1024, MAX_TOTAL_BYTES = 64 * 1024 * 1024;

// Walk the workspace without following links. A `.git` entry below the root would make host
// Git treat it as a nested repository and run that repository's own configuration.
function walkWorkspace(workspace, visit) {
  let total = 0, count = 0;
  function walk(at, prefix = '') {
    for (const name of fs.readdirSync(at)) {
      if (!prefix && name === '.git') continue;
      const rel = prefix ? `${prefix}/${name}` : name, file = path.join(at, name), stat = fs.lstatSync(file);
      invariant(name.toLowerCase() !== '.git', `nested Git repositories are unsupported: ${rel}`);
      if (stat.isDirectory()) { walk(file, rel); continue; }
      invariant(stat.isFile() || stat.isSymbolicLink(), `special files are unsupported: ${rel}`);
      invariant(++count <= MAX_FILES && stat.size <= MAX_FILE_BYTES, 'snapshot file limit exceeded');
      total += stat.size;
      invariant(total <= MAX_TOTAL_BYTES, 'snapshot byte limit exceeded');
      visit?.(rel, file, stat);
    }
  }
  walk(workspace);
}

// Enforce limits and reject nested repositories before host Git reads worker output.
export function scanWorkspace(workspace) {
  walkWorkspace(workspace);
}

// Do not trust Git's index stat cache or ignore rules as proof of tested bytes.
export function assertSnapshot(workspace, commit, git) {
  const algorithm = git(workspace, 'rev-parse', '--show-object-format');
  const expected = new Map();
  for (const entry of git(workspace, 'ls-tree', '-rz', '--full-tree', commit).split('\0').filter(Boolean)) {
    const tab = entry.indexOf('\t'), [mode, type, hash] = entry.slice(0, tab).split(' ');
    invariant(type === 'blob' && ['100644', '100755', '120000'].includes(mode), 'submodules and special Git entries are unsupported');
    expected.set(entry.slice(tab + 1), { mode, hash });
  }
  walkWorkspace(workspace, (rel, file, stat) => {
    const bytes = stat.isSymbolicLink() ? Buffer.from(fs.readlinkSync(file)) : fs.readFileSync(file);
    // Git records 100755 only when the owner executable bit is set.
    const mode = stat.isSymbolicLink() ? '120000' : (stat.mode & 0o100) ? '100755' : '100644';
    const hash = createHash(algorithm).update(`blob ${bytes.length}\0`).update(bytes).digest('hex');
    const wanted = expected.get(rel);
    invariant(wanted, `file not in the candidate commit (ignored or untracked; remove it): ${rel}`);
    invariant(wanted.hash === hash && wanted.mode === mode, `candidate bytes differ from commit: ${rel}`);
    expected.delete(rel);
  });
  invariant(expected.size === 0, 'candidate is missing committed files');
}
