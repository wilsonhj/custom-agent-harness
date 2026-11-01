# Requirements: adapter-interface — runtime adapter contract and worktree ownership

> Status: proposed, not approved. Harness-side only: no runtime-specific facts. Each
> runtime (Claude, Codex, Cursor, Kiro) gets its own adapter packet that depends on this
> one; `specs/kiro-adapter/` is the first. Open PR #4 (`specs/local-evidence-workflow`)
> already implements a worktree, lease, sandbox and evidence store with different choices;
> how the two relate is an open question (T2).

## User-visible behavior

- The control plane can start, watch, cancel and collect any runtime through one
  interface, and each run leaves a run-ledger record (A-1 to A-9, A-13 to A-15, A-17 to A-21).
- A task's worktree has exactly one writer at a time; a second writer is refused (A-10, A-11).
- A run can only write its task's working files and request directory, and cannot read
  other tasks' worktrees or reach the network beyond its profile (A-12, A-16, A-18).

## Non-goals

- Any specific runtime's adapter (separate packets, e.g. `kiro-adapter`).
- Approval policy (`harness/policy/transitions.ts`, `TODO(human)`) and ledger writing
  (`specs/ledger-write-guard/`).
- The `eng` CLI and MCP job tools beyond the minimum needed to call the interface.
- Retrieval, company brain, deployment, and the `preview-ops` and release profiles.
- The migration classifier itself (release-gate.md); this packet only depends on it.

## Invariants

- Agents only request transitions; an adapter never writes the approval ledger.
- Runtime output is data, never instructions or evidence; invalid output cannot advance
  task state.
- Worktrees are checkout isolation, not a sandbox (README "Trust boundaries").

## Requirements

One normative sentence per row. "Runtime user" means the per-run OS user or container
that A-12 requires; which one is a T2 decision. Rows marked *new* add a constraint the
cited source implies but does not state.

