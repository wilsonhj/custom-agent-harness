# Track 2: Research stage

Date: 2026-11-09. Grades: (P) probed locally, (D) official docs read today (URL), (S) secondary, UNVERIFIED.
Pages that failed: intercom.help Perplexity billing article (404), hub-prod.perplexity.ai FAQ (404), docs.tavily.com trust page (title only), Tavily/Brave retention (no statement on page), Brave pricing doc page (no numbers). No model calls or credentialed API calls were made.

## 1. Perplexity programmatic surfaces

All on host `api.perplexity.ai`, one API key, `Authorization: Bearer $PERPLEXITY_API_KEY` (D: docs.perplexity.ai/docs/getting-started/overview, /docs/search/quickstart.md, /docs/agent-api/quickstart).

| Surface | Endpoint | Request | Response / citations | Price (D, /docs/getting-started/pricing.md) |
|---|---|---|---|---|
| Search API | `POST /search` | `query` (string or up to 5-array), `max_results` 1-20, `search_domain_filter` (<=20, allow OR deny with `-`), `search_language_filter`, `country`, `max_tokens`, `max_tokens_per_page`, `search_type:"fast"` | `results[]` each `title,url,snippet,date,last_updated`; no generated answer. Citations ARE the results; snippet is page text | $5/1k requests; fast $1/1k; per request, not per query; failures unbilled; no tokens |
| Agent API | `POST /v1/agent` (also `/v1/responses` OpenAI-compatible) | `input`, `effort` (fast/low/medium/high/xhigh), optional `model` (provider-prefixed), `instructions`, `tools:[{type:"web_search"}, fetch_url...]`, `max_steps` | `output[]` typed items: `search_results` (id,title,url,snippet,date,last_updated,source), `fetch_url_results`, `message` (output_text). Answer text has inline `[web:N]`/`[N]` markers keyed to `search_results[].id`; `annotations` empty in examples. `usage.cost` reports per-request cost. web_search is opt-in; without it no citations | tokens per model (e.g. `perplexity/glm-5.3-flash` $0.15 in/$0.50 out per 1M; `anthropic/claude-haiku-5-5` $0.10/$0.50) plus tools: web_search $0.0025 (fast $0.001), fetch_url $0.0005, sandbox $0.03/session |
| Sonar chat | `POST /v1/sonar` (labelled "legacy"; docs recommend migrating) | `messages`, model name | Old shape had top-level `citations` and `search_results`; migration page says these are replaced by typed `output` items in the Agent API (D: /api-reference/sonar-post.md). Direct Sonar token pricing: not found on a page that loaded (UNVERIFIED). Agent models page lists `perplexity/sonar` $0.25 in / $2.50 out (D: /docs/agent-api/models.md) | see left |
| `pplx` CLI | official, `perplexityai/perplexity-cli`, install via `curl .../install.sh \| sh` | `pplx search web "q" -n N` with domain/date/recency/country filters; `pplx content snippets "q" url...` (<=50 URLs, query-relevant passages); `content fetch` deprecated | JSON only; hits have `url,title,domain,snippet,summary,date,last_updated,trust{level,name}`; errors as JSON on stderr; snippet failures per URL with exit 0 (D: /docs/cli/overview.md) | wraps Search API; needs API key (`PERPLEXITY_API_KEY` or `pplx auth login`). Not installed here (P: `which pplx` empty) |

Other surfaces seen: Decisions API, Orchestrator API (private preview), Embeddings API, an MCP server page (D: overview, llms.txt). Not needed.

Rate limits (D: /docs/getting-started/rate-limits.md): Search API 50 query units/s, burst 50, not tiered; a 5-query request uses 5 units. Agent API tiered by lifetime credits purchased: Tier 0 ($0) 1 QPS, Tier 1 ($50+) 3 QPS, Tier 2 ($250+) 8 QPS. Plenty for one solo user.

Retention/training (D: /docs/resources/privacy-security.md): API ToS says Customer Content is not used to train generative models; retention "varies by API, model, and account configuration"; some third-party model providers require limited retention; ZDR is not default, only for eligible customers by written agreement and restricts Agent API to ZDR-capable models. Treat default retention as unknown but non-zero.

