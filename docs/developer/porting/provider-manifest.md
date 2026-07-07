# Native Core: The Provider Manifest & Stream Decoders

> Status: **agreed design**, to be implemented in Phase 3 (alongside the chat
> orchestration / enclave work). Companion to [`overview.md`](./overview.md) and
> [`api-boundary.md`](./api-boundary.md). Records how v5 replaces v4's npm
> provider plugins, since the npm-plugin mechanism does not survive the port.
> Decided with Charlie, 2026-06-29.

## The problem this solves

v4 reaches every LLM provider through an **npm plugin** — each provider is a
published package (`qtap-plugin-anthropic`, `qtap-plugin-openai`, …) exporting a
class that implements `TextProvider` (`sendMessage` / `streamMessage` /
`validateApiKey` / `getAvailableModels`), registered into a singleton registry
(`lib/plugins/provider-registry.ts`). That mechanism depends on a Node runtime
dynamically importing JavaScript at startup. **v5 is a Rust core in a Tauri
shell — there is no Node, no dynamic `import()`, and shipping arbitrary
third-party JS into the core is exactly the trust boundary we don't want.**

So the npm-plugin path is gone. What replaces it has to keep the *good* property
v4 bought with plugins — "a new provider is a small, self-contained addition" —
without the dynamic-code-loading that made it work.

## The decision

A **two-layer design**:

1. A **declarative JSON manifest per provider**, JSON-Schema-validated at load
   (the same pattern v4 already uses for `.qtap` exports and `.qtap-theme`
   bundles). The manifest carries everything that is *data*: auth shape, base
   URL, request-envelope field names, static metadata, capability flags,
   attachment MIME lists, pricing, fallback model lists. A malformed third-party
   manifest fails loudly at load, never mid-stream.

2. A **fixed, compiled set of Rust components** that the manifest *selects* by
   enum discriminator but never *defines*: the stream decoders and a small set
   of named request-transform hooks. These hold the genuinely stateful logic
   that a declarative language cannot express without becoming a Turing tarpit.

