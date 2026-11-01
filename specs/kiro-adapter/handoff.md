# Handoff: kiro-adapter / packet revision after review and research

| Field | Value |
|---|---|
| Base SHA | feaf95f (packet draft; repo base 0b9d58f) |
| Head SHA | the commit containing this file (`git log -1 -- specs/kiro-adapter/handoff.md`) |
| Author (runtime / session ID) | Claude Code / session_01AaDiMJVSVxQVfoGPARPgBn |
| Date | 2026-10-28 |

## Changed files

- `specs/kiro-adapter/requirements.md` — K-14..K-21 added, K-1..K-13 tightened, Kiro claims graded (M)/(S)/UNVERIFIED with sources, questions re-pointed.
- `specs/kiro-adapter/acceptance-tests.md` — one test per requirement; Migration/Preview sections removed.
- `specs/kiro-adapter/design.md` — migration justification and rollout/rollback corrected.
- `specs/kiro-adapter/tasks.json` — T1..T7: research, plan review, human decisions, prerequisite adapter/worktree work, adapter, tests, code review.

## Implemented requirements

- None. Packet drafting only; no adapter code exists.

## Tests run and results

| Command | Result | Evidence |
|---|---|---|
| (none) | not run | `tests/kiro/*` do not exist yet (T5) |

## Review: Claude subagent adversarial review (NOT Codex)

`/codex:adversarial-review` is not installed in this session and no `codex` CLI is present,
so no Codex review has run (CLAUDE.md requires Codex findings be recorded here; still owed).
A fresh Claude subagent reviewed the first draft read-only. It is not an independent
cross-vendor review. Findings and resolutions:

| # | Sev | Finding | Resolution |
|---|---|---|---|
| 1 | High | No egress-class gate at task start; "any-provider" only a ceiling | Added K-14 + `egress-gate` test |
| 2 | High | Hook coverage tied to agent selection; worktree `.kiro/` untrusted | K-5, K-8 extended; `canary-agents`, `config-precedence` tests |
| 3 | High | No credential/secret handling, forbidden flags, redaction | Added K-15, K-18; `env-scrub`, `flags` tests |
| 4 | High | Kiro never established as approved provider | Added K-17 and open question; blocks T3/T4 |
| 5 | Med | K-9, K-12, K-13 untested; "procedure: TBD"; weak canary | Test added per requirement; canary variants listed. Commands are still named placeholders (`tests/kiro/*`) |
| 6 | Med | Subagent/MCP/context bypasses | K-7, K-9 extended; added K-19; tests added |
| 7 | Med | Prompt injection / forged output; instruction discovery | K-11 tightened; added K-20; invariant added |
| 8 | Med | Ledger/evidence integrity | K-3, K-16 and `request-only`, `evidence-store` tests |
| 9 | Med | Worktree race; adapter contract gaps | K-2 atomic lock; K-1, K-13 cover base SHA, deadline, process tree, session ID; T6 added |
| 10 | Med | Circular blockers; no managed-config/human task; review only at end | Blockers re-pointed; T2 plan review, T3 human decisions added |
| 11 | Med | "refused" vs "treated as any-provider" ambiguity | Added K-21 as a proposal; needs human confirmation |
| 12 | Med | Rollout misuses ledger | design.md rewritten |
| Low | | "No migration" claim, citations, UNVERIFIED labelling, template misuse | Citations now name sections and repo commit; labels graded; N/A template sections removed. Not done: the `Planned` transition request is not recorded |

## T1 update (2026-10-29): kiro.dev now reachable

kiro.dev was allowed in the environment. I fetched the pages listed under (P) in
requirements.md Sources with curl, converted them to text, and read headless, hooks,
exit codes, and the relevant parts of permissions, subagents, sandbox, governance, data
protection and compliance. Requirement statuses K-1, K-4, K-5, K-7..K-10, K-12, K-17, K-18
now cite those pages. Not every page was read in full (permissions, sandbox, settings and
CLI commands were searched, not read end to end). Changes versus the earlier research:

- Hook failure semantics (CORRECTED 2026-11-01): kiro.dev pages conflict. The exit-codes
  page says only exit 2 blocks a PreToolUse hook and other non-zero exits are warnings;
  the hooks actions page says any non-zero exit blocks a PreToolUse call. Not settled;
  must be tested, and the wrapper must exit 2 on every error.
- Subagents (CORRECTED 2026-11-01): on CLI V3 every subagent tool call is checked against
  the parent session policy (intersection) and `toolsSettings` is ignored; on 2.x a
  subagent's own pre-approved tools run even if the parent does not allow them. The docs
  list "Hook triggers" as isolated per subagent; whether parent hooks fire in subagents
  is not stated. `@builtin` and `*` in `tools` include `invoke_sub_agent`.
