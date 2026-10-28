#!/usr/bin/env node
// Claude Code PreToolUse hook: deny agent writes to the approval ledger root.
// Spec: docs/architecture/approval-ledger.md ("Who writes, and how it is enforced").
// Packet: specs/ledger-write-guard/. Defence in depth only; the OS permission on the
// ledger root is the real boundary, because shell commands can evade path matching.
//
// Deny = exit 2 with a reason on stderr. Allow = exit 0. Any internal failure denies.

import { readFileSync, realpathSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const DENY = 2;
const FILE_TOOLS = new Set(["Edit", "Write", "MultiEdit", "NotebookEdit"]);

function deny(reason) {
  process.stderr.write(`ledger-guard: ${reason}\n`);
  process.exit(DENY);
}

// realpath of the deepest existing ancestor, with the missing remainder appended, so a
// symlink in the path cannot hide the ledger root.
function realpathLoose(p) {
  let head = p;
  const tail = [];
  for (;;) {
    try {
      return path.join(realpathSync(head), ...tail.reverse());
    } catch {
      const parent = path.dirname(head);
      if (parent === head) return p;
      tail.push(path.basename(head));
      head = parent;
    }
  }
}

function isInside(root, candidate) {
  const rel = path.relative(root, candidate);
  return rel === "" || (rel !== ".." && !rel.startsWith(`..${path.sep}`) && !path.isAbsolute(rel));
}

// The root comes from a file beside this script, never from the environment: anything in
// the hook's environment can be set by the session (docs/security/data-egress.md).
function loadRoots() {
  const here = path.dirname(fileURLToPath(import.meta.url));
  let configured;
  try {
    configured = JSON.parse(readFileSync(path.join(here, "ledger-guard.config.json"), "utf8")).ledger_roots;
  } catch {
    deny("cannot read ledger-guard.config.json beside the hook; failing closed");
  }
  const valid =
    Array.isArray(configured) &&
    configured.length > 0 &&
    configured.every((r) => typeof r === "string" && path.isAbsolute(r));
  if (!valid) deny("ledger_roots must be a non-empty array of absolute paths; failing closed");
  const resolved = configured.map((r) => path.resolve(r));
  return [...new Set([...resolved, ...resolved.map(realpathLoose)])];
}

let input;
try {
  input = JSON.parse(readFileSync(0, "utf8"));
} catch {
  deny("hook input is not valid JSON; failing closed");
}

const roots = loadRoots();

if (input.hook_event_name !== undefined && input.hook_event_name !== "PreToolUse") {
  process.exit(0);
}

const tool = input.tool_name;
const args = input.tool_input ?? {};

if (FILE_TOOLS.has(tool)) {
  const target = args.file_path ?? args.notebook_path;
  if (typeof target !== "string" || target === "") deny(`${tool} call has no file path; failing closed`);
  const resolved = path.resolve(typeof input.cwd === "string" ? input.cwd : process.cwd(), target);
  const candidates = [resolved, realpathLoose(resolved)];
  if (roots.some((root) => candidates.some((c) => isInside(root, c)))) {
    deny(`agents may not write to the approval ledger (${tool}); request a transition instead`);
  }
} else if (tool === "Bash") {
  if (typeof args.command !== "string") deny("Bash call has no command; failing closed");
  if (roots.some((root) => args.command.includes(root))) {
    deny("shell commands may not reference the approval ledger; request a transition instead");
  }
}

process.exit(0);
