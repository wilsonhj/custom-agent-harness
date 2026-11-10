// Enforces the Rust workspace dependency rules (docs/superpowers/plans/2026-11-09-rust-spine.md, Task 1).
import { spawnSync } from 'node:child_process';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

const ALLOWED_EDGES = {
  'harness-cli': ['harness-engine', 'harness-core'],
  'harness-engine': ['harness-store', 'harness-exec', 'harness-adapters', 'harness-core'],
  'harness-store': ['harness-core'],
  'harness-exec': ['harness-core'],
  'harness-adapters': ['harness-core'],
  'harness-core': [],
};
const FORBIDDEN_CORE_STRINGS = ['std::fs', 'std::process', 'SystemTime::now', 'rand::'];

// fileURLToPath decodes percent-escapes (a checkout path with a space); URL.pathname does not.
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const r = spawnSync('cargo', ['metadata', '--format-version', '1', '--no-deps'], {
  cwd: root,
  encoding: 'utf8',
  maxBuffer: 64 * 1024 * 1024,
});
if (r.status !== 0) {
  console.error(`cargo metadata failed:\n${r.stderr || r.error}`);
  process.exit(1);
}
const meta = JSON.parse(r.stdout);
const members = new Map(meta.packages.map((p) => [p.name, p]));
const errors = [];

for (const name of Object.keys(ALLOWED_EDGES)) {
  if (!members.has(name)) errors.push(`missing workspace crate ${name}`);
}
for (const [name, pkg] of members) {
  const deps = pkg.dependencies.map((d) => d.name);
  if (name === 'harness-core') {
    for (const bad of ['tokio', 'rusqlite']) {
      if (deps.includes(bad)) errors.push(`harness-core must not depend on ${bad}`);
    }
  }
  if (name === 'harness-adapters' && deps.includes('tokio')) {
    errors.push('harness-adapters must not depend on tokio');
  }
  if (name !== 'harness-store' && deps.includes('rusqlite')) {
    errors.push(`${name} must not depend on rusqlite`);
  }
  const allowed = ALLOWED_EDGES[name];
  if (allowed) {
    // A crate may list itself as a dev-dependency to enable test-only features.
    for (const dep of deps.filter((d) => members.has(d) && d !== name)) {
      if (!allowed.includes(dep)) errors.push(`forbidden edge ${name} -> ${dep}`);
    }
  }
}

function* walk(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, entry.name);
    if (entry.isDirectory()) yield* walk(p);
    else if (entry.name.endsWith('.rs')) yield p;
  }
}
const coreSrc = path.join(root, 'crates/core/src');
if (fs.existsSync(coreSrc)) {
  for (const file of walk(coreSrc)) {
    const text = fs.readFileSync(file, 'utf8');
    for (const s of FORBIDDEN_CORE_STRINGS) {
      if (text.includes(s)) errors.push(`${path.relative(root, file)} contains forbidden "${s}"`);
    }
  }
}

if (errors.length) {
  for (const e of errors) console.error(e);
  process.exit(1);
}
console.log('check-deps: ok');
