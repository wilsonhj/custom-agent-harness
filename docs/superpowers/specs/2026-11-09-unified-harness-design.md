# Unified agent harness: first-principles design

Status: **approved by the owner on 2026-11-09** ("Yes, I accept", in reply to the handover
that listed the architect's calls and questions 1 to 5 of section 14). Recorded answers are
in section 14.
Date: 2026-11-09. Supersedes parts of `specs/adapter-interface/` and `specs/claude-adapter/`
(listed in section 12). Evidence for every factual claim is in
`research-2026-11-09/` beside this file; facts there are graded (P) probed locally,
(D) official docs, (S) secondary, UNVERIFIED.

Legend used below: **[Owner]** = decided by the repository owner in conversation.
**[Architect]** = the lead architect's call, not yet agreed; the owner can overturn any of them.

## 1. Intent

The owner is a solo AI software engineer who today is the courier between tools:
research in Perplexity, planning and implementation by one agent team (usually Claude
Code) that opens a PR, adversarial review by a different agent (Codex or Cursor), and
copying Vercel or Cloudflare logs into an agent when a deployment breaks. Each handoff is
manual and high-friction.

The harness is one command-line tool that carries a change through those stages, lets each
stage run on a different model or agent, hands files (never chat transcripts) between
stages, and stops for the human at the points where a human decision is required.

## 2. Goals, non-goals, anti-goals

**Goals**

1. One spine for six stages: research, spec, implement, review, repair, operate.
2. Model-agnostic: any stage can be assigned to Claude Code, Codex, later Cursor, or an
   OpenRouter-hosted model, by configuration.
3. The implementer and the reviewer always come from different model families.
4. The harness, not an agent, decides what counts as evidence. Agents never approve their
   own work.
5. Sandboxed by default on macOS; if the sandbox cannot start, nothing runs.
6. Cost-efficient: subscription logins for Claude Code and Codex; a hard, pre-checked budget
   for anything billed per token.
7. Per-project control over what may leave the machine.
8. A CLI today whose commands and events can later be served to a web, macOS or iOS client
   without redesign.
9. Open-source ready: a contributor can work in one crate without understanding the rest;
   extensions (skills, tools) are not Rust.

**Non-goals (for now)**

- A tool-using agent loop owned by the harness for OpenRouter models. They start as
  advisors: text in, structured findings out, no tools, no files. [Architect]
- Releasing or deploying. The harness reads deployment state; it never changes production.
- Replacing the vendors' agent loops, or reusing a vendor conversation across runtimes.
- Multi-user or multi-machine operation, and a hosted service.
- The signed, anchored approval ledger in `docs/architecture/approval-ledger.md`. The first
  slice records the human's decision in the local event log (section 6.5). [Architect]

**Anti-goals (would make this a failure even if it works)**

- A task marked done because an agent said so.
- Silent degradation: running unsandboxed, or with weaker privacy, when a control is missing.
- The harness storing, copying or forwarding a subscription login token.
- A mutation to Vercel or Cloudflare made without the human.
- Routing or counsel that costs more per verified task than the fixed default, unnoticed.

## 3. Constraints

- macOS 26 on Apple silicon is the first platform; Linux second.
- Installed and pinned for the first slice: Claude Code 2.1.318, codex-cli 0.166.0 (P).
  Cursor CLI, `vercel` and `cf` are installed; `pplx`, `wrangler` and a Rust toolchain are
  not (P).
- Claude Code and Codex run on the owner's existing subscriptions. OpenRouter ($200 credit)
  and any research API run on API keys. [Owner]
- The owner does not yet read Rust fluently and is the only maintainer. Security-critical
  logic must therefore be small, pure, and covered by tests a non-Rust reader can follow
  (golden tables, CLI-driven conformance cases).
- Some projects are HIPAA-eligible (canary). Consumer subscriptions carry no BAA or
  zero-retention terms.

## 4. Decisions

| # | Decision | Who | Why (short) |
|---|---|---|---|
| 1 | Core in Rust, from now on; no TypeScript-first phase | Owner | A later port is a rewrite; the cost of switching is lowest now |
| 2 | Library core + thin CLI; every command and event serializable | Owner | Web, macOS and iOS clients later; iOS can only ever be a remote client |
| 3 | Subscription logins for Claude Code and Codex | Owner | Cost |
| 4 | A hosted judgment model (Jev) is optional, never required | Owner | |
| 5 | Agents are child processes driven over stdio; no vendor SDK in-process | Architect | Language-neutral; SDK hooks fail open by default |
| 6 | The harness never handles a login token: each CLI is logged in by the user, once, inside a harness-owned config directory | Architect | Anthropic's terms allow a user signing in to the unmodified binary and bar tools from storing or intermediating credentials (D) |
| 7 | Harness sandbox outermost; each agent's own sandbox turned off and verified off at run start | Architect | Sandboxes do not nest on macOS (P) |
| 8 | Network by allowlist through a harness proxy on one localhost port | Architect | The macOS sandbox cannot filter by hostname (P) |
| 9 | Agents get no Git metadata, read or write; the harness makes every commit under hidden refs | Architect | Repository hooks and config are untrusted; a shared object store exposes other tasks' work |
| 10 | SQLite, one writer; a command, its events and its pending work commit in one transaction | Architect | Crash safety and idempotent retries |
| 11 | The event log is the audit chain (hash-chained, never pruned); the run-ledger JSONL of PR #7 becomes an export of it | Architect | One source of truth |
| 12 | An interrupted agent run is never replayed; the worktree resets to the last checkpoint and the task stops for the human | Architect | An agent run is not idempotent |
| 13 | A task becomes approved or verified only through a human decision recorded at the CLI for an exact packet digest or commit; the harness never does it on its own | Architect | Replaces the unwritten approval policy for local, single-user use (6.5) |
| 19 | Repair runs automatically for a bounded number of rounds (default 2), then the task stops for the human | Architect | Removes the courier work; an agent's findings can cause more work but never acceptance |
| 20 | The first slice is macOS only, and covers implement, review and repair on a hand-supplied packet | Architect | Smallest slice that removes the main friction |
| 14 | Counsel is opt-in and measured before it becomes a default | Architect | Published evidence for cross-model panels is thin; false positives rise |
| 15 | Review input is split into chunks | Architect | Reported review accuracy collapses on diffs over ~150 lines (S) |
| 16 | Cursor is not in the first slice | Architect | No schema flag, no usage data, MCP only from fixed paths (P, D) |
| 17 | Read-only against Vercel and Cloudflare is enforced by a typed allowlist in the harness, not by the token | Architect | Neither vendor offers a clean read-only credential (D) |
| 18 | On a subscription limit: pause and tell the human; failover to another runtime only if the routing table says so | Architect | Predictable cost and behaviour |

## 5. Architecture

```text
CLI (now)        daemon over a Unix socket -> web / macOS / iOS clients (later)
      \            /
       engine: submit Command -> one SQLite transaction -> effect worker -> outcome Command
         |            |                 |
       core         store             exec                        adapters
   (pure rules)   (SQLite,       (the only async crate:       (pure: build a launch
    states,        event log,     spawn, sandbox, kill,        plan, parse output
    transitions,   artifacts)     worktrees, host Git,         lines; never spawn)
    decide())                     checkpoints)                 claude | codex
                                      |
                              macOS sandbox + egress proxy
                                      |
                        claude -p / codex exec (subscription login)
later: providers (OpenRouter, research, Vercel, Cloudflare), daemon
```

Dependency direction: `cli -> engine -> { store, exec, adapters } -> core`. Nothing points
upward. `core` has no I/O, no async, no clock and no randomness: time and IDs arrive as
data, so every rule is a plain function that can be tested with a table.

### 5.1 Crates (first slice)

| Crate | Owns | Must not |
|---|---|---|
| `harness-core` | Types, task and stage state machines, `decide(task, command) -> {events, effects}`, failure classification, policy | Touch files, processes, time, SQLite, tokio |
| `harness-store` | SQLite schema, single writer thread, command receipts, hash-chained events, outbox, content-addressed artifacts | Be async; expose paths |
| `harness-exec` | Process supervision, sandbox, worktrees and lock, host Git, checkpoint refs | Contain policy |
| `harness-adapters` | Per-runtime launch plan and output parser; static capability description | Spawn, or depend on tokio |
| `harness-engine` | The durable loop and restart recovery; the public API for CLI and daemon | |
| `harness` (binary) | Argument parsing and output | Contain logic |

### 5.2 Interfaces that exist, and ones that deliberately do not

An interface is added only where two real implementations exist.

- `RuntimeAdapter` (claude, codex): `probe`, `plan`, `parse_line`, `finish`. Pure.
- `Sandbox`: `probe`, gated `spawn` and fallible cleanup; only confirmed descendant cleanup
  yields `TreeGone` (section 6.2).
  The first slice ships one concrete macOS module with this shape. The trait is extracted
  when the Linux bubblewrap implementation is added, which is the first follow-up after the
  slice; the profile type already carries the fields both need
  (`research-2026-11-09/1a-macos-sandbox.md` has the field-by-field mapping).
- Later, with their second implementation: `ModelProvider` (OpenRouter, fake),
  `DeploymentProvider` (Vercel, Cloudflare; read methods only), `ResearchProvider`.
- **No interface yet:** evidence store, worktree lock, Git, clock, judgment. Each is one
  concrete module. The pilot's leaked evidence paths are fixed by hiding paths inside that
  module, not by a trait.

### 5.3 State

Two closed state machines, both checked exhaustively by the compiler.

- **Task lifecycle:** the 16 states and transition table of `harness/policy/transitions.ts`,
  ported verbatim so the allowed-transition table of `tests/transitions.test.ts` becomes a
  golden oracle. Two things differ from the TypeScript:
  - The approval a gated transition needs is a `HumanDecision`, built only by the store from
    a decision event the human entered at the CLI, bound to an exact packet digest or commit
    (6.5). Without one, every gated transition is still denied, as today.
  - `PreviewVerified`, `MigrationGated`, `Released`, `Observed` and `Closed` stay in the table
    for parity but are unreachable: this design has no release path.
- **Stage run:** `Pending`, `EffectsInFlight`, `AwaitingHuman{reason}`, `Succeeded`,
  `Failed{code}`, `Cancelled`. `FailureCode` and the six termination reasons are the closed
  sets from PR #7, plus `LostOnRestart` and `LimitExhausted{reset_at}`.

The pipeline is data: a per-project table lists the stages and, for each, the runtime,
sandbox profile and verification commands. It is digested and frozen into the task when
the task is created.

### 5.4 Durable loop and crash rules

1. A command arrives with an idempotency key.
2. One transaction writes the command receipt, the resulting events, and an outbox row for
   each piece of pending work.
3. A worker performs the work. For a process, a small shim waits on a pipe until its PID is
   committed, so a crash before that commit provably ran nothing.
4. The outcome is submitted as a new command.

| In flight at crash | On restart |
|---|---|
| Agent run | Fail with `LostOnRestart`; obtain `TreeGone` under section 6.2 before resetting to the last checkpoint; stop for the human, retaining the lock if cleanup cannot be proved |
| Verification command | Obtain `TreeGone` under section 6.2 before retrying up to its attempt limit; retain the lock and stop if cleanup cannot be proved |
| External read (logs, search) | Retry up to its attempt limit |
| Checkpoint commit | Safe to repeat: commit time is fixed in the intent and the ref update is compare-and-swap |

### 5.5 Client path

Decided now so later clients are cheap: stable sortable IDs, one gapless global event
sequence, additive versioning with an `Unknown` variant, snapshots that carry the sequence
they were taken at, and an idempotency key on every command. The daemon is the same
`Envelope<Command | Event>` types as JSON lines over a Unix socket. It is not built now.

## 6. Running an agent

### 6.1 Sandbox (macOS)

A profile is generated per run and applied with `/usr/bin/sandbox-exec`.

- Default deny. Writes only to the task worktree and a per-task home. Reads denied for other
  worktrees, the harness state store, `~/.ssh`, `~/.aws`, and all Git metadata.
- Reads denied for a repository's executable agent configuration inside the worktree
  (`.claude/`, `.mcp.json`, `.codex/`, `.agents/`, `.cursor/`), because `claude -p` runs a
  repository's settings hooks and `.mcp.json` servers with no prompt (D). The harness
  supplies MCP servers itself. Consequence: a harness run cannot edit those paths.
- Instruction files (`AGENTS.md`, `CLAUDE.md`) stay readable as ordinary repository files.
  Runtime auto-loading of them is turned off where a flag exists, and the harness includes
  `AGENTS.md` from the base commit in every prompt, labelled as repository content, so all
  runtimes get the same conventions the same way. Instructions cannot widen permissions,
  which are enforced outside the agent.
- Existing required roots are resolved to their real paths before entering the profile:
  a rule written with `/tmp` did not stop a write through `/private/tmp` (P). A deny target
  that does not yet exist is resolved through its nearest existing ancestor, with its
  remaining normal path components appended, so its later creation remains denied.
  Broken symlinks, permission errors and other resolution failures fail closed; no deny
  rule is skipped (research `1a-macos-sandbox.md` §1; packet R-9).
- Network: one localhost port only. A harness proxy outside the sandbox passes listed hosts
  and logs every decision. Implementer and reviewer: the model vendor's hosts. Verifier: no
  network unless the project opts in to localhost.
- `sandbox-exec` is marked deprecated by Apple but works on macOS 26.6.2, and Codex, Claude
  Code and Cursor all depend on it (P, D). `eng doctor` fails closed if it stops working.

### 6.2 Process cleanup

Release the worktree lock or reset the worktree only after every descendant is confirmed
gone; the API requires a `TreeGone` proof (packet R-11). A separate unsandboxed supervisor
must remain alive while the task group is terminated and descendants are swept, including
after normal task exit. It must not `exec` the task or be reaped before cleanup completes
(research `1a-macos-sandbox.md` §6).

A live supervisor is necessary for the researched responsibility lookup, but is not proof
of containment: detached children or children that disclaim responsibility may escape a
group or responsibility-only sweep. Before opening the execution gate, the backend must
establish descendant containment for both cases; otherwise fail closed and start no task.
No such backend capability is established by this design. Tracking errors, loss of the
supervisor or cleanup timeout return an error without `TreeGone`; retain the worktree lock
and stop for human recovery. An ancestry poll or an empty responsibility snapshot alone
cannot establish that every descendant is gone (R-11; research §6's lookup limitations).

### 6.3 Subscription logins: the accepted exposure

**Treat an agent's own login as potentially readable by its shell commands.** The CLI
needs its login; the outer sandbox does not establish credential isolation between the
CLI and those commands (research `1a-macos-sandbox.md` §8).

Mitigations, none of which removes the exposure:

- The network allowlist limits reachable hosts; it does not prevent a readable token from
  entering a prompt, runtime transcript or another request to an allowed vendor host.
- A Claude reviewer gets no shell tool (`--tools Read,Grep,Glob`). Codex always has a shell,
  so a Codex reviewer has the same exposure as an implementer; what bounds it is the
  read-only filesystem profile and the network allowlist.
- Before the harness commits a candidate, it scans the diff for credential-shaped strings
  and fails the run on a match, because a token written into a tracked file would otherwise
  leave through the commit. [Architect]
- Each CLI is logged in inside a harness-owned config directory, not the user's everyday one.

Whether a sandboxed shell child can actually reach a real Keychain login item remains
UNVERIFIED. Section 11 uses only synthetic non-secret canaries and records readability
without values; it must not ask an agent to extract a real login. Such a probe does not
establish real credential isolation (decision 6; plan Task 11).

### 6.4 Launch plans

On two points this section overrides the launch plans in
`research-2026-11-09/1b-subscription-clis.md`: every agent's own sandbox is **off** (that
report turns them on, which fails under an outer sandbox), and the tool list is always
explicit.

- **Claude (implementer):** `claude -p --output-format stream-json --verbose --restricted
  --setting-sources user --strict-mcp-config --mcp-config <harness file>
  --disable-slash-commands --tools <explicit list including the shell and edit tools>`,
  harness-owned `CLAUDE_CONFIG_DIR`, `CLAUDE_CODE_DISABLE_CLAUDE_MDS=1`, own sandbox disabled
  through `--settings`. `--restricted` removes the shell unless it is named in `--tools`, so
  the list is required. `--bare` is not used: it cannot run on a subscription (P, D).
- **Claude (reviewer):** the same, with `--tools Read,Grep,Glob` and `--json-schema`.
- **Login location:** a harness-owned config directory is the plan. Test ordinary CLI
  authentication without inspecting credentials. If it fails there, stop for an explicit
  owner decision about any broader config-directory access; do not silently fall back to
  the CLI's everyday config directory (decision 6; section 9).
- **Codex (reviewer):** `codex exec --json --output-schema <findings schema>` with its own
  sandbox off (`-s danger-full-access`); read-only is enforced by the harness profile.
  `exec resume` and `exec review` have no `--sandbox` flag (P).
- A run succeeds only if the exit code is 0, the final event reports no error, and the
  output validates against the harness schema. Each adapter's version is checked against its
  pin before any task content reaches it.
- Budgets on subscription runs are turns, wall-clock time and repair rounds; there is no
  dollar cap. A rate-limit event produces `LimitExhausted{reset_at}`.
- MCP servers have one canonical definition in harness configuration, rendered per runtime.
  Skills (Markdown directories) are rendered the same way after the first slice.

### 6.5 Evidence and the human decision

- Agent output is a claim. Evidence is only (a) the output of verification commands the
  harness runs itself on the harness-made commit, inside the verifier profile, or (b)
  reviewer findings that pass the schema, labelled as review findings.
- **Human decisions.** `eng decide` records an event in the hash-chained log, bound to an
  exact packet digest or commit. Agents cannot reach the state store, so they cannot write
  one. There are two in the first slice:
  - *Approve the spec:* `Planned -> Approved` for the packet digest.
  - *Accept, reject or send back:* from `AwaitingHuman`, accept moves the task to `Verified`
    for that commit; reject fails it; send-back starts another repair round.
- **Automatic repair.** After the implementer's commit, the harness runs the verification
  commands, then the review. A repair round starts automatically if verification fails, or if
  the review has at least one finding at or above the project's severity threshold (default:
  high). After the round limit (default 2), or when verification passes and no finding
  reaches the threshold, the task stops at `AwaitingHuman` with all findings attached. A
  reviewer's "pass" never completes a task; it only ends the repair loop.
