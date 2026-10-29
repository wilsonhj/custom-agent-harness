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
    if (command === 'status') result = status(root);
    else if (command === 'run') {
      const controller = new AbortController();
      const abort = () => controller.abort();
      process.once('SIGINT', abort); process.once('SIGTERM', abort);
      try { result = await run(root, { signal: controller.signal }); }
      finally { process.removeListener('SIGINT', abort); process.removeListener('SIGTERM', abort); }
    } else if (command === 'request-decision') result = await requestDecision(root, requireValue('decision'));
    else if (command === 'record-decision') result = await recordDecision(root, { statementFile: requireValue('statement'),
      signatureFile: requireValue('signature'), principal: requireValue('principal') });
    else throw new Error(`unknown command: ${command}`);
  }
  console.log(JSON.stringify(result, null, 2));
  if (result.ok === false || result.state === 'failed') process.exitCode = 1;
} catch (e) {
  console.error(JSON.stringify({ error: e.message })); process.exitCode = 1;
}