- `managed-settings.json` supports enterprise deny/ask permission policies in an
  `administration` scope that deny-wins over all others; Kiro says admin controls are
  client-enforced and bypassable with local admin.
- Built-in file tools are outside the sandbox; stdio MCP only if wrapping is on; remote
  MCP is outside. Sandbox failure only warns unless `--sandbox` is passed.
- Headless: documented exit codes 0/1/3/4; exit 4 can follow tool calls by the default agent.
- stream-json is documented as a flag, but no event schema was found.
- Data: Bedrock-backed with possible global cross-region inference; tier-dependent
  service-improvement use and retention; CLI listed HIPAA eligible, no BAA details.
- Acceptance tests added: hook-failure, agent-missing, sandbox-required, workspace-trust.

Earlier research (2026-10-28) summary, kept for history:

kiro.dev was not readable. Findings come from public GitHub (KiroCrew offline mirror of the
kiro-cli docs, kiro-action, issue trackers) and search snippets; a research agent summarised
them, so details need re-checking on kiro.dev. Most consequential for the design:

- Upstream hooks appear fail-open on crash/timeout (S); subagent hook triggers are isolated (M).
  Governance needs a wrapper hook that always exits 2 on error, and a test that hooks fire in subagents.
- `managed-settings.json` is limited to deny/ask rules, sign-in methods, MCP registry and
  models, and Kiro says controls are bypassable with local admin (S). Pinning agents/hooks
  is not shown to be possible, which affects K-4.
- stdio MCP servers run outside the sandbox with full privileges (M); the sandbox covers
  shell, not built-in file tools (S).
- No documented exit codes or JSON output schema for `--no-interactive` (M); K-11 must parse
  defensively.
- Only the latest build is distributed, auto-update must be disabled to hold a version (M).
- Retention: up to 60 days for abuse detection regardless of opt-out; individual-tier content
  may be used for service improvement unless opted out (S). Kiro lists IDE and CLI as HIPAA
  eligible (S) but no Kiro BAA process was found. Kiro CLI is closed source (M).

## Known limitations

- Pages were fetched as HTML and converted to text by a script; they were not rendered, and some pages were only searched, not read in full. (M) and (S) items in requirements.md are not yet re-checked.
- The research summaries are by a subagent over fetched pages, not verbatim reads by me.
- Pricing figures are third-party and omitted from requirements decisions.

## Unresolved decisions

- Provider approval (K-17), K-21 rule, any BAA use of Kiro, managed-config/MDM install — owner: TBD (human), in T3.

## Bot review (cursor[bot], PR comment 2026-11-01)

Treated as claims to verify against kiro.dev (re-fetched 2026-11-01). Result:

| Finding | Verified? | Resolution |
|---|---|---|
| High: unattended runs are not gated by workspace trust | Yes (permissions page: non-interactive and V1/V2 load workspace config without asking) | K-8 rewritten: adapter-side neutralisation (clean checkout, strip `.kiro/`, harness-owned `HOME`) plus admin deny rules; test renamed `workspace-config` |
| High: K-5/K-7 overstate subagent coverage; V3 vs 2.x; `@builtin`/`*` | Yes for V3 vs 2.x and `@builtin`/`*`; hook semantics conflict between pages | K-5 and K-7 rewritten; V3 pinned and 2.x unsupported; tests added (`subagent-tools`, updated `subagent-limits`) |
| Medium: enabling Kiro is a human config change with no root-owned boundary | Yes (design gap) | Added K-22 and test `enable-flag-root-owned` |
| Medium: T1 vs next action inconsistent; K-21 cites a missing assumption | Yes | Next action fixed; K-21 assumption added below |
| Draft wording inconsistent | Yes | Banner changed to "proposed, not approved" |

## Assumptions made

- K-21 (proposed, needs human confirmation): data-egress.md says an unpinned account is
  both "refused" (L97) and "treated as any-provider" (L61). The packet resolves it as:
  refuse Kiro jobs on `baa-covered-providers` and `local-only` content; allow
  `any-provider` content only after the K-17 decision.

- Packet ID `kiro-adapter` given by the user; scope is the Kiro CLI surface only.
- `owner_role: human` for T3 and worktree name `kiro-adapter` in tasks.json are placeholders.
- A runtime with no verified managed configuration is capped at `any-provider` and only under K-17.
- T6 (adapter interface and worktree manager) may need its own packet.

## Exact next action

Finish T1: re-check the remaining (M)/(S) items on kiro.dev, settle the hook failure
semantics by test (not docs), and read the permissions, sandbox and settings pages end to
end. Then a human runs `/codex:adversarial-review` with the codex plugin and records the
findings here (T2), then makes the T3 decisions.