- The signed, anchored ledger of `docs/architecture/approval-ledger.md` is deferred. This is
  weaker: the decision is protected by the sandbox and local file permissions, not by a
  signature or a separate account.

## 7. The six stages

| Stage | Who runs it | Input | Output | Gate |
|---|---|---|---|---|
| Research | Search provider + optional advisor model | A question | `evidence.json`: claims, each with URL, retrieved-at, quoted span, and a verified flag | Harness re-fetches each URL and checks the quoted span is present |
| Spec | An agent drafts; the human approves | Request + verified evidence | The existing packet: requirements, design, tasks, acceptance tests, handoff | Human approves the spec |
| Implement | Runtime A, workspace-write profile | Approved packet | A harness-made commit | Harness-run verification commands |
| Review | Runtime B from a different family, read-only | Packet + diff in chunks | Schema-valid findings | Schema validation |
| Repair | Runtime A | Findings | A new commit | Bounded rounds, then stop |
| Operate | Harness fetches; an advisor with no tools diagnoses | Deployment URL, ID or PR | Diagnosis bundle + proposed fix as a new task | Any production change is a proposal for the human |

**First slice:** implement, review and repair only. The packet is written by hand, or with
any agent outside the harness, under `specs/<id>/` in the target repository.
`eng task new --packet specs/<id>` records its tree digest at the base commit
(`Draft -> Planned`); the human approves that digest with `eng decide`. Research, spec
drafting and operate are later projects.

