import * as fs from 'node:fs';
import path from 'node:path';
import { createHash, randomUUID } from 'node:crypto';

export function invariant(condition, message) {
  if (!condition) throw new Error(message);
}
export const digest = bytes => createHash('sha256').update(bytes).digest('hex');
export const json = value => JSON.stringify(value, null, 2) + '\n';
export const inside = (parent, child) => child === parent || child.startsWith(parent + path.sep);

// The controller and its OS account are trusted. Workers never receive these paths.
export function trustedPath(file, directory = false) {
  const resolved = fs.realpathSync(file);
  invariant(path.resolve(file) === resolved, `symlinked trust path: ${file}`);
  const stat = fs.statSync(resolved);
  invariant(directory ? stat.isDirectory() : stat.isFile(), `wrong path type: ${file}`);
  invariant(stat.uid === process.getuid() || stat.uid === 0, `untrusted owner: ${file}`);
  invariant((stat.mode & 0o022) === 0, `group/other writable trust path: ${file}`);
  for (let p = path.dirname(resolved); p !== path.dirname(p); p = path.dirname(p)) {
    const s = fs.statSync(p);
    invariant((s.mode & 0o022) === 0 || (s.mode & 0o1000) !== 0, `writable ancestor: ${p}`);
  }
  return resolved;
}

export function syncDirectory(dir) {
  const fd = fs.openSync(dir, 'r');
  try { fs.fsyncSync(fd); } finally { fs.closeSync(fd); }
}

export function atomic(file, bytes) {
  const temp = `${file}.${randomUUID()}.tmp`;
  const fd = fs.openSync(temp, 'wx', 0o600);
  try { fs.writeFileSync(fd, bytes); fs.fsyncSync(fd); } finally { fs.closeSync(fd); }
  fs.renameSync(temp, file);
  syncDirectory(path.dirname(file));
}

export function put(root, value) {
  const bytes = Buffer.isBuffer(value) ? value : Buffer.from(typeof value === 'string' ? value : json(value));
  const hash = digest(bytes);
  const file = path.join(root, 'objects', hash);
  fs.mkdirSync(path.dirname(file), { recursive: true, mode: 0o700 });
  try {
    const fd = fs.openSync(file, 'wx', 0o600);
    try { fs.writeFileSync(fd, bytes); fs.fsyncSync(fd); } finally { fs.closeSync(fd); }
    syncDirectory(path.dirname(file));
  } catch (e) {
    if (e.code !== 'EEXIST') throw e;
    get(root, hash);
  }
  return hash;
}

export function get(root, hash) {
  invariant(typeof hash === 'string' && /^[a-f0-9]{64}$/.test(hash), 'invalid object digest');
  const file = path.join(root, 'objects', hash);
  trustedPath(file);
  const bytes = fs.readFileSync(file);
  invariant(digest(bytes) === hash, `corrupt object: ${hash}`);
  return bytes;
}
export const object = (root, hash) => JSON.parse(get(root, hash));

export async function leased(root, fn) {
  trustedPath(root, true);
  const lock = path.join(root, 'controller.lock');
  let fd;
  try { fd = fs.openSync(lock, 'wx', 0o600); }
  catch (e) { if (e.code === 'EEXIST') throw new Error('run is locked; inspect controller PID before manual recovery'); throw e; }
  try {
    fs.writeFileSync(fd, json({ pid: process.pid })); fs.fsyncSync(fd);
    return await fn();
  } finally { fs.closeSync(fd); fs.unlinkSync(lock); }
}

export function treeDigest(dir, trusted = false) {
  const records = [];
  function walk(at, rel) {
    for (const name of fs.readdirSync(at).sort()) {
      const file = path.join(at, name), sub = path.join(rel, name), s = fs.lstatSync(file);
      invariant(!s.isSymbolicLink(), `symlink not permitted in trusted inputs: ${file}`);
      if (trusted) trustedPath(file, s.isDirectory());
      if (s.isDirectory()) walk(file, sub);
      else { invariant(s.isFile(), `non-file input: ${file}`); records.push([sub, s.mode & 0o777, digest(fs.readFileSync(file))]); }
    }
  }
  walk(dir, '');
  return digest(json(records));
}
