import * as fs from 'node:fs';
import { spawnSync } from 'node:child_process';
const files = fs.readdirSync('tests').filter(f => /\.test\.(?:mjs|js|ts)$/.test(f)).sort().map(f => `tests/${f}`);
if (!files.length) throw new Error('no tests found');
const r = spawnSync(process.execPath, ['--experimental-strip-types', '--test', ...files], { stdio: 'inherit' });
process.exitCode = r.status ?? 1;
