# Requirements: kiro-adapter — Kiro CLI agent adapter

> DRAFT. Kiro-side facts are UNVERIFIED: kiro.dev was unreachable from the authoring
> session (proxy 403 on CONNECT), so they come from search-result snippets of
> kiro.dev/docs pages. Re-check against https://kiro.dev/docs/cli/ before approval.

## User-visible behavior

- A task can name `kiro` as its runtime and the harness runs `kiro-cli` in that task's
  worktree, capturing exit status and output as artifacts (K-1, K-11).
- A Kiro session can never write the approval ledger and can only receive content its
  egress class permits (K-3, K-4, K-5, K-10).

## Non-goals

- Replacing Kiro's agent loop; sharing transcripts across runtimes.
- Letting Kiro approve or mark any phase transition.
- Cloud/web/mobile Kiro surfaces; this packet covers the CLI only.

## Invariants

- Agents only request transitions; approvals stay in the external ledger.
- One writer per worktree.
- Unknown or unverified runtime configuration is treated as `any-provider`, never wider.

## Adapter requirements

| ID | Requirement | Source | Kiro status |
|---|---|---|---|
| K-1 | Adapter starts a non-interactive run with a named agent (`kiro-cli chat --agent <name>`) in the task worktree and records exit status and output as an artifact. | README "Agent adapters" | `--agent` seen in snippet; headless mode UNVERIFIED |
| K-2 | Adapter refuses to start if the worktree has another active writer. | AGENTS.md | Harness-side |
| K-3 | Kiro agent config grants no write access to `<ledger-root>/**`; a managed hook rejects shell commands referencing it. OS user separation remains the real boundary. | approval-ledger.md "Who writes" | Hook/permission format UNVERIFIED (JSON vs Markdown agent files both described) |
| K-4 | Kiro runs under pinned configuration users and projects cannot override (login method, allowed hooks only). If Kiro has no managed-config equivalent it is `any-provider` only. | data-egress.md "Pinned configuration" | UNVERIFIED |
| K-5 | Read interception covers file read, grep, shell and MCP, and fails closed; canary `local-only` file is denied on every path. | data-egress.md "Read hooks", "Tests" | UNVERIFIED (hooks per agent, "CLI only"; failure default unknown) |
| K-6 | A hook learns its runtime from which root-owned script ran, never from its input. | data-egress.md item 4 | UNVERIFIED |
| K-7 | Subagent delegation does not widen permissions; orchestrators restrict spawnable agents (`availableAgents`). | AGENTS.md; Kiro docs snippet | Inheritance reported; needs test |
| K-8 | Harness-supplied agent definitions are not shadowed by same-named user-global agents. | Kiro docs snippet | Precedence (workspace wins) UNVERIFIED |
| K-9 | MCP servers available to a Kiro task come from the harness profile only. | data-egress.md "Indexing and embedding" | UNVERIFIED |
| K-10 | No BAA allowlist entry exists for Kiro; sessions are `any-provider` until one is added. | data-egress.md "BAA configuration allowlist" | No BAA terms found |
| K-11 | Output is normalised to the harness review schema or marked unstructured; it counts as evidence only with a digest in the content-addressed store. | README "Structured review results"; approval-ledger.md `evidence_refs` | Output format UNVERIFIED |
| K-12 | Verified `kiro-cli` version is recorded in README Prerequisites; the compatibility test fails on mismatch. | README "External components must be version-pinned" | Install method/version UNVERIFIED |
| K-13 | Adapter can cancel a run and records limits/cost per job. | README control plane | UNVERIFIED |

## Cited company decisions

| Constraint | Source |
|---|---|
| Approvals live outside agents' reach | `docs/architecture/approval-ledger.md` ("Who writes, and how it is enforced") |
| Egress classes, BAA allowlist, forced configuration | `docs/security/data-egress.md` |
| One writer per worktree; packet before code | `AGENTS.md` |
| External components are version-pinned | `README.md` "Capabilities and implementation status" |

## Unresolved questions

- [ ] Does Kiro CLI have managed/enterprise configuration, and can users override it? — owner: TBD, blocks: T1
- [ ] Do its hooks fail closed, and which tool calls do they intercept? — owner: TBD, blocks: T2
- [ ] Is there a documented headless mode and output format? — owner: TBD, blocks: T1
- [ ] Which auth paths exist; is any covered by a BAA or zero-retention terms? — owner: TBD, blocks: T3
- [ ] Is Kiro used at all for `baa-covered-providers` work? — owner: TBD, blocks: T3