Details per stage are in the research reports. The points that bind the builder:

**Research.** Default provider: Perplexity Search API; alternates Tavily, Exa, Brave; Claude's
and Codex's built-in search only as a fallback, because the harness cannot inspect the
queries they send. Span verification proves a quote exists at a URL, not that it supports
the claim. Only verified claims enter the spec; the rest go to its open-questions list.
Model-written retrieval programs are deferred.

**Review and counsel.** The findings schema is the README's (`verdict`, `summary`,
`findings[]` with `severity`, `file`, `line_start`, `line_end`, `confidence`,
`recommendation`) plus `claim`, `failure_scenario`, `category`, `suggested_check`. Counsel is
N advisors answering independently with that schema; the harness merges by file, overlapping
line range and category into corroborated, single-source and disagreement lists. No voting,
no averaging. Every finding stays a claim until a harness check or the human confirms it.

**Operate.** A typed client that can only issue an allowlisted set of GET routes, pinned to
one team and project; no generic request method. Environment variable names come through a
type with no value field; the harness never runs `vercel env list`, which prints values (P).
Logs are passed to the model as labelled untrusted text after fail-closed redaction.
Triggers: `eng triage <url|id|pr>` first, then polling GitHub checks; webhooks later.

## 8. Routing, budget, judgment

