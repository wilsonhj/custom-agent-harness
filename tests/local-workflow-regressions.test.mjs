import { test } from 'node:test';
import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { prepare, run, requestDecision, recordDecision, git } from '../harness/local/workflow.mjs';
import { object, json } from '../harness/local/artifacts.mjs';
import { capture } from '../harness/local/process.mjs';
import { doctor } from '../harness/local/sandbox.mjs';

// ---- Helpers copied from local-workflow.test.mjs (they are not exported) ----

// Fixture-only executor. The public CLI cannot select this unsandboxed implementation.
async function fixtureExecutor(config, workspace, argv, writable, context, signal) {
  assert.equal(writable, context.role === 'worker');
  return capture(argv.map(a => a.replaceAll('{workspace}', workspace)), { cwd: workspace,
    env: { PATH: '/usr/bin:/bin', HOME: '/nonexistent' }, input: JSON.stringify(context),
    deadlineMs: config.deadlineMs, maxOutputBytes: config.maxOutputBytes, signal });
}

function makeTools(tools, options = {}) {
  const worker = options.worker ?? "fs.writeFileSync('answer.txt', 'after\\n');";
  const verify = options.verifier ?? "if(fs.readFileSync('answer.txt','utf8')!=='after\\n') process.exit(1);";
  const review = options.reviewer ?? "console.log(JSON.stringify({schemaVersion:1,verdict:'pass',baseCommit:c.baseCommit,candidateCommit:c.candidateCommit,findings:[]}));";
  for (const [name, body] of [['worker', worker], ['verifier', verify], ['reviewer', review]]) {
    fs.writeFileSync(path.join(tools, `${name}.mjs`), "import * as fs from 'node:fs';\nconst c=JSON.parse(fs.readFileSync(0,'utf8'));\n" + body + '\n');
  }
}

function makeTrust(trust) {
  const key = path.join(trust, 'fixture-key');
  execFileSync('/usr/bin/ssh-keygen', ['-q', '-t', 'ed25519', '-N', '', '-f', key]);
  const allowedSigners = path.join(trust, 'allowed_signers');
  fs.writeFileSync(allowedSigners, 'fixture namespaces="local-review-v1" ' + fs.readFileSync(key + '.pub', 'utf8'));
  return { key, allowedSigners };
}

function runtimePaths() {
  return [...new Set(['/usr', '/bin', '/lib', '/lib64', path.dirname(process.execPath)])].filter(p => fs.existsSync(p));
}

function fixture(t, options = {}) {
  const base = fs.mkdtempSync(path.join(fs.realpathSync(os.tmpdir()), 'harness-rt-'));
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
  makeTools(tools, options);
  const { key, allowedSigners } = makeTrust(trust);
  const config = { schemaVersion: 1, toolsDirectory: tools, allowedSigners,
    runtimePaths: runtimePaths(),
    worker: [process.execPath, path.join(tools, 'worker.mjs')],
    verifier: [process.execPath, path.join(tools, 'verifier.mjs')],
    reviewer: [process.execPath, path.join(tools, 'reviewer.mjs')],
    deadlineMs: options.deadlineMs ?? 8000, maxOutputBytes: options.maxOutputBytes ?? 32768 };
  const configFile = path.join(base, 'config.json'); fs.writeFileSync(configFile, json(config), { mode: 0o600 });
  return { base, repo, tools, trust, key, config, configFile,
    args: { repo, packet: 'specs/example', store: path.join(base, 'state'), configFile } };
}

function sign(f, file, namespace = 'local-review-v1') {
  execFileSync('/usr/bin/ssh-keygen', ['-Y', 'sign', '-f', f.key, '-n', namespace, file], { stdio: 'pipe' });
  return { statementFile: file, signatureFile: file + '.sig', principal: 'fixture' };
}

// Re-commit the source checkout so prepare sees a clean tree after a mutation.
function recommit(repo, message) {
  git(repo, 'add', '--all'); git(repo, 'commit', '-qm', message);
}

