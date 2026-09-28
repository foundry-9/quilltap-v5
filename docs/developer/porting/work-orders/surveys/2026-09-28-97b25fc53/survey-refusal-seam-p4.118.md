# Survey — the TEXT-side structured refusal seam (P4.D225 follow-up, a v5 gap)

Dated **2026-09-28**. READ-ONLY survey. v4 read at the oracle baseline
`acadcc7cd` (checkout at `97b25fc53`, clean). `git log acadcc7cd..97b25fc53 --
lib/services/dangerous-content lib/services/chat-message lib/llm plugins`
returns ONE commit, `c3eefa752` ("Teach built-in character prompts to listen
and match register": the default-system-prompts plugin plus the prompt seeder).
It touches no refusal or failover code, so every v4 citation below holds at
HEAD too. v5 is `main` at `ef29a058e`.

The gap as recorded, confirmed: `StreamError::with_refusal`
(`model/stream.rs:368`) has **exactly one caller, and it is in the harness**
(`primary_stream_tier3_equivalence.rs:500`). `FallbackError::with_refusal`
(`llm_fallback/types.rs:333`) has **no caller at all**. No production text
path ever attaches a `RefusalError`.

---

## A. v4 at `acadcc7cd` — how a text-provider error reaches `provider-code`

### A.1 The classifier (`lib/services/dangerous-content/refusal.ts`)

- `PROVIDER_MODERATION_CODES` (:70) is
  `{'moderation_blocked','content_policy_violation','content_filter','safety','1301'}`.
  The doc comment gives provenance: OpenAI Images; `content_filter` for
  "OpenAI / Azure"; `safety` for Google; `1301` for Z.AI. Codes are compared
  **lower-cased**.
- `codeString(v)` (:109) returns a non-empty string as itself and a finite
  number as `String(v)`. Anything else gives `null`.
- `collectCodes(error)` (:116) reads, in order: (1) `record.code` and
  (2) `asRecord(record.error)?.code`, both through `codeString`. It reads
  nothing else: not `status`, `type`, `param`, `error.error.code`,
  `error.type`, or `innererror`.
- `messageOf(error)` (:128): an `Error` gives `.message`, a string gives
  itself, an object with a string `.message` gives that, anything else `''`.
- `classify` (:136), first hit wins:
  1. `record.code === 'MODERATION_REJECTED' || record.name === 'ModerationRejectionError'`
     gives `typed-error`.
  2. The first collected code whose lower-case form is in the set gives
     `provider-code` with detail `truncate(\`code ${code}${message ? \`: ${message}\` : ''}\`)`.
     The detail keeps the code's original case.
  3. `isModerationFinishReason(finishReason)` gives `finish-reason`.
  4. The lower-cased message contains one of the eleven
     `REFUSAL_MESSAGE_PATTERNS` → `message-pattern`. Note that
     `content_filter` is **not** among the patterns, and neither is Azure's
     wording ("content management policy") or "ResponsibleAIPolicyViolation"
     (the list has `responsible ai` with a space).
  5. `emptyBody && contentWasFlagged` gives `inferred`.
- So a benign-worded `content_filter` 400 can only be caught by evidence 2.
  Without the code the chain falls to `engine.ts`'s `/\b4\d\d\b/`, which
  returns `null` and means **no failover at all**.

### A.2 Where the caught error enters

- `lib/llm/fallback/engine.ts:100` `classifyFallbackTrigger(error)`. Its
  **first** line (:105) is `if (classifyRefusal({ error }).refused) return 'moderation-refusal'`.
  Only after that come the non-triggers, the typed ladder, the name tests,
  the network/provider patterns, and the auth/429/model-missing rules. Then
  `/\b4\d\d\b/` returns `null` (:141) and the remainder is `'provider-error'`.
- `primary-stream.service.ts:397`: the primary stream's catch calls
  `attemptHardErrorFailover({ state, error: streamingError, … })`. It does so
  after the tool-unsupported retry and the `isRecoverableRequestError`
  branches. The raw thrown object is passed through untouched.
- `provider-failover.service.ts:954` `attemptHardErrorFailover`:
  - `trigger = classifyFallbackTrigger(error)`; a `null` trigger returns
    "not fallback-eligible".
  - It is skipped when `state.hasStartedStreaming`.
  - `:993` is the `trigger === 'moderation-refusal'` branch. It calls
    `classifyRefusal({ error })` (:1003) and then
    `recordRouteFailure(…, 'refused', trigger, refusal.detail ?? failureMessage, refusal.evidence)`.
    Next it reads the Locked gate (`readCurrentConciergeState`), which on
    Locked posts `refusal-not-permitted` and walks the chain.
  - If `failoverAllowed && on-duty`, it calls `attemptUncensoredRetry(…
    refusalWasStated: true, substitute: true)`. On failure it walks the
    chain, cleared for the content.
  - Other callers: `:550` (the uncensored retry's own catch), `:852` (the
    understudy's catch through `classifyFallbackTrigger`), and `:269` (the
    empty-response same-provider retry).
- `lib/memory/cheap-llm-tasks/core-execution.ts:712`: the cheap path's
  stand-in chain opens with `classifyFallbackTrigger(error)`. A refusal
  gives `moderation-refusal`, then `buildCheapFallbackSelections({ dangerous: … })`.
  An uncoded 400 gives `null` and no stand-in. `:581` classifies only a
  stated finish reason on an empty body.
- `lib/services/chat-message/streaming.service.ts:375` (the `streamMessage`
  wrapper plus the watchdog) passes thrown errors through unchanged. The two
  tool loops (`native-tool-loop.service.ts:340/421`,
  `text-tool-loop.service.ts:390`) **rethrow** their stream errors.
  **`attemptHardErrorFailover` is called only from the primary stream.**

### A.3 The provider plugins (ten LLM providers under `plugins/dist/qtap-plugin-*/`; `provider.ts` is the source, `index.js` the bundle)

**No plugin re-wraps an SDK error.** Every text-path `catch` logs and then
`throw error`: deepseek :298, google :677/:882, nanogpt :337, ollama
:237/:485, openrouter :437/:955, and the shared base
`packages/plugin-utils/src/providers/openai-compatible.ts:466/:579`. The only
code-dropping throws are the openai/grok **200-with-`response.error`** paths
(`openai/provider.ts:498/:526`, `grok/provider.ts:428`). Those throw
`new Error(\`OpenAI API error: ${response.error.message}\`)`, which is a
200-status body, not an HTTP error.

| v4 provider | text transport | non-2xx becomes | `record.code` | `record.error.code` | can reach `provider-code`? |
|---|---|---|---|---|---|
| OPENAI | `openai` SDK 7.23.0 `responses.create` | `APIError` via `makeStatusError` | `body.error.code` | same value (`APIError.error` = `body.error`) | **YES** (`content_filter`, `content_policy_violation`, …) |
| OPENAI_COMPATIBLE (incl. Azure) | SDK `chat.completions.create` (base class) | `APIError` | `body.error.code` | same | **YES**: Azure's `{"error":{"code":"content_filter",…,"innererror":{…}}}` |
| DEEPSEEK | SDK (base class) | `APIError` | `body.error.code` | same | yes structurally (DeepSeek's own "Content Exists Risk" code is `invalid_request_error`, so no in practice) |
| NANOGPT | SDK (base class) | `APIError` | `body.error.code` | same | yes structurally |
| Z_AI | SDK `chat.completions.create` | `APIError` | `body.error.code` (`"1301"`) | same | **YES**: the second real failure case |
| GROK | SDK `responses.create` | `APIError` | xAI's body is `{"error":"<string>"}`, so `APIError.error` is a string and `code` is undefined | none | NO (message `400 "…"`) |
| ANTHROPIC | `@anthropic-ai/sdk` 0.115.0 | `APIError` (`generate` passes the **whole body** as `error`; there is **no `code` field**) | undefined | `body.code` (absent on real bodies) | NO in practice |
| GOOGLE | `@google/genai` 1.52.0 | `ApiError { name:'ApiError', status, message: JSON.stringify(body) }` (`throwErrorIfNotOK`) | none | none | NO (message-pattern only) |
| OPENROUTER | SDK `chat.send` (no tools/images) or raw `fetch` (tools/images) | SDK: `OpenRouterError` subclasses with `.error = {code:<HTTP number>, message, metadata}`. Fetch: `new Error(\`OpenRouter API error: ${status} - ${text}\`)` | none | a numeric HTTP code (`"403"`) | NO |
| OLLAMA | raw `fetch` | `new Error(\`Ollama API error: ${status} ${errorText}\`)` | none | none | NO |

### A.4 The SDK error shapes (cited from the installed type definitions)

- **openai 7.23.0** (`node_modules/openai/core/error.d.ts`, the same
  `APIError` in `src/core/error.ts`): it has `readonly status`, `headers`,
  `error` ("JSON body of the response"; in fact `body.error`, see below),
  `code: string|null|undefined`, `param`, `type`, and `requestID`. The
  constructor does `this.error = error; this.code = error?.code; this.param
  = error?.param; this.type = error?.type`.
  - `generate(status, errorResponse, …)` passes `errorResponse.error`.
  - `client.ts:864 makeStatusError` **normalizes first**: a JSON body whose
    `.error == null` becomes `{ error: body }`. A flat
    `{"code":"content_filter","message":"x"}` body therefore still yields
    `code: 'content_filter'` and the message `400 x`.
  - A non-JSON body gives `error: undefined` and the message
    `${status} ${text}`. An empty body gives `${status} status code (no body)`.
  - `makeMessage` produces `${status} ${error.message}`, or
    `JSON.stringify(error.message)` when that is not a string, or
    `JSON.stringify(error)` when there is no `.message`.
  - Subclasses: `BadRequestError` 400, `AuthenticationError` 401,
    `PermissionDeniedError` 403, `NotFoundError` 404, `ConflictError` 409,
    `UnprocessableEntityError` 422, `RateLimitError` 429,
    `InternalServerError`.
- **@anthropic-ai/sdk 0.115.0** (`plugins/dist/qtap-plugin-anthropic/node_modules/@anthropic-ai/sdk/src/core/error.ts`):
  `status`, `headers`, `error` (the **whole body**), `requestID`, and
  `type: ErrorType|null` (= `body.error.type`). There is **no `code`**, and
  `makeStatusError` does not normalize. The message is `${status}
  ${JSON.stringify(body)}`, because the body has no top-level `message`.
- **@google/genai 1.52.0** (`dist/genai.d.ts:335`): `class ApiError extends
  Error { status: number }`, with `ApiErrorInfo { message, status }` only.
  `throwErrorIfNotOK` (`dist/node/index.mjs:13465`) sets the message to
  `JSON.stringify(body)`. When the content-type is not JSON it synthesizes
  `{error:{message:text, code:status, status:statusText}}`.
- **@openrouter/sdk 1.3.28** (`esm/models/errors/openroutererror.d.ts`):
  `OpenRouterError { statusCode, body, headers, contentType, rawResponse }`.
  `BadRequestResponseError.error: { code: number; message; metadata? }`.
  The message is `err.error?.message || \`API error occurred: …\``.

### A.5 Does the streaming path throw the same `APIError`?

- **Stream creation: yes, identically.** `responses.create({stream:true})` or
  `chat.completions.create({stream:true})` reaches the same `makeRequest`,
  and a non-2xx at the headers throws `makeStatusError(status, errJSON, …)`
  (`client.ts:1846`) before any `Stream` exists. It carries the same `status`
  and `code`. Anthropic is the same (`client.ts:1192`), and so is Google
  (`throwErrorIfNotOK`).
- **Mid-stream error frames: a different constructor, with status
  `undefined`.** In openai `core/streaming.ts:149-154`, `event: error` throws
  `new APIError(undefined, data?.error ?? data, undefined, headers)`, and any
  `data` that has `.error` throws `new APIError(undefined, data.error, …)`.
  The `code` is still read (`data.error.code`, or `data.code` for the
  Responses `{type:'error',code,message}` event). Anthropic
  `streaming.ts:139` builds `APIError(undefined, body, …, type)` with no
  code. Google `index.mjs:13240` builds an `ApiError` with
  `status: errorJson.code`.
  - v4 routes a mid-stream throw **before the first content chunk** into
    `attemptHardErrorFailover`, because `hasStartedStreaming` is false.
- The ready-made v4 test shapes (all at `acadcc7cd`):
  - `__tests__/unit/lib/services/dangerous-content/refusal.test.ts:25-34,79-88`
    covers provider-code rows for `openAIError(code, msg)`, a nested-only
    `error.code`, Z.AI numeric `1301` and string `'1301'`, and
    `content_filter` as a finish reason.
  - `__tests__/unit/lib/services/chat-message/provider-failover-refusal.test.ts:121-150`
    has
    `policyError = Object.assign(new Error('400 Your request was rejected as a result of our safety system.'), { status: 400, code: 'content_policy_violation' })`.
    The expected trail is
    `['Primary','primary','refused','moderation-refusal','provider-code']`
    followed by `['Frank Desk','concierge','answered',…]`. Six more `it`s
    cover no-understudy, off-duty, not-permitted and Locked.
  - `__tests__/unit/lib/services/chat-message/provider-failover-chain.test.ts:391-413`
    covers a `content_filter` finish reason.
  - `__tests__/unit/plugins/concierge-moderation-rejections.test.ts:177-193`
    covers an incomplete `content_filter` on OpenAI and Grok.
  - **No v4 test poses a `content_filter` HTTP error through a real plugin.**
    v4's own tests pose the error object at the seam, so the wire proof is
    this port's to build.

---

## B. v5 on `main`

### B.1 The structured side (it exists, but nothing fills it on text)

- `model/stream.rs:343` `StreamError { message, kind, refusal: Option<Box<RefusalError>> }`.
  `new()` sets `refusal: None`. `:368`
  `pub fn with_refusal(mut self, refusal: RefusalError) -> Self` boxes the
  refusal into `self.refusal`.
- `llm_fallback/types.rs:320` `FallbackError::from_stream_error(&e)` threads
  `e.refusal.as_deref()` (this is correct, but it only ever sees `None`).
  `:333` `with_refusal(self, &RefusalError)` has **zero callers**.
- `services/dangerous_content/refusal.rs:107` `RefusalError { message, code,
  nested_code, name, provider_reason, status }` provides:
  - `message_only`;
  - `typed(…)`;
  - `from_record(message, &Map)`, v4's `collectCodes` over a JSON record
    (`code`, then `error.code`), plus `name`, `providerReason` and `status`;
  - `is_typed_refusal`.
- `:271` `classify` follows the evidence order exactly. Step 2 is
  `[code, nested_code].find(|c| SET.contains(c.to_lowercase()))`, with the
  detail `code {code}[: {message}]`. `:354 classify_refusal` adds the DEBUG
  and INFO lines (`quilltap::concierge_refusal`).
- `llm_fallback/engine.rs:107` `classify_fallback_refusal`: when
  `error.refusal` is `None` it **synthesizes**
  `RefusalError { message: error.message, name: error.name }`. So today
  every text error is classified on the message alone. `:149`
  `classify_fallback_trigger` runs that refusal check first, as v4 does.

### B.2 Where a text HTTP error becomes a `StreamError` or `CompletionError` today

All text providers share **one** HTTP boundary. **There is no per-provider
text error rendering in v5.**

- `model/transport.rs:209` `TransportError { message, status: Option<u16> }`
  (no body field, no headers). The only production implementation is
  `ReqwestTransport` (`quilltap-host/src/spine.rs:280`,
  `quilltap-host/src/providers.rs:242/252`). It renders **every** non-2xx
  as `format!("HTTP {status}: {text}")` with the raw body, at
  `transport.rs:326` (non-streaming) and `:376` (streaming).
- `model/streaming_provider.rs` (the `WireStreamingProvider` shared by all
  ten providers) carries it as a message only, through
  `single_error(message: String)` (:315):
  - `:378`: the prepare error, not HTTP.
  - **`:403`**: the Ollama think-retry's second failure.
  - **`:412`**: the plain pre-stream HTTP failure, which is the main site.
  - `:420`: the fallback prepare, not HTTP.
  - **`:440`**: the chained-fallback retry's failure (v4 surfaces the second
    error).
  - Mid-stream: `:475` and `:496` `StreamError::new(DecodeError.message)`,
    and `:480` a transport chunk error (not HTTP).
- `model/completion_provider.rs` (non-streaming, the cheap-LLM path):
  **`:235`** the think-retry second failure and **`:237`** the plain failure,
  both `CompletionError::new(e.message)`. `model/completion.rs:206`
  `CompletionError { message }` has **no refusal field**. It has zero struct
  literals (16 `::new` sites), so adding a defaulted field is cheap.
- `services/cheap_llm_exec.rs:1333-1340` builds
  `FallbackError::message(&error.message)`, or `named("CheapLLMTimeoutError")`
  for a deadline. Nothing structured is carried.
- `services/primary_stream.rs:1574` passes
  `FallbackError::from_stream_error(&err)` to the walk, which is correct and
  inert. `:1608` **rebuilds** the error as
  `StreamError::new(format!("{} ({summary})", err.message))`, which drops
  `refusal` (and `kind`). v4 rewrites `.message` in place and keeps the rest.
- **Premise check, and a correction to the order's premise:**
  - The brief says "v5 matched v4's error MESSAGE bytes per provider in
    earlier rounds". **For text HTTP errors that is not so.** No
    `transport_errors`, `provider_errors`, `error_sentence` or
    `tool_error_sentence` family exists: a grep of `crates/quilltap-harness/tests`
    and `crates/quilltap-core/src` finds nothing.
  - The stream recorder (`harness/oracle/providers/record-stream-fixtures.mjs:144-172`)
    hard-codes `status: 200`, so no text corpus has ever posed a non-2xx.
  - v5 renders `HTTP 400: {"error":{…raw…}}` where v4 renders `400 <error.message>`
    (SDK), `Ollama API error: 400 <text>`,
    `OpenRouter API error: 400 - <text>`, or `JSON.stringify(body)`
    (Google).
  - The only v4-faithful error re-rendering in v5 is the **image** helper
    `model/image_dialects.rs:1404 openai_sdk_error` (plus `:1105` for
    OpenRouter images).
  - What pins v5's **own** text rendering (for neutrality):
    `streaming_provider.rs` unit tests `:1046 pre_stream_failure_is_a_single_error`
    (`"HTTP 401: unauthorized"`), `:1124`, `:1385`, `:1455-1466`;
    `completion_provider.rs:891-973`;
    `harness/tests/ollama_think_retry_tier3_equivalence.rs:142/163`;
    `tool_wire_call_site.rs:600`; `primary_stream_tier3_equivalence.rs:1745`
    (the chaining arm, `"400 previous_response_not_found"`).
- **Mid-stream error frames are silently dropped by three v5 decoders.**
  `decoders/chat_completions_sse.rs:544-567` parses and passes to
  `handle_chunk` with no `error` check. `decoders/responses_api_sse.rs:106-132`
  has no `"error"` or `response.failed` arm. `ollama_ndjson.rs` has none
  either, which matches v4, since Ollama ignores them too.
  - The Anthropic decoder (`anthropic_sse.rs:114-119`) turns an error frame
    into `DecodeError(ev.to_string())`, which matches v4's
    `JSON.stringify(body)` (no code on either side).
  - Google (`google_parts.rs:289-292`) builds `got status: …`, which matches
    v4 (no code).

### B.3 What P4.D225 did wire (so the order can say what exists)

- **Image dialects** (`model/image_dialects.rs`):
  - `openai_sdk_error` (:1404) builds `RefusalError { message: "<status> <msg>", code = nested_code = body.error.code, status }`.
  - `map_sdk_image_moderation` (:~1450) handles OpenAI, Grok and Z.AI typing.
  - Also: Google's `is_google_safety_message`, the three throw sites, and
    Gemini's new message; Imagen's filtered 200; OpenRouter's refusal-body
    and declined. All of these set `ImageGenError.refusal` directly (struct
    field or `ImageGenError::moderation`, `model/image.rs:197`/`:217`).
    **None calls `with_refusal`**, which confirms the unification's finding.
  - Consumed by `services/dangerous_content/image_failover.rs:257/555`.
- **Text finish reasons:**
  - `response_parse.rs:769-772` `responses_has_refusal`.
  - `decoders/responses_api_sse.rs:124-177` treats `response.incomplete` as
    terminal and uses the real reason.
  - `decoders/chat_completions_sse.rs:134/434` (OpenRouterRaw
    `finish_reason` + `finishReason`).
  - `decoders/google_parts.rs:48-122/336` (`withBlockReason`).
  - `completion_provider.rs:253/775` (Google's blocked-prompt WARN and
    `blockReason`).
  - `extract_finish_reason`'s camelCase and `promptFeedback.blockReason` arms.
- **Classifier consumers:** `route_trail.rs:463` (`classify_empty_body`),
  `provider_failover.rs:929/1568/1622`, `cheap_llm_exec.rs:1103/1340`, and
  `model/image.rs:570`.

### B.4 The harness arm (`primary_stream_tier3_equivalence.rs:499-507`)

`chunk_to_result` handles a corpus chunk's `typed_refusal` by returning
`Err(StreamError::new(t.message).with_refusal(RefusalError::typed(t.message, t.status_code, t.provider_reason)))`.
The jest side is `harness/oracle/cases/primary-stream-tier3.test.ts:107-110,444-455`,
which `require`s the plugin contract's **real** `ModerationRejectionError`
and throws it at the mocked `streamMessage` seam. It poses **typed-error**
evidence at the seam only. It never exercises `provider-code`, and it never
passes through `WireStreamingProvider`.

---

## C. Seam proposal

### C.1 One reconstruction home, all text HTTP sites through it

- A NEW pure function, placed in a new `model/provider_error.rs` (or next to
  `ProviderKind` in `model/provider_io.rs`):
  `fn text_http_refusal(provider: &str, status: u16, body: &str) -> Option<RefusalError>`.
  It is keyed by `ProviderKind::of(provider)`.
- It rebuilds **the fields v4's thrown value carries**, and its `message` is
  **v4's rendering** of that value:

  | ProviderKind | reconstruction (message = v4 `messageOf(thrown)`) |
  |---|---|
  | OpenAi, OpenAiCompatible, DeepSeek, NanoGpt, ZAi, Grok (openai SDK) | `errJSON = safeJSON(body)`, then `makeStatusError`'s normalization (`errJSON.error == null` means `{error: errJSON}`), then `inner = norm.error`. `message = makeMessage(status, inner, errJSON ? undefined : body)`. `code = nested_code = code_string(inner.code)` when `inner` is an object. `status`. Grok's string `error` gives the message `400 "…"` and no code. |
  | Anthropic | `error = errJSON` (whole body). `message = makeMessage(status, errJSON, errJSON ? undefined : body)`. `code = None`. `nested_code = code_string(errJSON.code)`. `status`. |
  | Google | `name = "ApiError"`, `status`, `message = JSON.stringify(parsed body)`. Needs the content-type for v4's synthesized-body branch, see D.5. Code `None`. |
  | OpenRouter | v5 streams on v4's **fetch** path (`OpenRouterRaw`; the SDK path is a standing deliberate divergence, `streaming_provider.rs:49-53`). `message = "OpenRouter API error: {status} - {body}"`. No code. |
  | Ollama | `message = "Ollama API error: {status} {body}"`. No code. |

- Wire it at `streaming_provider.rs:403/412/440` (swap `single_error(e.message)`
  for a `single_error_from(provider, e)` that does
  `StreamError::new(e.message).with_refusal(r)` when `status.is_some()`) and
  at `completion_provider.rs:235/237`. The latter needs a `refusal` side on
  `CompletionError` (defaulted in `new`) that `cheap_llm_exec.rs:1338`
  threads through as `FallbackError::message(..).with_refusal(r)`, the first
  production caller of that method.
- Fix `primary_stream.rs:1608` so it mutates `err.message` in place (or
  carries `refusal`/`kind`), as v4 does.
- **The message bytes do not change.** `StreamError.message` and
  `CompletionError.message` stay `HTTP {status}: {raw}`. Only the side
  carries v4's rendering. Consequences:
  - The refused trail `detail` becomes v4's exact bytes, for example
    `code content_filter: 400 The response was filtered…`.
  - Step 4 (message-pattern) now runs over v4's rendering rather than v5's
    raw body. That moves toward v4. Over the raw JSON, v5 could hit a
    pattern inside a non-message field, such as `"code":"content_policy_violation"`
    matching the pattern `content_policy`, which would record
    `message-pattern` where v4 records `provider-code`. Code wins first
    anyway, but the detail bytes would differ.
  - Everything after step 1 of `classify_fallback_trigger` (the 4xx and
    network ladder), the failover WARN's `error`, `preserve_partial`, and
    the user-facing error keep reading the unchanged message.
- **Getting the body:** `TransportError` has no body field.
  - (a) Zero-churn: `TransportError::http_body(&self) -> Option<&str>`,
    which strips `format!("HTTP {status}: ")` when `status.is_some()`. Both
    `ReqwestTransport` producers and every harness fake that sets a
    `status` use exactly this format; pin it with a unit test.
  - (b) Cleaner: add `body`/`content_type` fields. That means **38 struct
    literals across 9 files**: 4 in core (`transport.rs`,
    `streaming_provider.rs`, `completion_provider.rs`, `ollama_think_retry.rs`)
    and 5 in the harness (`streaming_composer`, `primary_stream_tier3`,
    `tool_wire_call_site`, `ollama_think_retry_tier3`,
    `provider_header_common/mod.rs`), which triggers the
    required-field-on-a-shared-struct rule.
  - Recommend (a), unless Google's content-type branch is taken in scope.

### C.2 Families

- **Neutrality (must stay green, byte-unchanged):** the message-passthrough
  pins in B.2, plus `fallback_engine_equivalence` (`QT_ORACLE_FALLBACK_ENGINE`,
  `harness/oracle/cases/fallback-engine.ts`; builds `RefusalError` via
  `from_record`, so it is not on this path). Also:
  - `refusal_classify_equivalence` (`QT_ORACLE_REFUSAL_CLASSIFY`,
    `cases/refusal-classify.test.ts`, 74 rows: the classifier is unchanged);
  - `stream_decoders_equivalence`;
  - `ollama_think_retry_tier3_equivalence`;
  - `primary_stream_tier3_equivalence` (both oracles: `QT_ORACLE_PRIMARY_STREAM`
    and `QT_ORACLE_OPENAI_FALLBACK`);
  - `cheap_llm_fallback_equivalence` (`QT_ORACLE_CHEAP_FALLBACK` /
    `QT_ORACLE_CHEAP_REFUSAL`).
- **The classifier already sees the code once attached.**
  `refusal_classify_equivalence` has 13 provider-code rows, including the
  nested-only row, numeric and string `1301`, and mixed case. Tier 1 needs
  no new classifier rows. It may add a `content_filter`-with-Azure-message
  row as a readability pin.
- **NEW: the production-seam proof** (a tier-1-style wire family, provisionally
  `text_http_errors_equivalence`):
  - Oracle: a new `harness/oracle/providers/record-text-errors.mjs` plus a
    `regenerate-…sh`. It is a copy of `record-stream-fixtures.mjs` whose
    `wireToResponse` takes `{status, body, contentType}`, combined with
    `record-image-fixtures.mjs:54-69`'s `thrownFields(e)` recorder
    (`{message, code?, errorCode?, providerReason?, status?, name?}`).
  - It drives each of the ten real plugins' `streamMessage` **and**
    `sendMessage` from the plugin dirs at the pin, against posed 4xx bodies:
    - OpenAI `{"error":{"message":"Filtered.","type":"invalid_request_error","code":"content_filter"}}`;
    - the full Azure body through OPENAI_COMPATIBLE (`param:"prompt"`,
      `status:400`, `innererror.code:"ResponsibleAIPolicyViolation"`,
      message "…content management policy…");
    - Z.AI `{"error":{"code":"1301",…}}`;
    - a flat `{"code":"content_filter","message":…}` (the `makeStatusError`
      wrap);
    - Grok's string error; Anthropic's typed body; Google's JSON body;
      OpenRouter; Ollama;
    - benign 400s with a null code; a non-JSON body; an empty body.
  - It also records `classifyRefusal({error})` and `classifyFallbackTrigger(error)`
    per row.
  - Rust side: a scripted transport returning `TransportError { status,
    message: "HTTP s: body" }` through `WireStreamingProvider::stream_message`
    and `execute_completion`. It compares `StreamError.refusal` against
    `thrown`, and runs the same two classifications.
  - Red-first on main: every provider-code row comes back
    `trigger: null` / `refused: false`.
- **Downstream reroute:** a `primary_stream_tier3` arm.
  - Add a chunk key, for example
    `sdkError: { message, status?, code?, error?: {code} }`. Jest throws the
    **real** `APIError` from the openai SDK via `require` (as `typedRefusal`
    requires the real `ModerationRejectionError`), or
    `Object.assign(new Error(message), {status, code, error})`, as v4's own
    `policyError` does.
  - The Rust side builds `StreamError::new(…).with_refusal(RefusalError{…})`
    from the same fields.
  - Pose `400 The response was filtered…` with `code: 'content_filter'` on an
    OPENAI profile. Expect v4's trail:
    `['Primary','primary','refused','moderation-refusal','provider-code']`
    and then the uncensored `answered` row, plus the ledger/`recordTextRefusal`
    rows.
  - Add a twin with no code, which must NOT reroute (`not fallback-eligible`).
  - This is seam-posed. The wire family above is the proof that production
    fills the field.
- **Cheap path:** a `cheap_llm_fallback_equivalence` arm that poses a
  `throwsRecord` with `code` (today `:346` poses only a string), expecting
  v4's `moderation-refusal` stand-in retry. Its oracle is
  `cases/cheap-llm-fallback.test.ts`.

---

## D. Traps

1. **Cheap vs Salon:**
   - The cheap path is non-streaming (`CompletionError`, `completion_provider.rs:235/237`),
     and `cheap_llm_exec.rs:1338` builds the `FallbackError` from the
     message alone.
   - It needs its own carry. Wiring only `streaming_provider.rs` leaves
     core-execution's `moderation-refusal` stand-in unreachable for coded
     400s.
   - Also note the P4.D225 NIT: `record_cheap_refusal` gates on the log
     config where v4 gates on `chatId`.
2. **Tool loops:**
   - v4's tool loops rethrow, and `attemptHardErrorFailover` runs only from
     `primary-stream.service.ts:397`. A tool-loop re-stream's refusal
     **never reroutes in v4**, and it should not in v5.
   - Attaching the side is neutral there, provided no new classify call is
     added. This is the recorded E.11 asymmetry. Record it and do not "fix"
     it.
3. **Azure vs OpenAI:**
   - Azure's body nests `innererror` and carries `status` inside `error`.
     v4 reads only `error.code`, never `innererror.code` or `error.status`.
   - Azure's wording ("content management policy") matches **no** pattern,
     so the code is the only evidence.
   - OpenAI proper usually signals text moderation as `finish_reason:
     content_filter` (already wired) or HTTP 400 `invalid_prompt`, which is
     **not** in v4's set: parity, no reroute on either side.
   - Azure reaches v4 through OPENAI_COMPATIBLE, since there is no Azure
     plugin.
4. **Anthropic:**
   - `APIError.error` is the WHOLE body and there is no `.code`.
     `record.error.code` reads `body.code`, not `body.error.type`.
   - `type` (`error.error.type`) is never read by the classifier, so
     Anthropic never gives `provider-code` on real bodies. Do not invent a
     mapping from `error.type`.
5. **Google:**
   - `ApiError` is `{status, message}` only. The message is
     `JSON.stringify(body)`, and a non-JSON content-type gives a
     **synthesized** `{error:{message:text,code:status,status:statusText}}`.
   - v5's `TransportError` lacks the content-type and statusText. Either
     leave Google message-only (as today, since no code is possible) or take
     option (b). Pattern hits on Google text differ only in the detail bytes.
6. **The `makeStatusError` normalization:**
   - A body without `.error` is wrapped `{error: body}` (openai 7.23.0,
     `src/client.ts:864`). The **image** helper `openai_sdk_error`
     (`image_dialects.rs:1404-1432`) does NOT do this: for a flat body it
     takes `resp.body` as the message and gives no code.
   - The text reconstruction must do it, and the lane should measure whether
     the image helper is also wrong on a flat-body row, possibly as a finding
     for image-dialects.
   - An empty body gives `"<status> status code (no body)"` on v4, where
     the image helper gives `"<status> "`.
7. **Grok:** xAI's `{"error":"<string>"}` gives the message `400 "<string>"`
   (JSON-quoted, dogfood #104) and no code.
8. **OpenRouter:**
   - The SDK path's `error.code` is the numeric HTTP status (`"403"`), which
     is never a moderation code.
   - Surprise, outside this scope: v4's SDK-path message is `err.error.message`
     with **no status digits**, so `/\b4\d\d\b/` misses and v4 classifies an
     OpenRouter 403 moderation as `provider-error` **and fails over**. v5's
     `HTTP 403: …` hits the 4xx rule and does **not** fail over.
   - v5 has no SDK path (a deliberate divergence), so this is a message-bytes
     divergence to rule on separately.
9. **Plugins never re-wrap:** every text `catch` rethrows the original. The
   only code-dropping throws are the 200-status `response.error` paths
   (openai :498/:526, grok :428), which are plain Errors on v4 too. Parity.
10. **Mid-stream error frames (a separate pre-existing gap, name it):**
    - v4's SDK throws a coded `APIError` for Chat Completions `data:
      {"error":…}` and for a Responses `event: error`. Before the first chunk
      this reaches the hard-error failover.
    - v5's chat-completions and Responses decoders drop those frames, so the
      turn ends empty and takes the **empty-response** path instead.
    - Fixing this means new decoder arms (`DecodeError` → `StreamError` at
      `streaming_provider.rs:475/496` would then need the refusal too) plus
      stream wires with error frames, and the stream recorder records only
      `e.message` (widen it to `thrownFields`).
    - Recommend Tier 2, or a named follow-up. Keep it out of Tier 1 to hold
      the lane small.
11. **`primary_stream.rs:1608` drops the side** when it appends the
    understudy summary. Nothing downstream re-classifies today, but carry it.
12. **The classifier's DEBUG/INFO lines** fire on every classification,
    including the new wire family. Scope them out by target, as the
    P4.D225 families do, or pin them.
13. **`TransportError` is not `Default`**, which matters if option (b) is
    chosen. Also note that `ollama_think_retry::think_retry_request` reads
    `error.status` and `error.message`: keep both untouched.

---

## E. Size, ownership, tier

- **Size:**
  - Small to medium, roughly 250–400 lines of core plus about 600 lines of
    harness and oracle.
  - Core: one new pure module (the ten-way reconstruction plus about 20
    unit pins); four `streaming_provider.rs` sites; `CompletionError.refusal`
    and two `completion_provider.rs` sites; one `cheap_llm_exec.rs` line;
    `primary_stream.rs:1608`.
  - Harness: the new recorder and regen script, the committed corpus, the
    new family, one `primary_stream_tier3` chunk key plus 2–3 arms (oracle
    case and Rust), and one `cheap_llm_fallback` arm.
  - Tier 2 (mid-stream frames) adds two decoder arms plus a stream-corpus
    re-record.
- **Files a lane would own:**
  - core: `crates/quilltap-core/src/model/{provider_error.rs (new),
    streaming_provider.rs, completion.rs, completion_provider.rs,
    transport.rs (only if option (b))}`,
    `crates/quilltap-core/src/services/{cheap_llm_exec.rs, primary_stream.rs}`,
    and `mod.rs` for the new module. Tier 2 adds
    `model/decoders/{chat_completions_sse.rs, responses_api_sse.rs}`.
  - harness: `crates/quilltap-harness/tests/{text_http_errors_equivalence.rs
    (new), primary_stream_tier3_equivalence.rs, cheap_llm_fallback_equivalence.rs}`
    and `harness/oracle/providers/{record-text-errors.mjs,
    regenerate-text-errors.sh} (new)`, plus
    `harness/oracle/cases/{primary-stream-tier3.test.ts, cheap-llm-fallback.test.ts}`
    and a new fixture dir.
  - Tier 2 adds `record-stream-fixtures.mjs` plus the stream corpora.
  - With option (b), also the five harness files holding `TransportError`
    literals.
- **Disjointness:**
  - No overlap with `tools/generate_image.rs`,
    `services/image_scene_tasks.rs`, `services/story_background_job.rs`,
    `api/chat_cast.rs`, the wardrobe verb, the prompt templates and
    generators, the Post Office tools, `db/chats.rs`, or the
    scriptorium/render/embedding services.
  - The only shared hot spots are `primary_stream_tier3_equivalence.rs` and
    its oracle case, and `primary_stream.rs`. Check that no other lane this
    round touches those.
  - `image_dialects.rs` is **not** touched. Trap 6 is measure-and-report
    only.
- **Recommended agent tier:**
  - The most capable model for the reconstruction module and the recorder.
    The fidelity sits in the SDK rules (`makeStatusError`/`makeMessage`, the
    two code slots, Anthropic's whole-body `error`, Grok's string), and the
    red-first measurement needs judgment.
  - The mechanical wiring (the six call sites, the `CompletionError` field,
    the harness arm plumbing) can go to a cheaper agent under a precise
    brief.

---

## Summary (10 lines)

1. Confirmed: `StreamError::with_refusal` has only a harness caller; `FallbackError::with_refusal` has none; no text path fills `RefusalError`.
2. v4's `provider-code` reads only `record.code` then `record.error.code` against 5 codes; `content_filter` is in the code list but not the pattern list, so a benign-worded refusal is caught by the code alone.
3. Codes reach v4 only via the openai SDK's `APIError` (OPENAI, OPENAI_COMPATIBLE/Azure, Z.AI `1301`, and structurally DeepSeek and NanoGPT); Anthropic, Google, OpenRouter, Ollama and Grok never carry one.
4. Stream creation throws the same coded `APIError` as a non-streaming call. Mid-stream error frames throw it too (status `undefined`), but v5's two OpenAI decoders silently drop those frames, a separate gap.
5. v5 has ONE text HTTP boundary (`ReqwestTransport`, `HTTP {status}: {raw body}`) feeding 6 sites: `streaming_provider.rs:403/412/440` and `completion_provider.rs:235/237`; the cheap path's `CompletionError` has no refusal side.
6. **Surprise:** the "v5 matches v4's text error message bytes" premise is false. No such family exists, the stream recorder hard-codes status 200, and only the image helper re-renders.
7. Proposal: one `text_http_refusal(provider, status, body)` carrying v4's fields and v4-rendered message as the side only, leaving the message bytes unchanged, so the trail detail becomes v4-exact.
8. Proof: a NEW wire family recording v4's real plugins against posed 4xx bodies (built from the stream and image recorders), plus a `primary_stream_tier3` `sdkError` arm and a `cheap_llm_fallback` arm.
9. **Surprise:** the image helper `openai_sdk_error` lacks the SDK's `makeStatusError` `{error: body}` wrap (flat bodies lose their code); separately, v4 fails over an OpenRouter SDK 403 as `provider-error` (no digits in its message) where v5's `HTTP 403:` does not.
10. Size small to medium; files are disjoint from every listed lane (watch `primary_stream_tier3`); use the most capable tier for the reconstruction and recorder, a cheaper one for the wiring.
