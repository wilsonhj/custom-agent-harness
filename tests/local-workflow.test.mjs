import { test } from 'node:test';
import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { prepare, run, status, requestDecision, recordDecision, git } from '../harness/local/workflow.mjs';
import { object, json, leased } from '../harness/local/artifacts.mjs';
import { capture } from '../harness/local/process.mjs';
import { doctor, execute, sandboxCommand } from '../harness/local/sandbox.mjs';

// Fixture-only executor. The public CLI cannot select this unsandboxed implementation.
async function fixtureExecutor(config, workspace, argv, writable, context, signal) {
  assert.equal(writable, context.role === 'worker');
  return capture(argv.map(a => a.replaceAll('{workspace}', workspace)), { cwd: workspace,
    env: { PATH: '/usr/bin:/bin', HOME: '/nonexistent' }, input: JSON.stringify(context),
    deadlineMs: config.deadlineMs, maxOutputBytes: config.maxOutputBytes, signal });
}

function fixture(t, options = {}) {
  const base = fs.mkdtempSync(path.join(fs.realpathSync(os.tmpdir()), 'harness-test-'));
  t.after(() => fs.rmSync(base, { recursive: true, force: true }));
  const repo = path.join(base, 'repo'), tools = path.join(base, 'tools'), trust = path.join(base, 'trust');
  for (const d of [repo, tools, trust]) fs.mkdirSync(d, { mode: 0o700 });
  git(repo, 'init', '-q');
  fs.mkdirSync(path.join(repo, 'specs/example'), { recursive: true });
  for (const file of ['requirements.md', 'design.md', 'acceptance-tests.md', 'handoff.md']) {
    fs.writeFileSync(path.join(repo, 'specs/example', file), '# Synthetic fixture\n');
  }
  fs.writeFileSync(path.join(repo, 'specs/example/tasks.json'), '[]\n');
  fs.writeFileSync(path.join(repo, 'answer.txt'), 'before\n');
  git(repo, 'add', '--all'); git(repo, 'commit', '-qm', 'synthetic base');
  const worker = options.worker ?? "fs.writeFileSync('answer.txt', 'after\\n');";
  const verify = options.verifier ?? "if(fs.readFileSync('answer.txt','utf8')!=='after\\n') process.exit(1);";
  const review = options.reviewer ?? "console.log(JSON.stringify({schemaVersion:1,verdict:'pass',baseCommit:c.baseCommit,candidateCommit:c.candidateCommit,findings:[]}));";
  for (const [name, body] of [['worker', worker], ['verifier', verify], ['reviewer', review]]) {
    fs.writeFileSync(path.join(tools, `${name}.mjs`), "import * as fs from 'node:fs';\nconst c=JSON.parse(fs.readFileSync(0,'utf8'));\n" + body + '\n');
  }
  const key = path.join(trust, 'fixture-key');
  execFileSync('/usr/bin/ssh-keygen', ['-q', '-t', 'ed25519', '-N', '', '-f', key]);
  const allowedSigners = path.join(trust, 'allowed_signers');
  fs.writeFileSync(allowedSigners, 'fixture namespaces="local-review-v1" ' + fs.readFileSync(key + '.pub', 'utf8'));
  const config = { schemaVersion: 1, toolsDirectory: tools, allowedSigners,
    runtimePaths: [...new Set(['/usr', '/bin', '/lib', '/lib64', path.dirname(process.execPath)])].filter(p => fs.existsSync(p)),
    worker: [process.execPath, path.join(tools, 'worker.mjs')],
    verifier: [process.execPath, path.join(tools, 'verifier.mjs')],
    reviewer: [process.execPath, path.join(tools, 'reviewer.mjs')],
    deadlineMs: options.deadlineMs ?? 5000, maxOutputBytes: options.maxOutputBytes ?? 32768 };
  const configFile = path.join(base, 'config.json'); fs.writeFileSync(configFile, json(config), { mode: 0o600 });
  return { base, repo, tools, trust, key, config, configFile,
    args: { repo, packet: 'specs/example', store: path.join(base, 'state'), configFile } };
}
async function completed(t, options) {
  const f = fixture(t, options), p = await prepare(f.args);
  const result = await run(p.root, { executor: fixtureExecutor });
  return { ...f, ...p, result };
}
function sign(f, file, namespace = 'local-review-v1') {
  execFileSync('/usr/bin/ssh-keygen', ['-Y', 'sign', '-f', f.key, '-n', namespace, file], { stdio: 'pipe' });
  return { statementFile: file, signatureFile: file + '.sig', principal: 'fixture' };
}

