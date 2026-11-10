import * as fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
function check(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const file = path.join(dir, entry.name);
    if (entry.isDirectory()) check(file);
    else if (file.endsWith('.mjs')) execFileSync(process.execPath, ['--check', file], { stdio: 'inherit' });
  }
}
for (const dir of ['harness/local', 'scripts', 'tests']) check(dir);
console.log('JavaScript syntax checks passed');