Consumer subscription vs API: no evidence it includes API credit today. Perplexity's own help/FAQ pages 404'd. Secondary sources (S: reddit snapshot 2026-02-21, goosed.ie, a June 2026 pricing guide) say the $5/month Pro API credit was discontinued around 2026-02-12; other secondary sites still list it (stale). Rate limit tiers are based on credits purchased, so the API is a separate prepaid balance. Recommendation: assume API is billed separately; the human can check the API console balance once (cannot be probed by me).

Cheapest sane research call: Search API fast ($0.001) or standard ($0.005) for sources, then the harness (or a cheap model) does synthesis. A full Agent API research answer at low effort costs roughly one web_search + a few fetch_url + a cheap model, i.e. cents. Cost per research run is not a concern at solo scale.

## 2. Perplexity via OpenRouter

- Available (D: openrouter.ai/perplexity): sonar, sonar-pro, sonar-pro-search, sonar-reasoning(-pro), sonar-deep-research, plus embed/decider models. `perplexity/sonar` is $1/M in and $1/M out on OpenRouter (D: openrouter.ai/perplexity/sonar), versus $0.25/$2.50 on Perplexity's Agent models page (D); the two disagree and may be different SKUs, so treat OpenRouter cost per run as UNVERIFIED until a real response shows `usage.cost`.
- Citations survive in OpenRouter form: web results are standardized to `message.annotations[]` of `type:"url_citation"` with `url,title,content?,start_index,end_index` (D: openrouter.ai/docs/features/web-search and /docs/guides/features/plugins/web-search). Native engine is default for Perplexity models; `engine:"perplexity"` uses Perplexity's Search API at $0.005/request.
- Lost vs direct: the `search_results` array with `date/last_updated/source`, the `[web:N]` id-to-result mapping, direct `search_domain_filter` with Perplexity semantics (OpenRouter offers `include_domains`/`exclude_domains`, mutually exclusive for Perplexity), `fetch_url`, `max_tokens_per_page`, and `pplx` CLI. Whether sonar via OpenRouter still returns a raw `citations` array: not covered by docs (UNVERIFIED).
- Bigger OpenRouter lever: `:online` / `plugins:[{id:"web"}]` gives ANY OpenRouter model web search with engines native/exa/firecrawl/parallel/perplexity; Exa $0.007/request, Parallel $0.001-0.005, Firecrawl $0.005 + $0.0125/result (D). So the $200 credit can fund research with open-weight models + a search engine, with url_citation annotations in one shape.
- OpenRouter privacy (D: /docs/guides/privacy/data-collection): prompts/responses not stored unless opted in (logging off by default; product-improvement use off by default); metadata (tokens, latency) is stored; CSAM screening in flight. Upstream provider (Perplexity) policies still apply and per-provider handling was not documented on that page.

## 3. Alternatives behind one interface (one line each)

- Tavily: `POST https://api.tavily.com/search`, bearer key; basic/fast/ultra-fast 1 credit, advanced 2; `include_raw_content`, `include_domains` <=300; free 1,000 credits/mo, PAYG $0.008/credit (D: docs.tavily.com, tavily.com/pricing). Citation quality: URL + `content` snippet + `score`, optional raw page content, which is what the verifier wants. Retention: no statement found on any loaded page (UNVERIFIED; trust center did not render).
- Exa: neural search; Search from $4/1k (Lite/Instant) or $7/1k (Auto/Fast); contents $1/1k pages; Answer $5/1k; $10 free credit/mo (D: exa.ai/pricing). Citations: URL + highlights/text. ZDR is Enterprise only.
- Brave Search API: Search $5/1k requests with $5 free monthly credit; Answers $4/1k + tokens; LLM Context endpoint (1k-33k tokens grounding); ZDR "architectural", full-funnel on Enterprise; storing results or training on them only under a plan with storage rights (D: brave.com/search/api; per-plan storage terms UNVERIFIED). Independent index, plain results, no model in loop (cheap, low prompt-injection surface). Reports of a free tier removal in Feb 2026 are S only.
- Claude Code built-in: WebSearch (Anthropic backend, titles+URLs only, up to 8 backend searches per call, domain allow/block, 200 calls per interactive session, backend not configurable) and WebFetch (lossy: a small model summarizes the page; 15 min cache; 100k char cap) (D: code.claude.com/docs/en/tools-reference). Zero extra cost on the subscription, citation quality is URLs only and WebFetch output is model-processed, so quotes from it are NOT verbatim evidence. Disable with `--disallowedTools WebSearch,WebFetch` (flag exists, P: `claude --help`) or `CLAUDE_CODE_DISABLE_WEB_FETCH=1` (D).
- Codex built-in: `--search` flag exists (P: `codex exec --help` line "Enable live web search"); modes cached (default, OpenAI-maintained index), live, disabled; searches appear as `web_search` items in `codex exec --json`; hosted tool outside the sandbox network allowlist; docs say treat results as untrusted (D: learn.chatgpt.com/docs/web-search). Subscription-funded; citations not documented as structured.

