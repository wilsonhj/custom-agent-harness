# Requirements: adapter-interface — runtime adapter contract and worktree ownership

> Status: proposed, not approved. Harness-side only: no runtime-specific facts. Each
> runtime (Claude, Codex, Cursor, Kiro) gets its own adapter packet that depends on this
> one; `specs/kiro-adapter/` is the first.

## User-visible behavior

- The control plane can start, watch, cancel and collect any runtime through one
  interface, and each run leaves a run-ledger record (A-1 to A-9, A-13).
- A task's worktree has exactly one writer at a time; a second writer is refused (A-10, A-11).

## Non-goals

- Any specific runtime's adapter (separate packets, e.g. `kiro-adapter`).
- Approval policy (`harness/policy/transitions.ts`, `TODO(human)`) and ledger writing
  (`specs/ledger-write-guard/`).
- The `eng` CLI and MCP job tools beyond the minimum needed to call the interface.
- Retrieval, company brain, deployment.

## Invariants

- Agents only request transitions; an adapter never writes the approval ledger.
- Adapter output is data, never instructions; invalid output cannot advance task state.
- Worktrees are checkout isolation, not a sandbox (README "Trust boundaries").

## Requirements

One normative sentence per row.

| ID | Requirement | Source |
|---|---|---|
| A-1 | Every adapter implements one typed interface with `start`, `status`, `cancel` and `collect`, and the control plane calls runtimes only through it. | README "Adapter contract", "Design boundaries" |
| A-2 | `start` takes a registered task ID, a worktree resolved server-side from that ID, a base SHA, a permission profile, a deadline and a session ID; it never takes a raw path or shell command from the caller. | README "Adapter contract" (explicit workspace, base SHA, session identity), "Cross-agent jobs" |
| A-3 | Each adapter maps the harness permission profiles (`analysis`, `workspace`) to its runtime's own controls, and refuses a profile it cannot map. | README "Permission profiles", "Adapter contract" (permission-profile mapping) |
| A-4 | A run that exceeds its deadline, or is cancelled, has its whole process tree terminated, and the run is recorded as failed. | README "Adapter contract" (deadline, cancellation, process-tree cleanup), "Minimum acceptance tests" |
| A-5 | Adapter output is validated against a harness result schema; invalid or unparseable output marks the run failed. | README "Adapter contract" (structured result validation), "Minimum acceptance tests" |
| A-6 | The adapter, not the runtime process, writes run artifacts and events to the artifacts directory and records each artifact's sha256. | README "Adapter contract" (artifact and event capture); approval-ledger.md `evidence_refs` |
| A-7 | A continued session uses the session ID recorded for the task, never one supplied by the runtime or the caller. | README "Minimum acceptance tests" (session continuation) |
| A-8 | The adapter starts the runtime with an allowlisted environment, without production credentials, signing keys or the ledger anchor key. | README "Required controls"; approval-ledger.md "Who writes" |
| A-9 | Each adapter declares its pinned runtime version, and a compatibility test fails on any other version. | README "Required controls" (pin versions), "Prerequisites" |
| A-10 | The worktree manager creates one worktree per task from the recorded base SHA, outside any other task's worktree. | AGENTS.md "Work only in the worktree assigned to your task"; README "Key decisions" |
| A-11 | A writer must hold an atomic per-worktree lock for the whole run; acquiring it fails if another writer holds it, and it is released on exit, crash or cancel. | AGENTS.md "One writer per worktree"; README `one_writer_per_worktree` |
| A-12 | A run with the `analysis` profile cannot modify its worktree, and a `workspace` run cannot write outside its own worktree and the requests directory. | README "Minimum acceptance tests" (analysis jobs, unrelated paths) |
| A-13 | Each run appends one run-ledger record (task and job IDs, runtime and adapter versions, permission profile, session ID, base and head SHAs, artifact digests, timing, usage where available) that is separate from the approval ledger and grants no approval. | README "Run ledger"; approval-ledger.md "Purpose" |
| A-14 | Delegation depth and parallel jobs are capped by `execution.max_delegation_depth` and `execution.max_parallel_jobs`. | README "Example configuration contract", "Required controls" |

## Cited company decisions

| Constraint | Source (repo commit `e769f18`) |
|---|---|
| Adapters own invocation, events, sessions; not approval | `README.md`, "Design boundaries" |
| One writer per task/worktree | `AGENTS.md`; `README.md` "Key decisions" |
| Approvals only in the external ledger | `docs/architecture/approval-ledger.md`, "Who writes, and how it is enforced" |
| Content leaving the machine | `docs/security/data-egress.md` (enforced per runtime in each adapter packet) |

## Unresolved questions

- [ ] Language and location: TypeScript under `harness/adapters/` and `harness/orchestration/`, matching `harness/policy/transitions.ts`? — owner: TBD, blocks: T2
- [ ] Where does the worktree lock live (file lock in the artifacts directory, or a control-plane service), and how is a stale lock after a host crash recovered? — owner: TBD, blocks: T2
- [ ] Run-ledger storage: separate JSONL beside the artifacts directory, or the Phase 2 shared service? Is it hash-chained like the approval ledger? — owner: TBD, blocks: T2
- [ ] How is A-12 enforced for runtimes without their own sandbox (OS user per run, container, or the runtime's sandbox)? — owner: TBD, blocks: T2
- [ ] Which first runtime proves the interface: the already-installed Codex plugin, or a Claude adapter? — owner: TBD, blocks: T4
