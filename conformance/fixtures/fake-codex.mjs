#!/usr/bin/env node
// Fake `codex` (codex-cli 0.166.0) for engine and conformance tests. Never calls a model.
//
// `--version` prints the pinned version. Otherwise it reads the prompt on stdin (argv ends with
// `-`) and prints the `exec --json` events the Codex adapter parses (shapes copied from
// `crates/adapters/fixtures/codex/`). A reviewer is recognised by `--output-schema <file>`; the
// file must exist and be JSON. The scenario comes from `FAKE_SCENARIO`:
//   clean (default)   reviewer: verdict pass, no findings; implementer: edit like fake-claude
//   blocking          reviewer: one high-severity finding on feature.txt
import fs from 'node:fs';

const args = process.argv.slice(2);
if (args.includes('--version')) {
  process.stdout.write('codex-cli 0.166.0\n');
  process.exit(0);
}
const scenario = process.env.FAKE_SCENARIO || 'clean';
const schemaAt = args.indexOf('--output-schema');
const reviewer = schemaAt >= 0;
if (reviewer) JSON.parse(fs.readFileSync(args[schemaAt + 1], 'utf8'));
fs.readFileSync(0, 'utf8');
const emit = (event) => process.stdout.write(JSON.stringify(event) + '\n');

let text = 'Done.';
if (reviewer) {
  const review =
    scenario === 'blocking'
      ? {
          verdict: 'changes_requested',
          summary: 'One blocking issue.',
          findings: [
            {
              severity: 'high',
              category: 'correctness',
              file: 'feature.txt',
              line_start: 1,
              line_end: 1,
              claim: 'feature.txt is wrong',
              failure_scenario: 'the feature does not work',
              recommendation: 'fix feature.txt',
              suggested_check: null,
              confidence: 0.9,
            },
          ],
        }
      : { verdict: 'pass', summary: 'No issues.', findings: [] };
  text = JSON.stringify(review);
} else {
  fs.appendFileSync('ran.txt', 'run\n');
  fs.writeFileSync('feature.txt', 'implemented\n');
}

emit({ type: 'thread.started', thread_id: 'thr_fake' });
emit({ type: 'turn.started' });
emit({ type: 'item.completed', item: { id: 'item_1', type: 'agent_message', text } });
emit({
  type: 'turn.completed',
  usage: { input_tokens: 10, cached_input_tokens: 0, output_tokens: 5, reasoning_output_tokens: 0 },
});
