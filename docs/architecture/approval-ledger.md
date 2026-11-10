# Approval and evidence ledger

Status: Design (Phase 1). Type definitions: `harness/policy/transitions.ts`.

## Purpose

Record every approved task-state transition outside the agents' reach, so that
"approved" is a fact a human or a policy wrote, never a claim an agent made. The
state machine itself is defined in `harness/policy/transitions.ts`.

## Storage

- One append-only JSONL file per project: `<ledger-root>/<project-id>.jsonl`.
- `<ledger-root>` lives **outside every worktree** and outside the repository
  (e.g. `<ledger-root>` = `/var/lib/<harness-name>/ledger/`), so a checkout, a
  `git clean`, or an agent's workspace write scope never reaches it.
- One JSON object per line. Lines are never edited or deleted; a correction is a new
  record (e.g. a transition to `Blocked` or `Superseded`).

## Record schema

| Field | Type | Meaning |
|---|---|---|
| `id` | string | Unique record ID (e.g. ULID). |
| `task_id` | string | Feature packet ID (`specs/<id>/`). |
| `from_state` | `TaskState` | State before the transition. |
| `to_state` | `TaskState` | State after the transition. |
| `actor` | `"human"` \| `"policy"` | Kind of approver. Never an agent. |
| `approver` | string | Identity of the human (e.g. `<git-host-username>`) or the policy (`<policy-name>@<version>`). |
| `target` | object | Exact thing approved: `{ commit_sha, deployment_target? }`. `deployment_target` is `<provider>/<project>/<environment>` when the transition concerns a deployment. |
| `evidence_refs` | `{ name, digest: { sha256 } }[]` | Evidence files supporting the decision (test output, review findings, preview record), bound by digest. Files live in the content-addressed store `<local-artifacts-directory>/evidence/sha256/<digest>` so a verifier can re-hash them. Digests only; evidence contents never enter the ledger. |
| `policy_digest` | string | SHA-256 (hex) of the policy version that evaluated the transition (the `harness/policy/` module at that commit), so a later policy change cannot silently re-interpret old approvals. |
| `timestamp` | string | ISO 8601 UTC time the record was written. Informational only: never used as the time for signature verification (see "Approver identity"). |
| `prev_record_hash` | string | SHA-256 (hex) of the previous line's exact bytes; 64 zeros for the first record. |
| `signature` | string | SSH signature over the record's canonical bytes without this field (see "Approver identity"). |

A record binds to the exact commit **and** the exact evidence: re-running tests with a
different result produces a different digest, so the old approval no longer matches.
This follows the in-toto Statement pattern (subject plus digests) without adopting
the full format.

Example line (wrapped for reading):

```json
{"id":"<ulid>","task_id":"<feature-id>","from_state":"Reviewing","to_state":"Verified",
 "actor":"human","approver":"<username>","target":{"commit_sha":"<sha>"},
 "evidence_refs":[{"name":"review.json","digest":{"sha256":"<sha256-hex>"}}],
 "policy_digest":"<sha256-hex>",
 "timestamp":"2026-10-26T12:00:00Z","prev_record_hash":"<sha256-hex>",
 "signature":"<sshsig>"}
```

## Tamper evidence

- Each record carries `prev_record_hash`, forming a hash chain. Verification walks
  the file and recomputes each hash; any edited, removed, or reordered line breaks
  the chain from that point on.
- The chain detects tampering; it does not prevent it, and it protects a record only once
  something later commits to it. Until then the tail can be edited or truncated and the
  remaining prefix still verifies.
- **Anchor before acting.** Before the control plane acts on an approval, the hash of the
  record carrying it must be written to the anchor repository. The control plane checks
  that the file's chain extends the latest anchored head exactly; a ledger that ends before
  the latest anchor has been truncated, and the task is treated as `Blocked`.

### Anchor repository

- A separate private git repository, `<ledger-anchor-repo>`, on a remote. After each
  append the writer commits `{ head_hash, record_count }` to it as an SSH-signed commit and
  pushes.
- On the remote, force-push and branch deletion are blocked with no bypass, and the writer
  pushes with a non-admin deploy key.
- The anchor commit time is the **trusted clock** for signature verification
  (`verify-time`). An RFC 3161 timestamp of the head hash may be added later; only the hash
  leaves the machine.
- If the remote is unreachable, approvals fail closed: nothing is acted on until the
  record is anchored.
- Not used: a public transparency log (e.g. Sigstore Rekor). Entries there are public and
  permanent and carry signer identity, which a HIPAA-adjacent organization should not publish.
- Limit: the repository owner can disable branch protection. This protects against agents
  and a compromised writer, not against the owner.

## Serialization

- Exactly one process appends: the ledger writer (`<approve-command>` and the policy
  actor both submit through it). It holds an exclusive lock on `<ledger-root>/<project-id>.lock`
  for the whole read-head, append, fsync sequence.
- Each append request carries the `prev_record_hash` it expects. The writer rejects the
  request if that is not the current head (compare-and-append), so two approvals made from
  the same stale view cannot both land or fork the chain.

## Approver identity

File ownership controls who can write, not who approved: `actor` and `approver` are
otherwise strings the writer copies in.

- **Every record is signed** with `ssh-keygen -Y sign` over its canonical bytes: compact
  JSON with sorted keys, all fields except `signature`.
- **One namespace per role**, enforced by `allowed_signers`, so one role's signature
  cannot be replayed as the other's:
  - Human: `-n approval-v1@<harness-name>`. The key is hardware-backed (a FIDO `-sk` key
    on a security key, or a Secure Enclave key), with **two keys enrolled**, since a
    hardware key cannot be backed up. Apple's `ssh-keygen` needs a FIDO provider for
    `-sk` keys; the Secure Enclave flow is not yet verified end to end.
  - Policy: `-n policy-v1@<harness-name>`. A software ed25519 key held only by the policy
    actor's account, rotated on a short schedule (`<policy-key-rotation-period>`).
