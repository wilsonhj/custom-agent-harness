// Run: node --test tests/ledger-guard.test.mjs
// Packet: specs/ledger-write-guard/acceptance-tests.md
import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, realpathSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SOURCE = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../harness/hooks/claude/ledger-guard.mjs");

let base, hookDir, hook, ledger, work;

before(() => {
  base = realpathSync(mkdtempSync(path.join(tmpdir(), "ledger-guard-")));
  hookDir = path.join(base, "hooks");
  ledger = path.join(base, "ledger");
  work = path.join(base, "work");
  for (const dir of [hookDir, ledger, work, path.join(base, "ledger-old")]) mkdirSync(dir);
  hook = path.join(hookDir, "ledger-guard.mjs");
  copyFileSync(SOURCE, hook);
  writeConfig({ ledger_roots: [ledger] });
});

after(() => rmSync(base, { recursive: true, force: true }));

function writeConfig(value) {
  writeFileSync(path.join(hookDir, "ledger-guard.config.json"), JSON.stringify(value));
}

function run(payload, { raw, env } = {}) {
  const input = raw ?? JSON.stringify({ hook_event_name: "PreToolUse", cwd: work, ...payload });
  return spawnSync(process.execPath, [hook], { input, encoding: "utf8", env: { ...process.env, ...env } });
}

const denied = (r) => {
  assert.equal(r.status, 2, `expected deny, got ${r.status}: ${r.stderr}`);
  assert.match(r.stderr, /^ledger-guard: /);
};
const allowed = (r) => assert.equal(r.status, 0, `expected allow, got ${r.status}: ${r.stderr}`);

test("denies Edit, Write, MultiEdit and NotebookEdit inside the ledger root", () => {
  const file = path.join(ledger, "project.jsonl");
  denied(run({ tool_name: "Edit", tool_input: { file_path: file } }));
  denied(run({ tool_name: "Write", tool_input: { file_path: file } }));
  denied(run({ tool_name: "MultiEdit", tool_input: { file_path: file } }));
  denied(run({ tool_name: "NotebookEdit", tool_input: { notebook_path: path.join(ledger, "n.ipynb") } }));
});

test("denies the ledger root itself", () => {
  denied(run({ tool_name: "Write", tool_input: { file_path: ledger } }));
});

test("denies .. traversal into the root", () => {
  denied(run({ tool_name: "Write", tool_input: { file_path: path.join(work, "..", "ledger", "x.jsonl") } }));
});

test("resolves relative paths against cwd", () => {
  denied(run({ tool_name: "Write", cwd: ledger, tool_input: { file_path: "x.jsonl" } }));
});

test("denies a symlink that points into the root", () => {
  const link = path.join(work, "sneaky");
  symlinkSync(ledger, link);
  denied(run({ tool_name: "Write", tool_input: { file_path: path.join(link, "x.jsonl") } }));
});

test("denies a dangling symlink aimed at a ledger file that does not exist yet", () => {
  const link = path.join(work, "future.jsonl");
  symlinkSync(path.join(ledger, "new.jsonl"), link);
  denied(run({ tool_name: "Write", tool_input: { file_path: link } }));
});

test("denies Bash commands that name the root, and allows other commands", () => {
  denied(run({ tool_name: "Bash", tool_input: { command: `echo hi >> ${ledger}/project.jsonl` } }));
  allowed(run({ tool_name: "Bash", tool_input: { command: "ls -la && git status" } }));
});

test("allows writes outside the root, including a sibling that shares its name prefix", () => {
  allowed(run({ tool_name: "Write", tool_input: { file_path: path.join(work, "a.txt") } }));
  allowed(run({ tool_name: "Write", tool_input: { file_path: path.join(base, "ledger-old", "a.txt") } }));
});

test("ignores other tools and other hook events", () => {
  allowed(run({ tool_name: "Read", tool_input: { file_path: path.join(ledger, "x") } }));
  allowed(run({ hook_event_name: "PostToolUse", tool_name: "Write", tool_input: { file_path: path.join(ledger, "x") } }));
});

test("fails closed on malformed input, missing paths, and missing commands", () => {
  denied(run({}, { raw: "not json" }));
  denied(run({ tool_name: "Write", tool_input: {} }));
  denied(run({ tool_name: "Write", tool_input: { file_path: "" } }));
  denied(run({ tool_name: "Bash", tool_input: {} }));
});

test("takes the root from the config file, not the environment", () => {
  const decoy = path.join(base, "decoy");
  mkdirSync(decoy, { recursive: true });
  const env = { LEDGER_ROOT: decoy, LEDGER_ROOTS: decoy, LEDGER_GUARD_CONFIG: decoy };
  denied(run({ tool_name: "Write", tool_input: { file_path: path.join(ledger, "x") } }, { env }));
  allowed(run({ tool_name: "Write", tool_input: { file_path: path.join(decoy, "x") } }, { env }));
});

test("fails closed on a missing or invalid config", () => {
  try {
    writeConfig({ ledger_roots: [] });
    denied(run({ tool_name: "Write", tool_input: { file_path: path.join(work, "a.txt") } }));
    writeConfig({ ledger_roots: ["relative/ledger"] });
    denied(run({ tool_name: "Write", tool_input: { file_path: path.join(work, "a.txt") } }));
    writeFileSync(path.join(hookDir, "ledger-guard.config.json"), "{ broken");
    denied(run({ tool_name: "Write", tool_input: { file_path: path.join(work, "a.txt") } }));
    rmSync(path.join(hookDir, "ledger-guard.config.json"));
    denied(run({ tool_name: "Write", tool_input: { file_path: path.join(work, "a.txt") } }));
  } finally {
    writeConfig({ ledger_roots: [ledger] });
  }
});
