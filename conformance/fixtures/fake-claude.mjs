#!/usr/bin/env node
// Fake `claude` (Claude Code 2.1.318) for engine and conformance tests. Never calls a model.
//
// `--version` prints the pinned version. Otherwise it reads the prompt on stdin, edits files in
// its working directory as an implementer (a reviewer, recognised by `--json-schema`, edits
// nothing), and prints the stream-json events the Claude adapter parses (shapes copied from
// `crates/adapters/fixtures/claude/`). The scenario comes from `FAKE_SCENARIO`:
//   clean (default)        append "run" to ran.txt, write feature.txt, succeed
//   blocking               as clean (only the reviewer side of a scenario differs)
//   write_token            also write a token-shaped string into config.txt
//   hang                   write the files, print `init`, then never exit
//   sandbox_unconfirmed    as clean, but `init` does not confirm the own sandbox is off
//   plant_home             as clean, and also plant `$HOME/.cargo/config.toml` (a config file
//                          that would change what a later `cargo test` in that HOME does)
//   echo_file              as clean, and also copy the file named by `FAKE_ECHO_FILE` to stderr
import fs from 'node:fs';

const args = process.argv.slice(2);
if (args.includes('--version')) {
  process.stdout.write('2.1.318 (Claude Code)\n');
  process.exit(0);
}
const scenario = process.env.FAKE_SCENARIO || 'clean';
const reviewer = args.includes('--json-schema');
const prompt = fs.readFileSync(0, 'utf8');
const session = '22222222-2222-4222-8222-222222222222';
const emit = (event) => process.stdout.write(JSON.stringify(event) + '\n');

if (!reviewer) {
  fs.appendFileSync('ran.txt', 'run\n');
  const repair = prompt.includes('Review findings to address') || prompt.includes('verification output');
  fs.writeFileSync('feature.txt', repair ? 'repaired\n' : 'implemented\n');
  if (scenario === 'plant_home') {
    fs.mkdirSync(`${process.env.HOME}/.cargo`, { recursive: true });
    fs.writeFileSync(`${process.env.HOME}/.cargo/config.toml`, '[alias]\ntest = "help"\n');
  }
  if (scenario === 'echo_file') {
    process.stderr.write(fs.readFileSync(process.env.FAKE_ECHO_FILE));
  }
  if (scenario === 'write_token') {
    // Assembled so this file never contains a contiguous token-shaped string.
    fs.writeFileSync('config.txt', 'api_key=' + ['sk', 'ant', 'api03', 'x'.repeat(40)].join('-') + '\n');
  }
}

const init = {
  type: 'system',
  subtype: 'init',
  session_id: session,
  tools: reviewer ? ['Read', 'Grep', 'Glob'] : ['Bash', 'Read', 'Edit', 'Write', 'Grep', 'Glob'],
  mcp_servers: [],
  permissionMode: 'default',
  apiKeySource: 'none',
  claude_code_version: '2.1.318',
};
if (scenario !== 'sandbox_unconfirmed') init.sandbox = { enabled: false };
emit(init);

if (scenario === 'hang') {
  fs.writeFileSync('agent.pid', String(process.pid));
  setInterval(() => {}, 60_000);
} else {
  emit({
    type: 'assistant',
    message: { role: 'assistant', content: [{ type: 'text', text: 'Working on it.' }] },
    session_id: session,
  });
  const result = {
    type: 'result',
    subtype: 'success',
    is_error: false,
    result: reviewer ? '' : 'Done.',
    session_id: session,
    num_turns: 1,
    usage: { input_tokens: 10, output_tokens: 5 },
  };
  if (reviewer) result.structured_output = { verdict: 'pass', summary: 'No issues.', findings: [] };
  emit(result);
}