- **`allowed_signers` lives in the anchor repository**, so every key change is a signed,
  versioned commit. Values must be quoted. Example:

  ```text
  <username>@<harness-name> namespaces="approval-v1@<harness-name>",valid-after="20261026Z" sk-ecdsa-sha2-nistp256@openssh.com AAAA...
  policy@<harness-name> namespaces="policy-v1@<harness-name>",valid-after="20261026Z",valid-before="20270126Z" ssh-ed25519 AAAA...
  ```

- **Verification time.** An SSH signature carries no timestamp. Verify with
  `-O verify-time=` set to the anchor commit time of the record, never the record's own
  `timestamp`; otherwise a retired key could backdate approvals.
- **Rotation:** add the new key with `valid-after=T`; set the old key's `valid-before=T`.
  Records anchored before `T` keep verifying.
- **Compromise:** set the key's `valid-before` to the last trustworthy anchor time. Records
  anchored earlier stay valid; anything later must be re-approved. Revocation with `-r`
  invalidates every record the key ever signed, so use it only for a key that was never
  trustworthy.
- A record whose signature is missing, fails verification, or whose namespace does not
  match its `actor` is invalid and is not counted as an approval.

## Who writes, and how it is enforced

| Party | May | Enforced by |
|---|---|---|
| Agents (any runtime) | Request a transition by writing a request artifact under `<local-artifacts-directory>/requests/` | Agent permission profiles grant no write access to `<ledger-root>` |
| Human approver | Append a record via `<approve-command>` | OS file ownership: only the approver/ledger account can write `<ledger-root>` |
| Policy actor | Append a record when its checks pass | Runs as the ledger account, outside agent processes |
| Control plane | Read the ledger; call `canTransition` before acting | Read-only access |

Enforcement is filesystem permissions and hooks, **not instructions**:

- `<ledger-root>` is owned by a separate OS user (`<ledger-user>`), mode `0750` for
  the directory and `0640` for files; agent processes run as a user without write access.
- Each runtime's managed configuration (see `docs/security/data-egress.md`, "Enforcement")
  denies writes to `<ledger-root>/**` and its managed hook rejects shell commands
  referencing it. These are defence in depth; the OS permission is the real boundary,
  because shell commands can evade path matching.
- Agents hold neither signing key, so even a successful write could not produce a valid
  record.
- `AGENTS.md` and `CLAUDE.md` restate the rule only so agents do not waste turns trying.

## Reading the ledger

The current state of a task is the `to_state` of its latest record whose chain
verifies. A task with no records is in `Draft`. If verification fails, the task is
treated as `Blocked` until a human resolves it.

A `Blocked` task resumes only to the state it was blocked from, and only at the commit it
was blocked at: the control plane passes the whole record that entered `Blocked` (a
`BlockedRecord`) plus the current commit to `canTransition` in its `context`. Resume is
allowed only if the target equals that record's `from_state` and the current commit equals its
`target.commit_sha`; otherwise it is denied. Both values come from the verified ledger and
the worktree, never from the agent's request. A commit pushed while the task was `Blocked`
therefore cannot inherit the earlier approvals. `canTransition` also rejects an approval
whose `from_state`/`to_state` differ from the requested transition.

## Rationale and sources (adopted 2026-10-26)

| Decision | Why | Source |
|---|---|---|
| Bind to commit **and** evidence digests, plus the policy digest | A commit-only approval lets evidence be regenerated after approval. In-toto Statements require a digest per subject; SLSA's Verification Summary Attestation records digests of its inputs and its policy. | https://github.com/in-toto/attestation/blob/main/spec/v1/statement.md, https://slsa.dev/spec/v1.1/verification_summary |
| Private anchor repository; no public log | A local anchor shares the ledger's failure domain. Sigstore Rekor entries are public, permanent and carry signer identity. RFC 3161 proves time, not completeness. | https://docs.sigstore.dev/about/security/, https://www.rfc-editor.org/rfc/rfc3161, https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches |
| Per-role namespaces, quoted validity windows, anchor time as `verify-time`, `valid-before` instead of `-r` on compromise | Reproduced locally on OpenSSH 10.3p1: unquoted values are rejected; an expired key verifies only with `-O verify-time=`; a key listed in `-r` fails even with an old `verify-time`. SSHSIG carries no timestamp. | `man ssh-keygen`, https://raw.githubusercontent.com/openssh/openssh-portable/master/PROTOCOL.sshsig |

Optional later: shape records as in-toto Statements with a harness-specific
`predicateType`, so standard attestation tooling can read them.

## Open questions

- Request format: a file in the artifacts directory, an MCP tool, or a CLI command?
- Which transitions need a `human` actor and which may a `policy` actor approve?
  (Placeholder: `TODO(human)` in `harness/policy/transitions.ts`.)
- Whether the account's plan supports blocking force-push with no bypass on a private
  anchor repository (rulesets are documented for Team and Enterprise; unverified for
  personal plans).
- Whether to add RFC 3161 timestamps alongside the anchor commit time.
- Which hardware key path to use for the human key (security key with a FIDO provider,
  or Secure Enclave), once verified end to end.
- Recovery procedure after a detected chain break or anchor mismatch.
- Evidence files must themselves be PHI-free: who checks that, and how?
- Migration path from this per-machine file to the Phase 2 shared ledger service
  (README Roadmap) without breaking the chain or re-signing past records.
- Retention, and whether evidence referenced by `evidence_refs` must be retained as long
  as the ledger.