// ===========================================================================
// 1. Sandbox escape: a malicious nested-repo git clean filter must never run
//    on the host. Uses the real bubblewrap executor.
// ===========================================================================
test('bug1: nested-repo clean filter cannot write to a host path (sandbox escape)', async t => {
  const capability = await doctor();
  if (!capability.ok) { t.skip(`host does not permit sandbox: ${capability.reason}`); return; }
  const markerDir = fs.mkdtempSync(path.join(fs.realpathSync(os.tmpdir()), 'harness-marker-'));
  t.after(() => fs.rmSync(markerDir, { recursive: true, force: true }));
  const marker = path.join(markerDir, 'ESCAPED');
  const worker = `const { execFileSync } = await import('node:child_process');
const g=(...a)=>execFileSync('/usr/bin/git',a,{cwd:'/work',env:{...process.env,GIT_CONFIG_NOSYSTEM:'1',GIT_CONFIG_GLOBAL:'/dev/null',GIT_AUTHOR_NAME:'a',GIT_AUTHOR_EMAIL:'a@a',GIT_COMMITTER_NAME:'a',GIT_COMMITTER_EMAIL:'a@a'}});
fs.mkdirSync('/work/sub');
g('init','-q','/work/sub');
fs.writeFileSync('/work/sub/file.txt','original\\n');
g('-C','/work/sub','add','file.txt'); g('-C','/work/sub','commit','-qm','nested');
g('-C','/work/sub','config','filter.x.clean','sh -c "touch ${marker}; cat"');
fs.writeFileSync('/work/sub/.gitattributes','* filter=x\\n');
// Same size, newer mtime: Git must re-hash the file, which runs the clean filter.
fs.utimesSync('/work/sub/file.txt', new Date(Date.now() + 60000), new Date(Date.now() + 60000));
fs.writeFileSync('/work/answer.txt','after\\n');`;
  const f = fixture(t, { worker });
  const p = await prepare(f.args);
  const r = await run(p.root); // real bubblewrap executor
  assert.equal(r.state, 'failed', `expected the run to fail, got ${r.state}: ${r.failure}`);
  assert.equal(fs.existsSync(marker), false, 'host marker file must not be created by a sandboxed worker');
});

// ===========================================================================
// 2. Ignored / mode-0600 packet files in the source must not break a run.
// ===========================================================================
test('bug2a: an ignored file inside the packet directory still yields a successful run', async t => {
  const f = fixture(t);
  fs.writeFileSync(path.join(f.repo, '.gitignore'), 'ignored.txt\n');
  recommit(f.repo, 'add gitignore');
  fs.writeFileSync(path.join(f.repo, 'specs/example/ignored.txt'), 'local scratch\n'); // ignored, untracked
  assert.equal(git(f.repo, 'status', '--porcelain', '--untracked-files=all'), '', 'source must be clean');
  const p = await prepare(f.args);
  const r = await run(p.root, { executor: fixtureExecutor });
  assert.equal(r.state, 'awaiting-decision', `expected success, got ${r.state}: ${r.failure}`);
});

test('bug2b: a packet file with mode 0600 in the source still yields a successful run', async t => {
  const f = fixture(t);
  fs.chmodSync(path.join(f.repo, 'specs/example/requirements.md'), 0o600);
  assert.equal(git(f.repo, 'status', '--porcelain', '--untracked-files=all'), '', 'source must be clean');
  const p = await prepare(f.args);
  const r = await run(p.root, { executor: fixtureExecutor });
  assert.equal(r.state, 'awaiting-decision', `expected success, got ${r.state}: ${r.failure}`);
});

// ===========================================================================
// 3. A worker-created file with mode 0o654 must not fail the snapshot check.
// ===========================================================================
test('bug3: a candidate file with mode 0o654 passes the snapshot check', async t => {
  const worker = "fs.writeFileSync('answer.txt','after\\n');\n" +
    "fs.writeFileSync('extra.bin','payload\\n');\nfs.chmodSync('extra.bin',0o654);";
  const f = fixture(t, { worker });
  const p = await prepare(f.args);
  const r = await run(p.root, { executor: fixtureExecutor });
  assert.equal(r.state, 'awaiting-decision', `expected success, got ${r.state}: ${r.failure}`);
});

