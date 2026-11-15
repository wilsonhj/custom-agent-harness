import * as fs from 'node:fs';
import path from 'node:path';

const RUNTIME_ROOTS = ['/usr', '/bin', '/lib', '/lib64'];

function dirAllowed(dir) {
  if (dir === '/opt') return false;
  if (dir.startsWith('/opt/')) return true;
  return RUNTIME_ROOTS.some(root => dir === root || dir.startsWith(root + '/'));
}

function mountRoot(dir) {
  if (dir.startsWith('/opt/')) {
    const [, opt, name] = dir.split('/');
    return `/${opt}/${name}`;
  }
  for (const root of RUNTIME_ROOTS) {
    if (dir === root || dir.startsWith(root + '/')) return root;
  }
  return dir;
}

// Fixture commands must use a Node binary bubblewrap is allowed to mount.
export function resolveTestNode() {
  const candidates = [];
  if (fs.existsSync('/usr/bin/node')) candidates.push('/usr/bin/node');
  candidates.push(process.execPath);
  if (fs.existsSync('/opt')) {
    for (const name of fs.readdirSync('/opt')) {
      const node = path.join('/opt', name, 'bin', 'node');
      if (fs.existsSync(node)) candidates.push(node);
    }
  }
  for (const candidate of candidates) {
    try {
      if (!fs.existsSync(candidate)) continue;
      const resolved = path.resolve(candidate);
      const dir = path.dirname(resolved);
      if (!dirAllowed(dir)) continue;
      const extra = mountRoot(dir);
      const runtimePaths = [...new Set([...RUNTIME_ROOTS, extra].filter(p => fs.existsSync(p) && p !== '/opt'))];
      return { node: resolved, runtimePaths };
    } catch {
      continue;
    }
  }
  throw new Error('tests require node under /usr, /bin, /lib, /lib64, or /opt/<name>/bin');
}