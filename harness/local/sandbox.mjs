import * as fs from 'node:fs';
import path from 'node:path';
import { invariant, inside } from './artifacts.mjs';
import { capture } from './process.mjs';

export function sandboxCommand(config, workspace, argv, writable) {
  invariant(process.platform === 'linux', 'Linux bubblewrap is required; no unsandboxed fallback');
  const mounts = config.runtimePaths;
  invariant(mounts.some(p => inside(p, argv[0])), 'executable must be in a configured runtime directory');
  const args = ['/usr/bin/bwrap', '--unshare-all', '--unshare-user', '--die-with-parent',
    '--new-session', '--cap-drop', 'ALL', '--clearenv'];
  for (const p of mounts) args.push('--ro-bind', p, p);
  args.push('--proc', '/proc', '--dev', '/dev', '--tmpfs', '/tmp', '--dir', '/work',
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
  if (process.platform !== 'linux' || !fs.existsSync('/usr/bin/bwrap')) {
    return { ok: false, reason: 'Linux /usr/bin/bwrap is required; no unsandboxed fallback' };
  }
  const paths = ['/usr', '/bin', '/lib', '/lib64'].filter(p => fs.existsSync(p));
  const argv = ['/usr/bin/bwrap', '--unshare-all', '--unshare-user', '--die-with-parent',
    '--new-session', '--cap-drop', 'ALL', '--clearenv'];
  for (const p of paths) argv.push('--ro-bind', p, p);
  argv.push('--proc', '/proc', '--dev', '/dev', '--tmpfs', '/tmp', '--', '/usr/bin/true');
  const result = await capture(argv, { deadlineMs: 5000, maxOutputBytes: 8192 });
  return { ok: result.exitCode === 0 && !result.failure, reason: result.failure ?? result.stderr.trim(),
    platform: process.platform, bwrap: path.resolve('/usr/bin/bwrap') };
}