The boundary is the whole point: **if you can express a new provider by writing
JSON that points at an existing decoder + hook combination, you write only
JSON.** You touch Rust only when a provider speaks a wire protocol or has a
request quirk that no existing component covers — which, historically, is rare
(the last genuinely new wire protocol was OpenAI's Responses API).

### Why not JSON-only (the rejected option)

A JSONPath-style response mapping (`content = $.choices[0].delta.content`)
expresses *non-streaming* extraction fine. It cannot express "buffer partial
tool-call JSON across N SSE events keyed by block index until the block closes"
— which is exactly what the real providers do. Encoding that in JSON means
inventing a stream-transducer DSL: you lose Rust's type safety, the differential
harness can no longer oracle it field-by-field, and debugging a misbehaving
provider becomes debugging your interpreter. The stateful 20% stays compiled.

## The five stream decoders

This is the count that matters, and it is **five, not three** — established by
reading every provider in v4's `plugins/dist/*/provider.ts`. The earlier
"three dialects" guess was wrong because it assumed OpenAI and Grok were on Chat
Completions; both actually moved to the **Responses API**, whose streaming event
taxonomy is as distinct from Chat-Completions SSE as Anthropic's is. Ollama is
not SSE at all.

| Decoder enum | v4 providers on it | Wire shape | Stateful work the decoder owns |
|---|---|---|---|
| `chat-completions-sse` | openai-compatible, deepseek, z-ai, openrouter | OpenAI Chat Completions SSE | `choices[0].delta.content`; **tool-call accumulator** keyed by `tool_calls[].index`, concatenating fragmented argument strings; usage in trailing chunk |
| `responses-api-sse` | openai, grok | OpenAI **Responses API** SSE | `response.output_text.delta`; `response.reasoning_summary_text.delta` (cumulative); terminal `response.completed` carrying the full response + usage |
| `anthropic-sse` | anthropic | Anthropic Messages SSE | `content_block_start/delta/stop` state machine; `input_json_delta` partial-JSON buffering per block index; `thinking`/`signature` block accumulation; usage split across `message_start` + `message_delta` |
| `google-parts` | google | genai `generateContentStream` | `candidates[].content.parts` iteration; routing `thought === true` parts to reasoning vs. text; `thoughtSignature` capture from the final chunk |
| `ollama-ndjson` | ollama | **newline-delimited JSON** (not SSE) | manual `reader.read()` + decode loop; `message.content` deltas; tool_calls arrive as objects (not fragmented strings) and are normalized to OpenAI shape |

Every decoder is a **leaf unit** in the port discipline: it gets a tier-1
differential test that feeds a recorded provider stream through both v4's real
parser and the Rust decoder and diffs the normalized `StreamChunk` sequence
field-by-field. (Record real streams once into fixtures; replay deterministically
both sides.)

## The request-transform hooks (the other compiled piece)

Reading the code surfaced a category beyond stream decoding: **request-side logic
that is conditional and stateful, not field-mapping.** These cannot live in the
manifest either. The manifest names which hook a provider uses; the hook is Rust.

- **Anthropic** — `applyMidHistoryBreakpoint` (places a second cache-control
  breakpoint stepped by message count so the active breakpoint never falls
  outside the 20-block lookback) + consecutive-tool-result batching into one
  user message + tool/system/message cache-breakpoint hierarchy.
- **OpenAI** — `previous_response_id` conversation chaining, including the
  send-only-the-last-user-message optimization **and the fallback-to-full-input
  on chaining failure**.
- **Google** — the recursive JSON-Schema **sanitizer** (strips ~20 unsupported
  schema fields, uppercases `type`) + the `thoughtSignature` round-trip that
  Gemini 3 thinking models require on every assistant turn when tools are on.
- **DeepSeek** — the rule that `reasoning_content` from the prior turn **must**
  be echoed back on a tool-call turn or the next request 400s.

Model this as a `RequestTransform` enum the manifest selects, with `none` as the
default for plain OpenAI-compatible endpoints. New providers usually pick `none`.

## What the manifest carries (sketch)

```jsonc
{
  "schemaVersion": 1,
  "id": "anthropic",
  "displayName": "Anthropic",
  "auth": { "kind": "header", "header": "x-api-key", "extra": { "anthropic-version": "2023-06-01" } },
  "baseUrl": "https://api.anthropic.com/v1",
  "endpoints": { "chat": "/messages", "models": "/models" },
  "streamDecoder": "anthropic-sse",       // <- enum into the five
  "requestTransform": "anthropic",         // <- enum into the hooks; "none" for most
  "capabilities": { "tools": true, "webSearch": false, "attachments": true },
  "attachmentMimeTypes": ["image/jpeg", "image/png", "image/webp", "application/pdf", "text/plain"],
  "charsPerToken": 3.5,
  "defaultContextWindow": 200000,
  "fallbackModels": ["claude-opus-4-6", "claude-sonnet-4-6", "..."],
  "pricing": { "claude-sonnet-4-6": { "input": 3.0, "output": 15.0 } }
}
```

The `streamDecoder` and `requestTransform` fields are **discriminators into
compiled code**, never definitions of behavior. Everything else is pure data and
is the part a third party (or a future you) can add without a Rust change.

## A deliberately moving target

The wire protocols here change — OpenAI shipped the Responses API; Gemini 3
changed how thinking is configured; DeepSeek added thinking-mode tool rules.
**This is accepted, not designed around.** `schemaVersion` lets the manifest
format evolve; the decoder/transform enums are closed sets that grow by adding a
compiled variant (with its differential test) when a genuinely new protocol
appears. The manifest absorbs *data* drift for free; *protocol* drift is a
small, well-bounded Rust addition. We are not trying to make the manifest
Turing-complete to chase a target that will keep moving regardless.

A live example landed after this doc was written: v4 `733fa12c`/`36d04ab0`
(2026-06-30) — Sonnet 5 / Opus 4.7+ / Fable / Mythos reject
`temperature`/`top_p`/`top_k` outright, and fixed-budget thinking
(`{type:'enabled', budget_tokens}`) 400s on them; they need adaptive thinking
with `display: 'summarized'` requested explicitly (their default omits the
thinking text Quilltap's `reasoningContent` capture reads). v4 encodes this as
a model-prefix regex list inside the anthropic plugin's `requestTransform`
paths. When porting the anthropic transform, port it from **current** v4
source and consider lifting the affected-model list into manifest *data* (a
per-model capability flag) so the next such change is a data edit.

## Embedding providers — inventoried, and they are easy

Four embedding providers, all **request/response, no streaming** — the whole
streaming-decoder problem evaporates here.

| Provider | Wire shape | Response parsing | Notes |
|---|---|---|---|
| **builtin** (`tfidf-bm25-v1`) | none — **local, in-process** | n/a | `LocalEmbeddingProvider`: TF-IDF + BM25 + Porter stemming + bigrams, fit-on-corpus, serializable state. **Already ported** as Phase-1 pure functions (see the embedding vector-math hot paths in CLAUDE.md Status). |
| **openai** | `POST /embeddings` | `data[0].embedding` (float array) | optional `dimensions`; usage tokens |
| **ollama** | `POST /api/embed` (legacy `/api/embeddings` fallback) | `embeddings[0]` | derives `num_ctx` from `/api/show` (cached, in-flight-deduped); `truncate:true`; **rejects non-finite (NaN/Inf) vectors** — a real correctness guard worth carrying forward verbatim |
| **openrouter** | SDK `embeddings.generate` | `data[0].embedding` | embedding may be **base64-packed Float32** → decode (same byte-format v5 already ports on the read side via `float32_to_blob`) |

Design consequence: embedding providers need **one trivial request/response
shape in the manifest** (auth, endpoint, `input`/`model` field names, where the
vector lives) plus **zero stream decoders**. The only compiled-logic items are
the Ollama `num_ctx` derivation + NaN guard and the OpenRouter base64 decode —
both small, both already have Rust analogues. The local builtin provider is not
a network provider at all and stays as the already-ported pure functions.

## Image providers — inventoried, with two surprises

Five image providers. The headline: **image generation does not reduce to one
"images" dialect.** It splits three ways, and one provider runs two different
wire protocols internally.

| Provider | Endpoint / shape | Image bytes location | Quirks |
|---|---|---|---|
| **openai** | SDK `images.generate` (`/images/generations`) | `data[].b64_json` (DALL·E) **or** `data[].url` (gpt-image-1) | per-model **size whitelists** with normalization to nearest legal size; `quality`/`style` only on DALL·E |
| **grok** | `images.generate` via OpenAI SDK @ x.ai | `data[].b64_json`/`url` | `aspect_ratio` not `size`; `resolution: '2k'` for `-pro` |
| **z-ai** | `images.generate` via OpenAI SDK @ z.ai | `data[].url` (valid **30 days**) or `b64_json` | per-model default sizes; `validateApiKey` is a **no-op stub** (defers to text provider to avoid a paid call) |
| **google** | **TWO APIs in one provider** (see below) | varies by sub-API | model-name dispatch; safety-filter rejection on HTTP 200 |
| **openrouter** | **chat completions** (`/chat/completions`, `modalities:["image","text"]`) | `message.images[].image_url.url` (data URI) **+ 3 fallback shapes** | image-gen is a *chat* call; multi-shape response parser; refusal detection |

**Surprise 1 — there is no single image wire protocol.** Three distinct request
shapes: the OpenAI `/images/generations` family (openai, grok, z-ai), Google's
prediction/generateContent APIs, and **OpenRouter doing image generation through
the *chat completions* endpoint** with `modalities:["image","text"]`. The
response-parsing for OpenRouter has four accepted shapes (documented
`message.images[]`, content-array `image_url`, Gemini `inline_data` passthrough,
plus refusal text). This is closer in spirit to the text-decoder situation than
to embeddings: image providers need **request/response "image dialects," ~3 of
them**, not one mapping.

**Surprise 2 — Google's image provider is two providers wearing one coat.** It
dispatches on model name to *either* the Imagen `:predict` API *or* the Gemini
`:generateContent` API, with completely different request bodies (`instances`/
`parameters` vs `contents`/`generationConfig` + `responseModalities`) and
different response shapes (`predictions[].bytesBase64Encoded` vs
`candidates[].content.parts[].inlineData`). And the **safety filter returns HTTP
200 with an empty `predictions` array** (or a `raiFilteredReason`), which the
plugin deliberately converts into a moderation *error* phrased to match
`isImageModerationError` in the story-background handler so the Lantern can fall
back to an uncensored profile. That cross-component contract (image provider →
error string → story-background fallback) is a non-obvious invariant a naïve
port would silently drop.

Design consequence: image providers want the same manifest+discriminator split
as text, but with their **own decoder enum** (`openai-images` /
`google-imagen-predict` / `google-gemini-image` / `openrouter-chat-image`) and a
shared concern the text side doesn't have: **moderation-rejection normalization**
is part of the contract, not an afterthought — the empty-200 and refusal-text
cases must map to a typed `ImageModerationError` the Lantern can pattern-match.
This is more than "add a row to the manifest" — the contract is written up
separately in
[`lantern-image-moderation-contract.md`](./lantern-image-moderation-contract.md).

## The porting decomposition (W4.7) — scoped 2026-07-06

The design above is settled; this section decomposes the build into six
units, each with its own differential strategy, sized from a survey of the
surrounding v4 machinery (registry 583+377 lines, tool-executor parse 1282,
plugin-tool-builder 560, pricing-fetcher 473, embedding-service 595,
llm-logging 339, errors 411, plus the five provider plugins ~3,500). Work
orders live in [`work-orders/`](./work-orders/); every unit now has one —
a/b/c are DONE, and the remaining three were specced 2026-07-07 from fresh
v4 surveys:
[`w4.7d-transport-errors-api-keys.md`](./work-orders/w4.7d-transport-errors-api-keys.md),
[`w4.7e-pricing-capability-logging-embeddings.md`](./work-orders/w4.7e-pricing-capability-logging-embeddings.md),
[`w4.7f-image-dialects-moderation-search.md`](./work-orders/w4.7f-image-dialects-moderation-search.md).
**Survey corrections to the table below (2026-07-07, authoritative in the
orders):** W4.7e — "builtin already ported" is WRONG (only the
`tfidf_vocabulary` storage repo is; the TF-IDF/BM25 vectorizer is W4.7e
sub-unit 5, splittable); the `api_keys` collection is hand-rolled INSIDE
v4's `ConnectionProfilesRepository` (plaintext `key_value`, no dedicated
repo); v4 has NO transport-tier timeout/abort/retry anywhere (SDK defaults
apply silently). W4.7f — there are FIVE image providers (z-ai was omitted),
and the refusal-keyword gap covers openrouter AND google-gemini AND z-ai
(all faithful, never "fix").

**One architectural rule for the whole batch: the core stays sans-IO.**
Decoders consume already-received bytes/events and emit `StreamChunk`s;
request builders produce a request *value* (method, url, headers, body);
the actual HTTP lives behind a small `ProviderTransport` trait wired by the
host (Phase 4 / CLI). This is what makes every unit differentially testable
without a network, and it is the same seam discipline as `EmbeddingProvider`
/ `CompletionProvider` — which the real implementations here finally fill.

| Unit | Scope | Verification |
|---|---|---|
| **W4.7a** — manifest + registry core — **DONE** (2026-07-06) | The manifest schema (serde structs — deserialization IS the validation, fail-loud with a typed `ManifestError` naming the field, `schemaVersion` gated); the `StreamDecoder`/`RequestTransform` closed enums (values W4.7b/c implement against); the nine built-in manifests transcribed from v4's registry metadata via a checked-in generator (`harness/oracle/providers/gen-provider-manifests.mjs`; `include_str!` + `LazyLock`); `get_provider` EXACT-case lookup (v4 does NOT resolve `legacyNames` — display metadata only); `rewrite_localhost_url` (pure, gateway injected); the four registry-seam replacements closed in their LEAF consumers (`message_formatter` name-field now registry-backed — a real behavior change: DEEPSEEK/Z_AI/OPENAI_COMPATIBLE now support the name field; `model_context`/`cheap_model` oracles regenerated with the real registry; `tool_build` web-search stays a corpus knob) — pins moved to registry-parity | Registry-dump parity: `provider_registry_equivalence` (tsx oracle drives v4's real getters over every provider × getter, 253 rows, tier-1 exact incl. defaults + legacy-name-misses + determinism) + malformed-manifest fail-loud unit tests; each replaced seam's consumer differential regenerated green. **Deferred:** spine-side seam removals (sourcing the injected inputs from the registry at the orchestrator composition point) → orchestrator-spine owner; `baseUrl`/`endpoints`/`auth` transcribed but not differential-checked (W4.7b/c refine) |
| **W4.7b** — the five stream decoders (**DONE** 2026-07-06) | `chat-completions-sse`, `responses-api-sse`, `anthropic-sse`, `google-parts`, `ollama-ndjson` — each a push-bytes-in / `StreamChunk`s-out state machine per the table above, incl. tool-call accumulation, reasoning routing, usage assembly. Ported in `crate::model::decoders` (a shared `sse` frame splitter + the five decoders) over the existing `StreamChunk` (NOT extended). **Three STOP-rule divergences from the table below were confirmed against v4 source and are handled** (see the "Divergences" note in `model::decoders`): the four "chat-completions-sse" providers do NOT share one normalization — deepseek/z-ai (OpenAI SDK) vs openrouter (raw-fetch `streamViaChatCompletions`), and deepseek vs z-ai differ on cache source + `rawProviderUsage` — reproduced via an internal `Flavor` selector over one shared parser (the decoder enum NAME is unchanged); `google-parts` is `data:`-prefixed SSE (the SDK's `processStreamResponse`), not JSON-array/newline; openrouter's no-tools OpenResponses SDK path is a tracked deferral (a distinct undocumented wire). | Tier-1 `stream_decoders_equivalence`: a checked-in fetch-mock recorder (`harness/oracle/providers/record-stream-fixtures.mjs`) drives v4's REAL plugin `streamMessage` over committed wire transcripts → normalized-chunk NDJSON; the Rust decoders replay at whole/per-frame/byte-at-a-time and diff the chunk sequence + `rawResponse`. Adversarial cases banked (keep-alives, fragmented tool-call JSON with escaped quotes, mid-stream error, empty/usage-absent, interleaved thinking/text, cumulative reasoning, split JSON line). Two documented transport-artifact normalizations: google `sdkHttpResponse` stripped; ollama byte-at-a-time is bug-parity (no cross-read buffer) diffed at line-aligned chunkings only. |
| **W4.7c** — request builders + transforms + tool wire — **DONE** (2026-07-07) | **PART 1 — the tool wire** (`crate::model::tool_wire`): `formatTools` reshape per provider (closes W4.1g: `tool_build::format_tools_for_provider`, wiring into `build_tools` a spine handoff), `parseToolCalls` per provider (closes the native-loop `ToolCallDetector`: `RegistryToolCallDetector`), and the provider text-markers strategy (closes the text-tool-loop seam: `ProviderTextMarkersStrategy`), dispatched by the manifest `toolFormat`; the one backreference regex hand-rolled. **PART 2 — the request builders + the four `RequestTransform` hooks** (`crate::model::request_builder`): the sans-IO `build_request(provider, &RequestInput) -> BuiltRequest`, dispatched by the manifest; anthropic (cache breakpoints + adaptive-thinking/sampling-rejection — the model list a compiled constant, NOT lifted to the manifest [prefix-regex matching, no manifest slot; a `samplingRejectedModelPrefixes` field is a deferred manifest-schema follow-up]); openai (`previous_response_id` chaining, fallback = transport); google (schema sanitizer + `thoughtSignature`, the genai-SDK config→wire framing deferred to W4.7d); deepseek (`reasoning_content` echo + thinking-param strip); + the plain providers (z-ai/openrouter/ollama/grok/openai-compatible). | **PART 1:** `tool_wire_equivalence` (231 rows tier-1 byte-exact via `record-tool-wire.mjs`) + `native_tool_loop_tier3` / `text_tool_loop_tier3` regenerated with the real detector/strategy. **PART 2:** `request_builder_equivalence` (31 rows byte-exact body/url/method vs v4's real plugin requests, captured by intercepting fetch in `record-request-envelopes.mjs`) + `request_builder_google_equivalence` (5 rows: google contents/thoughtSignature + the sanitizer via the wire functionDeclarations). **Spine handoff (part 1):** wire the real detector/strategy/reshape into the `orchestrator` spine (regenerate its oracle with the real registry). |
| **W4.7d** — transport, errors, keys | The `ProviderTransport` trait + a reqwest host impl (timeouts, abort, SSE reads); the `api_keys` **table port** (the one unported repo — read + marshaling, plaintext-within-encrypted-DB) closing every `ApiKeyResolver` seam at composition points; `validateApiKey` / `getAvailableModels` surfaces; `sendMessage` non-streaming over the same builders/decoders | api_keys tier-2 (standard repo differential); error-classifier rows extend the ported `lib/llm/errors` differential; transport itself is host-tier (integration smoke, no oracle) |
| **W4.7e** — pricing, capability, logging, embeddings | The pricing fetcher (OpenRouter models fetch, 24 h cache + 5 min negative cache, 3 s timeout) as a host-seamed data source over the ported fallback tables; `checkModelSupportsTools` closure (replaces the injected `model_supports_native_tools`); `logLLMCall` closure (the llm_logs write shape + the autonomous-run-id context — repo already ported); the real embedding providers (openai / ollama with `num_ctx` derivation + the NaN/Inf reject guard / openrouter base64-Float32 decode; builtin already ported) + the interactive-priority throttle decision logic | Pricing/capability: tier-1 over canned fetch payloads both sides; logLLMCall: tier-2 llm_logs row diff driven through a real call site; embeddings: tier-1 request/parse over canned payloads; regenerate the differentials whose pins close (`cheap_llm_exec` API-key/log deferrals, `tool_build` capability input) |
| **W4.7f** — image dialects + moderation + web search — **DONE** | The FIVE image dialects (`model::image_dialects` — OPENAI / GOOGLE Imagen + Gemini / GROK / OPENROUTER / Z-AI; z-ai was omitted from the plan) as sans-IO `build_image_request` + `parse_image_response` over the new `model::wire::WireTransport` seam, with **moderation-rejection normalization** (the empty-200 / `raiFilteredReason` cases → the exact `content policy` string `isImageModerationError` matches) and the three refusal-keyword GAPs carried faithfully (Gemini "No images returned", OpenRouter "Model declined", z-ai's absent moderation handling); the real `RealImageProvider` / `RealModerationProvider` (closes W4.2's seam) / `RealWebSearchProvider` (closes the W4.1d5 seam, incl. the env-var fallback error set); orientation data transcribed into `image_gen_data` | Three tier-1 differentials vs v4's REAL plugins (`image_dialects_equivalence` incl. the orientation transcription, `moderation_wire_equivalence`, `web_search_wire_equivalence`); regenerated `web_search_tool` (real provider + fallback), `danger_gatekeeper_tier3` (moderation un-mocked to canned wire, failure = canned 500), and `image_generation` (real dialect behind canned transport) all green. Api-key lookups stay behind the existing seams pending W4.7d's `db::api_keys` |

Sequencing: a ∥ b first (independent); c after b (it reuses decoder types)
and closes three live seams; d/e/f in any order after a. The enclave (Unit
4) does not depend on any of these; W4.9a consumes f only at the wire layer
(its differential runs with a canned transport either way).

## Open items (not yet decided)

- **Third-party trust.** A JSON manifest can't run arbitrary code (good), but it
  can still point at a malicious `baseUrl` to exfiltrate an API key. Decide
  whether third-party manifests are signed (Ed25519, mirroring the theme
  registry) or restricted to a vetted set.
- **Where decoders are validated against live providers.** Tier-1 differential
  tests prove Rust-decoder ≡ v4-decoder on recorded streams; they do not prove
  either matches a live provider that has since changed. A periodic
  recorded-fixture refresh is worth scheduling.

## Provenance

Classification above comes from reading v4 source directly:
`plugins/dist/{anthropic,openai,grok,google,deepseek,z-ai,openrouter,ollama,openai-compatible}/provider.ts`
and `packages/plugin-utils/src/providers/openai-compatible.ts`. The decoder
count and the request-quirk list are grounded in that code, not in the provider
vendors' public docs.