// ===========================================================================
// 4. The reviewer context.patch must end with "\n" and apply on the base.
// ===========================================================================
test('bug4: reviewer patch ends with a newline and applies with git apply --check', async t => {
  const f = fixture(t);
  const p = await prepare(f.args);
  const r = await run(p.root, { executor: fixtureExecutor });
  assert.equal(r.state, 'awaiting-decision', r.failure);
  const evidence = object(p.root, r.evidence);
  const reviewer = object(p.root, evidence.jobs[2]);
  const patch = reviewer.context.patch;
  assert.ok(patch.endsWith('\n'), 'reviewer patch must terminate with a newline');
  // The patch must apply cleanly against the base commit in a fresh checkout.
  const check = path.join(f.base, 'applycheck');
  git(f.repo, 'worktree', 'add', '--detach', check, r.baseCommit);
  const patchFile = path.join(f.base, 'candidate.patch');
  fs.writeFileSync(patchFile, patch);
  const applied = execFileSync('/usr/bin/git', ['-C', check, 'apply', '--check', patchFile],
    { encoding: 'utf8', env: { PATH: '/usr/bin:/bin', HOME: '/nonexistent' } });
  assert.equal(applied, '');
});

// ===========================================================================
// 5. A config whose toolsDirectory contains the repository is rejected.
// ===========================================================================
test('bug5: prepare rejects a toolsDirectory that contains the repository', async t => {
  const base = fs.mkdtempSync(path.join(fs.realpathSync(os.tmpdir()), 'harness-rt-'));
  t.after(() => fs.rmSync(base, { recursive: true, force: true }));
  const tools = path.join(base, 'td');           // toolsDirectory
  const repo = path.join(tools, 'repo');          // repository nested INSIDE toolsDirectory
  const trust = path.join(base, 'trust');
  for (const d of [tools, repo, trust]) fs.mkdirSync(d, { mode: 0o700 });
  git(repo, 'init', '-q');
  fs.mkdirSync(path.join(repo, 'specs/example'), { recursive: true });
  for (const file of ['requirements.md', 'design.md', 'acceptance-tests.md', 'handoff.md']) {
    fs.writeFileSync(path.join(repo, 'specs/example', file), '# Synthetic fixture\n');
  }
  fs.writeFileSync(path.join(repo, 'specs/example/tasks.json'), '[]\n');
  fs.writeFileSync(path.join(repo, 'answer.txt'), 'before\n');
  git(repo, 'add', '--all'); git(repo, 'commit', '-qm', 'synthetic base');
  makeTools(tools);
  const { allowedSigners } = makeTrust(trust);
  const config = { schemaVersion: 1, toolsDirectory: tools, allowedSigners, runtimePaths: runtimePaths(),
    worker: [process.execPath, path.join(tools, 'worker.mjs')],
    verifier: [process.execPath, path.join(tools, 'verifier.mjs')],
    reviewer: [process.execPath, path.join(tools, 'reviewer.mjs')],
    deadlineMs: 8000, maxOutputBytes: 32768 };
  const configFile = path.join(base, 'config.json'); fs.writeFileSync(configFile, json(config), { mode: 0o600 });
  await assert.rejects(prepare({ repo, packet: 'specs/example', store: path.join(base, 'state'), configFile }),
    /tools|repository|contain|inside|outside/i);
});

