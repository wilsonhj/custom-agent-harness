#!/usr/bin/env node
import { parseArgs } from 'node:util';
import path from 'node:path';
import { prepare, run, status, requestDecision, recordDecision } from './workflow.mjs';
import { doctor } from './sandbox.mjs';

try {
  const { positionals, values: v } = parseArgs({ allowPositionals: true, options: Object.fromEntries(
    ['repo', 'packet', 'store', 'config', 'run', 'decision', 'statement', 'signature', 'principal'].map(k => [k, { type: 'string' }])) });
  const [command] = positionals;
  if (positionals.length !== 1) throw new Error('one command required: prepare, run, status, request-decision, record-decision, doctor');
  const requireValue = name => { if (!v[name]) throw new Error(`--${name} required`); return v[name]; };
  let result;
  if (command === 'doctor') result = await doctor();
  else if (command === 'prepare') result = await prepare({ repo: requireValue('repo'), packet: requireValue('packet'),
    store: requireValue('store'), configFile: requireValue('config') });
  else {
    const root = path.resolve(requireValue('run'));
    // While a command holds the run lease, SIGINT/SIGTERM/SIGHUP must not kill Node before the lease
    // is released: `run` cancels its sandbox job, other commands finish their bounded work.
    const controller = new AbortController();
    const abort = () => controller.abort();
    process.on('SIGINT', abort); process.on('SIGTERM', abort); process.on('SIGHUP', abort);
    try {
      if (command === 'status') result = status(root);
      else if (command === 'run') result = await run(root, { signal: controller.signal });
      else if (command === 'request-decision') result = await requestDecision(root, requireValue('decision'));
      else if (command === 'record-decision') result = await recordDecision(root, { statementFile: requireValue('statement'),
        signatureFile: requireValue('signature'), principal: requireValue('principal') });
      else throw new Error(`unknown command: ${command}`);
    } finally { process.removeListener('SIGINT', abort); process.removeListener('SIGTERM', abort);
      process.removeListener('SIGHUP', abort); }
  }
  console.log(JSON.stringify(result, null, 2));
  if (result.ok === false || result.state === 'failed') process.exitCode = 1;
} catch (e) {
  console.error(JSON.stringify({ error: e.message })); process.exitCode = 1;
}
