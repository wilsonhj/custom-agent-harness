# Local evidence workflow (0.1 pilot)

This implements the first local slice: packet -> isolated worker -> verification -> separate
reviewer -> human review decision -> durable handoff. Linux, Node 24 and Git are required.
No hosted model calls, task-phase approvals, migrations or deployments are implemented.
The existing `canTransition` policy remains fail closed. This pilot is independent of the
pending policy and ledger-hook work in PRs #2 and #3.

## Trust and installation

Run the controller from a reviewed, fixed installation outside the candidate checkout.
Do not run candidate-controlled controller code or load commands from a feature packet.
The controller account, its Git installation and the source repository's local Git config
are trusted. For untrusted repositories use a separate machine/VM; this pilot does not
sanitize arbitrary Git extensions, filters, credential helpers or filesystem configuration.

Workers receive only the worktree and operator-selected runtime/tool mounts. Bubblewrap
provides the boundary: private network/PID/user namespaces, no inherited environment or
home directory, and an immutable `.git` placeholder. Only workers get a writable worktree.
Verifier and reviewer mounts are read-only. There is no unsandboxed CLI flag.

Use a dedicated controller account if another agent has unrestricted host access as your
normal user. File modes alone cannot distinguish two host processes running as the same
user. Protect the controller installation, external configuration, tools and signer list.
The run store must be outside the source checkout and Git administration directory.

Install bubblewrap using your OS package manager. Then, from the trusted installation:

```bash
node harness/local/cli.mjs doctor
npm run check
npm test
```

Doctor exits nonzero when namespaces are unavailable. Never replace this with a direct
command execution fallback. CI sets `HARNESS_REQUIRE_SANDBOX=1` so a skipped sandbox test
cannot produce a passing gate. macOS and Windows execution backends are not implemented.

## Operator configuration

Copy `config/local-workflow.example.json` OUTSIDE both the source repository and run store.
Replace every example path. Set executable paths to the actual Node or local-tool binary;
`command -v node` may resolve outside `/usr`. Add its specific system runtime directory to
`runtimePaths` when necessary. Mount the minimum directories needed; never mount secrets,
home directories, the controller store, credentials or an entire general-purpose `/opt`.
No install step or inherited package manager/model credentials run inside a job.

`toolsDirectory` contains independently reviewed local command scripts. It is mounted at
`/tools`, read-only. Its directory and every file must be owned by the controller or root
and not group/other writable. Config, tool bytes and modes, and the signer file are pinned
when a run is prepared; changing any of them requires a new run.

- `worker`: a local executable that edits the worktree; no stdout schema is required.
- `verifier`: a trusted acceptance command; exit 0 means its checks passed. Keep the
  verifier's entrypoint outside the candidate; a candidate-supplied test suite alone is
  not an independent acceptance specification.
- `reviewer`: a distinct command that inspects the patch and candidate against the packet.
  It emits ONLY the review JSON described below, and exits 0.

All command arrays are argv, not shell strings. Executables must be absolute paths within
runtime mounts. Arguments may contain `{workspace}`, replaced with `/work`. Absolute
references under `toolsDirectory` are translated to `/tools`. Each process starts in
`/work` and receives JSON on stdin:

```json
{
  "schemaVersion": 1,
  "runId": "uuid",
  "role": "reviewer",
  "baseCommit": "base SHA",
  "candidateCommit": "candidate SHA",
  "packet": "specs/example",
  "packetDigest": "sha256",
  "patch": "git binary diff; present for reviewer only"
}
```

The worker's candidateCommit is null. A reviewer cannot use the hidden Git administration
directory; the controller supplies the base-to-candidate patch. A structured review is:

```json
{
  "schemaVersion": 1,
  "verdict": "pass",
  "baseCommit": "the supplied base SHA",
  "candidateCommit": "the supplied candidate SHA",
  "findings": []
}
```

