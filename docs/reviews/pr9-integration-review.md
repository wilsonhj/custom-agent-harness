# External review brief for PR 9 integration

This branch carries the reviewed adapter/Kiro specification correction onto current main
without changing the original PR branch. Original PR: https://github.com/wilsonhj/custom-agent-harness/pull/9.

- Original reviewed head: `6369aec0bdd2a2650808c952a4b14449953db80b`.
- Integration baseline: `dd5a2009661c562672c090bff82656ab36cb6ff1` (`main` after PRs 7 and 8).
- Fix branch: `codex/review-fixes-pr9`; integration commit `a50344c`.

No new functional specification defect was found in PR 9. Its merge conflict was in
the adapter handoff's next action: the old branch said T2 decisions were outstanding,
while main already recorded them. The resolution preserves those recorded decisions,
requires review/CI on this follow-up, and requests the external T2→T3 transition before
implementation. It does not approve that transition or alter task statuses.

Review the complete packet delta and ensure these requirements survive integration:

- Both adapter and runtime users cannot read or write private approval signing keys or
  the anchor deploy key. The separate approval-ledger write ban and Kiro ledger-read
  denial remain intact (A-21 and K-3, including planned acceptance cases).
- Only schema-valid structured reviewer findings admitted under A-17 can be evidence;
  arbitrary runtime assertions remain untrusted.
- A-22's independently trusted verifier entrypoint and sandbox boundary remain intact.
- Existing human-owned T2 decisions are preserved; no agent approves a task phase.

Useful review commands:

```bash
git diff dd5a2009661c562672c090bff82656ab36cb6ff1...HEAD -- specs/adapter-interface specs/kiro-adapter
git diff 6369aec0bdd2a2650808c952a4b14449953db80b HEAD -- specs/adapter-interface/handoff.md
git diff --check
npm run check
```

The packet's adapter/Kiro acceptance suites are planned, not implemented or executed by
this branch. The supported Linux/Node 24 CI job must validate the published branch;
old PR-head CI is not proof for this integration. The Linux pilot's Bubblewrap tests
cannot be treated as passing on macOS.

Local evidence (Node 24.20.0, macOS): syntax and diff checks pass; all 8 packets /
42 task dependencies resolve, with acceptance references for all 22 A / 21 K requirements.
`npm test` reports 86 pass, 2 fail, 2 skipped out of 90. The failures are the Linux-only
sandbox-argv case and restricted-host CLI-message case; see the packet handoff for names
and exact limitations. These results do not replace Linux CI.

Exact next action: external review of the conflict resolution and preserved contracts,
followed by owner review of current supported-host CI. Do not start T3 from this brief.