- Routing is a fixed per-stage table: runtime, pinned model, fallback, cap. A fallback may
  never have weaker privacy than the primary.
- For per-token paths the harness reserves the worst-case cost in SQLite before each call and
  replaces it with the reported cost afterwards. Every request sets a token limit and a
  price ceiling. OpenRouter uses a dedicated key with its own credit limit.
- A judgment model, if enabled, runs in shadow mode: it records what it would have chosen
  and never decides. Routing quality is judged by cost per verified task.

## 9. Privacy

`docs/security/data-egress.md` is binding: missing or unknown repository/document class
means `local-only`, and derived content inherits the most restrictive input class. A new
project permits no hosted runtime, research or advisor disclosure until a reviewed class
and explicit project/provider authorization permit it. A subscription login, spec approval
or network host allowlist does not grant that authorization.

The project declaration separately bounds agent runtimes (which providers), research
queries (`none`, `public-questions`, `project-context`), and third-party advisors (none,
specs only, sanitized diffs, full diffs), within the content's class. Before any hosted
disclosure or runtime spawn, the harness checks both the class and authorization for
the exact runtime/account/settings configuration. `baa-covered-providers` also requires a
current verified matching BAA allowlist entry; missing, unverified or expired entries deny
that content. When classification, provider identity or enforced read isolation cannot be
established, return `PolicyDenied` before the call or spawn. The check covers every readable input,
including repository instructions and tool results, not only the initial prompt (policy
"Egress classes", "BAA configuration allowlist" and "Enforcement"). Record a hash of
authorized content sent, never credential values.

