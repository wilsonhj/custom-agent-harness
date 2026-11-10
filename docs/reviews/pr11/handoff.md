# External review brief for PR 11 correction

This branch corrects one verified factual issue in the RL research synthesis. It is a
review branch, not an approval or release. Original PR: https://github.com/wilsonhj/custom-agent-harness/pull/11.

- Original reviewed head: `120f2932011c129b684b26e700a8dc4957070c97`.
- Integration baseline: `dd5a2009661c562672c090bff82656ab36cb6ff1` (`main` after PRs 7 and 8).
- Fix branch: `codex/review-fixes-pr11`. A draft PR, when published, will identify its exact published tip.
- Relevant change: `Research_Docs/agent_harness_reinforcement_learning.md`, EvoHarness-RL row.

The original text incorrectly called the abstract's 8.5-point gain unreproducible. In
[Table 1 of EvoHarness-RL v2](https://arxiv.org/html/2608.05446v2#S3.T1), success-rate
differences over SkillRL are 95.0−85.0, 80.6−73.2 and 26.0−18.0. Their mean is
8.4667, rounded to 8.5. The rounded Δ-column difference 43.9−36.3=7.6 additionally
includes WebShop Score and must not replace the success-rate average. The correction
distinguishes the two aggregations and preserves the surrounding caveats.

Review the arithmetic and metric labels directly against the primary table. Check that
the correction does not strengthen the paper's claims about generalization or uncertainty.
The original PR description still contains the old claim; this branch describes the
corrected result, and a future draft PR should use that wording. No other research
conclusion was intentionally changed.

To isolate the correction after fetching this branch:

```bash
git diff 120f2932011c129b684b26e700a8dc4957070c97 HEAD -- Research_Docs/agent_harness_reinforcement_learning.md
git diff --check
npm run check
```

The Linux pilot is inherited from current main. Its Bubblewrap execution tests require
Linux; macOS failures are environment limitations, not evidence of a research regression.
The new adapter/Kiro suites described by other packets remain future work.

Local validation on Node 24.20.0: `npm run check` and `git diff --check` pass.
The success-rate mean independently recomputes as 8.466666666666663. No executable
files differ from integration baseline `dd5a200`; this prose correction adds no new test.
The inherited Node 24.20.0 macOS `npm test` baseline is 90 total: 86 passed, 2 failed,
2 skipped. The failures are `sandbox argv is offline, minimally mounted, and does not
expose controller state` (Linux Bubblewrap required) and `default CLI execution fails
closed when isolation is unavailable` (expected `/worker failed/`, received
`Linux bubblewrap is required; no unsandboxed fallback`). Each published branch still
requires supported Linux CI.

Exact next action: an external agent reviews this correction and its source arithmetic,
then the owner chooses whether to update or replace PR 11. No merge is authorized by
this brief.