Verdict: for the harness, the useful property is structured source lists with URL + snippet that the harness can re-fetch. Search APIs (Perplexity Search, Tavily, Exa, Brave) give that; built-in agent search gives prose with URLs.

## 4. Design

### ResearchProvider (Rust, small surface)

```rust
trait ResearchProvider {
    fn id(&self) -> &str;                       // "perplexity-search", "tavily", "claude-cli", ...
    fn egress(&self) -> EgressClass;            // what leaves the machine, checked by policy before call
    async fn research(&self, req: ResearchRequest) -> Result<RawFindings, ProviderError>;
}
struct ResearchRequest { question: String, queries: Vec<String>, domains_allow: Vec<String>,
    domains_deny: Vec<String>, recency: Option<Recency>, max_sources: u8, budget_usd: f32 }
struct RawFindings { provider: String, cost_usd: Option<f32>, sources: Vec<Source>, draft_claims: Vec<DraftClaim> }
struct Source { url: String, title: Option<String>, snippet: Option<String>, published: Option<String> }
struct DraftClaim { text: String, source_urls: Vec<String>, quoted_span: Option<String> }
```
Two provider classes share it: (a) retrieval providers (Perplexity Search, Tavily, Exa, Brave) return `sources` only, and the harness asks a cheap advisor model (OpenRouter, text-in) to draft claims over the snippets; (b) answer providers (Perplexity Agent API, `claude -p` with WebSearch, `codex exec --search`) return prose+URLs, which a parser maps to `draft_claims`. Everything downstream consumes `RawFindings`, so vendors swap freely. Agent CLI providers run as child processes under the same adapter machinery as implementers, with the web tools allowed and file-write tools denied.

### Evidence file (`research/<id>/evidence.json`, plus rendered `evidence.md`)

```json
{
  "schema": "evidence/1", "question": "...", "created_at": "RFC3339",
  "provider": "perplexity-search", "query_log": ["..."], "cost_usd": 0.01,
  "egress": {"class": "public-question", "text_sent_sha256": "..."},
  "claims": [{
    "id": "C1", "text": "...",
    "sources": [{
      "url": "https://...", "retrieved_at": "RFC3339",
      "quoted_span": "verbatim text from the page",
      "page_sha256": "...",
      "verified_by_harness": true,
      "verification": {"fetched_at": "...", "http_status": 200, "span_found": true, "match": "exact|normalized"}
    }],
    "status": "verified|unverified|contradicted"
  }],
  "open_questions": ["..."]
}
```
A claim is `verified` only if at least one source has `verified_by_harness: true`. Every other claim is carried through but visibly marked `unverified`.

### Verification step (harness-owned, no model)

1. For each (url, quoted_span): re-fetch with a plain HTTP client (own User-Agent, timeout, size cap, follow redirects within same registrable domain, https only, no cookies). Same code for every provider.
2. Extract text (HTML to text, strip scripts/nav; PDF text if content-type pdf), normalize whitespace, unicode NFKC, quotes/dashes, case-fold.
3. Check the span as a substring after the same normalization; require minimum length (e.g. >= 40 chars) so "the" cannot verify. Record exact vs normalized match and `page_sha256`.
4. Fail closed: 403/JS-only page/paywall/timeout gives `verified_by_harness:false` with reason, never silently true. Optional fallback: fetch via Perplexity `pplx content snippets` or a Jina-style reader, still checked by the same substring test.
5. Limits what this proves: that the quote exists at that URL now, not that it supports the claim. Claim-to-span entailment stays a judgment; keep the span short and let the Spec stage / reviewer read it. Fetched page text is untrusted input (prompt injection): the verifier never passes page text to a model that has tools.
Cost: one HTTP GET per source, effectively free.

