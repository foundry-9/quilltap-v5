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