// ===========================================================================
// 6. Regressions guarding commit 5287b46.
// ===========================================================================
test('bug6a: runtimePaths entry escaping via .. or a symlink is rejected', async t => {
  const f = fixture(t);
  // Non-canonical traversal that resolves outside the allowlist.
  fs.writeFileSync(f.configFile, json({ ...f.config, runtimePaths: [...f.config.runtimePaths, '/usr/../tmp'] }), { mode: 0o600 });
  await assert.rejects(prepare(f.args), /runtime|allowlist|system/i);
  // A symlink whose real path leaves the allowlist.
  const link = path.join('/opt', `pr4dbg-link-${process.pid}-${Date.now()}`);
  const outside = fs.mkdtempSync(path.join(fs.realpathSync(os.tmpdir()), 'harness-out-'));
  t.after(() => { try { fs.unlinkSync(link); } catch {} fs.rmSync(outside, { recursive: true, force: true }); });
  try { fs.symlinkSync(outside, link); }
  catch (e) { t.diagnostic(`cannot create /opt symlink: ${e.message}`); return; }
  fs.writeFileSync(f.configFile, json({ ...f.config, runtimePaths: [...f.config.runtimePaths, link] }), { mode: 0o600 });
  await assert.rejects(prepare(f.args), /allowlist|runtime|real path/i);
});

test('bug6b: a verifier/reviewer argv containing {workspace} is rejected', async t => {
  const f = fixture(t);
  fs.writeFileSync(f.configFile, json({ ...f.config,
    verifier: [...f.config.verifier, '{workspace}/answer.txt'] }), { mode: 0o600 });
  await assert.rejects(prepare(f.args), /workspace/i);
});

test('bug6c: a decision statement with a duplicate "decision" key is rejected', async t => {
  const f = fixture(t);
  const p = await prepare(f.args);
  const r = await run(p.root, { executor: fixtureExecutor });
  assert.equal(r.state, 'awaiting-decision', r.failure);
  const request = await requestDecision(p.root, 'reject');
  const raw = fs.readFileSync(request.file, 'utf8');
  fs.writeFileSync(request.file, raw.replace('"decision": "reject"', '"decision": "reject", "decision": "accept"'));
  await assert.rejects(recordDecision(p.root, sign(f, request.file)), /canonical bytes/);
});

// ===========================================================================
// 7. A worker leaving an ignored build artifact fails closed, naming the path.
// ===========================================================================
test('bug7: an ignored worker artifact fails the run and the error names the path', async t => {
  const worker = "fs.writeFileSync('answer.txt','after\\n');\n" +
    "fs.writeFileSync('.gitignore','__pycache__/\\n');\n" +
    "fs.mkdirSync('__pycache__');\nfs.writeFileSync('__pycache__/a.pyc','bytecode\\n');";
  const f = fixture(t, { worker });
  const p = await prepare(f.args);
  const r = await run(p.root, { executor: fixtureExecutor });
  assert.equal(r.state, 'failed', `expected failure, got ${r.state}`);
  assert.match(r.failure, /__pycache__\/a\.pyc/);
});

// ===========================================================================
// 8. A packet file that is ignored (on disk but not in the commit) is rejected by prepare.
// ===========================================================================
test('bug8: a packet file missing from the commit is rejected even if present and ignored on disk', async t => {
  const f = fixture(t);
  git(f.repo, 'rm', '-q', '--cached', 'specs/example/handoff.md');
  fs.writeFileSync(path.join(f.repo, '.gitignore'), 'specs/example/handoff.md\n');
  recommit(f.repo, 'untrack handoff');
  assert.equal(git(f.repo, 'status', '--porcelain', '--untracked-files=all'), '', 'source must be clean');
  await assert.rejects(prepare(f.args), /missing packet file: handoff.md|ENOENT/);
});

// ===========================================================================
// 9. A store path through a symlinked parent into the repository creates nothing there.
// ===========================================================================
test('bug9: a store reached through a symlink into the repository is rejected before creation', async t => {
  const f = fixture(t);
  const link = path.join(f.base, 'into-repo');
  fs.symlinkSync(f.repo, link);
  await assert.rejects(prepare({ ...f.args, store: path.join(link, 'state') }), /outside the source checkout/);
  assert.equal(fs.existsSync(path.join(f.repo, 'state')), false);
});
