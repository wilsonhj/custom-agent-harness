# Requirements: local-evidence-workflow

Implement the local milestone requested after the architectural review: complete feature
packet -> isolated worker -> runner-issued verification -> separate reviewer -> signed
human decision -> durable handoff. This packet is an implementation plan, not an approval.

## User-visible behavior

1. A dependency-free Node 24 CLI prepares a dedicated detached Git worktree from a clean
   repository and a complete feature packet. One controller owns each run.
2. Operator-owned configuration outside the repository chooses worker, verifier, reviewer,
   deadlines and output limits. Repository text never selects executable commands.
3. Linux bubblewrap provides a writable worker worktree and read-only verifier/reviewer
   worktrees, no network, fresh environment, no host home or credential mounts. Missing or
   unusable isolation fails closed. No hosted model or cloud invocation is implemented.
4. Runner-issued evidence binds commands, exit status, output digests, packet digest,
   configuration digest, base commit and exact candidate commit. Failed, timed-out,
   excessive-output, malformed-review and changed-snapshot runs cannot request acceptance.
5. A separate reviewer process returns a strict structured review pinned to both SHAs.
   Requesting changes prevents acceptance. A human can sign a digest-bound accept/reject
   statement out of band; the CLI only verifies/imports it, never signs it.
6. Evidence, events and handoffs are durable outside the repository. Corruption, stale
   evidence, concurrent writers and changed candidates are rejected. Restart inspects a
   run; interrupted execution is not automatically repeated.

## Non-goals

Production operations, task-phase approval, implementation of the proposed global ledger,
hosted agents/egress policy, AWS, company-brain MCP, macOS/Windows sandbox support, automatic
repair, multi-agent scheduling and protection against the trusted host controller itself.

## Invariants and sources

Repository sources are pinned to upstream 0b9d58f7dfb1bf8a05b8726921628b56e8d63261.

| Constraint | Source |
|---|---|
| Packet first; assigned worktree; evidence and handoff | AGENTS.md |
| Worker and release authority separate; deny-path tests | README, Security and approvals |
| No self-approved task transitions; external ledger remains authoritative | docs/architecture/approval-ledger.md |
| Offline, unclassified content never sent to hosted providers | docs/security/data-egress.md |
| Production release state machine remains fail closed | harness/policy/transitions.ts; docs/architecture/release-gate.md |

## Assumptions and unresolved questions

- User authorization covers implementing and publishing this proposed milestone as a PR;
  it does not constitute a signed task-state transition or acceptance of its output.
- Linux is the first supported execution platform. macOS requires a separately tested
  backend; there is no weaker fallback. This host's namespace restrictions may prevent the
  real sandbox acceptance test; report that separately from deterministic orchestration tests.
- Local review attestations describe acceptance of a local artifact, NOT approval of a
  task phase, release or knowledge publication. A separate account must own the controller
  and trust configuration when other agents have unrestricted same-user host access.
- Pin an independently maintained allowed-signers file outside both repository and store.
  No commit timestamps or remote Git trust root are used in this milestone.
