// Builds a throwaway Git repository holding a spec packet (`specs/demo`) and the pipeline
// (`.eng/pipeline.toml`), committed on `main`. Uses a fixed identity and no user or system Git
// configuration.
import { spawnSync } from 'node:child_process';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));

export const PACKET = 'specs/demo';

export function git(dir, ...args) {
  const r = spawnSync('/usr/bin/git', ['-C', dir, ...args], {
    encoding: 'utf8',
    env: {
      PATH: '/usr/bin:/bin',
      HOME: '/nonexistent',
      GIT_CONFIG_NOSYSTEM: '1',
      GIT_CONFIG_GLOBAL: '/dev/null',
      GIT_AUTHOR_NAME: 'Fixture',
      GIT_AUTHOR_EMAIL: 'fixture@example.invalid',
      GIT_COMMITTER_NAME: 'Fixture',
      GIT_COMMITTER_EMAIL: 'fixture@example.invalid',
    },
  });
  if (r.status !== 0) throw new Error(`git ${args.join(' ')}: ${r.stderr}`);
  return r.stdout.trim();
}

/** Returns `{ repo, base }`; `dir` must exist. */
export function makeRepo(dir) {
  const repo = path.join(dir, 'repo');
  fs.mkdirSync(path.join(repo, PACKET), { recursive: true });
  fs.mkdirSync(path.join(repo, '.eng'), { recursive: true });
  git(repo, 'init', '-q', '-b', 'main');
  fs.writeFileSync(path.join(repo, 'AGENTS.md'), 'Use small commits.\n');
  fs.writeFileSync(path.join(repo, 'README.md'), 'demo\n');
  fs.copyFileSync(path.join(here, 'packet/requirements.md'), path.join(repo, PACKET, 'requirements.md'));
  fs.copyFileSync(path.join(here, 'pipeline.toml'), path.join(repo, '.eng/pipeline.toml'));
  git(repo, 'add', '-A');
  git(repo, 'commit', '-q', '-m', 'base');
  return { repo, base: git(repo, 'rev-parse', 'HEAD') };
}
