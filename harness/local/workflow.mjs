import * as fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { atomic, digest, get, inside, invariant, json, leased, object, put, treeDigest, trustedPath } from './artifacts.mjs';
import { capture } from './process.mjs';
import { execute } from './sandbox.mjs';
import { assertSnapshot, scanWorkspace } from './snapshot.mjs';

const gitEnv = { PATH: '/usr/bin:/bin', HOME: '/nonexistent', LANG: 'C.UTF-8',
  GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: '/dev/null', GIT_TERMINAL_PROMPT: '0',
  GIT_AUTHOR_NAME: 'Harness Runner', GIT_AUTHOR_EMAIL: 'harness@localhost',
  GIT_COMMITTER_NAME: 'Harness Runner', GIT_COMMITTER_EMAIL: 'harness@localhost' };
// Host Git must never run code a worker controls: no hooks, fsmonitor or submodule recursion
// (a nested repository's own config, e.g. filter commands, would otherwise run on the host).
const gitArgs = ['--no-pager', '-c', 'core.hooksPath=/dev/null', '-c', 'core.fsmonitor=false',
  '-c', 'commit.gpgSign=false', '-c', 'submodule.recurse=false', '-c', 'diff.ignoreSubmodules=all',
  '-c', 'status.submoduleSummary=false', '-c', 'protocol.file.allow=never'];
function gitRaw(repo, args, maxBuffer = 16 * 1024 * 1024) {
  return execFileSync('/usr/bin/git', [...gitArgs, ...args], { cwd: repo, env: gitEnv, encoding: 'utf8', maxBuffer });
}
export function git(repo, ...args) {
  return gitRaw(repo, args).trim();
}
const PACKET_FILES = ['requirements.md', 'design.md', 'tasks.json', 'acceptance-tests.md', 'handoff.md'];

function checkPacketName(packet) {
  invariant(typeof packet === 'string' && /^specs\/[a-z0-9][a-z0-9-]*$/.test(packet), 'packet must be specs/<feature-id>');
}

// The packet is identified by its Git tree in a commit, not by on-disk metadata that Git does
// not track (ignored files, permission bits other than the executable bit).
function packetTree(repo, commit, packet) {
  checkPacketName(packet);
  const tree = git(repo, 'rev-parse', '--verify', '--end-of-options', `${commit}:${packet}`);
  invariant(git(repo, 'cat-file', '-t', tree) === 'tree', 'packet must be a directory in the commit');
  return tree;
}

function validatePacket(workspace, packet) {
  checkPacketName(packet);
  const dir = path.join(workspace, packet);
  invariant(fs.realpathSync(dir) === dir, 'packet path must not contain symlinks');
  for (const name of PACKET_FILES) {
    const file = path.join(dir, name);
    invariant(fs.lstatSync(file).isFile(), `missing packet file: ${name}`);
    invariant(fs.readFileSync(file, 'utf8').trim().length > 0, `empty packet file: ${name}`);
  }
  invariant(Array.isArray(JSON.parse(fs.readFileSync(path.join(dir, 'tasks.json'), 'utf8'))), 'tasks.json must be an array');
}

// An argument and, for `--name=value`, its value: both can name a path.
const argumentValues = a => a.includes('=') ? [a, a.slice(a.indexOf('=') + 1)] : [a];
const SANDBOX_WORKSPACE = '/work';

// Verifier and reviewer entrypoints must stay outside the candidate. They start in the
// workspace, so a relative argument (`review.mjs`, `.`) would resolve to worker-written
// files; refuse any argument that names an existing candidate path.
function assertNoCandidateArguments(workspace, role, argv) {
  for (const value of argv.slice(1).flatMap(argumentValues)) {
    if (!value || path.isAbsolute(value)) continue;
    const target = path.resolve(workspace, value);
    if (!inside(workspace, target)) continue;
    let exists = true;
    try { fs.lstatSync(target); } catch { exists = false; }
    invariant(!exists, `${role} argument names a candidate file; keep its entrypoint outside the candidate: ${value}`);
  }
}