test('full local flow: separate processes, evidence, fixture human signature, restart and handoff', async t => {
  const f = await completed(t);
  assert.equal(f.result.state, 'awaiting-decision', f.result.failure);
  assert.equal(fs.readFileSync(path.join(f.repo, 'answer.txt'), 'utf8'), 'before\n');
  assert.equal(git(f.repo, 'rev-parse', 'HEAD'), f.baseCommit);
  assert.notEqual(f.result.candidateCommit, f.baseCommit);
  const e = object(f.root, f.result.evidence);
  assert.equal(e.jobs.length, 3);
  assert.deepEqual(e.jobs.map(ref => object(f.root, ref).role), ['worker', 'verifier', 'reviewer']);
  assert.match(object(f.root, e.jobs[2]).context.patch, /answer.txt/);
  const request = await requestDecision(f.root, 'accept');
  assert.equal(request.authority, 'request-only');
  const d = await recordDecision(f.root, sign(f, request.file));
  assert.equal(d.state, 'decision-recorded');
  assert.equal(object(f.root, d.decision).authority, 'local-review-only');
  assert.equal(status(f.root).decision, d.decision);
  assert.match(fs.readFileSync(path.join(f.root, 'handoff.md'), 'utf8'), /external approval ledger/);
  await assert.rejects(recordDecision(f.root, sign(f, (await (async () => {
    const file = path.join(f.base, 'second.json'); fs.writeFileSync(file, '{}'); return file;
  })()))), /not awaiting/);
});

test('reject decision is distinct from accept and is durably recorded', async t => {
  const f = await completed(t), request = await requestDecision(f.root, 'reject');
  const d = await recordDecision(f.root, sign(f, request.file));
  const record = object(f.root, d.decision);
  assert.equal(JSON.parse(fs.readFileSync(path.join(f.root, 'objects', record.statement))).decision, 'reject');
});

test('unsigned, wrong-principal and wrong-namespace decisions are rejected', async t => {
  const f = await completed(t), request = await requestDecision(f.root, 'accept');
  const args = sign(f, request.file, 'wrong-namespace');
  await assert.rejects(recordDecision(f.root, args), /signature verification failed/);
  await assert.rejects(recordDecision(f.root, { ...args, principal: 'unlisted' }), /signature verification failed/);
  fs.writeFileSync(args.signatureFile, 'not a signature');
  await assert.rejects(recordDecision(f.root, args), /signature verification failed/);
  assert.equal(status(f.root).state, 'awaiting-decision');
});

test('a non-canonical decision statement cannot flip reject into accept', async t => {
  const f = await completed(t), request = await requestDecision(f.root, 'reject');
  const raw = fs.readFileSync(request.file, 'utf8');
  fs.writeFileSync(request.file, raw.replace('"decision": "reject"', '"decision": "reject", "decision": "accept"'));
  await assert.rejects(recordDecision(f.root, sign(f, request.file)), /canonical bytes/);
  assert.equal(status(f.root).state, 'awaiting-decision');
});

test('signed decision bound to another candidate or evidence is rejected', async t => {
  const f = await completed(t), request = await requestDecision(f.root, 'accept');
  const statement = JSON.parse(fs.readFileSync(request.file)); statement.evidenceDigest = 'a'.repeat(64);
  fs.writeFileSync(request.file, json(statement));
  await assert.rejects(recordDecision(f.root, sign(f, request.file)), /does not match/);
});

test('another run cannot consume a valid signature', async t => {
  const f = await completed(t), request = await requestDecision(f.root, 'accept');
  const args = sign(f, request.file);
  const other = await prepare(f.args); await run(other.root, { executor: fixtureExecutor });
  await assert.rejects(recordDecision(other.root, args), /does not match/);
});

for (const [name, options, expected] of [
  ['worker nonzero exit', { worker: 'process.exit(7)' }, /worker failed/],
  ['verifier failure', { verifier: 'process.exit(9)' }, /verifier failed/],
  ['worker timeout', { worker: 'setInterval(()=>{},1000)', deadlineMs: 100 }, /deadline exceeded/],
  ['excessive output', { worker: "console.log('x'.repeat(20000))", maxOutputBytes: 128 }, /output limit exceeded/],
  ['invalid reviewer JSON', { reviewer: "console.log('not json')" }, /JSON|Unexpected/],
  ['stale reviewer result', { reviewer: "console.log(JSON.stringify({schemaVersion:1,verdict:'pass',baseCommit:c.baseCommit,candidateCommit:'wrong',findings:[]}))" }, /another commit/],
  ['review asks for changes', { reviewer: "console.log(JSON.stringify({schemaVersion:1,verdict:'changes_requested',baseCommit:c.baseCommit,candidateCommit:c.candidateCommit,findings:[]}))" }, /review requested changes/],
  ['passing review with findings', { reviewer: "console.log(JSON.stringify({schemaVersion:1,verdict:'pass',baseCommit:c.baseCommit,candidateCommit:c.candidateCommit,findings:[{severity:'high',message:'broken'}]}))" }, /no unresolved findings/],
  ['worker packet rewrite', { worker: "fs.writeFileSync('specs/example/requirements.md','changed')" }, /changed feature packet/],
  ['verifier mutates candidate', { verifier: "fs.writeFileSync('answer.txt','changed')" }, /worktree changed/],
]) {
  test(name + ' retains failure evidence and blocks decision', async t => {
    const f = await completed(t, options);
    assert.equal(f.result.state, 'failed'); assert.match(f.result.failure, expected);
    assert.ok(status(f.root).jobs.length > 0);
    await assert.rejects(requestDecision(f.root, 'accept'), /not awaiting/);
    await assert.rejects(run(f.root, { executor: fixtureExecutor }), /not prepared/);
  });
}

