import { spawn } from 'node:child_process';

// Shared bounded runner for sandbox processes and detached signature verification.
export function capture(argv, { cwd, env = {}, input = '', deadlineMs = 60000, maxOutputBytes = 1048576, signal } = {}) {
  return new Promise(resolve => {
    let outputBytes = 0, stdout = [], stderr = [], failure, settled = false;
    const startedAt = new Date().toISOString();
    const child = spawn(argv[0], argv.slice(1), { cwd, env, shell: false, detached: true, stdio: ['pipe', 'pipe', 'pipe'] });
    function kill(reason) {
      failure ??= reason;
      if (child.pid) { try { process.kill(-child.pid, 'SIGKILL'); } catch (e) { if (e.code !== 'ESRCH') failure = e.message; } }
    }
    const onAbort = () => kill('cancelled');
    signal?.addEventListener('abort', onAbort, { once: true });
    if (signal?.aborted) onAbort();
    const timer = setTimeout(() => kill('deadline exceeded'), deadlineMs);
    function collect(target, chunk) {
      outputBytes += chunk.length;
      if (outputBytes > maxOutputBytes) kill('output limit exceeded');
      else target.push(chunk);
    }
    child.stdout.on('data', c => collect(stdout, c));
    child.stderr.on('data', c => collect(stderr, c));
    // EPIPE is expected when a command exits without consuming its context.
    child.stdin.on('error', () => {});
    child.stdin.end(input);
    function finish(code, exitSignal) {
      if (settled) return;
      settled = true;
      clearTimeout(timer); signal?.removeEventListener('abort', onAbort);
      // Also clean up a descendant that inherited pipes after its parent exited.
      if (child.pid) { try { process.kill(-child.pid, 'SIGKILL'); } catch {} }
      resolve({ argv, startedAt, finishedAt: new Date().toISOString(), exitCode: code,
        signal: exitSignal, failure, stdout: Buffer.concat(stdout).toString(), stderr: Buffer.concat(stderr).toString() });
    }
    child.on('error', e => { failure = e.message; finish(null, null); });
    child.on('close', finish);
  });
}
