# Acceptance tests: docs-alignment

Each item names the command or procedure that proves it and the requirement it covers.
Check an item only when the result is recorded in `handoff.md`.

## Behavior

- [ ] D1: the doc defines the encoding rules and the line/signature relation, and contains
  a worked example — `grep -n -i "canonical" docs/architecture/approval-ledger.md` shows
  the new subsection — covers: D1
- [ ] D1: the worked example's bytes and SHA-256 are reproduced by an independent
  one-off script (not copy-pasted from the doc); script and output pasted into
  `handoff.md` — covers: D1
- [ ] D1: a reordered/nested-key variant of the same record, run through the same script,
  yields identical bytes — covers: D1
- [ ] D2: every recovery edge named in `release-gate.md` exists in `ALLOWED_TRANSITIONS` —
  `node -e 'import("./harness/policy/transitions.ts").then(m=>console.log(JSON.stringify([m.ALLOWED_TRANSITIONS.MigrationGated.includes("Repair"),m.ALLOWED_TRANSITIONS.Released.includes("Failed"),m.ALLOWED_TRANSITIONS.Failed.includes("Repair")])))'`
  prints `[true,true,true]` — covers: D2
- [ ] D2: `release-gate.md` no longer says `Failed or Repair` without a source state, and
  states where an applied production migration is recorded —
  `grep -n "Failed\|Repair" docs/architecture/release-gate.md` read in context — covers: D2
- [ ] D3: `data-egress.md` contains the order `local-only` → `baa-covered-providers` →
  `any-provider` and the unmatched-session sentence — `grep -n "most restricted\|unmatched" docs/security/data-egress.md` — covers: D3
- [ ] D3: `grep -n "mode 600" docs/security/data-egress.md` returns nothing; `0600` and
  `0700` appear — covers: D3
- [ ] D4: `grep -n "Preview verified\|Migration gated\|untrusted-origin" README.md` returns
  nothing — covers: D4
- [ ] D4: the Phase 1 paragraph no longer says the decision "is enforced from Phase 1" as a
  present-tense fact —
  `grep -n "enforced from Phase 1" README.md` returns nothing — covers: D4
- [ ] D4: the "Run ledger and budgets" row does not cite `approval-ledger.md` as defining
  budgets — read `README.md` capability table — covers: D4
- [ ] D5: with `design.md` saying "No migration", the template still tells the author to
  keep a classifier line — read `specs/_template/acceptance-tests.md` — covers: D5
- [ ] D5: `specs/_template/tasks.json` parses and its statuses are not `TaskState` names —
  `node -e 'const t=require("./specs/_template/tasks.json");const s=new Set(["Draft","Planned","Approved","Implementing","Reviewing","Verified","PreviewVerified","MigrationGated","Released","Observed","Closed","Blocked","Failed","Cancelled","Repair","Superseded"]);process.exit(t.some(x=>s.has(x.status))?1:0)'`
  exits 0 — covers: D5

## Invariants

- [ ] Only these paths changed: `docs/architecture/approval-ledger.md`,
  `docs/architecture/release-gate.md`, `docs/security/data-egress.md`, `README.md`,
  `specs/_template/*`, `specs/docs-alignment/*` — `git diff --name-only main...`
- [ ] No `harness/` file changed — `git diff --name-only main... | grep '^harness/'`
  prints nothing
- [ ] No rule loosened: reviewer confirms in `handoff.md` that default-`local-only`,
  unverified-entry-counts-as-none, and classifier fail-closed text is unchanged or stricter

## Migration

No migration (see `design.md`). One classifier line kept per the gate rule:

- [ ] Migration classifier reports `clean` for the diff — `<migration-classifier-command>`
  — **not available**: no classifier exists for this repository yet. Record "n/a: no
  classifier" in `handoff.md`; do not report this item as passing.

## Preview

- [ ] n/a: no deployment. Record "n/a" in `handoff.md`.
