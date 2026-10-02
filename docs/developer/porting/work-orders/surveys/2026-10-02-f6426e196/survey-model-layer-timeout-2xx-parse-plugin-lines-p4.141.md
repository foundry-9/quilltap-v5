# Survey — P4.141: the model layer — P4.128's two recorded approximations (the non-streaming timeout wording, the 2xx-body-fails-to-parse gap) + the nine unported GOOGLE / OLLAMA / OpenRouter-raw plugin ERROR lines

**Date:** 2026-10-02 · **v4:** `f6426e196` (tree dirty by the three recorded docs paths) · **v5 main:** `cb9ecf256` · **Kind:** read-only measurement — source reads of both trees, the SDKs under `node_modules`, and a throwaway probe (`/private/tmp/claude-503/p4141/probe.mjs`, run from the v4 root under `npx tsx`, Node 24.13.1). The probe is the `record-text-errors.mjs` idiom with no DB: the plugin-logger bridge plus a mocked `fetch`. It drove the ten REAL text plugins `sendMessage`/`streamMessage` against posed 2xx bodies, a posed hang with `requestTimeoutMs: 50`, and a posed `fetch` throw. A second probe (`trig.mjs`) ran v4's real `classifyFallbackTrigger` over the thrown texts. Nothing was built and nothing in either repo was written except this file.

## The finding in one line

All three items are recordable through the REAL plugins, and each one is wider than the record says. (a1) v4's text plugins never run on the SDK's 10-minute default: every plugin passes a 300 s budget. A 50 ms `params.requestTimeoutMs` drops `maxRetries` to 0 and records `Request timed out.` in about 100 ms. The bigger miss is the TRIGGER: v4 files every timeout, and every raw-`fetch` connect failure, as `network`, while v5 files both kinds of timeout as `provider-error`. That includes the streaming headers timeout P4.128 believed handled. (a2) The 2xx gap is a behaviour divergence for SEVEN providers on the non-streaming send: OAC, DeepSeek, NanoGPT, Z.AI, OpenAI, Grok and Anthropic all throw where v5 answers `Ok("")`. Google, Ollama and OpenRouter-raw answer `Ok` in v4 too. The stream mode is NOT a failure for the SDK providers. The non-JSON half already errs on v5; only its lines and bytes differ. On top of that, v5's transport SWALLOWS a 2xx body-read failure into an empty body. (h) The nine lines are ten. They fire on transport failures, timeouts and non-JSON 2xx bodies as well as on non-2xx, and their `error` sits inside the CONTEXT object on five of the ten.

---

## §A (a1) The non-streaming timeout wording

### A1 v4 — where `Request timed out.` comes from (openai 7.23.0, every SDK plugin's copy is 7.23.0)

- The classes: `node_modules/openai/core/error.js:78-86` `APIConnectionError` (`message || 'Connection error.'`). `:88-92` `APIConnectionTimeoutError extends APIConnectionError` (`message ?? 'Request timed out.'`).
- **Throw site 1 — the fetch failed** (`client.js:792-841`). `fetchWithTimeout` (`:997-1031`) arms `setTimeout(abort, ms)` and passes `signal: controller.signal` into `fetch`. When `fetch` rejects:
  - `isTimeout = isAbortError(response) || /timed? ?out/i.test(String(response) + String(response.cause))` (`:799-800`). `isAbortError` is `internal/errors.js:5-12`: `name === 'AbortError'`, or Expo's `FetchRequestCanceledException`.
  - It retries if retries remain (`:801-814`).
  - Otherwise, on a timeout: `cause.code === 'UND_ERR_HEADERS_TIMEOUT'` gives the LONG variant (`'Request timed out. Node.js fetch timed out waiting for response headers; configure a matching undici fetch and fetchOptions.dispatcher with an Agent whose headersTimeout is at least the SDK timeout.'`, `:829-836`). Anything else gives the plain `new APIConnectionTimeoutError()` (`:837`).
  - A non-timeout throws `APIConnectionError({ message: getConnectionErrorMessage(response) })` (`:848-851`). `getConnectionErrorMessage` (`:1679-1684`) returns `undefined`, so the message is `Connection error.`, except for an undici dispatcher-version mismatch.
- **Throw site 2 — the 2xx BODY read** (`client.js:545-640`, `parseResponseWithTimeout`). A non-streaming response whose body read outlives the remaining budget throws `APIConnectionTimeoutError()` (`:580-585`). It retries if retries remain (`:611-633`). So a 2xx whose body stalls is ALSO `Request timed out.` in v4.
- **Budget actually in force.**
  - ⚠ NOT the SDK's 10-minute `DEFAULT_TIMEOUT` (`client.js:1606`, `600000`). Every SDK plugin overrides it.
  - `@quilltap/plugin-utils` `src/providers/request-budget.ts:28` `DEFAULT_REQUEST_TIMEOUT_MS = 300_000`. `buildSdkClientOptions` (`:77-86`) gives `{timeout: params.requestTimeoutMs > 0 ? it : 300000, maxRetries: capped ? 0 : 2}`. `buildSdkRequestOptions` (`:60-66`) gives `{timeout, maxRetries: 0}` only when the caller capped.
  - The OAC plugin's own copy is 2.6.2: `plugins/dist/qtap-plugin-openai-compatible/node_modules/@quilltap/plugin-utils/dist/providers/index.js:115-131`. The constructor sets `this.requestTimeoutMs = DEFAULT_REQUEST_TIMEOUT_MS` (`:208`/`:215`). `createClient` passes `timeout`/`maxRetries` (`:272-280`). `sendMessage`/`streamMessage` pass `buildRequestOptions(params)` per call (`:421-424`, `:461-464`).
  - OpenAI `provider.ts:472-479`/`:533-540`, Grok `:361-368`/`:463-470` and Z.AI `:79-84` spread `buildSdkClientOptions(params)`. Anthropic does the same at `:359`/`:556`.

### A2 v4 — which provider goes through which client (non-streaming `sendMessage`)

