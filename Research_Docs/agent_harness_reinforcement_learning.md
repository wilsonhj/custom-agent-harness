# Reinforcement Learning for an Agent Harness

Research findings exported October 28, 2026.

## Executive summary

Reinforcement learning can improve an agent system by learning the harness’s control decisions or by training the model running inside it. For an existing agent, a practical starting hypothesis is a small controller around a frozen model, evaluated against a well-optimized static harness.

Here, an agent harness means the execution layer responsible for tool routing, context assembly, memory, verification, retries, and stopping. The strongest practical lesson from the reviewed work is not that RL always wins, but that baseline quality, reliable rewards, data coverage, and execution interfaces substantially affect results.

This document preserves the prior research synthesis. Cited sources and numerical findings were checked against the source pages on October 28, 2026; corrections from that check are incorporated below. Long appendices were only partly read verbatim, so appendix-level details carry less certainty. Recommendations and proposed designs below are distinguished from reported experimental results.

## 1. What should learn?

| Approach | What changes | Access needed | Relevant use |
|---|---|---|---|
| Harness-policy learning | A controller selects execution actions or configurations | Can use a frozen, black-box LLM | Learning when to retrieve, verify, revise, or stop |
| Agent-model RL | Model weights change using rewarded trajectories | A trainable model and suitable training infrastructure | Improving tool use and task-solving behavior within a harness |
| Prompt/program optimization | Instructions, demonstrations, or textual components change | Does not require model-weight access | Establishing a strong baseline before adding RL |