function loadConfig(file, repo, store) {
  trustedPath(file);
  invariant(!inside(repo, file) && !inside(store, file), 'config must be outside repository and store');
  const bytes = fs.readFileSync(file), c = JSON.parse(bytes);
  invariant(c.schemaVersion === 1, 'unsupported config schema');
  invariant(Number.isSafeInteger(c.deadlineMs) && c.deadlineMs >= 10 && c.deadlineMs <= 3600000, 'invalid deadlineMs');
  invariant(Number.isSafeInteger(c.maxOutputBytes) && c.maxOutputBytes >= 128 && c.maxOutputBytes <= 16777216, 'invalid maxOutputBytes');
  c.toolsDirectory = trustedPath(c.toolsDirectory, true);
  c.allowedSigners = trustedPath(c.allowedSigners);
  for (const p of [c.toolsDirectory, c.allowedSigners]) {
    invariant(!inside(repo, p) && !inside(store, p) && !inside(p, repo) && !inside(p, store),
      'tools and trust roots must not contain or be inside the repository or store');
  }
  invariant(Array.isArray(c.runtimePaths) && c.runtimePaths.length > 0, 'runtimePaths required');
  const runtimeRoots = ['/usr', '/bin', '/lib', '/lib64', '/opt'];
  const resolvedRuntimes = [];
  for (const p of c.runtimePaths) {
    invariant(typeof p === 'string' && path.isAbsolute(p) && p !== '/', 'only system runtime directories may be mounted');
    const resolved = path.resolve(p);
    invariant(runtimeRoots.some(root => inside(root, resolved)) && resolved !== '/opt', 'only system runtime directories may be mounted');
    const real = fs.realpathSync(resolved);
    invariant(runtimeRoots.some(root => inside(root, real)), 'runtime mount real path leaves the allowlist');
    invariant(fs.statSync(real).isDirectory(), 'runtime mount is not a directory');
    invariant(!inside(real, repo) && !inside(real, store) && !inside(real, c.allowedSigners), 'runtime mount exposes trusted state');
    resolvedRuntimes.push(resolved);
  }
  c.runtimePaths = resolvedRuntimes;
  for (const role of ['worker', 'verifier', 'reviewer']) {
    const argv = c[role];
    invariant(Array.isArray(argv) && argv.length > 0 && argv.every(a => typeof a === 'string' && !a.includes('\0')), `invalid ${role} argv`);
    invariant(path.isAbsolute(argv[0]) && c.runtimePaths.some(p => inside(p, argv[0])), `${role} executable must be absolute in runtime mount`);
    if (role !== 'worker') {
      invariant(argv.every(a => !a.includes('{workspace}') && argumentValues(a).every(v =>
        !(path.isAbsolute(v) && inside(SANDBOX_WORKSPACE, path.resolve(v))))), `${role} must not run a workspace path`);
    }
    for (const arg of argv.filter(a => path.isAbsolute(a))) {
      const resolved = path.resolve(arg);
      invariant(!inside(repo, resolved) && !inside(store, resolved), `${role} path must stay outside the repository and store`);
    }
  }
  invariant(json(c.worker) !== json(c.reviewer), 'worker and reviewer commands must be distinct');
  return { config: c, configDigest: digest(bytes), toolsDigest: treeDigest(c.toolsDirectory, true),
    signersDigest: digest(fs.readFileSync(c.allowedSigners)) };
}

function load(root) {
  trustedPath(root, true);
  const m = JSON.parse(fs.readFileSync(trustedPath(path.join(root, 'run.json')), 'utf8'));
  invariant(m.schemaVersion === 1 && typeof m.id === 'string', 'invalid run manifest');
  for (const ref of m.events) object(root, ref);
  return m;
}

