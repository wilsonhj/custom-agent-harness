// Shared helpers for the conformance cases. They drive the `eng` binary named by $ENG_BIN
// (default `target/debug/eng`) and nothing else of the implementation.
import { spawn, spawnSync } from 'node:child_process';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { git, makeRepo, PACKET } from '../fixtures/make-repo.mjs';

export { git, PACKET };

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const fixtures = path.join(root, 'conformance/fixtures');

export const ENG_BIN = path.resolve(root, process.env.ENG_BIN ?? 'target/debug/eng');
if (!fs.existsSync(ENG_BIN)) {
  throw new Error(`${ENG_BIN} not found: run "cargo build --workspace" (it also builds eng-shim) or set ENG_BIN`);
}

const NODE = fs.realpathSync(process.execPath);

/** A temp directory with a repository, a state directory and fake agents. */
export function setup(tag, { claudeScenario = 'clean' } = {}) {
  const tmp = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), `eng-conf-${tag}-`)));
  const { repo, base } = makeRepo(tmp);
  const state = path.join(tmp, 'state');
  fs.mkdirSync(state, { mode: 0o700 });
  // Exercise the production operator-grant path, using only this synthetic repository.
  fs.writeFileSync(path.join(state, 'egress-policy.json'), JSON.stringify({
    version: 1,
    project_root: repo,
    egress_class: 'any-provider',
    allowed_runtimes: ['claude', 'codex'],
    reviewed_by: 'fake-agent conformance fixture',
  }), { mode: 0o600 });
  const bin = path.join(tmp, 'bin');
  fs.mkdirSync(bin);
  const marker = path.join(tmp, 'agent-started');
  // Fake Claude behind a wrapper that records every start; fake Codex is launched the way real
  // Codex is: `node <entry>`.
  const claude = path.join(bin, 'claude');
  fs.writeFileSync(
    claude,
    `#!/bin/sh\ntouch '${marker}'\nFAKE_SCENARIO=${claudeScenario}\nexport FAKE_SCENARIO\n` +
      `exec '${NODE}' '${path.join(fixtures, 'fake-claude.mjs')}' "$@"\n`,
    { mode: 0o755 },
  );
  const env = {
    PATH: process.env.PATH,
    HOME: process.env.HOME,
    ENG_CLAUDE_BIN: claude,
    ENG_CODEX_BIN: path.join(fixtures, 'fake-codex.mjs'),
    ENG_NODE_BIN: NODE,
    ENG_EXTRA_READ_DIRS: [fixtures, path.dirname(NODE)].join(':'),
  };
  const ctx = { tmp, repo, base, state, bin, marker, env };
  ctx.eng = (args, opts = {}) => eng(ctx, args, opts);
  ctx.engTty = (args, input, opts = {}) => engTty(ctx, args, input, opts);
  ctx.cleanup = () => fs.rmSync(tmp, { recursive: true, force: true });
  return ctx;
}

function fullArgs(ctx, args, state = ctx.state) {
  return [...args, '--state-dir', state];
}

/** Runs `eng` with stdin closed (not a terminal). */
export function eng(ctx, args, { env = {}, json = false, state } = {}) {
  const r = spawnSync(ENG_BIN, fullArgs(ctx, json ? [...args, '--json'] : args, state), {
    encoding: 'utf8',
    env: { ...ctx.env, ...env },
    input: '',
    timeout: 120_000,
  });
  return { status: r.status, stdout: r.stdout, stderr: r.stderr, json: json ? parseJson(r.stdout) : undefined };
}

function parseJson(text) {
  const line = text.trim().split('\n').filter((l) => l.startsWith('{')).pop();
  return line ? JSON.parse(line) : undefined;
}

/**
 * Runs `eng` on a pseudo-terminal (`script(1)`), typing `input` once. `script` accepts an
 * anonymous pipe on stdin but not a socket (Node's 'pipe') or a FIFO, so a small shell builds
 * the pipe: a feeder types the input, then waits until `script` has finished, so the pty is
 * not closed early.
 */
export async function engTty(ctx, args, input, { env = {} } = {}) {
  const base = path.join(ctx.tmp, `tty-${Math.random().toString(36).slice(2)}`);
  const outPath = `${base}.out`;
  const donePath = `${base}.done`;
  const outFd = fs.openSync(outPath, 'w');
  const script =
    '( sleep 0.3; printf "%s" "$ENG_TTY_INPUT"; while [ ! -e "$ENG_TTY_DONE" ]; do sleep 0.05; done ) | ' +
    '( /usr/bin/script -q /dev/null "$@"; rc=$?; : > "$ENG_TTY_DONE"; exit $rc )';
  try {
    const child = spawn('/bin/sh', ['-c', script, 'sh', ENG_BIN, ...fullArgs(ctx, args)], {
      stdio: ['ignore', outFd, outFd],
      env: { ...ctx.env, ...env, ENG_TTY_INPUT: input, ENG_TTY_DONE: donePath },
    });
    const status = await new Promise((resolve, reject) => {
      const t = setTimeout(() => {
        child.kill('SIGKILL');
        reject(new Error(`eng ${args.join(' ')} on a pty timed out`));
      }, 120_000);
      child.on('close', (code) => {
        clearTimeout(t);
        resolve(code);
      });
    });
    const stdout = fs.readFileSync(outPath, 'utf8').replaceAll('\r\n', '\n');
    return { status, stdout, stderr: '' };
  } finally {
    fs.closeSync(outFd);
    fs.rmSync(outPath, { force: true });
    fs.rmSync(donePath, { force: true });
  }
}

/** `task new` for the demo packet; returns the task id and packet digest. */
export function newTask(ctx) {
  const r = ctx.eng(['task', 'new', '--repo', ctx.repo, '--packet', PACKET], { json: true });
  if (r.status !== 0) throw new Error(`task new failed: ${r.stdout}${r.stderr}`);
  return { id: r.json.task, packet: r.json.packet };
}

/** `task new` then `decide approve-spec` (typed `yes`). */
export async function approvedTask(ctx) {
  const t = newTask(ctx);
  const d = await ctx.engTty(['decide', t.id, 'approve-spec'], 'yes\n');
  if (d.status !== 0) throw new Error(`approve-spec failed (${d.status}): ${d.stdout}`);
  return t;
}

/** Create, approve and run to the first human gate. */
export async function runToGate(ctx) {
  const t = await approvedTask(ctx);
  const run = ctx.eng(['run', t.id], { json: true });
  return { ...t, run };
}