OpenRouter's no-retention and no-training filters rest on what each provider declares; they
are not audited (D).

## 10. Build order

Each step names its proof. Steps marked ∥ can run in parallel worktrees once `core` is fixed.

1. Conformance scaffolding in Node: golden `transitions.json`, fixture-repo builder, fake
   agents. Proof: green against the Node pilot where it applies.
2. `harness-core`. Proof: golden table equality; property tests that no event sequence
   reaches `Verified` without a human decision for that commit, none reaches
   `AwaitingHuman` after a clean review without harness verification evidence on the same
   commit, and repair rounds never exceed the limit.
3. ∥ `harness-store`. Proof: abort after each statement, reopen, chain verifies, a retried
   command returns the same receipt.
4. ∥ `harness-exec`, macOS only. Proof: process-kill, lock and regression cases ported from
   the pilot (whose originals run only on Linux, in CI); macOS denial probes.
5. ∥ `harness-adapters`. Proof: parsers against screened synthetic fixtures reconstructed
   from reviewed event shapes, including truncated-line cases (section 11; plan Task 11).
6. `harness-engine`. Proof: kill the engine at each effect state; outcomes match 5.4.
7. `harness` CLI: `eng task new`, `run`, `status`, `cancel`, `decide`, `audit verify`,
   `doctor`. Proof: full flow with fake agents, then one supervised real run.