function save(root, m, event) {
  m.events.push(put(root, { event, state: m.state, at: new Date().toISOString() }));
  const text = `# Local workflow ${m.id}\n\nState: ${m.state}\nBase: ${m.baseCommit}\n` +
    `Candidate: ${m.candidateCommit ?? 'not recorded'}\nPacket digest: ${m.packetDigest}\n` +
    `Evidence: ${m.evidence ?? 'not recorded'}\nDecision: ${m.decision ?? 'not recorded'}\n\n` +
    `## Limitations\n\nLocal execution evidence only. No task-phase or release approval.\n` +
    `Configuration, controller OS account and initial Git configuration must be trusted.\n\n` +
    `## Exact next action\n\n${m.state === 'awaiting-decision' ? 'A human inspects evidence and independently signs an accept/reject statement; import it with record-decision.' :
      m.state === 'decision-recorded' ? 'Human inspects the recorded decision; use the external approval ledger for any task phase transition.' :
      m.state === 'prepared' ? 'Run the configured isolated worker, verifier and reviewer.' :
      'Inspect recorded failure/interruption evidence and prepare a new run; do not replay this run.'}\n` +
    (m.failure ? `\nFailure: ${m.failure}\n` : '');
  m.handoff = put(root, text);
  atomic(path.join(root, 'handoff.md'), text);
  atomic(path.join(root, 'run.json'), json(m));
}

function currentConfig(m) {
  const c = loadConfig(m.configFile, m.repo, m.store);
  for (const name of ['configDigest', 'toolsDigest', 'signersDigest']) invariant(c[name] === m[name], `${name} changed; prepare a new run`);
  return c.config;
}

function candidateUnchanged(m) {
  scanWorkspace(m.workspace);
  invariant(git(m.workspace, 'rev-parse', 'HEAD') === m.candidateCommit, 'candidate HEAD changed');
  invariant(git(m.workspace, 'status', '--porcelain', '--untracked-files=all', '--ignore-submodules=all') === '', 'candidate worktree changed');
  assertSnapshot(m.workspace, m.candidateCommit, git);
  invariant(packetTree(m.workspace, m.candidateCommit, m.packet) === m.packetDigest, 'feature packet changed');
}

export async function prepare({ repo, packet, store, configFile }) {
  repo = fs.realpathSync(repo); store = path.resolve(store); configFile = path.resolve(configFile);
  invariant(git(repo, 'rev-parse', '--show-toplevel') === repo, 'repo must be the checkout root');
  invariant(git(repo, 'status', '--porcelain', '--untracked-files=all') === '', 'source checkout must be clean');
  const common = fs.realpathSync(path.resolve(repo, git(repo, 'rev-parse', '--git-common-dir')));
  // Check the requested location (through its deepest existing ancestor's real path) before
  // creating anything, then again after resolving it.
  const storeOk = s => !inside(repo, s) && !inside(s, repo) && !inside(common, s);
  let existing = store;
  while (!fs.existsSync(existing)) existing = path.dirname(existing);
  const planned = path.join(fs.realpathSync(existing), path.relative(existing, store));
  invariant(storeOk(store) && storeOk(planned), 'store must be outside the source checkout and Git administration directory');
  fs.mkdirSync(store, { recursive: true, mode: 0o700 }); store = trustedPath(store, true);
  invariant(storeOk(store), 'store must be outside the source checkout and Git administration directory');
  const frozen = loadConfig(configFile, repo, store);
  validatePacket(repo, packet);
  const baseCommit = git(repo, 'rev-parse', 'HEAD'), pDigest = packetTree(repo, baseCommit, packet);
  const id = randomUUID(), root = path.join(store, id), workspace = path.join(root, 'workspace');
  fs.mkdirSync(root, { mode: 0o700 });
  try {
    git(repo, 'worktree', 'add', '--detach', workspace, baseCommit);
    // The packet must be complete in the commit, not only on disk (an ignored file would pass).
    validatePacket(workspace, packet);
    const m = { schemaVersion: 1, id, repo, store, workspace, packet, configFile, baseCommit,
      packetDigest: pDigest, configDigest: frozen.configDigest, toolsDigest: frozen.toolsDigest,
      signersDigest: frozen.signersDigest, state: 'prepared', events: [] };
    save(root, m, 'prepared');
    return { root, ...m };
  } catch (e) {
    // Retain partial state for diagnosis; never delete a user checkout on failure.
    throw new Error(`prepare failed; inspect ${root}: ${e.message}`);
  }
}