test('altered candidate after verification invalidates the decision request', async t => {
  const f = await completed(t);
  fs.writeFileSync(path.join(f.workspace, 'answer.txt'), 'tampered');
  await assert.rejects(requestDecision(f.root, 'accept'), /worktree changed/);
});

test('ignored files cannot influence verified bytes without being in the candidate commit', async t => {
  const f = await completed(t);
  fs.writeFileSync(path.join(f.repo, '.git/info/exclude'), 'hidden.txt\n');
  fs.writeFileSync(path.join(f.workspace, 'hidden.txt'), 'uncommitted input');
  await assert.rejects(requestDecision(f.root, 'accept'), /not in the candidate commit|bytes differ|worktree changed/);
});

test('Git assume-unchanged flags cannot hide modified candidate bytes', async t => {
  const f = await completed(t);
  git(f.workspace, 'update-index', '--assume-unchanged', 'answer.txt');
  fs.writeFileSync(path.join(f.workspace, 'answer.txt'), 'other\n');
  assert.equal(git(f.workspace, 'status', '--porcelain'), '');
  await assert.rejects(requestDecision(f.root, 'accept'), /candidate bytes differ/);
});

test('content-addressed evidence corruption is detected on restart and import', async t => {
  const f = await completed(t);
  const job = object(f.root, object(f.root, f.result.evidence).jobs[1]);
  fs.writeFileSync(path.join(f.root, 'objects', job.stdout), 'tampered');
  assert.throws(() => status(f.root), /corrupt object/);
  await assert.rejects(requestDecision(f.root, 'accept'), /corrupt object/);
});

test('modified trust/config/tools require a new run', async t => {
  const f = await completed(t);
  fs.appendFileSync(path.join(f.tools, 'reviewer.mjs'), '// changed\n');
  await assert.rejects(requestDecision(f.root, 'accept'), /toolsDigest changed/);
});

test('changed configuration and signer list invalidate a pending decision', async t => {
  const f = await completed(t);
  const original = fs.readFileSync(f.configFile);
  fs.writeFileSync(f.configFile, json({ ...f.config, deadlineMs: 1000 }));
  await assert.rejects(requestDecision(f.root, 'accept'), /configDigest changed/);
  fs.writeFileSync(f.configFile, original);
  fs.appendFileSync(f.config.allowedSigners, '# changed\n');
  await assert.rejects(requestDecision(f.root, 'accept'), /signersDigest changed/);
});

test('worktree, config, trust path and packet preparation boundaries', async t => {
  const f = fixture(t);
  await assert.rejects(prepare({ ...f.args, store: path.join(f.repo, 'state') }), /outside the source checkout/);
  // A rejected store location is not created inside the source checkout.
  assert.equal(fs.existsSync(path.join(f.repo, 'state')), false);
  fs.chmodSync(f.configFile, 0o666);
  await assert.rejects(prepare(f.args), /writable trust path/);
  fs.chmodSync(f.configFile, 0o600);
  await assert.rejects(prepare({ ...f.args, packet: '../outside' }), /packet must/);
  fs.unlinkSync(path.join(f.repo, 'specs/example/design.md'));
  git(f.repo, 'add', '--all'); git(f.repo, 'commit', '-qm', 'incomplete packet');
  await assert.rejects(prepare(f.args), /ENOENT/);
});

test('prepared candidate may not change before execution', async t => {
  const f = fixture(t), p = await prepare(f.args);
  fs.writeFileSync(path.join(p.workspace, 'extra'), 'outside edit');
  const r = await run(p.root, { executor: fixtureExecutor });
  assert.equal(r.state, 'failed'); assert.match(r.failure, /prepared worktree changed/);
});