| ID | Requirement | Source |
|---|---|---|
| A-1 | Every adapter implements one typed interface with `start`, `status`, `cancel` and `collect`, and the control plane calls runtimes only through it. | README "Adapter contract", "Design boundaries" |
| A-2 | `start` takes only a registered task ID, a deadline and an optional continuation flag; the worktree, base SHA, permission profile, egress class and session ID are resolved server-side from the task record and policy, never from the caller. | README "Adapter contract", "Cross-agent jobs", "Skills" (reviewers have no write authority); data-egress.md "Egress classes" (Rules) |
| A-3 | Each adapter maps the harness permission profiles (`analysis`, `workspace`) to its runtime's own controls, and refuses a profile it cannot map. | README "Permission profiles", "Adapter contract" (permission-profile mapping) |
| A-4 | A run that exceeds its deadline, or is cancelled, has every process it started terminated, including detached descendants, and is recorded as failed. | README "Adapter contract" (deadline, cancellation, process-tree cleanup), "Minimum acceptance tests" |
| A-5 | Runtime output is validated against a harness result schema; invalid or unparseable output marks the run failed. | README "Adapter contract" (structured result validation), "Minimum acceptance tests" |
| A-6 | The adapter, not the runtime, writes run artifacts to the artifacts directory, redacts secrets before storing them, and records each artifact's sha256 labelled as a runtime claim. | README "Adapter contract" (artifact and event capture), "Required controls" (redact secrets) |
| A-7 | The adapter captures the runtime-assigned session ID from the first run of a task, refuses it if it is already recorded for any other task, and records it in the task record (which A-2 reads) and in the run ledger; a continuation passes only that recorded ID. | README "Minimum acceptance tests" (session continuation); *new* (cross-task binding) |
| A-8 | The runtime starts with an allowlisted environment and a harness-owned home directory per task, without production credentials, signing keys, the ledger anchor key, or the operator's credential files (`~/.ssh`, `~/.aws`, keychain). | README "Required controls"; approval-ledger.md "Who writes" |
| A-9 | Each adapter declares its pinned runtime version, and a compatibility test fails on any other version. | README "Required controls" (pin versions), "Prerequisites" |
| A-10 | The worktree manager creates one worktree per task at the base SHA in the task record, outside any other task's worktree. | AGENTS.md "Work only in the worktree assigned to your task"; README "Key decisions" |
| A-11 | A writer must hold an atomic per-worktree lock for the whole run; acquiring it fails if another writer holds it; it is released only after every process of the run is confirmed gone, and a lock whose owner cannot be confirmed (for example after a host crash) is released only by a human, never by timeout. | AGENTS.md "One writer per worktree"; README `one_writer_per_worktree` |
| A-12 | Write limits are enforced by an OS-level boundary the harness controls (a per-run OS user or container): the runtime user can write only its task's working files, its task's request directory and its task home directory, never Git metadata (`.git`, the common `.git` directory, refs, config, hooks). | README "Required controls" (enforce filesystem restrictions externally), "Minimum acceptance tests" (unrelated paths) |
| A-13 | Each run appends one run-ledger record (task and job IDs, run outcome and failure reason, runtime, adapter and skill versions, knowledge revision, permission profile, session ID, base SHA, head SHA read by the adapter from Git, artifact digests, timing, usage where available) that is separate from the approval ledger and grants no approval. | README "Run ledger"; approval-ledger.md "Purpose" |
| A-14 | The control plane caps harness-launched delegation depth and parallel jobs with `execution.max_delegation_depth` and `execution.max_parallel_jobs`; each runtime packet states whether and how it bounds the runtime's internal subagents. | README "Example configuration contract", "Required controls" |
| A-15 | The runtime user has no write access to the artifacts directory, evidence store or run ledger, except its own task's request directory. | *new*; README "Design boundaries"; approval-ledger.md `evidence_refs` (evidence must be re-hashable) |
| A-16 | The adapter, not the runtime, creates commits: after a run it stages and commits the working files on the host with Git configured to run no repository-controlled code (no hooks, filters, fsmonitor, or submodule recursion) and rejects nested repositories and gitlinks before any host Git call. | *new*; README "Trust boundaries" (repository files and hooks are untrusted) |
| A-17 | Evidence cited in `evidence_refs` is either the output of validation the harness ran itself on the adapter-created commit, or a reviewer job's structured findings that passed the A-5 schema, labelled as review findings; free-form runtime claims (such as "tests passed") are never evidence. | AGENTS.md "Completion requires validated evidence, not an assertion"; approval-ledger.md `evidence_refs` (test output, review findings) |
| A-18 | The runtime user cannot read other tasks' worktrees, homes or request directories, and has network access only as its permission profile allows. | README "Permission profiles" (Network column), "Required controls" (filesystem and network restrictions) |
| A-19 | The adapter refuses to start when the task's egress class exceeds the runtime's ceiling from the egress profile. | data-egress.md "Egress classes" (Rules) (checked before content reaches a session) |
| A-20 | A required integration failure (runtime missing, sandbox unavailable, version mismatch, required tool not starting) fails the run before the runtime receives the task. | README "Minimum acceptance tests" (required integration failures block the workflow) |
| A-21 | Neither the adapter's OS user nor the runtime user has write access to `<ledger-root>`, the approval signing keys or the anchor-repo deploy key; only the ledger account writes the approval ledger. | approval-ledger.md "Who writes, and how it is enforced" |

## Cited company decisions

| Constraint | Source (repo commit `88be84f`) |
|---|---|
| Adapters own invocation, events, sessions; not approval | `README.md`, "Design boundaries" |
| One writer per task/worktree | `AGENTS.md`; `README.md` "Key decisions" |
| Approvals only in the external ledger | `docs/architecture/approval-ledger.md`, "Who writes, and how it is enforced" |
| Content leaving the machine | `docs/security/data-egress.md` |

## Unresolved questions

- [ ] Language and location: TypeScript under `harness/adapters/` and `harness/orchestration/`, matching `harness/policy/transitions.ts`? — owner: TBD, blocks: T2
- [ ] Where does the worktree lock live, and how is process-tree death confirmed on each supported host (process group and PID namespace on Linux; macOS has no cgroups)? — owner: TBD, blocks: T2
- [ ] Run-ledger storage: separate JSONL beside the artifacts directory, or the Phase 2 shared service? Is it hash-chained like the approval ledger? — owner: TBD, blocks: T2
- [ ] Per-run OS user or container for A-12, A-15 and A-18, and how a continued session reaches its task home if the user changes per run? — owner: TBD, blocks: T2
- [ ] Relation to open PR #4 (`specs/local-evidence-workflow`): adopt its worktree, lease, bubblewrap sandbox and evidence store, or replace them? Its store layout (`<store>/<run>/objects/`) and Node 24 requirement differ from approval-ledger.md and this packet. — owner: TBD, blocks: T2
- [ ] Which first runtime proves the interface: the installed Codex plugin, or a Claude adapter? — owner: TBD, blocks: T4