function reviewResult(stdout, m) {
  const r = JSON.parse(stdout);
  invariant(r.schemaVersion === 1 && ['pass', 'changes_requested'].includes(r.verdict), 'invalid review verdict');
  invariant(r.baseCommit === m.baseCommit && r.candidateCommit === m.candidateCommit, 'review is for another commit');
  invariant(Array.isArray(r.findings) && r.findings.every(f => typeof f === 'object' && f !== null &&
    ['critical', 'high', 'medium', 'low'].includes(f.severity) && typeof f.message === 'string' && f.message.trim()), 'invalid review findings');
  invariant(r.verdict !== 'pass' || r.findings.length === 0, 'passing review must have no unresolved findings');
  return r;
}

// Executor injection is only a trusted-controller testing seam. CLI always uses execute.
export async function run(root, { executor = execute, signal } = {}) {
  return leased(root, async () => {
    const m = load(root);
    invariant(m.state === 'prepared', 'run is not prepared; interrupted/finished runs are not replayed');
    const config = currentConfig(m), jobs = [];
    m.state = 'running'; save(root, m, 'execution-started');
    try {
      scanWorkspace(m.workspace);
      invariant(git(m.workspace, 'rev-parse', 'HEAD') === m.baseCommit &&
        git(m.workspace, 'status', '--porcelain', '--untracked-files=all', '--ignore-submodules=all') === '', 'prepared worktree changed');
      assertSnapshot(m.workspace, m.baseCommit, git);
      for (const role of ['worker', 'verifier', 'reviewer']) {
        currentConfig(m);
        // The candidate was checked in full right after the previous role, and only controller
        // code runs in between, so it is not re-hashed here.
        if (role !== 'worker') assertNoCandidateArguments(m.workspace, role, config[role]);
        const context = { schemaVersion: 1, runId: m.id, role, baseCommit: m.baseCommit,
          candidateCommit: m.candidateCommit ?? null, packet: m.packet, packetDigest: m.packetDigest };
        // Untrimmed: a patch must keep its final newline. The buffer covers the snapshot limits
        // (64 MiB of files) after base85 encoding of binary changes.
        if (role === 'reviewer') context.patch = gitRaw(m.workspace, ['diff', '--no-ext-diff', '--no-textconv',
          '--binary', m.baseCommit, m.candidateCommit, '--'], 256 * 1024 * 1024);
        const result = await executor(config, m.workspace, config[role], role === 'worker', context, signal);
        const { stdout, stderr, ...execution } = result;
        const evidence = { role, ...execution, command: config[role], context,
          stdout: put(root, stdout ?? ''), stderr: put(root, stderr ?? '') };
        jobs.push(put(root, evidence));
        // Persist each completed job before advancing. Errors retain prior evidence.
        m.jobs = [...jobs]; save(root, m, `${role}-finished`);
        invariant(result.exitCode === 0 && !result.failure, `${role} failed: ${result.failure ?? result.exitCode}`);
        if (role === 'worker') {
          // Reject nested repositories, special files and oversized output before host Git
          // reads or stores any worker-written bytes.
          scanWorkspace(m.workspace);
          invariant(git(m.workspace, 'rev-parse', 'HEAD') === m.baseCommit, 'worker changed Git HEAD');
          git(m.workspace, 'add', '--all');
          const tree = git(m.workspace, 'write-tree');
          m.candidateCommit = git(m.workspace, 'commit-tree', tree, '-p', m.baseCommit, '-m', `Local candidate ${m.id}`);
          git(m.workspace, 'reset', '--hard', m.candidateCommit);
          invariant(packetTree(m.workspace, m.candidateCommit, m.packet) === m.packetDigest, 'worker changed feature packet');
          save(root, m, 'candidate-recorded');
          candidateUnchanged(m);
        } else {
          candidateUnchanged(m);
          if (role === 'reviewer') {
            const review = reviewResult(stdout, m);
            invariant(review.verdict === 'pass', 'review requested changes');
          }
        }
      }
      currentConfig(m);
      m.evidence = put(root, { schemaVersion: 1, runId: m.id, baseCommit: m.baseCommit,
        candidateCommit: m.candidateCommit, packetDigest: m.packetDigest, configDigest: m.configDigest,
        toolsDigest: m.toolsDigest, jobs });
      m.state = 'awaiting-decision'; save(root, m, 'decision-ready');
    } catch (e) {
      m.state = 'failed'; m.failure = e.message; save(root, m, 'execution-failed');
    }
    return { root, ...m };
  });
}