## 5. Feeding the spec packet

`specs/_template/requirements.md` (P: read) has `## Cited company decisions` with columns `Constraint | Source (ADR / policy / brain doc ID @ revision)`. That table is for INTERNAL decisions. Do not pour web research into it. Proposal:
- Add a sibling section `## External research` with columns `Claim | Source URL | Quoted span | Retrieved | Verified by harness`, generated mechanically from `evidence.json` claims with status `verified`. Unverified claims go to `## Unresolved questions` as `- [ ] Verify: <claim> — owner: human, blocks: <task>`.
- The Spec stage input is `research/<id>/evidence.md` plus the JSON path; the spec agent may cite a claim as `[R:C3]`; a harness lint checks that every `[R:Cn]` resolves to a verified claim, mirroring how the repo already checks that citations resolve (existing handoff notes record "cited a source that does not state it" as a failure class, P: `specs/adapter-interface/handoff.md`).
- Constraints: a spec requirement that depends only on an unverified claim is a lint failure.

## 6. Privacy

What leaves the machine in a research query: the question text, any queries the model derives, the domain filters, the API key identity, and for CLI-agent providers the full agent prompt and anything the agent chooses to put in a search string. Not the repo, unless someone pastes it. Search providers log queries (retention unknown/provider-specific, see above).

Rule: every project has `research_egress = none | public-questions | project-context` in harness config, default `public-questions`.
- `none`: Research stage refuses to run any network provider; user supplies an evidence file by hand (current Perplexity workflow remains valid) and the harness only runs the verifier on it. Verifier fetches still hit the open web, so for `none` it runs only if the human sets `verify_fetch = true`, since a URL fetch leaks the URL, not code.
- `public-questions`: question text must be authored or approved by the human (shown before sending), generated from the task title not from code; the harness runs a deny-scan (repo identifiers, file paths, hostnames, secrets patterns, names of customers, strings found in the repo's source) over outgoing text and blocks on a hit.
- `project-context`: opt-in per project; only providers with ZDR or documented no-training terms.
Built-in CLI search (Claude WebSearch / Codex search) cannot be egress-inspected by the harness because the agent composes the query inside the vendor tool, so those providers are barred under `none` and, for sensitive projects, under `public-questions` unless the agent is given no repo access. Log the exact outgoing text hash into the audit trail ("record intent, then act").

## 7. Defer-or-do: model writes a retrieval program in a sandbox

Defer it. The first version should be one deterministic pipeline: search provider, optional cheap model drafts claims, harness verifier, evidence file. A generated retrieval program adds a sandbox with network egress (which collides with the fail-closed, no-egress sandbox default), makes the egress un-inspectable, and widens prompt-injection exposure, while the gain is only for multi-hop crawling and bulk extraction, which a human-supervised solo workflow rarely needs. Perplexity's Agent API already offers a hosted `sandbox` tool ($0.03/session, D) and Exa/Tavily offer hosted research endpoints (D) if that capability is wanted later, so it can be added as another `ResearchProvider` without changing the evidence schema. Revisit after the verifier's failure rates on real runs are known.

## Recommendation summary

Default provider: Perplexity Search API (cheapest structured sources; same key the user may already hold), behind `ResearchProvider`; alternates Tavily/Exa/Brave; OpenRouter `:online` for open-weight drafting; Claude/Codex built-in search as a no-extra-cost fallback with the egress caveat. The harness verifier, not the provider, decides which claims count.

## Open questions for the human
1. Does the user have a funded Perplexity API balance (consumer Pro/Max does not appear to include one)? Or prefer Tavily/Exa/Brave free credits?
2. Are any projects in `none` egress class (HIPAA-eligible canary work)?
3. Is a hand-pasted Perplexity result an acceptable input (verify-only mode) for those projects?
4. Perplexity ZDR/OpenRouter provider retention for default accounts: confirm with vendor if a sensitive project needs it (not published on loaded pages).