After the spine: research, operate, and counsel/routing are independent of each other and
are built in parallel, each behind its own provider module. The daemon and clients come last.

The Node pilot is deleted only when its behaviours pass in the conformance suite against
the Rust binary.

## 11. Live tests needed before or during the build

Authenticated runs need the owner's explicit go-ahead for the step and project/provider
authorization under section 9. Synthetic probes use disposable non-secret data, never the
owner's real credential files or login items. In order:

1. Does Claude Code run correctly inside the outer sandbox with its own sandbox off? Codex?
2. Can a sandboxed shell child read synthetic credential-location canaries? Record only
   `readable`, `denied` or `unavailable`, never canary contents. Real credential readability
   remains UNVERIFIED; do not query or print a real login item.
3. Do Codex and Cursor honour `HTTPS_PROXY`? What are the exact vendor host lists?
4. Does the flag set in 6.4, with the read rules in 6.1, keep an untrusted repository's hooks
   and MCP servers from loading while the harness's own `--mcp-config` still works? Only if
   it does not is `--safe-mode` evaluated as a replacement.
5. Session resume from a harness-owned config directory, for both runtimes.
6. Reconstruct secret-free adapter fixtures from reviewed event shapes using synthetic
   values. Raw authenticated transcripts and probe artifacts are not committed or used as
   fixtures; screening must precede any retention or sharing (decision 6; section 9).