function verifiedEvidence(root, m) {
  const e = object(root, m.evidence);
  invariant(e.runId === m.id && e.baseCommit === m.baseCommit && e.candidateCommit === m.candidateCommit &&
    e.packetDigest === m.packetDigest && e.configDigest === m.configDigest && e.toolsDigest === m.toolsDigest, 'evidence context mismatch');
  invariant(e.jobs.length === 3, 'incomplete evidence');
  for (const [i, ref] of e.jobs.entries()) {
    const job = object(root, ref);
    invariant(job.role === ['worker', 'verifier', 'reviewer'][i] && job.exitCode === 0 && !job.failure, 'job evidence is not successful');
    get(root, job.stdout); get(root, job.stderr);
    if (job.role === 'reviewer') invariant(reviewResult(get(root, job.stdout).toString(), m).verdict === 'pass', 'review requested changes');
  }
  return e;
}

function decisionStatement(m, decision) {
  invariant(['accept', 'reject'].includes(decision), 'decision must be accept or reject');
  return { schemaVersion: 1, kind: 'local-review-decision', runId: m.id, baseCommit: m.baseCommit,
    candidateCommit: m.candidateCommit, evidenceDigest: m.evidence, decision };
}

export async function requestDecision(root, decision) {
  return leased(root, async () => {
    const m = load(root);
    invariant(m.state === 'awaiting-decision', 'run is not awaiting a decision');
    currentConfig(m); candidateUnchanged(m); verifiedEvidence(root, m);
    const statement = json(decisionStatement(m, decision));
    const file = path.join(root, `request-${decision}.json`);
    atomic(file, statement);
    return { file, sha256: digest(statement), namespace: 'local-review-v1', authority: 'request-only' };
  });
}

export async function recordDecision(root, { statementFile, signatureFile, principal }) {
  return leased(root, async () => {
    const m = load(root);
    invariant(m.state === 'awaiting-decision', 'run is not awaiting a decision');
    const c = currentConfig(m);
    candidateUnchanged(m); verifiedEvidence(root, m);
    invariant(typeof principal === 'string' && /^[A-Za-z0-9@._+-]+$/.test(principal), 'explicit signer principal required');
    const bytes = fs.readFileSync(statementFile);
    let statement;
    try { statement = JSON.parse(bytes); } catch { invariant(false, 'decision statement is not JSON'); }
    invariant(typeof statement === 'object' && statement !== null && !Array.isArray(statement), 'decision statement must be a JSON object');
    invariant(bytes.equals(Buffer.from(json(statement))), 'decision statement is not the canonical bytes');
    invariant(json(statement) === json(decisionStatement(m, statement.decision)), 'decision does not match run and evidence');
    const signature = fs.readFileSync(signatureFile);
    invariant(signature.length < 16384, 'oversized signature');
    // Verify the exact signature bytes retained as evidence, not a replaceable input path.
    const sigDigest = put(root, signature), sig = path.join(root, 'objects', sigDigest);
    const verification = await capture(['/usr/bin/ssh-keygen', '-Y', 'verify', '-f', c.allowedSigners,
      '-I', principal, '-n', 'local-review-v1', '-s', sig], { input: bytes, deadlineMs: 5000, maxOutputBytes: 16384 });
    invariant(verification.exitCode === 0 && !verification.failure, 'human signature verification failed');
    currentConfig(m); candidateUnchanged(m); verifiedEvidence(root, m);
    m.decision = put(root, { statement: put(root, bytes), signature: sigDigest, principal,
      verifiedAt: new Date().toISOString(), verification: put(root, verification), authority: 'local-review-only' });
    m.state = 'decision-recorded'; save(root, m, 'human-decision-recorded');
    return { root, ...m };
  });
}

export function status(root) {
  const m = load(root);
  get(root, m.handoff);
  for (const ref of m.jobs ?? []) { const j = object(root, ref); get(root, j.stdout); get(root, j.stderr); }
  if (m.evidence) verifiedEvidence(root, m);
  if (m.decision) {
    const d = object(root, m.decision);
    get(root, d.statement); get(root, d.signature); get(root, d.verification);
  }
  return { root, ...m };
}