test('exclusive lease prevents a second controller and interrupted jobs are not replayed', async t => {
  const f = fixture(t), p = await prepare(f.args);
  let unlock, entered;
  const ready = new Promise(resolve => { entered = resolve; });
  const gate = new Promise(resolve => { unlock = resolve; });
  const first = leased(p.root, async () => { entered(); await gate; });
  await ready;
  await assert.rejects(run(p.root, { executor: fixtureExecutor }), /locked/);
  unlock(); await first;
  const manifest = JSON.parse(fs.readFileSync(path.join(p.root, 'run.json')));
  manifest.state = 'running'; fs.writeFileSync(path.join(p.root, 'run.json'), json(manifest));
  await assert.rejects(run(p.root, { executor: fixtureExecutor }), /not prepared/);
});

test('bounded runner kills process group descendants on deadline and cancellation', async t => {
  const f = fixture(t), pidfile = path.join(f.base, 'descendant.pid');
  const script = `const cp=require('node:child_process'),fs=require('node:fs');const p=cp.spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{stdio:'inherit'});fs.writeFileSync(${JSON.stringify(pidfile)},String(p.pid));setInterval(()=>{},1000);`;
  const r = await capture([process.execPath, '-e', script], { deadlineMs: 500, maxOutputBytes: 1000 });
  assert.equal(r.failure, 'deadline exceeded');
  const pid = Number(fs.readFileSync(pidfile, 'utf8'));
  // A killed orphan can briefly remain as a zombie until the host reaps it.
  try { const stat = fs.readFileSync(`/proc/${pid}/stat`, 'utf8'); assert.equal(stat.split(' ')[2], 'Z'); }
  catch (e) { if (e.code !== 'ENOENT') throw e; }
  const controller = new AbortController();
  const pending = capture([process.execPath, '-e', 'setInterval(()=>{},1000)'], { signal: controller.signal });
  controller.abort(); assert.equal((await pending).failure, 'cancelled');
});

test('sandbox argv is offline, minimally mounted, and does not expose controller state', t => {
  const f = fixture(t), argv = sandboxCommand(f.config, f.repo, f.config.worker, true);
  for (const flag of ['--unshare-all', '--unshare-user', '--new-session', '--die-with-parent', '--clearenv']) assert.ok(argv.includes(flag));
  assert.ok(!argv.includes('--share-net'));
  assert.ok(!argv.includes(f.args.store));
  assert.ok(!argv.includes(f.trust));
  assert.ok(argv.includes('/work/.git'));
});

test('real bubblewrap workflow and denied read/write/network paths', async t => {
  const capability = await doctor();
  if (!capability.ok) {
    assert.notEqual(process.env.HARNESS_REQUIRE_SANDBOX, '1', `sandbox required: ${capability.reason}`);
    t.skip(`host does not permit sandbox: ${capability.reason}`); return;
  }
  const f = fixture(t), p = await prepare(f.args);
  const r = await run(p.root);
  assert.equal(r.state, 'awaiting-decision', r.failure);
  const secret = path.join(f.trust, 'secret'); fs.writeFileSync(secret, 'synthetic secret');
  for (const [writable, source] of [
    [true, "import fs from 'node:fs';try{fs.writeFileSync('/work/.git','x');process.exit(1)}catch{}"],
    [true, `import fs from 'node:fs';try{fs.readFileSync(${JSON.stringify(secret)});process.exit(1)}catch{}`],
    [true, "import fs from 'node:fs';try{fs.writeFileSync('/tools/worker.mjs','x');process.exit(1)}catch{}"],
    [false, "import fs from 'node:fs';try{fs.writeFileSync('/work/answer.txt','x');process.exit(1)}catch{}"],
    [false, "import net from 'node:net';const s=net.connect(80,'1.1.1.1');s.on('connect',()=>process.exit(1));s.on('error',()=>process.exit(0));setTimeout(()=>process.exit(2),2000);"],
  ]) {
    const denied = await execute(f.config, p.workspace, [process.execPath, '--input-type=module', '-e', source], writable, {});
    assert.equal(denied.exitCode, 0, denied.stderr); assert.equal(denied.failure, undefined);
  }
  const request = await requestDecision(p.root, 'accept');
  const decision = await recordDecision(p.root, sign(f, request.file));
  assert.equal(decision.state, 'decision-recorded');
});

test('default CLI execution fails closed when isolation is unavailable', async t => {
  const capability = await doctor();
  if (capability.ok) { t.skip('sandbox is available; denial is tested on restricted hosts'); return; }
  const f = fixture(t), p = await prepare(f.args), r = await run(p.root);
  assert.equal(r.state, 'failed'); assert.match(r.failure, /worker failed/);
  assert.equal(fs.readFileSync(path.join(p.workspace, 'answer.txt'), 'utf8'), 'before\n');
  await assert.rejects(requestDecision(p.root, 'accept'), /not awaiting/);
});
