# Track 4: Counsel, adversarial review by non-agent models, routing

Date 2026-11-09. Grades: (P) probed locally, (D) official docs read today, (S) secondary, UNVERIFIED.
No API calls were made with credentials. The one network probe was an UNAUTHENTICATED `GET https://openrouter.ai/api/v1/models` (public catalog) and `?output_modalities=decisions`; saved to `tracks/4tmp/` (P for catalog contents, as the API returned them today).
Note: WebFetch summarisation is lossy; items marked (D) are what the fetched docs page said. The WebFetch of `/api/v1/models` returned a nonsense list (anthropic/-prefixed ids); I discarded it and used curl instead.

## 1. OpenRouter chat completions

**Request** (D: https://openrouter.ai/docs/api-reference/overview). `POST https://openrouter.ai/api/v1/chat/completions`, Bearer key. Fields: `model`, `messages` (or `prompt`), `stream`, `response_format`, `tools`, `tool_choice`, `max_tokens`, `temperature`, `top_p`, `seed`, plus OpenRouter-specific `models` (fallback list), `route`, `provider` (routing prefs), `user`, `session_id`, `debug.echo_upstream_body`. Parameters a model does not support are silently IGNORED unless `provider.require_parameters: true`.

**Structured output** (D: https://openrouter.ai/docs/guides/features/structured-outputs):
```json
"response_format": {"type":"json_schema","json_schema":{"name":"review","strict":true,"schema":{...,"additionalProperties":false}}},
"provider": {"require_parameters": true}
```
- Support is per ENDPOINT (provider), not per model, and can change. `require_parameters: true` makes OpenRouter route only to endpoints that support every parameter in the request. Without it the schema may be ignored (D: provider-selection page: "When false, providers may ignore unknown parameters"; response format only a soft preference).
- "Strict" enforcement differs by provider: some guarantee conformance, some treat schema as a hint (D). So the harness MUST validate the returned JSON itself and treat invalid as a failed advisor call. Strict mode may restrict schema features (keep schema flat: enums, strings, integers, arrays, required, additionalProperties:false; no `$ref`/`oneOf` tricks).
- A "Response Healing" plugin exists for non-streaming json_schema requests (D). Do not rely on it; validate locally.
- Catalog (P): `supported_parameters` per model lists `structured_outputs` / `response_format`. For all candidates in section 3 except `z-ai/glm-5.3-prime` (RF only, no SO) both appear.

**Streaming** (D): `stream: true`, SSE, ignore comment lines; usage arrives once in the final chunk before `[DONE]`. For advisors, do NOT stream (non-streaming gives one complete JSON and one usage object; simpler cap accounting). Streaming only if long wall-clock timeouts become a problem.

**Usage and cost** (D): response `usage` has `prompt_tokens`, `completion_tokens`, `total_tokens`, `prompt_tokens_details.{cached_tokens,cache_write_tokens}`, `completion_tokens_details.reasoning_tokens`, `cost` (credits = USD), `is_byok`, `cost_details.{upstream_inference_cost,...}`. Response `model` and (observed in Decisions docs, not confirmed for chat) `provider` identify who served it; `id` is the generation id.
**Generation stats** (D: https://openrouter.ai/docs/api-reference/get-a-generation): `GET /api/v1/generation?id=gen-...` returns `data.{total_cost, usage, cache_discount, tokens_prompt, tokens_completion, native_tokens_*, provider_name, model, finish_reason, latency, generation_time, provider_responses (incl. fallback attempts), cancelled, data_region}`. The page documents no availability lag; UNVERIFIED whether a record is immediately queryable. Use response `usage.cost` as the primary figure and the generation endpoint only for audit/reconciliation, with retry.

**Prompt caching** (D: https://openrouter.ai/docs/guides/best-practices/prompt-caching). Sticky routing keeps follow-ups on the same provider (60 min session TTL; disabled if you set manual provider `order`); `session_id` / `x-session-id` (<=256 chars) pins it. Per-family: DeepSeek, Moonshot, xAI, Z.AI automatic; Qwen and Anthropic explicit `cache_control`; Gemini implicit 2.5+. Cache reads typically 0.1x-0.25x input price. Relevance for counsel: N advisors on DIFFERENT models share no cache; but a repair-loop re-review of the same spec+diff prefix on the same model benefits. Put the stable spec/diff first, the round-specific instructions last.

**Rate limits** (D: https://openrouter.ai/docs/api-reference/limits). Paid models: "no platform-level request cap"; upstream provider limits still apply (so 429 handling needed). Free `:free` models: 20 req/min, 50/day under 10 credits, 1000/day at >=10 credits. There is also an IN-FLIGHT SPENDING BUDGET (a fraction of balance, capped) that can reject requests: `402` with `error.metadata.limit_source` = `openrouter_in_flight_budget` (wait `Retry-After`), `openrouter_key_limit`, or `openrouter_credits`. `GET /api/v1/key` returns `limit_remaining`, budgets. The page had blank status codes in my fetch; mapping to 402/429 is inferred. Do not use `:free` models for code (they typically retain/train; see section 2).

## 2. Privacy controls

Sources: https://openrouter.ai/docs/guides/routing/provider-selection (D), https://openrouter.ai/docs/guides/features/zdr (D), https://openrouter.ai/docs/guides/privacy/data-collection (D).

| Control | Where | What it does | What it does NOT guarantee |
|---|---|---|---|
| `provider.data_collection: "deny"` | per request (default `"allow"`); also an account-wide setting | Only providers that "do not collect user data" are eligible (docs: allow = providers that "store user data non-transiently and may train on it") | It is a provider-DECLARED policy filter. It is not the same as zero retention; abuse-monitoring retention is a separate category (see Safety Retention). No audit by us. |
| `provider.zdr: true` | per request; account privacy settings (per model group: Anthropic, OpenAI, Google, "SpaceXAI", all others); guardrails (can only be stricter) | Routes only to Zero-Data-Retention endpoints. Combined with account/guardrail: ANY of them enabling ZDR enforces it. Request-level cannot weaken account-level. | Not covering plugins/tools (e.g. web search). "Safety Retention" endpoints (retain for abuse detection, not train) remain eligible unless account "ZDR only" is on. CSAM screening of all traffic incl. ZDR from 2026-10-27 (flagged content may be retained). Google Vertex: Google requires Safety Retention for Google models from 2026-10-18, flagged prompts up to 90 days. Unverified endpoints are treated as retaining. In-memory caching is not "retention". |
| `provider.only` / `ignore` / `order` + `allow_fallbacks:false` | per request | Hard allow/deny list by provider slug | If `allow_fallbacks` is true (default) OR `only` lists several, OpenRouter picks among them. Slugs must be maintained by us. |
| `provider.require_parameters` | per request | Only endpoints supporting every parameter | Nothing about privacy. |
| Account: Private Input & Output Logging | Observability settings | Off by default; OpenRouter does not store prompts/responses unless you opt in (D) | Metadata (tokens, latency) always stored. OpenRouter may sample prompts for anonymous categorisation if you have NOT opted into training use (D). Training-use opt-in gives 1% discount: keep OFF. |
| Per-endpoint policy | model page "Providers" | Shows each provider's retention/training claims | Claims, not audited. |

**Recommended expression of "only providers that do not retain or train"** for every advisor request:
```json
"provider": {"zdr": true, "data_collection": "deny", "require_parameters": true, "allow_fallbacks": true}
```
plus account-level "ZDR only" for ALL model groups (including "Safety Retention" excluded) set once by the human. Treat the combination as best-effort: failure mode is "no endpoint available" (HTTP error), which the harness must treat as fail-closed (do not retry without privacy flags). Enforce in code: the `OpenRouterProvider` constructor refuses to build a request lacking these flags, and the harness verifies from the response/generation record (`provider_name`, `data_region`) and logs it. Tell the human plainly: open-weight models on OpenRouter are served by third-party hosts; ZDR shrinks the eligible host set (some models will become unavailable, per docs) and we cannot verify host claims. For code that is actually sensitive (e.g. the canary-backend HIPAA repo per user memory), advisor use should default OFF; an explicit per-repo `advisor_policy: allow|deny` flag. Note the existing memory entry: no third-party indexing of canary-backend without explicit authorization; sending a diff to an advisor is the same class of disclosure.

Also: advisor input is a text bundle (spec + diff). Add a pre-send secret scrub (deterministic regex/entropy + `.env`/key-file path exclusion) and record a hash of exactly what was sent.

## 3. Candidate models (catalog fetched today, P; prices per 1M tokens as listed; context as listed)

Caveats: several listed prompt prices are odd (e.g. `kimi-k3` $0.192 in / $16 out; `glm-5.3` $0.0699 in) and the catalog has variants (`:batch`, dated suffixes, `~...-latest` aliases that float). Prices can be per-provider/time-of-day (minimax-m3.1-flash has UTC time-based overrides; grok-4.7 doubles above 200k prompt tokens). I did not verify per-endpoint prices; "listed" = catalog top-level. Model quality for review is UNVERIFIED by me: no benchmark was run; I have no graded evidence on these specific models. Pin versioned ids, not `~latest` aliases.

(a) Open-weight (catalog shows a `hugging_face_id`):
| ID | ctx | $/M in | $/M out | notes |
|---|---|---|---|---|
| `moonshotai/kimi-k3.1-pro` | 1,048,576 | 2.00 | 10.00 | cache read 0.20; SO+RF |
| `deepseek/deepseek-v4-pro-0813` | 1,048,576 | 0.111 | 4.44 | cheapest strong-sounding; cache read 0.097 |
| `z-ai/glm-5.3` | 1,048,576 | 0.0699 | 1.40 | very cheap |
| `qwen/qwen3.8-2.4t-a95b` | 1,048,576 | 2.00 | 6.00 | Qwen max-class open (HF id present); `qwen3.8-max-0902` same price but no HF id (closed?) |
| `mistralai/mistral-large-4-0` | 1,048,576 | 0.70 | 2.10 | non-Chinese-lab diversity |
| `nvidia/nemotron-3-ultra-550b-a55b` | 262,144 | 0.60 | 2.40 | non-Chinese-lab diversity |
| `xiaomi/mimo-v2.6-pro` | 1,050,000 | 0.435 | 0.87 | |
| `moonshotai/kimi-k2.7-code` | 262,144 | 0.60 | 3.50 | code-tuned name; older |
(b) Frontier non-Anthropic/OpenAI: `google/gemini-4-argon` (1.05M ctx, $2.00 in / $10.00 out, cache read 0.10), `x-ai/grok-4.7` (500k ctx, $2/$6; $4/$12 above 200k prompt tokens), `google/gemini-3.8-flash` ($0.825/$4.125, 65k max out).

**Cost of one review: 60k in + 4k out** (visible output only, formula in*0.06 + out*0.004 USD, no cache):
| Model | per review | reviews per $200 | with 16k out (reasoning-heavy) | per $200 |
|---|---|---|---|---|
| glm-5.3 | $0.010 | ~20,400 | $0.027 | ~7,500 |
| deepseek-v4-pro-0813 | $0.024 | ~8,200 | $0.078 | ~2,600 |
| mimo-v2.6-pro | $0.030 | ~6,800 | $0.040 | ~5,000 |
| nemotron-3-ultra | $0.046 | ~4,400 | $0.074 | ~2,700 |
| mistral-large-4-0 | $0.050 | ~4,000 | $0.076 | ~2,600 |
| gemini-3.8-flash | $0.066 | ~3,000 | $0.115 | ~1,700 |
| grok-4.7 | $0.144 | ~1,400 | $0.216 | ~900 |
| qwen3.8-2.4t-a95b | $0.144 | ~1,400 | $0.216 | ~900 |
| kimi-k3.1-pro | $0.160 | ~1,250 | $0.280 | ~710 |
| gemini-4-argon | $0.160 | ~1,250 | $0.280 | ~710 |
Reasoning models bill hidden reasoning tokens as output; the 16k column is the realistic planning figure. Budget a 3-advisor counsel from {deepseek-v4-pro, kimi-k3.1-pro, gemini-4-argon} at ~$0.26-0.44 per review => ~450-770 counsel rounds on $200. Even the most expensive full panel is cheap relative to the implementation stage; the binding constraint is quality and privacy, not money.

Recommended starting panel (hypothesis, to be validated by the eval in section 7): one cheap open-weight (`deepseek-v4-pro-0813`), one frontier non-US-lab-diverse (`gemini-4-argon` or `grok-4.7`), one more open-weight from a different lab (`kimi-k3.1-pro` or `mistral-large-4-0`). Routing tables must hold model ids as config, never hard-coded.

## 4. Jev / OpenRouter Decisions API

CONFIRMED (D: https://openrouter.ai/docs/api/api-reference/alphadecisions/submit-a-decisions-request.md and https://openrouter.ai/skills/openrouter-decisions):
- `POST https://openrouter.ai/api/alpha/decisions` (path is outside `/api/v1`), Bearer auth. **Stability: alpha; docs state the shape may change.** (The skill page does not mention alpha; the API reference does. Treat as alpha.)
- Request: required `model` (e.g. `typesafe/jev-1.13`), `state` (string/object/array), `questions` (map name -> `{type: noul|choice|score|box, instructions, criteria}`); optional `assets`, `provider` (order/only/ignore/allow_fallbacks/zdr/sort...), `session_id`, `trace`, `user`.
- Response: `{id, model (versioned, e.g. typesafe/jev-1.13-20260917), provider, answers{name: {type, noul|choice|score, confidence, probabilities, legend}}, usage{input_tokens, output_tokens, cost, cost_details, is_byok}}`; an answer may be `type:"refusal"`.
- Price (P, catalog `output_modalities=decisions`): `typesafe/jev-1.13` and `typesafe/jev-1.14`: prompt $0.000000042/token = **$0.042 per M input tokens**, completion $0. Context 262,144. `~typesafe/jev-latest` is a floating alias: do not use. Docs example call cost $0.00002 for 476 in tokens (D). `box` needs Jev 1.14 (D, skill page). Many other decision models exist on the same endpoint (liquid, perplexity pplx-decider, cloudflare clef, microsoft, openai gpt-6-luna-decisions, etc.), so the interface should be vendor-neutral.
- The skill page says: pin versioned id/`canonical_slug`, log the response `model` with the answer; "the model judges, code computes."
NOT confirmed: any TypeSafe product page, accuracy claims, retention/training policy for TypeSafe's endpoint (use `provider.zdr:true` and check the endpoint listing), or whether it is any good at routing coding tasks. It is not a chat or coding model (D). Fit for our use is a hypothesis to test in shadow mode.

## 5. Evidence on multi-model review / counsel (graded honestly)

I read only abstracts/search summaries; no primary-paper results sections. All are preprints. Grade S throughout; none is decisive.
- SWR-Bench (arXiv 2509.01494; abstract read, D-grade for the abstract only): aggregating multiple generated reviews raised F1 by up to 43.67% on 1,000 PRs; abstract does not say whether the gain is from same-model reruns or cross-model, and flags precision as the open problem. Search snippet claims the F1 gain held for both multi-run and multi-model aggregation (S).
- arXiv 2606.15689 (abstract read): 5 models, 150 samples; small Haiku 4.5 beat Sonnet 4.6 (F1 0.365 vs 0.343); F1 collapses on real PRs (0.066) vs synthetic injected bugs (0.847) and on larger diffs (0.657 under 10 lines -> 0.043 over 150 lines). A search-summary claim that "union ensembles hurt F1" is NOT in the abstract I read: UNVERIFIED. The size effect is directly relevant: a 2,000-line diff is far past the regime where reviews are reliable, so chunk the review by file or concern.
- Codex-Verify / information-theory multi-agent verification (arXiv 2511.16708): reports low inter-agent correlation (rho 0.05-0.25) and 76.1% catch rate on 99 labelled samples (S, small benchmark, abstract-level).
- DVDR-LLM (arXiv 2512.12536, vulnerabilities): +10-12% detection vs average single model; raising agreement threshold cuts false positives but misses more true positives (S).
- Refute-or-Promote (arXiv 2604.19049): proposes adversarial stage-gated review with a cross-model critic to raise precision; I saw only the title/abstract snippet (S).
- Correlated errors and echo chambers in debate/consensus (Estornell & Liu 2024; "popularity trap" consensus selection discarding minority-correct answers; MAD amplifying bias in LLM-as-judge, arXiv 2510.12697 / 2410.04663) (S). Council Mode (arXiv 2604.02923) is a technical report advocating heterogeneous models; limited peer review (S).
- Self-preference, verbosity and position bias in LLM judges are well documented (S).

**What the evidence supports (conservatively):** (1) more independent passes and more diverse models raise recall; (2) every extra reviewer also adds false positives, so a verification/agreement step is mandatory; (3) same-family or consensus-voting setups share errors and can suppress the lone correct finding, so do NOT use majority vote to drop findings; (4) review quality degrades sharply with diff size; (5) there is no solid head-to-head study of cross-family counsel versus one strong reviewer on real PRs. So the design should treat counsel as a recall-extender whose output is filtered by verification, and the harness should measure it (section 7) instead of assuming it helps.

Known failure modes to design against: correlated blind spots (diversify families and prompts, also vary "lens": security, concurrency, API contract, tests-missing); false-positive inflation (verification gate, cap findings per advisor, severity floor); verbosity (hard `maxItems`, `maxLength` in schema, token cap); hallucinated file/line refs (harness checks `file` exists in the diff and line range falls inside the supplied text; drop or flag otherwise); prompt injection from the diff into the advisor (advisor has no tools, so blast radius is a poisoned finding only; still mark advisor output untrusted).

## 6. Design

### 6.1 `ModelProvider` trait (small surface)
```rust
pub trait ModelProvider {
    fn id(&self) -> &str;                       // "openrouter"
    fn complete(&self, req: AdvisorRequest, ctx: &CallContext) -> Result<AdvisorResponse, ProviderError>;
    // blocking or async; one method. No streaming, no tools, no files.
}
pub struct AdvisorRequest {
    pub model: String,                 // versioned id from routing table
    pub system: String,
    pub user: String,                  // spec + diff bundle (already scrubbed)
    pub json_schema: serde_json::Value,// the findings schema
    pub max_output_tokens: u32,
    pub privacy: PrivacyPolicy,        // zdr, data_collection, allowed/denied providers
    pub est_cost_ceiling_usd: f64,     // harness-computed worst case
    pub idempotency_key: String,       // run_id + stage + advisor_slot
}
pub struct AdvisorResponse {
    pub text: String,                  // raw model output (stored)
    pub parsed: Option<serde_json::Value>,  // schema-valid or None
    pub usage: Usage { prompt_tokens, completion_tokens, reasoning_tokens, cached_tokens, cost_usd: Option<f64> },
    pub served_model: String, pub served_provider: Option<String>, pub generation_id: Option<String>,
    pub finish_reason: String, pub latency_ms: u64,
}
```
`OpenRouterProvider` builds exactly the JSON of section 1/2 with privacy flags non-optional. The trait deliberately has no `tools` field, so an advisor cannot be granted tools without changing the trait (reviewable). A separate `DecisionProvider` trait (section 7) is NOT folded into this one: different request and response shape.

### 6.2 Findings schema (superset of the repo README reviewer schema)
README (P: repo README line 105): `verdict`, `summary`, `findings[]` with `severity`, `file`, `line_start`, `line_end`, `confidence`, `recommendation`, `next_steps`.
```json
{
  "verdict": "approve|changes_requested|insufficient_context",
  "summary": "string, maxLength 800",
  "findings": [{          // maxItems 15
    "id": "string",                       // advisor-local
    "severity": "critical|high|medium|low|info",
    "category": "correctness|security|concurrency|api_contract|test_gap|spec_mismatch|other",
    "file": "string", "line_start": 1, "line_end": 1,
    "claim": "string, maxLength 400",     // one falsifiable sentence
    "failure_scenario": "string, maxLength 600",
    "confidence": 0.0,                     // number 0..1 (README leaves the type open; use number)
    "recommendation": "string, maxLength 400",
    "suggested_check": "string|null"       // a command/test the HARNESS could run to verify
  }],
  "next_steps": ["string"]
}
```
Additive fields (`claim`, `failure_scenario`, `category`, `suggested_check`) mean existing consumers still parse. Harness-added fields after receipt (never from the model): `advisor_slot`, `model_served`, `status: unverified|corroborated|verified|refuted|human_dismissed`, `evidence_ref`.

### 6.3 `Counsel` procedure
1. Build one input bundle (spec packet + diff chunks + required-checks results). Chunk by file group so no bundle exceeds the size where quality collapses (section 5); target well under 60k tokens for each chunk. Scrub secrets, hash the bundle.
2. Fan out to N advisors (distinct families), same bundle, same schema, independent (no advisor sees another's output). Different lens prompt per advisor allowed and recorded.
3. Validate each response against the schema locally; invalid => `AdvisorFailed`, no repair-by-retry beyond 1 attempt.
4. Deterministic merge: normalise `file`; cluster findings where same file and line ranges overlap (or within a tolerance of 5 lines) AND same `category`; claim text similarity is NOT used to decide identity at this stage (avoid a model in the merge). Each cluster records `supporters: [advisor_slot...]`, max severity, per-advisor confidence, and the original claims verbatim.
5. Output three lists: `corroborated` (>=2 advisors), `single_source`, and `disagreements` (same cluster, severity differing by >=2 levels, or one advisor's verdict `approve` while another has a critical finding in the cluster, or contradictory recommendations). Disagreements are surfaced, never averaged; no vote, no majority-based dropping. Single-source findings are NOT discarded.
6. Verification gate (all findings are claims): for each finding the harness tries, in order, (a) mechanical grounding: file and lines exist in the diff; (b) `suggested_check` run by the harness in the sandbox only if it matches an allowlisted command form (otherwise human); (c) else routes to the human or to the repair loop as "reviewer asserts X" with its status `unverified`. Only `verified` (harness check reproduced it) or human-confirmed findings may block a merge; `unverified` findings may be shown, may be sent to the implementer as hints, and can force a human stop, but cannot by themselves count as evidence.
7. Persist everything (inputs hash, raw outputs, merged result) as artifacts; emit events.

Counsel on spec/design critique uses the same machinery with a different schema (`concerns[]` with `section`, `claim`, `severity`).

## 7. Routing

### 7.1 Fixed per-stage table (config, TOML shown)
```toml
[stage.research]   runtime = "perplexity-api" model = "..."           fallback = "none"            max_cost_usd = 1.00
[stage.spec]       runtime = "claude-code"    model = "default"       fallback = "codex"           max_cost_usd = 0   # subscription: no $ cap, use turn/time cap
[stage.implement]  runtime = "claude-code"    model = "default"       fallback = "codex"           max_turns = 60  max_wall_s = 3600
[stage.review]     runtime = "codex"          model = "default"       fallback = "cursor"          max_cost_usd = 0
[stage.counsel]    runtime = "openrouter"     panel = ["deepseek/deepseek-v4-pro-0813","google/gemini-4-argon","moonshotai/kimi-k3.1-pro"]
                   fallback = "skip-and-record" max_cost_usd = 1.00     privacy = "zdr"
[stage.repair]     runtime = "claude-code"    max_rounds = 3
[stage.triage]     runtime = "claude-code"    read_only = true
```
Keys: stage, runtime (cli-subscription | openrouter | api), model/panel (versioned ids), fallback (ordered list or `skip-and-record`/`stop`), max_cost_usd (API paths) or max_turns/max_wall_s (subscription paths). Fallback to a model with weaker privacy than the primary is forbidden; privacy policy is stage-level, not model-level. Subscription paths have no reliable dollar figure; track wall time, turns, and whatever usage the CLI reports (grade for that is another track's business).

### 7.2 What to log per run (per stage attempt) so routing can be evaluated as cost per verified task
`run_id, task_id, stage, attempt, route_chosen (table row id + version hash), decider ("table"|"jev-shadow"|"human"), runtime, model_requested, model_served, provider_served, input_bytes/tokens, output_tokens, reasoning_tokens, cached_tokens, cost_usd (nullable for subscription), latency_ms, wall_s, outcome (ok|schema_invalid|error|budget_stop|timeout), findings_count, findings_by_status (verified/refuted/unverified/dismissed), human_verdict, repair_rounds, final_acceptance (harness checks passed? merged?), diff_size_lines, repo, language`. Derived metrics: cost per verified finding, advisor precision (verified/total), unique-find rate (verified findings found by only one advisor), cost per accepted task, rounds to green. Needs the shared core to store these as events in the SQLite ledger.

### 7.3 Where Jev plugs in (shadow only)
`trait RouteAdvisor { fn recommend(&self, state: &RoutingState, options: &[RouteOption]) -> Result<Recommendation> }` with `TableRouter` (authoritative) and `JevShadow`. At stage entry the harness builds a small `state` (task title, stage, diff size, languages, touched paths classes, prior failures; NOT code) and asks one `choice` question ("which route?" with `criteria` = one description per route row) plus optionally `score` for difficulty. Record the answer, probabilities, versioned `model`, and cost in the ledger as `decider: jev-shadow`, never used to choose. Later compare against outcomes to see whether it would have reduced cost per verified task. Cost is negligible (~$0.00002 per call, D example). Caveats: alpha API, UNVERIFIED accuracy for this use, and the `state` leaves the machine so apply `zdr` and a no-code rule. If the endpoint errors or is absent, shadow call is skipped silently with a logged note; it can never block a stage.

## 8. Budget enforcement for API-key paths

Rule: the harness, not the provider, owns the cap. A `BudgetGuard` per task (and per run) in the SQLite ledger:
1. Before each request: `spent_usd (sum of recorded costs) + reserved_usd (in-flight) + worst_case(req) <= task_cap`, where `worst_case = prompt_tokens_est * in_price + max_output_tokens * out_price` using catalog prices (refresh at run start; take the max of listed price and any override, e.g. grok >200k tier). `max_tokens` MUST be set on every request; reasoning tokens count against output, so set it to cover reasoning. Refuse (record `budget_stop`) otherwise. Write the reservation row BEFORE sending ("record intent, then act").
2. After the response: replace reservation with actual `usage.cost`. If `cost` absent, compute from tokens and prices, and later reconcile via `/generation?id=` (retry for lag).
3. Failed/timed-out/cancelled requests: still reserved until reconciled via generation record; a dropped connection may still be billed (UNVERIFIED how OpenRouter bills cancelled non-streaming calls; the generation record has a `cancelled` field, D). Keep reservation until resolved or the human clears it.
4. Defence in depth: create a dedicated OpenRouter API key with a **per-key credit limit** (D: optional per-key credit cap; `GET /api/v1/key` reports `limit_remaining`) equal to a sensible total; the human sets it in the dashboard. That bounds loss even if the harness has a bug. Also OpenRouter's in-flight budget gives a platform-side brake (D).
5. Possible overshoot: bounded by sum of worst-case reservations of concurrent requests (we reserve worst-case, so overshoot is ~0 unless catalog prices are stale, a provider is routed at a higher price than the catalog listed, or the provider reports more tokens than `max_tokens` allows), plus unresolved cancelled calls. Use `provider.max_price` (D) to pin per-request price ceilings to the catalog price so a pricier fallback endpoint cannot be used. Counsel fan-out reserves the sum for all N before launching any.
6. Report spend in events so a client UI can show it; on cap hit: stop the stage, mark the task `Blocked(budget)`, wait for the human.

## 9. What this track needs from the shared core
- `Event`/ledger types: `StageAttempt`, `ModelCall{request_hash, response_hash, usage, cost, served_model, served_provider}`, `Finding{status, supporters}`, `BudgetReservation`, `RouteDecision{decider, route, shadow_alternatives}`.
- A `Artifact` store keyed by hash (inputs bundle, raw advisor outputs, merged counsel result).
- A `Verifier` interface that runs allowlisted checks in the sandbox and returns pass/fail evidence (counsel's `suggested_check` plugs into it).
- A `Config` loader for the routing table with versioning and hash, and a secrets interface that gives the OpenRouter key to exactly one component (provider), never to agents or sandboxes.
- The findings JSON schema as a shared, versioned type, reused by Codex/Cursor reviewer adapters.

## 10. Open questions for the human
1. Which repos are allowed to send code to third-party-hosted advisors at all (canary-backend is HIPAA-eligible and private; default deny?).
2. Accept "ZDR + data_collection deny" as sufficient, given it is provider-declared, plus account ZDR-only? Or restrict advisors to spec/design text and sanitized diffs only?
3. Do you want a dollar cap per task for counsel (suggest $1) and a per-key limit on the OpenRouter key (suggest $50 of the $200 until the eval shows value)?
4. Will you label verified-vs-false findings for the first ~30 reviews so we get real precision numbers per advisor? Without that, panel choice is a guess.
5. Is TypeSafe Jev worth even shadow mode given alpha status and unknown retention terms?