| provider | v4 client on `sendMessage` | throw text on a timeout | on a connect failure | catch line? |
|---|---|---|---|---|
| OPENAI_COMPATIBLE | openai SDK `chat.completions.create` (plugin-utils base `:415-447`) | `Request timed out.` | `Connection error.` | `OpenAICompatible API error in sendMessage` |
| DEEPSEEK | openai SDK `chat.completions.create` (`provider.ts:252-303`) | same | same | `DeepSeek API error in sendMessage` |
| NANOGPT | openai SDK `chat.completions.create` (`:307-348`) | same | same | `NanoGPT API error in sendMessage` |
| Z_AI | openai SDK `chat.completions.create` (`:345-347`, NO try) | same | same | none |
| OPENAI | openai SDK `responses.create` (`:517`) | same | same | none |
| GROK | openai SDK `responses.create` (`:416`) | same | same | none |
| ANTHROPIC | `@anthropic-ai/sdk` `messages.create` (`:505`) | `Request timed out.` (probed) | `Connection error.` (probed) | none |
| GOOGLE | `@google/genai` `generateContent` (`:610-611`, `httpOptions.timeout` `:529`) | `This operation was aborted` (DOMException `AbortError`, probed) | `fetch failed` (probed; genai does not retry) | `Error calling Google Gemini API` |
| OLLAMA | raw `fetch` + `buildRequestAbortSignal` (`:184-196`) | `The operation was aborted due to timeout` (`AbortSignal.timeout`'s `TimeoutError`, probed) | `fetch failed` | `Ollama sendMessage failed` |
| OPENROUTER (no image) | `@openrouter/sdk` (`:166-173`, `timeoutMs`) | unrecordable — speakeasy retries connection errors for up to `maxElapsedTime: 3600000` (`@openrouter/sdk/esm/lib/retries.js:10`); the probe hung past 60 s | same | none |
| OPENROUTER (image) | raw `fetch` `sendViaChatCompletions` (`:422-442`, `buildRequestAbortSignal`) | `The operation was aborted due to timeout` (probed) | `fetch failed` (probed) | `Error in sendViaChatCompletions` (the TENTH line, §C) |

On streaming the same table holds, with these exceptions:
- The raw-fetch streams arm a plain `AbortController` (Ollama `:262-275`, OpenRouter `:810-829`), so they throw `This operation was aborted`.
- Google's stream budget is caller-only (`:689-697`).

**Probe output (`hang_budget50`, `requestTimeoutMs: 50`, the mocked `fetch` rejecting with `signal.reason` the way undici does):** every SDK provider threw `APIConnectionTimeoutError: Request timed out.` after `fetchCalls: 1` in 51–132 ms. The three catch-line plugins logged `<Name> API error in sendMessage|streamMessage` with `error: 'Request timed out.'`. Google, Ollama and OpenRouter-raw logged their own lines (§C).

**v4's trigger** (`lib/llm/fallback/engine.ts:73-83`, `NETWORK_ERROR_PATTERNS` = `/timed? ?out/i, /timeout/i, …, /fetch failed/i, /network/i, /aborted/i`; probe `trig.mjs`):

| thrown text | trigger |
|---|---|
| `Request timed out.` | `network` |
| `This operation was aborted` | `network` |
| `The operation was aborted due to timeout` | `network` |
| `fetch failed` | `network` |
| `Connection error.` | `provider-error` |
| a TypeError / SyntaxError text | `provider-error` |

### A3 v5 today

- `TransportError { message, status }` (`crates/quilltap-core/src/model/transport.rs:206-213`) has no kind. `TransportError::headers_timeout(ms)` (`:231-236`) is message `provider did not send response headers within {ms}ms`. `is_headers_timeout` (`:241-243`) sniffs that message prefix (`HEADERS_TIMEOUT_PREFIX` `:246`).
- **`ReqwestTransport::execute` (non-streaming, `:337-372`).**
  - `.timeout(policy.timeout)` bounds the whole exchange (`:350`).
  - A `send()` error becomes `TransportError { message: e.to_string(), status: None }` (`:360-364`). reqwest 0.12.28's `Display` for `Kind::Request` is `error sending request for url (<url>)` (`reqwest-0.12.28/src/error.rs:231,267-269`), identical for a timeout and for a refused connect. `is_timeout()` (`:116-136`, a `TimedOut` / hyper timeout / `io::ErrorKind::TimedOut` in the source chain) is NEVER consulted.
  - ⚠ **`:353` `resp.bytes().await.map(..).unwrap_or_default()`** — a body-read failure (including the `.timeout()` firing mid-body) becomes an EMPTY body. On a 2xx that returns `Ok(TransportResponse { status: 200, body: [] })`, and the parse then fails as `response parse: EOF while parsing a value at line 1 column 0` (`completion_provider.rs:274-276`). v4 throws `Request timed out.` there (A1 throw site 2). So the P4.128 approximation ("maps to `Connection error.`") is wrong for this arm: it is not a transport error at all on v5.
- **`execute_stream` (`:381-425`).** `tokio::time::timeout` around `send()` becomes `headers_timeout` (`:389-391`). A reqwest error from `send()` itself, such as an OS connect timeout, is a plain message (`:392-395`).
- `provider_error::sdk_thrown_message` (`crates/quilltap-core/src/model/provider_error.rs:76-103`) maps the catch line's `error` field:
  - the side's message on a non-2xx;
  - `Request timed out.` iff `is_headers_timeout()`;
  - `Connection error.` otherwise.

  It is consumed only by `PluginCatchLog::emit_transport` (`streaming_provider.rs:452-461`). Doc `:88-93` records the approximation.
- **The classification.**
  - Salon: `FallbackError::from_stream_error` (`crates/quilltap-core/src/llm_fallback/types.rs:320-330`) hands the classifier `e.message`.
  - Cheap path: `cheap_llm_exec.rs:1398-1410` does the same with `FallbackError::message(&error.message)`.
  - v5's `NETWORK_ERROR_PATTERNS` (`llm_fallback/engine.rs:62-72`) are v4's verbatim. But **neither v5 timeout message matches them**: `error sending request for url (…)` and `provider did not send response headers within 300000ms` contain no `timeout` / `timed out` / `aborted`. Both fall to the final `Some(ProviderError)` (`engine.rs:224-226`).
  - v4 says `network` for both. **The same holds for the raw-fetch providers' connect failure** (v4 `fetch failed` → `network`; v5 → `provider-error`).
  - The trigger reaches the persisted route trail and the `[Failover] … trigger:` line.
- The test rig exists: `transport.rs:513-720` (`#[cfg(all(test, feature = "native-transport"))]`, `Behavior::{NeverAnswers, SlowButFlowing, AlwaysFails}`). `non_streaming_abandons_a_provider_that_never_answers` (`:606-627`) asserts only `status.is_none()`, never the kind.

### A4 What differs, and the proof

| arm | v4 thrown / trigger | v5 message / trigger | catch-line `error` today |
|---|---|---|---|
| non-streaming, no response in budget (SDK providers) | `Request timed out.` / network | `error sending request for url (…)` / provider-error | `Connection error.` ✗ |
| non-streaming, 2xx body stalls | `Request timed out.` / network | `response parse: EOF…` / provider-error | no line ✗ |
| streaming headers timeout (SDK providers) | `Request timed out.` / network | `provider did not send response headers within Nms` / provider-error ✗ | `Request timed out.` ✓ |
| raw-fetch providers, timeout | `This operation was aborted` or `The operation was aborted due to timeout` / network | as above / provider-error ✗ | no line ✗ (§C) |
| raw-fetch providers, connect failure | `fetch failed` / network | `error sending request for url (…)` / provider-error ✗ | no line ✗ (§C) |

**Recordable through the real SDK** (refuting the "may be unrecordable" premise):
- The recorder poses a hang: the mocked `fetch` returns a promise that rejects with `init.signal.reason` (or `input.signal.reason` for a `Request`) on abort. The case sets `params.requestTimeoutMs`, which every plugin honours (A1), and `maxRetries` drops to 0.
- `fetchCalls: 1`, about 100 ms per row.
- The SDK's real `APIConnectionTimeoutError` is what gets recorded. No bytes are copied from source.
- The ONLY exclusion is OpenRouter's two SDK modes (`stream`, `send`), where speakeasy retries for an hour. Those are the `openrouter-sdk` class v5 never runs anyway.

**Red-first (predicted from the probe; the lane measures it at the regen):**
- **Trigger:** a new `transport_hang` case is 9 providers × 2 modes + OpenRouter `stream_tools`/`send_vision` = 20 rows, all v4 `network`. Every one is red on `trigger` on today's v5 when `PosedFailure` poses a status-less timeout.
- **Catch line:** the three SDK catch-line providers × 2 modes = 6 rows red on the catch line's `error` while v5 renders a posed non-headers timeout as `Connection error.`.
- **The widened fetch-throws case** (§D) adds 6 raw-provider trigger reds.

**Mutation proof:** map `Timeout` back to `Connect` in the new `TransportError` kind → the 20 hang rows red on `trigger`, plus the 6 catch-line rows. In the native rig, delete the `is_timeout()` consult → the new `non_streaming_timeout_is_kind_timeout` unit red. Restore the `:353` swallow → the new `HeadersThenStalls` unit red.

---

## §B (a2) The 2xx body that fails to parse

### B1 v4 — `sendMessage` on posed 2xx bodies (probe, `application/json`, status 200; the throw text is verbatim)

| body | OAC / DEEPSEEK / NANOGPT | Z_AI | OPENAI / GROK | ANTHROPIC | GOOGLE | OLLAMA | OR-SDK (`send`) | OR-raw (`send_vision`) |
|---|---|---|---|---|---|---|---|---|
| `{"choices":[]}` | `TypeError: Cannot read properties of undefined (reading 'message')` + catch line | same throw, no line | `TypeError: response.output is not iterable` | `TypeError: Cannot read properties of undefined (reading 'filter')` | **OK** `''` (+ WARN `No candidates found in Google response`, `:265-269`) | **OK** `''` | `ResponseValidationError: Response validation failed` | **OK** `''` |
| `{"choices":[{}]}` | `…(reading 'tool_calls')` OAC / `'reasoning_content'` DS / `'reasoning'` NanoGPT + line | `…(reading 'reasoning_content')` | as above | as above | OK + WARN | OK | as above | (not probed) |
| `{"choices":[{"message":{}}]}` | **OK** `''` | OK | as above | as above | OK + WARN | OK | as above | — |
| `{}` | `…(reading '0')` + line | same, no line | as above | as above | OK + WARN | OK | as above | — |
| `not json` | `SyntaxError: Unexpected token 'o', "not json" is not valid JSON` + line | same, no line | same | same | same + `Error calling Google Gemini API` | same + `Ollama sendMessage failed` | same | same, NO line (`response.json()` is outside the try, `:454`) |
| `` (empty, JSON content-type) | `…(reading 'choices')` + line (the SDK returns `undefined` for an empty JSON body, `openai/internal/parse.js:32-44`) | same, no line | `TypeError: Cannot use 'in' operator to search for 'object' in undefined` | `SyntaxError: Unexpected end of JSON input` | same + Google line | same + Ollama line | same | (not probed) |

The v4 sites:
- OAC: plugin-utils dist (2.6.2) `providers/index.js:425-429` (`response.choices[0]`, `choice.message.tool_calls`), catch `:439-446`.
- DeepSeek: `provider.ts:258-260`, catch `:297-303`.
- NanoGPT: `:313-320`, catch `:343-348`.
- Z.AI: `:350-353`, no try.
- OpenAI: `:440` `for (const item of response.output)`.
- Grok: `extractTextFromResponse` `:239`.
- Anthropic: `:508-510` `.filter(...)`.

All of these TypeErrors and SyntaxErrors classify `provider-error` on v4 (`trig.mjs`).

**Stream mode on the same bodies (probe):**
- Every openai-SDK provider ends CLEAN: one empty `done` chunk, no throw, no catch line. OPENAI/GROK WARN `Stream ended without response.completed event`.
- ANTHROPIC and OLLAMA end with zero chunks. OpenRouter-raw `stream_tools` ends clean.
- **GOOGLE throws `Error: Incomplete JSON segment at the end`** and logs `Error streaming from Google Gemini API`.
- OR-SDK throws.

So the "three catch-line providers × 2 modes" arm the record names has a STREAM half that is a silence leg, not a failure.

### B2 v5 today

- `completion_provider.rs:271-276`: `resp.json().map_err(|e| CompletionError::new(format!("response parse: {e}")))?`. A non-JSON or empty 2xx is already an `Err`, but the message is serde's (`response parse: expected value at line 1 column 1`), there is no catch line, and there is no Google/Ollama line.
- Any JSON body goes to `parse_for_provider_ex` (`response_parse.rs:1144-1170`). **Every parser is total**: `parse_chat_completions` (`:256-263`) defaults a missing `choices[0]` / `message` to `Value::Null` and content to `""`. `parse_responses_api` (`:715`) and `parse_anthropic` (`:865`) are likewise total. So every JSON shape above is `Ok(CompletionResponse { content: "" })`.
- Google's `No candidates found in Google response` WARN is unported (grep: zero hits in `crates/`).

**Where it reaches.**
- Not the Salon turn: the Salon streams (`primary_stream.rs`), and the stream half is a silence leg (B1). Only the Google stream throw is a real stream-mode gap.
- The non-streaming `CompletionProvider` serves `cheap_llm_exec.rs`, `compression.rs`, `file_fallback.rs`, `dangerous_content/gatekeeper.rs`, `memory_recap/mod.rs`, `outfit_selections.rs`, `announcer/voice_rewrite_core.rs`, `build_context.rs` and `message_context.rs` (grep `send_message`).
- On the cheap path, v4 throws → `classifyFallbackTrigger` → `provider-error` → the stand-in chain tries the next route. v5's `Ok("")` instead reaches `cheap_llm_exec.rs:1153-1207`: the empty-body refusal ledger plus the uncensored retry when one is configured, otherwise **an empty result handed to the task** (a blank title, an empty extraction).
- **This is a behaviour divergence**, but in the cheap / gatekeeper / describe paths, not "an empty assistant message in the Salon".

### B3 The design

- **The v5 fix:** an `Err` with v4's thrown bytes, and the catch line for the three providers, so the cheap stand-in chain engages.
  - A new `model/sdk_response_shape.rs` (or a section of `completion_provider.rs`) holds, per provider family, v4's own READ CHAIN as a JS-faithful property walk. For the three catch-line providers it is: `response` → `.choices` → `[0]` → `.message` → the provider's next read (`tool_calls` OAC, `reasoning_content` DS/Z.AI, `reasoning` NanoGPT). It returns V8's `Cannot read properties of undefined|null (reading '<k>')` at the first `undefined`/`null`.
  - Responses (OPENAI/GROK): `response.output` must be iterable, else `response.output is not iterable`. An undefined body gives the SDK's `Cannot use 'in' operator to search for 'object' in undefined`.
  - ANTHROPIC: `content` must exist, else `…(reading 'filter')`.
  - Empty JSON body: the SDK's `undefined` for openai-SDK plugins, else V8 `Unexpected end of JSON input`.
  - Non-JSON: `crate::generators::optimizer::v8_json_parse_message` (`src/generators/optimizer.rs:893`, already measured against V8; it returns `None` for in-value failures, which falls back to serde's text as a RECORDED divergence). Call it in place: P4.139 edits `optimizer.rs`, so do not move it.
  - GOOGLE / OLLAMA / OpenRouter-raw stay `Ok` on JSON shapes, as v4 does.
- **`CompletionError.message` for these arms = v4's thrown text.** No v5 transport text exists to preserve, so the §S.5 "message bytes stay v5's" ruling does not bite, and v4's text classifies `provider-error` like any other.
- **The recorder arm.** The existing `cases.json` mechanism already poses any status and body: `new Response(body, {status})` at `record-text-errors.mjs:223-231`. Five new cases need no recorder change beyond the `outcome: 'ok'` rows it already emits (`:234,263`): `ok_choices_empty`, `ok_choice_no_message`, `ok_empty_object`, `ok_non_json`, `ok_empty_body_json`. Each runs × all ten providers, 22 rows per case, 110 rows in all.
- **The Rust side.** `text_http_errors_equivalence.rs` gains a posed-2xx transport: `execute` → `Ok(TransportResponse {200, body})`; `execute_stream` → a channel yielding the body then EOF. The `assert_eq!(row.outcome, "thrown")` (`:855`) becomes a per-row outcome comparand (v4 `ok` ↔ v5 `Ok`). On thrown 2xx rows, v5's `CompletionError.message` is diffed against `thrown.message`.
- **Red-first (predicted):**
  - 21 send rows (7 providers × 3 JSON shapes) red on OUTCOME.
  - 15 catch-line rows (3 providers × 5 cases) red on the line.
  - 4 Google/Ollama send lines (non-JSON + empty).
  - The non-JSON/empty message bytes on every SDK send row.
  - 5 Google stream rows if v5's `google_parts` decoder does not throw on a non-SSE body (MEASURE).
- **Mutation proof:** delete the shape guard → the 21 outcome rows red. Drop the 2xx emit → the 15 catch-line rows red.

---

## §C (h) The nine (ten) unported plugin ERROR lines

`text_http_errors_equivalence.rs:748-761` `UNPORTED_PLUGIN_ERROR_LINES`, verbatim (`(provider, v4 mode, message)`):

```
("GOOGLE", "send", "Error calling Google Gemini API"),
("GOOGLE", "stream", "Error streaming from Google Gemini API"),
("OLLAMA", "send", "Ollama API error response"),
("OLLAMA", "send", "Ollama sendMessage failed"),
("OLLAMA", "stream", "Ollama streaming API error"),
("OLLAMA", "stream", "Ollama streamMessage failed"),
("OPENROUTER", "send_vision", "OpenRouter API error"),
("OPENROUTER", "stream_tools", "OpenRouter API error"),
("OPENROUTER", "stream_tools", "Error in streamViaChatCompletions"),
```

All ten are ERROR. In the table below, a context `error` field and a third-argument `error` are DIFFERENT carriers (the recorder's `context.error` vs `error`).

| # | v4 site (`plugins/dist/…/provider.ts`) | try scope | fields (in order) | fires on |
|---|---|---|---|---|
| 1 | google `:678-682` `logger.error('Error calling Google Gemini API', {context:'GoogleProvider.sendMessage', model: params.model, error: msg})` — NO 3rd arg | `:610-684` (call + parse) | `context, model, error` | non-2xx (`error` = genai `ApiError` message = v5's `google_api_error` side message), `fetch failed`, `This operation was aborted`, non-JSON/empty 2xx (V8 SyntaxError) |
| 2 | google `:883-887` `'Error streaming from Google Gemini API'`, same shape, `context:'GoogleProvider.streamMessage'` | `:777-889` (whole stream incl. mid-stream) | `context, model, error` | as 1, plus a non-SSE 2xx (`Incomplete JSON segment at the end`) |
| 3 | ollama `:204` `'Ollama API error response'` `{context:'OllamaProvider.sendMessage', status, error: errorText}` | inside `:183-240` | `context, status, error` (raw body) | non-2xx only, BEFORE #4 |
| 4 | ollama `:238` `'Ollama sendMessage failed'` `{context, baseUrl}`, 3rd arg the error | `:183-240` | `context, baseUrl` + error | every throw: non-2xx (`Ollama API error: {status} {body}` = v5 `fetch_error` side message), `fetch failed`, `The operation was aborted due to timeout`, non-JSON 2xx |
| 5 | ollama `:284` `'Ollama streaming API error'` `{context:'OllamaProvider.streamMessage', status, error: errorText}` | `:248-489` | `context, status, error` | non-2xx, BEFORE #6 |
| 6 | ollama `:486` `'Ollama streamMessage failed'` `{context, baseUrl}` + error | `:248-489` | `context, baseUrl` + error | every throw incl. `This operation was aborted` |
| 7 | openrouter `:446-450` `'OpenRouter API error'` `{context:'OpenRouterProvider.sendViaChatCompletions', status, error: errorText}` | OUTSIDE the try (the try `:422-442` wraps `fetch` only) | `context, status, error` | non-2xx on the IMAGE send only; no second line |
| 8 | openrouter `:835-839` `'OpenRouter API error'` `{context:'OpenRouterProvider.streamViaChatCompletions', status, error}` | inside `:805-961` | `context, status, error` | non-2xx on the tools/images stream, BEFORE #9 |
| 9 | openrouter `:956-958` `'Error in streamViaChatCompletions'` `{context}` + error | `:805-961` | `context` + error | every throw (`OpenRouter API error: {status} - {body}`, `fetch failed`, `This operation was aborted`) |
| **10** | openrouter `:438-440` `'Error in sendViaChatCompletions'` `{context:'OpenRouterProvider.sendViaChatCompletions'}` + error | the `fetch`-only try | `context` + error | **a `fetch` throw / timeout on the image send** — probed (`fetch failed`, `The operation was aborted due to timeout`). Absent from the corpus because `transport_fetch_throws` covers only three providers. |

**v5 today.**
- No arm emits any of them (grep: zero hits for every message in `crates/quilltap-core/src`).
- `PluginCatchLog::for_call` (`streaming_provider.rs:380-421`) returns `None` for every provider but the three.
- The emission points exist and are shared:
  - the pre-stream `fail` closure (`streaming_provider.rs:588-593`);
  - the mid-stream decode and transport arms (`:695-710`) and the finish arm (`:722-728`);
  - `completion_provider.rs:236-247` (the transport `fail`) and `:274-276` (the parse arm that §B makes a line-bearing arm).
  - The Ollama think-retry arms (`streaming_provider.rs:604-624`, `completion_provider.rs:252-268`) surface the SECOND failure, as v4's `fetchWithThinkRetry` does.
- `PluginCatchLog` takes no `model`, which Google needs. It emits ONE line where Ollama / OpenRouter-raw need TWO on non-2xx, and its line shape is fixed at `context=… baseUrl=… error=…`.
- The OpenRouter gate: v5 always takes the raw path. v4 logs #8/#9 only when `hasTools || hasImages` (`:779-790`) and #7/#10 only when `hasImageAttachments` (`:162-164`). v5 has `openrouter_non_streaming_is_vision` (`request_builder/chat_completions.rs:313`) for the send but **no streaming twin**.
- ⚠ The family's `run_stream` (`text_http_errors_equivalence.rs:542-582`) passes `stream_params` WITHOUT tools for `stream_tools` rows. It must pass a tool for that mode, or the gate cannot be proven.

**The port.**
- Generalize `PluginCatchLog` into a per-(provider, method, path) catch SHAPE in a new `model/plugin_catch_log.rs`. This folds P4.128's nit, since the type lives in `streaming_provider.rs` while serving `completion_provider.rs`.
- The shape takes `model` and the OpenRouter raw-path predicate. It emits:
  - (i) the optional status line `{context, status, error: <raw body>}` on a non-2xx (Ollama #3/#5, OpenRouter #7/#8 — #7 is the ONLY line on the image send's non-2xx);
  - (ii) the catch line, with v4's field order and v4's thrown text as `error` (`sdk_thrown_message` generalized per family to `fetch failed` / the two abort texts for the raw-fetch providers).
- Google carries `error` as a CONTEXT field. Under the existing `error = %…` convention the capture renders `error=` identically, so the family's renderer only needs the field ORDER from v4's `context` object.
- Mid-stream arms emit too (v4's try covers them). No stream corpus row throws for these providers (`grep -c pluginErrorLog` = 0 in `google_parts`, `ollama_ndjson` and `openrouter.recorded.ndjson`), so unit-pin them.
- **Red-first.** `diff_catch_lines` (`:769-831`) today filters v5 to `… API error in …` lines and asserts v5's whole capture FREE of the nine (`:774-781`). The fix: replace the filter with "every ERROR line on the two model targets" and diff against the WHOLE `pluginErrorLog`.
  - `UNPORTED_PLUGIN_ERROR_LINES` and its exercised-count assert (`:948-957`) are DELETED.
  - Red until the port lands, on every GOOGLE (35 + 35 rows), OLLAMA (33 + 33) and OpenRouter-raw (33 + 33) row of the corpus: **202 rows on the existing 736**, plus the new cases' raw-provider rows.
- **Mutation proofs.** Drop Google's emit → 70 rows red. Drop Ollama's status line only → 66 red. Invert the OpenRouter raw gate → the `stream`/`send` SDK rows red with a line v4 never logged.

**P4.128 nits to fold:**
- (1) `PluginCatchLog` / `CatchMethod` re-homed as above.
- (2) The tautological `assert!(v5.message.starts_with("error sending request for url ("))` (`:887-894`). `PosedFailure::fail` builds exactly that string (`:441-444` → `posed_connect_failure` `:451-453`), so the assert cannot fail. Replace it with `assert_eq!(v5.message, posed_connect_failure(<the request url>))` (the composer must pass the transport's text through unchanged), plus an assert on the new transport KIND.
- (3) The `fetch_calls == 3` assert (`:896`) must become per-provider-recorded-not-compared: raw-fetch rows are 1, SDK rows 3, hang rows 1.

---

## §D The recorder, `cases.json`, and the corpus (shared by §A–§C)

`harness/oracle/providers/record-text-errors.mjs` (276 lines; the logger bridge is at `:119-151`, the `fetch` mock at `:221-232`, `paramsFor` at `:188-206`, `modesFor` at `:208-212`). Changes:

1. A `"transport": "hang"` case kind. The mock returns a promise that rejects with `(init?.signal ?? (input instanceof Request ? input.signal : undefined)).reason` on abort (undici rejects with `signal.reason`). The probe needed both, and the `Request` form for genai.
2. A case-level `"requestTimeoutMs"`, threaded by `paramsFor` into `params.requestTimeoutMs`.
3. A case-level `"modes"` filter, so hang and fetch-throws rows skip OpenRouter's SDK `stream`/`send` (speakeasy's 1-hour retry; the `openrouter-sdk` class v5 never runs).
4. **Keep the event loop alive** (an interval cleared at the end). `AbortSignal.timeout`'s timer is unref'd, so without it Node exits `13` with an unsettled top-level await. Measured: the first probe run exited 13 on Google/Ollama/OpenRouter-raw.
5. **`transport_fetch_throws` widened** from three providers to all ten (OpenRouter raw modes only). That adds 14 rows (6 → 20) and the TENTH line (`Error in sendViaChatCompletions`).

The new cases are:
- `transport_hang` (20 rows);
- the five `ok_*` 2xx cases (110 rows).

The corpus grows **736 → 880 rows**. It is regenerated by THIS lane at the round pin through `regenerate-text-errors.sh` from a pinned worktree (ledger §5.1). The family's row-count formula (`:843-847`) and the `transport_rows == 6` / `refused_v4 == 162` / `catch_lines == 204` constants (`:932-946`) are re-derived. Tier 2: a `pluginWarnLog` field for Google's `No candidates found …` WARN.

The posed-server tool: `harness/tools/refusal-server.py` (the dogfood instrument, modes list `:15-55`) gains `empty-choices` (200 `{"choices":[]}` non-streaming), `hang` (accept, never answer) and `stall-body` (headers, then silence). `harness/tools/stall-server.py` exists for mid-stream stalls. These are for the dogfood 💸 rows only; the differential needs no server.

---

## What the recorded description got wrong

1. **"v4's SDK default is 10 min" (this lane's brief) is wrong for every text plugin.** All ten pass a 300 s budget (`DEFAULT_REQUEST_TIMEOUT_MS = 300_000`, plugin-utils `request-budget.ts:28`; OAC `index.js:208`; OpenAI/Grok/Z.AI/Anthropic `buildSdkClientOptions`; Google `:529`; Ollama/OpenRouter `buildRequestAbortSignal`). v5's `transport.rs:103` already matches.
2. **"Propose a short `timeout` only if v4's plugin passes one through — otherwise unrecordable" — refuted.** `params.requestTimeoutMs` IS passed through by every plugin and caps `maxRetries` at 0. A 50 ms posed hang records the SDK's real `Request timed out.` in about 100 ms with `fetchCalls: 1`. Only OpenRouter's SDK modes are unrecordable (speakeasy's 1-hour retry), and v5 never runs those.
3. **P4.128's mapping fixed only the catch line's text; the TRIGGER was never measured.** v4 classifies `Request timed out.`, both abort texts and `fetch failed` as `network`. v5's two timeout messages and its raw-provider connect failures classify `provider-error`. That includes the **streaming headers timeout** P4.128 treated as done (`provider did not send response headers within Nms` matches no network pattern). The ruling "the failover classifier's input stays v5's" (P4.128 §B1) was made for `Connection error.`, where both sides agree (`provider-error`). For timeouts it locks in a divergence. **Needs a planning ruling.** The recommendation keeps the message bytes and carries a KIND (§Tier 1 item 3).
4. **"A non-streaming reqwest timeout maps to `Connection error.`" is only half the arm.** A body-read failure on a 2xx, including the whole-exchange `.timeout()` firing mid-body, is swallowed by `transport.rs:353` `unwrap_or_default()` into an EMPTY 200. It surfaces as `response parse: EOF…`, not as a transport error at all.
5. **"`{"choices":[]}` … the three catch-line providers × 2 modes".** The send gap spans SEVEN providers: OAC, DeepSeek, NanoGPT, Z.AI (no line), OpenAI/Grok (`response.output is not iterable`) and Anthropic (`…(reading 'filter')`). Google, Ollama and OpenRouter-raw answer `Ok` in v4 too. The STREAM mode is no failure for any openai-SDK provider (the SSE parse ends clean); only Google's stream throws.
6. **"The 2xx-body-fails-to-parse gap … where v5 answers `Ok` with empty content" holds for JSON shapes only.** On a non-JSON or empty 2xx, v5 already answers `Err(response parse: …)`. The gap there is the missing catch / Google / Ollama lines and the message bytes (V8's `JSON.parse` text), not behaviour.
7. **"Is this a behaviour divergence that reaches the Salon?" — not through the Salon turn.** The Salon streams, and the stream half is clean on both sides. It reaches the cheap path (`cheap_llm_exec.rs:1153-1207`: an empty result instead of v4's stand-in failover), the gatekeeper, file describe, compression and memory recap. `primary_stream.rs` needs NO edit.
8. **"The nine unported … lines" are ten, and they fire on more than the non-2xx rows.** #10 `Error in sendViaChatCompletions` is the OpenRouter image send's `fetch`-throw/timeout line, invisible because `transport_fetch_throws` is limited to three providers. Google and Ollama also log on transport failures, timeouts and non-JSON 2xx bodies; Google also logs on a non-SSE stream body. Five of the ten carry `error` in the CONTEXT object, not as the third argument.
9. **P4.128 B3's Tier 3 "Google's incomplete-segment throw unmeasured" is now measured.** A non-SSE 2xx stream body makes `@google/genai` throw `Error: Incomplete JSON segment at the end`, logged by `Error streaming from Google Gemini API`.
10. **"Eleven providers" (this brief) — there are ten** (`ProviderKind`, `model/provider_io.rs:19-30`).
11. **P4.128's openai-SDK timeout description is accurate but incomplete.** Besides `isAbortError || /timed? ?out/i`, a `cause.code === 'UND_ERR_HEADERS_TIMEOUT'` yields a LONG `Request timed out. Node.js fetch timed out waiting for response headers; …` message (`client.js:829-836`). undici's default `headersTimeout` is also 300 s, so at the default budget the SDK's own timer fires first. The long variant is reachable only with a caller budget above 300 s (Tier 3).

Re-verified as recorded:
- `PluginCatchLog` / `CatchMethod` live in `streaming_provider.rs:366-462` while serving `completion_provider.rs:236`.
- The `starts_with("error sending request for url (")` assert is tautological.
- The 736-row corpus.
- The nine triples are exactly what v4 emits on the existing rows (re-extracted from the committed NDJSON: 35/35/33×7 occurrences).

## Proposed tiered deliverables

### Tier 1 — must land

1. **`TransportError` gains a kind** — `TransportErrorKind { Http, Connect, Timeout }` via constructors `http` / `connect` / `timeout`. `headers_timeout` becomes a `Timeout` constructor; `is_headers_timeout` becomes `is_timeout`, read off the kind (the `HEADERS_TIMEOUT_PREFIX` message sniff retired; the message bytes unchanged).
   - `ReqwestTransport` sets `Timeout` from `reqwest::Error::is_timeout()` on `execute`'s send arm (`:360`) and on `execute_stream`'s `send()` arm (`:392`), and keeps `headers_timeout` for the `tokio` deadline.
   - **The `:353` swallow is replaced**: a body-read error is a `TransportError` (`Timeout` if `is_timeout()`, else `Connect`), never an empty 2xx.
   - Unit pins in the native rig: `NeverAnswers` non-streaming → `Timeout`; a new `HeadersThenStalls` behaviour → `Timeout`, not `Ok(empty)`; `AlwaysFails` → `Http`.
   - ~45 struct literals across 5 core files and 7 harness files convert to the constructors (the shared-struct-field trap; none is owned by a sibling — §Cross-lane).
2. **v4's thrown text per provider family** — `sdk_thrown_message` → `v4_thrown_message(provider, method, raw_path, &err)`:
   - openai/Anthropic SDK: `Connection error.` / `Request timed out.`;
   - Google: `fetch failed` / `This operation was aborted`;
   - Ollama and OpenRouter-raw: `fetch failed` / `The operation was aborted due to timeout` (send) / `This operation was aborted` (stream).

   It is used by every catch line.
3. **The timeout trigger (RULING REQUIRED at planning — it extends P4.128's confinement ruling).** `StreamError` and `CompletionError` gain an optional `transport_kind`, set by `pre_stream_error` and `completion_error_from`. `FallbackError::from_stream_error` (`llm_fallback/types.rs:320`) and the cheap path's mapping (`cheap_llm_exec.rs:1398-1410`) set `kind: Some(LlmErrorKind::Network)` exactly when v4's thrown text is network-class: every `Timeout`, and a `Connect` on a raw-fetch provider. The message stays v5's.
   - Red-first: 20 `transport_hang` rows + 6 widened fetch-throws rows red on `trigger`.
   - Mutation: always `None` → those 26 red.
4. **The posed-2xx arm** (§B3): five `ok_*` cases × all providers (110 rows).
   - The v5 shape guard returns `Err` with v4's thrown bytes for the seven throwing providers, with the catch line for the three.
   - `v8_json_parse_message` covers the non-JSON bytes.
   - The family compares outcome + message on 2xx rows.
   - Red-first ≈ 21 outcome + 15 catch-line rows (MEASURE).
5. **The ten plugin ERROR lines** (§C) through a re-homed, generalized `model/plugin_catch_log.rs`: the optional status line + the catch line, v4's field order, the OpenRouter raw-path gate (a streaming predicate added beside `openrouter_non_streaming_is_vision`), and `model` for Google.
   - Pre-stream, mid-stream and non-streaming arms.
   - `UNPORTED_PLUGIN_ERROR_LINES` deleted; the family diffs every ERROR line on the two model targets against the whole `pluginErrorLog`.
   - Red-first ≈ 202 existing rows + the new raw-provider rows.
6. **The recorder + corpus** (§D): `hang`, `requestTimeoutMs`, per-case `modes`, the keep-alive, the widened `transport_fetch_throws`. The NDJSON is regenerated at the round pin, 736 → ~880 rows; the existing rows stay byte-identical bar ordering (parsed-row compare, the P4.128 practice).
7. **The family** (`text_http_errors_equivalence.rs`):
   - `PosedFailure` → a posed exchange (status-less Connect / Timeout / 2xx body);
   - `run_stream` passes a tool for `stream_tools` (and `request_timeout_ms` on hang rows);
   - outcome rows, generalized `fetch_calls`;
   - the tautology replaced (§C nit 2);
   - every count re-derived.

### Tier 2 — should land

8. Google's `No candidates found in Google response` WARN (`{context:'GoogleProvider.extractTextFromResponse', modelName, blockReason}`) on the send path's JSON-shape rows. The recorder's `pluginWarnLog` (WARN capture) and the family diff come with it.
9. Stream-mode 2xx rows pinned:
   - silence for the openai-SDK providers;
   - Google's `Incomplete JSON segment at the end` throw + line, if v5's `google_parts` decoder diverges (MEASURE first);
   - Anthropic and Ollama end with zero chunks on v4 — chunk counts are not compared, so record only.
10. Unit pins for the mid-stream emission of #2 / #6 / #9 (no stream corpus row throws for those providers).
11. `refusal-server.py` modes `empty-choices`, `hang` and `stall-body`, documented in `harness/tools/README.md`, for the dogfood rows.

### Tier 3 — loud deferrals

12. The `UND_ERR_HEADERS_TIMEOUT` long message (reachable only with a caller budget > 300 s).
13. The SDK's content-type branch: a 2xx `text/plain` body makes `response` a STRING (`openai/internal/parse.js:49-50`). v5's `TransportResponse` carries no headers — the same class as the existing `google_json_as_text_plain`.
14. Exotic JS shapes beyond `undefined`/`null` property reads: a non-array `tool_calls` → `… is not a function` / `… is not iterable` with V8's callee text; `choices` as a string. Recorded divergences.
15. OpenRouter SDK modes on hang / fetch-throws (speakeasy's 1-hour retry; v5 never runs the SDK).
16. The SDK's own retry counts on the body-timeout arm (`client.js:611-633`) — the 2026-07-23 provider-I/O ruling; recorded, not compared.
17. v4's MID-stream transport-failure bytes, and Anthropic's raw `SyntaxError` (P4.128 Tier 3), stay deferred.

## Files the lane would edit

**Core (`crates/quilltap-core/src/`):**
- `model/transport.rs` — the kind, the constructors, `is_timeout`, the three `ReqwestTransport` arms, the `:353` swallow, the native-rig unit tests.
- `model/provider_error.rs` — `sdk_thrown_message` → `v4_thrown_message`, its unit tests (`:420-435`), the doc `:76-103`.
- `model/streaming_provider.rs` — `PluginCatchLog` moved out, `pre_stream_error`, the `fail` closure, the mid-stream emits, `for_call` call sites, the unit tests `:945-990`, the literals.
- `model/completion_provider.rs` — the catch-log call, `completion_error_from`, the 2xx shape guard at `:271-276`, the literals + tests.
- `model/plugin_catch_log.rs` (NEW).
- `model/sdk_response_shape.rs` (NEW, or a section of `completion_provider.rs`).
- `model/mod.rs` — module declarations.
- `model/ollama_think_retry.rs` — 2 literals.
- `model/stream.rs` — the `StreamError` transport-kind field.
- `model/completion.rs` — the `CompletionError` transport-kind field.
- `model/request_builder/chat_completions.rs` — the OpenRouter streaming raw-path predicate beside `:313`.
- `llm_fallback/types.rs` — `from_stream_error`, Tier 1 item 3.
- `services/cheap_llm_exec.rs:1393-1410` — Tier 1 item 3 only.
- **`services/primary_stream.rs` is NOT edited**: the classification travels inside `StreamError` into `from_stream_error`.

**Harness:**
- `crates/quilltap-harness/tests/text_http_errors_equivalence.rs` — EDITED, regenerated corpus.
- `TransportError` literal conversions, NOT regenerated, run green neutral:
  - `crates/quilltap-harness/tests/streaming_composer_equivalence.rs`
  - `crates/quilltap-harness/tests/primary_stream_tier3_equivalence.rs`
  - `crates/quilltap-harness/tests/tool_wire_call_site.rs`
  - `crates/quilltap-harness/tests/ollama_think_retry_tier3_equivalence.rs`
  - `crates/quilltap-harness/tests/provider_header_common/mod.rs`
  - `crates/quilltap-harness/tests/cheap_llm_fallback_equivalence.rs`

**Oracle / fixtures / tools:**
- `harness/oracle/providers/record-text-errors.mjs`
- `harness/oracle/providers/regenerate-text-errors.sh` (header only, if the recipe text moves)
- `harness/oracle/fixtures/text-http-errors/cases.json`
- `harness/oracle/fixtures/text-http-errors/text-http-errors.recorded.ndjson` — **REBUILT by this lane at the round pin**.
- `harness/tools/refusal-server.py` and `harness/tools/README.md` (Tier 2).

**Docs:**
- `docs/developer/porting/status-log.md` (lane record)
- `CHANGELOG.md`
- the order file.

Families RUN neutral after the literal conversions (no regen): `streaming_composer_equivalence`, `stream_decoders`, `primary_stream_tier3_equivalence`, `cheap_llm_fallback_equivalence`, `ollama_think_retry_tier3_equivalence`, `tool_wire_call_site`, `request_envelopes` / `request_builder_equivalence`, `file_attachment_tier3`, `stream_watchdog_wrap_census`, `spelling_guard`.

## Files the lane must READ but not edit

- `crates/quilltap-core/src/generators/optimizer.rs:893` (`v8_json_parse_message` — called in place; P4.139 edits this file).
- `crates/quilltap-core/src/llm_fallback/engine.rs` (the patterns).
- `crates/quilltap-core/src/model/response_parse.rs` (the total parsers stay).
- `crates/quilltap-core/src/services/primary_stream.rs`.
- `crates/quilltap-core/src/model/decoders/*`.
- `crates/quilltap-core/src/test_support.rs` (the capture rendering, `:58-83`).
- `crates/quilltap-host/src/spine.rs:293-337` (`WireCompletionProvider`).
- v4:
  - `plugins/dist/qtap-plugin-{openai,openai-compatible,deepseek,nanogpt,z-ai,grok,anthropic,google,ollama,openrouter}/provider.ts`
  - each plugin's `node_modules/@quilltap/plugin-utils/dist/providers/index.js` (the OAC copy is 2.6.2)
  - `node_modules/openai/{client.js,core/error.js,internal/parse.js,internal/errors.js}`
  - `lib/llm/fallback/engine.ts`
  - `harness/oracle/providers/record-stream-fixtures.mjs` (the bridge's origin).

## Cross-lane adjacencies / risks

- **P4.140** (Salon spine) edits `orchestrator.rs`, `carina_query.rs` and the Option-V inputs. This lane touches none of them, and `primary_stream.rs` is read-only for both lanes. P4.140's Carina `CHAT_MESSAGE` row will likely run `primary_stream_tier3` / orchestrator families. This lane edits `primary_stream_tier3_equivalence.rs` for `TransportError` literals only, so the hunks are disjoint, but **name it in the ownership table** so the unifier expects a two-lane file.
- **P4.139** (API-key class) edits `generators/optimizer.rs` (`:1991-1993`) and `api/settings.rs`. This lane only CALLS `optimizer::v8_json_parse_message` — do not re-home it this round. P4.139's keyed-recorder pin touches the composition that `WireCompletionProvider` serves, but not `completion_provider.rs`.
- **P4.144** reads `cheap_llm_exec.rs` (`:465-530`, `:1240-1295`) and `model/completion.rs:570` (`CannedCompletionProvider::with_failure`) without editing them. This lane edits `cheap_llm_exec.rs:1393-1410` and adds a field to `CompletionError`. If `with_failure` builds `CompletionError` by struct literal rather than `::new`, P4.144's fixture code would break. Measured: `CompletionError` is built only via `new` (no literal outside `completion.rs`), so the risk is low. Keep the field `Option` + `new()`-defaulted.
- **The shared-struct trap** (memory: a required field on a shared struct crosses lane ownership): the `TransportError` kind touches ~45 literal sites across 12 files. Convert them to constructors in ONE commit, early, and land it first so siblings rebase onto it.
- **The ruling** (Tier 1 item 3) must be taken at planning. Otherwise the 26 trigger rows become `EXPECTED_DIVERGENCES` pinned both ways.
- **The regen** runs from a pinned worktree (ledger §5.1). Never run two sweeps concurrently. `regenerate-text-errors.sh` is not a sweep family, but its 880 rows include about 20 hang rows at ~100 ms and 14 SDK fetch-throws rows at ~1.3 s (SDK backoff), so allow ~1 minute.

## Versions / bumps

- `quilltap-core` (patch bump: transport kind, catch shapes, shape guard).
- `quilltap-harness` (patch bump: the family + literal conversions).
- `quilltap-host` / `quilltap-web` / `quilltap-cli` / `quilltap-tauri` unchanged (`ReqwestTransport` is core's, behind `native-transport`).
- SPA unchanged.
- `quilltap-sqlite3mc-sys` never.