The full list of unverified facts is in `research-2026-11-09/1b-subscription-clis.md` §9.

## 12. What changes in this repository

| Existing | Becomes |
|---|---|
| adapter-interface D1 (Linux bubblewrap only) | macOS first, Linux second, behind one sandbox interface |
| adapter-interface D4 (TypeScript modules) | Rust workspace |
| adapter-interface D5 (`claude -p` first) | Kept |
| claude-adapter C-2 (`--bare`), C-7 (proxy holds the API key), C-8 (proxy-metered budget) | Replaced by section 6; the metering proxy idea survives for OpenRouter |
| README non-goal "replacing vendor agent loops" | Kept; OpenRouter models are advisors only |
| PR #7 run-ledger | Merge as specification; implemented as an export of the event log |
| PR #8 pilot hardening, PR #9 spec fixes | Independent of this design; PR #8's cases join the conformance suite |
| `harness/local/*.mjs` | Reference pilot until parity, then deleted |
| Signed approval ledger | Deferred; see 6.5 |

## 13. Left to the builder

Crate-internal structure, error types, SQLite schema details (a proposal is in
`research-2026-11-09/5-rust-core.md` §4), CLI flag names, the proxy implementation, test
framework choices, and the exact Seatbelt profile text (a probed starting point is in
`research-2026-11-09/1a-macos-sandbox.md`).

## 14. Open questions for the owner

Answers recorded 2026-11-09: questions 1 and 2 **accepted**. Question 3 is read as
accepted as design intent, not blanket authorization to inspect credentials or disclose
project content. Each authenticated step still needs explicit go-ahead and section 9's
project/provider check. Questions 4 and 5 were not answered: until they are, unclassified
content remains `local-only`, no hosted disclosure is authorized by default, and no code
is copied from another project.

1. Accept the exposure in 6.3 (an implementer's shell can read its own subscription login,
   limited by the network allowlist and the diff scan)?
2. Accept the ambiguity in Anthropic's consumer terms ("ordinary, individual usage") for
   scripted runs of the unmodified CLI under your own login?
3. May I run the live tests in section 11 on your subscriptions?
4. Which projects must send nothing to third parties, and what may canary send to whom?
5. License: Apache-2.0 (lets us copy Codex's sandbox fragments with attribution) or dual
   MIT/Apache-2.0?
Questions 6 to 8 can wait until their project starts.

6. Research: is there a funded Perplexity API balance, or should the default be a provider
   with a free tier?
7. Operate: which Vercel plan, and will you create project-scoped tokens for the harness?
8. Starting caps: $1 per task for counsel and $50 on the OpenRouter key?