Use `changes_requested` and findings with `severity` (critical/high/medium/low) and `message`
when changes are required. A passing review with unresolved findings is rejected. Fixture
reviewers in tests verify this transport contract; they do not establish AI review quality.
Bring a separately operated offline reviewer/local model through this command interface.
Networked Codex/Claude integrations require a later egress-governed adapter.

## Prepare and execute

The source checkout must be clean and contain a committed packet with requirements.md,
design.md, tasks.json (array), acceptance-tests.md and handoff.md. Packet files cannot be
symlinks or empty. They remain immutable for the run. The source checkout is never edited.

```bash
node harness/local/cli.mjs prepare \
  --repo /absolute/source-checkout \
  --packet specs/example \
  --store /absolute/controller-state \
  --config /absolute/operator-owned/config.json

node harness/local/cli.mjs run --run /absolute/controller-state/RUN_ID
node harness/local/cli.mjs status --run /absolute/controller-state/RUN_ID
```

Prepare prints the run ID and path. The candidate is a detached commit recorded by the
controller after the worker exits. Before and after verification/review the controller
compares file bytes, modes and paths against the commit, including ignored files. Git stat
caches and assume-unchanged flags cannot hide differences. Submodules/special files are
unsupported. Pilot limits: 10,000 files, 16 MiB per file, 64 MiB total candidate bytes;
deadlines and output limits are configured per process. Use external filesystem quotas
and resource limits for hostile workloads; disk/CPU/memory quotas are not implemented.

The store holds run.json, handoff.md, a controller lease and content-addressed objects.
Evidence records the command, stage, input context, times, exit code and output digests.
Logs stay local and may be sensitive. No redaction or hosted log upload is performed.

## Human decision (performed by the human, outside the worker)

Provision an independently pinned allowed-signers file outside repository and run store:

```text
YOUR_PRINCIPAL namespaces="local-review-v1" YOUR_HUMAN_PUBLIC_KEY
```

Point config.allowedSigners at it. The controller never receives the private key. Generate
an acceptance or rejection request after inspecting evidence:

```bash
node harness/local/cli.mjs request-decision \
  --run /absolute/controller-state/RUN_ID --decision accept
```

The human inspects the request and evidence, then signs the returned file using their key:

```bash
ssh-keygen -Y sign -f /path/to/HUMAN_PRIVATE_KEY \
  -n local-review-v1 /absolute/controller-state/RUN_ID/request-accept.json
```

Import the detached signature:

```bash
node harness/local/cli.mjs record-decision \
  --run /absolute/controller-state/RUN_ID \
  --statement /absolute/controller-state/RUN_ID/request-accept.json \
  --signature /absolute/controller-state/RUN_ID/request-accept.json.sig \
  --principal YOUR_PRINCIPAL
```

Use `reject` and request-reject.json for rejection. The exact signed statement binds run,
base/candidate and evidence digest. Missing/wrong signatures, changed inputs or reused
decisions fail closed. Verification uses current time and the pinned signer namespace,
never a Git timestamp. The CLI verifies and records; it has no signing command.

A recorded decision is a local review attestation. It does not move a TaskState, approve
a production operation, publish knowledge or replace the external approval ledger.

## Failure and recovery

Failures retain completed evidence and write a handoff with the next action. Inspect with
status and read the referenced objects locally. Each artifact is rehashed before use.
An interrupted `running` job is never automatically replayed. Prepare a new run after
inspection. A stale controller.lock requires checking that its PID is dead before a human
removes the lock; the tool never guesses from timestamps. Retain evidence according to
your local policy. Remove abandoned worktrees with `git worktree remove` from the source
repo; do not remove their evidence before the review/retention period ends.

## Validation scope

The deterministic tests inject a fixture-only executor and synthetic signing keys. The
CLI always selects the real sandbox. The separate real-sandbox test runs a full fixture
workflow and probes denied secret reads, `.git`/tool writes, reviewer writes and network.
If namespaces are blocked, it explicitly skips and the denial test verifies the CLI path
does not run an unisolated worker. Set HARNESS_REQUIRE_SANDBOX=1 for a release gate.

Research and implementation decisions: `specs/local-evidence-workflow/design.md`.