Harness-policy learning is exemplified by [HarnessMDP](https://arxiv.org/html/2607.05458v3). Model RL inside native coding harnesses is investigated by [Lego-RL](https://arxiv.org/html/2608.17393). Reflective prompt optimization is described in the [DSPy GEPA documentation](https://dspy.ai/current/tutorials/gepa_ai_program/).

Recommendation: choose the optimization target before the algorithm. A small external controller and billion-parameter model training have very different implementation and data requirements.

## 2. Research evidence and limitations

| Research | Reported finding | Important qualification |
|---|---|---|
| [HarnessMDP, v3](https://arxiv.org/html/2607.05458v3) | Offline structural execution control with a frozen LLM; six-domain averages improved by about 2.0 percentage points in rubric accuracy and 2.8 points in task success; public-benchmark adapters improved by 13–18 points | Twenty held-out tasks per setting; no confidence intervals or significance tests reported; coding supplies most of the six-domain accuracy gain and three domains worsened; adapters have 16 training tasks and use custom rubric scoring rather than official benchmark scoring |
| [EvoHarness-RL, v2](https://arxiv.org/abs/2608.05446) | External workspace, supervised initialization, and cost-aware GRPO; the abstract reports an 8.5-point average-success advantage over the strongest open-source baseline | Table 1 reproduces the 8.5-point mean success-rate gain over SkillRL: (10.0 + 7.4 + 8.0) / 3 = 8.4667. The 7.6-point difference between rounded Δ columns uses four metrics, including WebShop Score, and is a different aggregation. An always-on-workspace ablation isolates learned access control (−6.4 points seen, −4.5 unseen on ALFWorld); no ablation removes the workspace itself |
| [Lego-RL](https://arxiv.org/html/2608.17393) | Qwen3.5-35B-A3B improved from 64.0% to 70.4% on SWE-bench Verified under OpenHands SDK | Headline figures are peak validation scores across checkpoints on SWE-bench Verified itself, with no separate test split; main production configurations ran once each; harnesses were trained separately |
| [Multi-Harness Reinforcement Learning, v2](https://arxiv.org/abs/2609.04518) | Mean source-harness training gains of 0.48–0.77 percentage points versus a 7.13-point gap between execution interfaces | Gains are point estimates (the corrected interval for the +0.77 gain includes zero); neither main normalization variant significantly improved over SFT on the unseen harness |
| [Frozen-model controller comparison](https://arxiv.org/html/2607.25415) | Optimized static control generally matched or beat adaptive bandit/REINFORCE controllers at practical budgets, usually at lower token cost | Single-author study with 25–60 episodes per run; static control lost on coding with one model; a 729-configuration action space; the longer experiment, which covered one domain and model, did not close the gap |

Interpretation: learned control is promising when recurring, consequential decisions are poorly served by a fixed policy. HarnessMDP's comparison against always-on verification and EvoHarness-RL's always-on-workspace ablation both favor learned selection over always applying an intervention. It is less compelling when the static baseline is already strong, rewards are unreliable, or the action space exceeds available experience. In the Multi-Harness study, execution-interface choice produced a larger difference (7.13 points) than RL training did (0.48–0.77 points); the larger RL gains reported by EvoHarness-RL, HarnessMDP and the SWE-bench study above come from different setups and do not support that comparison across the reviewed work.

### HarnessMDP details

The study uses six controlled domains with 100 tasks each, split into 80 training and 20 held-out tasks. Two public-benchmark adapters use 16 training and 20 held-out tasks each. Three seeds and three rollouts per held-out task yield 180 evaluation episodes per setting, but not 180 independent task identities.

Reported rubric-score changes are coding 0.712 to 0.812, a 10.0-percentage-point increase; planning 0.385 to 0.412; knowledge-work 0.450 to 0.464; long-memory 0.453 to 0.451; multi-tool 0.528 to 0.515; and research 0.314 to 0.311. Research task success fell 2.6 points, which the authors attribute to increased early submission. The public-benchmark adapters changed from 0.337 to 0.519 (τ-bench retail) and 0.415 to 0.547 (DB-Bench), but rest on 16 training tasks and the authors' own rubric.

The learned policy beat a forced-verification baseline in five of eight settings and was slightly worse in the other three. No uncertainty estimates are reported, and several per-domain changes are under 1.5 points, so they may be within noise. These outcomes caution against treating improved process behavior as guaranteed improvement in every task metric. See [the experimental setup and results](https://arxiv.org/html/2607.05458v3).

### Lego-RL details

The study reports SWE-bench Verified changes of 64.0% to 70.4% for OpenHands SDK, 62.4% to 68.2% for Claude Code, and 57.2% to 66.6% for OpenCode. Training uses a 2,699-task OpenSWE-derived index reported as disjoint from evaluation at repository and instance levels. These figures are peak validation scores across training checkpoints, and SWE-bench Verified serves as the validation set with no separate test split, so they likely overstate held-out performance. Each harness is trained separately, so these results do not establish cross-harness generalization. See [the paper](https://arxiv.org/html/2608.17393).

## 3. Proposed architecture

Start with a frozen executor and a constrained learned controller. This is a proposed design informed by the reviewed work, not a claim that one paper validates the exact system below.

```text
Task + environment
        |
        v
Observation builder
        |
        v
Learned harness controller
        |
        v
Deterministic permissions + budget gate
        |
        v
Existing executor -> tools / memory / verifier
        |
        v
Transition log + terminal outcome
        |
        v
Offline training -> held-out evaluation -> gated deployment
```

| Component | Suggested starting design |
|---|---|
| Observations | Task type, remaining budget, tool errors, verifier outcomes, progress indicators, retrieval availability, recent actions |
| Actions | Continue, retrieve, verify, revise, change an approved route, stop, escalate |
| Executor | Keep the LLM and tool implementations fixed initially |
| Guardrails | Enforce permissions, spending limits, and required approvals outside the learned policy |
| Learning boundary | Version controller updates between deployments rather than exploring uncontrollably in production |

A compact state vector should be treated as an observation, not proof that the process is Markovian. Retain a short action history and structured progress evidence, then test whether additional history improves held-out performance.

## 4. Choosing a learning method

| Decision structure | Starting option | Basis |
|---|---|---|
| One configuration selected at task start | Contextual bandit | Appropriate framing for configuration selection followed by an episode outcome; investigated in the [controller comparison](https://arxiv.org/html/2607.25415) |
| Repeated choices affecting subsequent execution | Sequential controller, potentially trained offline | [HarnessMDP](https://arxiv.org/html/2607.05458v3) learns structural actions from trajectory outcomes |
| Improve generated tool calls and responses | Model RL with a supported policy-gradient trainer | [Lego-RL](https://arxiv.org/html/2608.17393) uses GSPO; [EvoHarness-RL](https://arxiv.org/abs/2608.05446) uses cost-aware GRPO after supervised initialization |
| Improve instructions with limited rollouts | Prompt optimization baseline | [GEPA](https://dspy.ai/current/tutorials/gepa_ai_program/) uses reflection over execution feedback to evolve candidates |

Avoid a combinatorial cold start. The 729-configuration controller study illustrates the exploration burden introduced by jointly selecting prompt style, tool or retrieval policy, memory, planning, verification, and step budget (six levers with three values each). A practical proposed alternative is to begin with a few reviewed configurations or one decision family, then expand after demonstrating value.

## 5. Reward design

Verifier quality is foundational. Exact matching, executable tests, schema checks, state comparisons, and task-specific rubrics can supply rewards; suitability depends on the task. [NeMo Gym’s environment documentation](https://docs.nvidia.com/nemo/gym/about/concepts/environments/) separates the dataset, harness, verifier, and state.

Proposed objective: maximize verified task success subject to approved cost, latency, and execution constraints. Track objective components separately before combining them into a weighted scalar, so trade-offs remain visible.

### Reward outcomes, not rituals

Do not automatically reward verification, reflection, or memory use. [HarnessMDP](https://arxiv.org/html/2607.05458v3) explains that action-pattern bonuses can alter the optimum and favor unnecessary steps; its experiments use terminal task quality.

Proposed rule: discretionary checks should justify themselves through better outcomes. Checks required for safety or compliance remain deterministic requirements outside the learned policy.

### Keep in-loop checks separate from the grader

If the controller's verify action consults the same check that computes the terminal reward, the reward leaks into the episode. The controller can then learn to verify until the check passes and stop, which scores well only because it observes the grader. Proposed rule: in-loop checks (for example, visible tests or schema validation) and the terminal grader (for example, held-out tests or an independent rubric) must be distinct.

### Score escalation explicitly

Assign escalation a defined cost below success and above unhandled failure. If escalation is free, the controller learns to escalate every hard task; if it counts as failure, the controller never escalates. Report the escalation rate alongside success.

### Separate failure types

Distinguish:

- Task failure: execution ran normally, but the task was not solved.
- Policy-budget exhaustion: a valid run reached an allowed interaction limit.
- Infrastructure failure: sandbox, dependency, service, or verifier malfunction.
- Policy violation: a prohibited action was attempted or executed.

Use these distinctions for training admission and operational reporting. [Lego-RL](https://arxiv.org/html/2608.17393) masks invalid execution failures from training while retaining valid trajectories that exhaust configured interaction budgets.

Record the action that preceded each infrastructure failure. If some actions, such as verification, cause sandbox timeouts more often, excluding those episodes from training hides a real cost of those actions.

## 6. Instrumentation

Proposed controller transition record:

```text
task_id, split_id, episode_id, step_id
model_version, harness_version, controller_version
observation, available_actions
selected_action, behavior_probability_if_known
tool_result, verifier_result
cost, latency, termination_reason
terminal_outcome, reward_components
```

Record behavior probabilities when known from randomized collection. Do not invent probabilities for historical deterministic logs.

Model-weight RL requires more than a final transcript. Capture actual per-call contexts, generated token IDs, generation-time log probabilities, and response masks at the serving boundary. Harness compaction and history rewriting can otherwise cause mismatches between rollout generation and training likelihoods. See [Lego-RL](https://arxiv.org/html/2608.17393).

## 7. Infrastructure options

| System | Useful role | Boundary |
|---|---|---|
| [Agent Lightning](https://microsoft.github.io/agent-lightning/0.2.0/how-to/write-first-algorithm/) | Separates execution and optimization using runners, structured traces, resources, and a central store | Integration infrastructure does not choose the application’s objective |
| [NeMo Gym](https://docs.nvidia.com/nemo/gym/about/concepts/environments/) | Structures datasets, harnesses, verifiers, and state as environments | Verification and environment design remain task-specific |
| [Lego-RL](https://arxiv.org/html/2608.17393) | Model RL inside native coding harnesses, serving-boundary capture, sandbox execution | More relevant to model training than lightweight external routing |
| [DSPy / GEPA](https://dspy.ai/current/tutorials/gepa_ai_program/) | Optimized instruction/program baselines using execution feedback | Not equivalent to sequential RL for delayed action consequences |

Implementation recommendation: preserve an operationally sound harness. Add a clean decision interface, trustworthy scoring, and trace collection before considering migration.

## 8. Evaluation plan

The key comparison is learned control versus the best static control at matched resources, not versus an intentionally weak default. Both the [static-controller comparison](https://arxiv.org/html/2607.25415) and the [multi-harness study](https://arxiv.org/abs/2609.04518) make baseline and interface choice material confounders.

Proposed sequence:

1. Establish the current harness, optimized static harness, simple rules, and learned controller as separate baselines.
2. Measure headroom before training (see below).
3. Separate training, controller selection, and untouched final evaluation; keep related task families together where leakage is plausible. Do not select checkpoints on the final evaluation set.
4. Compare success at matched spending and latency, alongside unconstrained results.
5. Repeat evaluation and report uncertainty and task-level outcomes.
6. Test transfer across task families, model versions, tool behavior, and context limits.
7. Audit reward exploitation, premature stopping, unnecessary verification, and prohibited action attempts.

Do not assume a gain transfers across harnesses or model upgrades. The [October multi-harness revision](https://arxiv.org/abs/2609.04518) reports substantial interface dependence and no significant improvement on its unseen harness. Plan for retraining or revalidating the controller when the underlying model changes.

### Headroom check

Run every candidate static policy on every training task. Compare the best single policy with an oracle that picks the best policy for each task in hindsight. The oracle gap is an upper bound on what any task-level selector can gain; if it is only a few points, stop before training a controller.

### Sample size

Task identities, not rollouts, are the unit of evidence; repeated rollouts per task reduce noise but do not add tasks. At 20 held-out tasks and a success rate near 50%, the standard error is about 11 points. For a paired comparison where the two policies disagree on about 10% of tasks, detecting a 3-point gain at 80% power (McNemar test, α = 0.05) needs roughly 870 held-out tasks. Set the target task count before collecting data.

## 9. Recommended first experiment

Learn verification and stopping for one task family while leaving task execution, retrieval, permissions, and the underlying model unchanged.

- Define approved actions: continue, verify, revise after feedback, stop, escalate.
- Collect diverse sandbox trajectories, including failures, with randomized action selection so behavior probabilities are known.
- Compare an outcome-trained controller with optimized fixed rules.
- Measure verified success, cost per successful task, latency, escalation rate, and premature termination.
- Expand into memory or routing only after a repeatable held-out improvement.

Proposed phasing:

| Phase | Work | Proceed when |
|---|---|---|
| 0 | Build a trustworthy grader separate from in-loop checks, transition logging, and fixed rules tuned on the training split (for example, verify after the first complete answer and revise at most k times, with k chosen from data). Run the headroom check | The oracle gap over the best fixed rule is large enough to matter |
| 1 | Train a contextual bandit that chooses among three to five fixed rules at task start | It shows a repeatable held-out gain at matched cost |
| 2 | Train the sequential controller over the approved actions | — |

Phase 0 is useful even if no learned controller wins. The reviewed evidence makes "optimized fixed rules match the learned controller" a likely outcome, and the grader, logs, and tuned rules remain valuable in that case.

This isolates a learnable control problem without simultaneously introducing prompt search, memory evolution, tool selection, and model training. The experiment tests whether selecting the right intervention is more valuable than always applying it. [HarnessMDP](https://arxiv.org/html/2607.05458v3) tests a version of this through its forced-verification baseline, which the learned policy beat in five of eight settings.
