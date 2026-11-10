import * as fs from 'node:fs';
import { invariant, inside } from './artifacts.mjs';
import { capture } from './process.mjs';

const BWRAP = '/usr/bin/bwrap';

// Isolation flags and read-only runtime mounts shared by real runs and `doctor`, so the
// preflight exercises the same namespaces and mounts that a run needs.
function bwrapPrefix(mounts) {
  const args = [BWRAP, '--unshare-all', '--unshare-user', '--die-with-parent',
    '--new-session', '--cap-drop', 'ALL', '--clearenv'];
  for (const p of mounts) args.push('--ro-bind', p, p);
  args.push('--proc', '/proc', '--dev', '/dev', '--tmpfs', '/tmp');
  return args;
}

export function sandboxCommand(config, workspace, argv, writable) {
  invariant(process.platform === 'linux', 'Linux bubblewrap is required; no unsandboxed fallback');
  const mounts = config.runtimePaths;
  invariant(mounts.some(p => inside(p, argv[0])), 'executable must be in a configured runtime directory');
  const args = bwrapPrefix(mounts);
  args.push('--dir', '/work',
    writable ? '--bind' : '--ro-bind', workspace, '/work', '--ro-bind', '/dev/null', '/work/.git',
    '--ro-bind', config.toolsDirectory, '/tools', '--chdir', '/work',
    '--setenv', 'HOME', '/tmp', '--setenv', 'TMPDIR', '/tmp', '--setenv', 'PATH', '/usr/bin:/bin',
    '--setenv', 'LANG', 'C.UTF-8', '--');
  return args.concat(argv.map(a => a.replaceAll('{workspace}', '/work')
    .replaceAll(config.toolsDirectory + '/', '/tools/')));
}

export async function execute(config, workspace, argv, writable, context, signal) {
  const command = sandboxCommand(config, workspace, argv, writable);
  return capture(command, { input: JSON.stringify(context), deadlineMs: config.deadlineMs,
    maxOutputBytes: config.maxOutputBytes, signal });
}

export async function doctor() {
  if (process.platform !== 'linux' || !fs.existsSync(BWRAP)) {
    return { ok: false, reason: `Linux ${BWRAP} is required; no unsandboxed fallback` };
  }
  const paths = ['/usr', '/bin', '/lib', '/lib64'].filter(p => fs.existsSync(p));
  const argv = [...bwrapPrefix(paths), '--', '/usr/bin/true'];
  const result = await capture(argv, { deadlineMs: 5000, maxOutputBytes: 8192 });
  return { ok: result.exitCode === 0 && !result.failure, reason: result.failure ?? result.stderr.trim(),
    platform: process.platform, bwrap: BWRAP };
}
