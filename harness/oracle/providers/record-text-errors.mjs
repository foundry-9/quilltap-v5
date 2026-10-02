/**
 * Text-provider HTTP-error recorder (P4.118 — the TEXT-side structured
 * refusal seam).
 *
 * Drives each of v4's TEN real text-provider plugins' `streamMessage` AND
 * `sendMessage` against a posed non-2xx response (`global.fetch` mocked, the
 * `record-stream-fixtures.mjs` idiom) and records the value the plugin THREW —
 * as the fields v4's refusal classifier duck-types off it (the
 * `record-image-fixtures.mjs` `thrownFields` recorder, widened with `name`) —
 * plus v4's two verdicts over that same thrown value: `classifyRefusal({
 * error })` and `classifyFallbackTrigger(error)`. Never a hand-built error
 * object: the SDKs build them (`openai` 7.23.0's `makeStatusError` +
 * `APIError.makeMessage`, `@anthropic-ai/sdk` 0.115.0's whole-body `error`,
 * `@google/genai` 1.52.0's `throwErrorIfNotOK`, `@openrouter/sdk` 1.3.28's
 * `OpenRouterError`s, and the two raw-`fetch` plugins' own `Error`s).
 *
 * The Rust differential (`text_http_errors_equivalence`) poses the same
 * `{status, body}` as a `TransportError` through `WireStreamingProvider::
 * stream_message` and `execute_completion`, and diffs the `refusal` side the
 * production reconstruction (`model::provider_error`) attached.
 *
 * Run FROM the v4 checkout ROOT (a pinned worktree — ledger §5.1) under
 * `npx tsx`, so v4's `@/` imports (the classifier) resolve through the root
 * tsconfig; each plugin's bare imports resolve from the plugin FILE's own
 * `node_modules` (the `an-oracle-case-outside-the-v4-tree` rule):
 *
 *   cd <V4-PIN> && npx tsx <V5>/harness/oracle/providers/record-text-errors.mjs \
 *     --v4 <V4-PIN> \
 *     --cases <V5>/harness/oracle/fixtures/text-http-errors/cases.json \
 *     --out <V5>/harness/oracle/fixtures/text-http-errors/text-http-errors.recorded.ndjson
 *
 * `regenerate-text-errors.sh` is the one-shot wrapper.
 *
 * NDJSON line shape (one per case × provider × mode):
 *   { case, provider, mode, status, statusText, contentType, body,
 *     fetchCalls, outcome: 'thrown'|'ok',
 *     thrown: { message, name, code?, errorCode?, providerReason?, status? } | null,
 *     refusal: { refused, evidence?, detail? },
 *     trigger: <FallbackTrigger>|null,
 *     okResult?: { content } | { chunks, content },
 *     pluginErrorLog?: [{ plugin, message, context, error }],
 *     pluginWarnLog?: [{ plugin, message, context }] }
 *
 * `pluginErrorLog` (P4.128): the ERROR-level lines the PLUGIN logged while the
 * row ran, through the host-bridge logger global every plugin's
 * `createPluginLogger` consults (`globalThis.__quilltap_logger_factory` — the
 * PLUGIN-logger hook, the `record-stream-fixtures.mjs` bridge copied
 * verbatim). Present only when the plugin logged at ERROR: the three
 * openai-SDK plugins' `… API error in streamMessage|sendMessage` catch lines
 * (the OpenAI-compatible base, DeepSeek, NanoGPT). `error` is the thrown
 * value's `message` — the SDK's `APIError` text.
 *
 * A case with `"transport": "fetch-throws"` (P4.128) poses a TRANSPORT
 * failure instead of a response: the mocked `fetch` throws
 * `TypeError('fetch failed')` (undici's own shape), so the SDK surfaces its
 * `APIConnectionError` (`Connection error.`) after its retries; `status`,
 * `statusText`, `contentType` and `body` are `null`. `fetchCalls` on those
 * rows is the SDK's retry count (1 + `DEFAULT_MAX_RETRIES` 2) — recorded, and
 * NOT a comparand: the 2026-07-23 provider-I/O ruling (retry counts are the
 * transport's, not the port's contract).
 *
 * A case with `"transport": "hang"` (P4.141) poses a provider that accepts
 * the request and never answers: the mocked `fetch` returns a promise that
 * rejects with the request's `AbortSignal.reason` once that signal aborts —
 * undici's own behaviour (`fetch` rejects with `signal.reason`). The signal
 * is read from `init.signal`, or from the `Request` itself when the SDK hands
 * `fetch` one (`@google/genai`). Paired with a case-level `"requestTimeoutMs"`
 * (threaded into `params.requestTimeoutMs`, which every v4 text plugin honours
 * through plugin-utils' `buildSdkClientOptions` / `buildRequestAbortSignal`,
 * and which caps the SDKs' `maxRetries` at 0), the row records the value the
 * SDK REALLY throws on a deadline — `APIConnectionTimeoutError` (`Request timed
 * out.`), the raw-`fetch` plugins' `TimeoutError` / `AbortError` — in about
 * 100 ms with `fetchCalls: 1`.
 *
 * A case-level `"modes"` object overrides the mode list for the providers it
 * names (`{ "openrouter": ["stream_tools", "send_vision"] }`): the hang and
 * fetch-throws cases skip OpenRouter's two `@openrouter/sdk` modes, whose
 * speakeasy retry loop runs for up to `maxElapsedTime: 3600000` on a
 * connection error (`@openrouter/sdk/esm/lib/retries.js:10`) — the
 * `openrouter-sdk` class v5 never runs.
 *
 * A row whose plugin did NOT throw (`outcome: 'ok'` — the posed 2xx `ok_*`
 * cases, P4.141) carries `okResult`: `{ content }` for a send, `{ chunks,
 * content }` for a stream (the yielded chunk count and their joined
 * `content`). `pluginWarnLog` (P4.141, the WARN half of the bridge — same
 * shape as `pluginErrorLog` without `error`) is present when the plugin logged
 * at WARN.
 *
 * `AbortSignal.timeout`'s timer is unref'd, so a top-level `await` on a
 * hanging `fetch` would let Node exit (`13`, an unsettled top-level await)
 * before the abort fires; a keep-alive interval holds the loop open and is
 * cleared before the corpus is written.
 *
 * `code` / `errorCode` are the RAW values (any JSON type) — the Rust side runs
 * them through its `code_string` (v4's `codeString`) before comparing.
 *
 * Modes: `stream` and `send` for every provider; OPENROUTER adds
 * `stream_tools` (a tool present → v4's raw Chat Completions `fetch` path, the
 * path v5 ALWAYS streams on — `streaming_provider.rs`'s deliberate divergence)
 * and `send_vision` (an image attachment → `sendViaChatCompletions`, the
 * fetch path v4's non-streaming send takes for vision — v5's
 * `openrouter_non_streaming_is_vision`).
 */

import { readFileSync, writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';

// Hold the event loop open across a hanging `fetch` (see the header).
const keepAlive = setInterval(() => {}, 1000);

function parseArgs() {
  const args = process.argv.slice(2);
  const out = {};
  for (let i = 0; i < args.length; i += 2) out[args[i].replace(/^--/, '')] = args[i + 1];
  return out;
}

const args = parseArgs();
const V4 = args.v4;
if (!V4 || !args.cases || !args.out) {
  console.error('usage: --v4 <pin> --cases <cases.json> --out <ndjson>');
  process.exit(1);
}

const plugin = (name, file = 'provider.ts') =>
  import(pathToFileURL(resolve(V4, 'plugins/dist', `qtap-plugin-${name}`, file)).href);

// The canonical-id → plugin factory map (the stream recorder's, by absolute path).
const PROVIDERS = {
  openai: { id: 'OPENAI', make: async () => new (await plugin('openai')).OpenAIProvider() },
  // The SUBCLASS the plugin registers (P4.D83), not the re-exported base.
  'openai-compatible': {
    id: 'OPENAI_COMPATIBLE',
    make: async () =>
      new (await plugin('openai-compatible')).OpenAICompatibleEndpointProvider(
        'http://localhost:8080/v1'
      ),
  },
  deepseek: { id: 'DEEPSEEK', make: async () => new (await plugin('deepseek')).DeepSeekProvider() },
  nanogpt: { id: 'NANOGPT', make: async () => new (await plugin('nanogpt')).NanoGPTProvider() },
  'z-ai': { id: 'Z_AI', make: async () => new (await plugin('z-ai')).ZAIProvider() },
  grok: { id: 'GROK', make: async () => new (await plugin('grok')).GrokProvider() },
  anthropic: { id: 'ANTHROPIC', make: async () => new (await plugin('anthropic')).AnthropicProvider() },
  google: { id: 'GOOGLE', make: async () => new (await plugin('google')).GoogleProvider() },
  openrouter: {
    id: 'OPENROUTER',
    make: async () => new (await plugin('openrouter')).OpenRouterProvider(),
  },
  ollama: {
    id: 'OLLAMA',
    make: async () => new (await plugin('ollama')).OllamaProvider('http://localhost:11434'),
  },
};

// The per-row ERROR-level plugin log (see the header). `null` between rows.
// Copied verbatim from `record-stream-fixtures.mjs` so the corpora agree on
// the `pluginErrorLog` shape; installed BEFORE any plugin is imported, since a
// plugin's logger resolves the factory when it is built.
let pluginErrorSink = null;
let pluginWarnSink = null;
function captureLogger(prefix, baseContext = {}) {
  const fmt = (context) => {
    const merged = { ...baseContext, ...context };
    const entries = Object.entries(merged)
      .filter(([key]) => key !== 'context')
      .map(([key, value]) => `${key}=${JSON.stringify(value)}`)
      .join(' ');
    return entries ? ` ${entries}` : '';
  };
  return {
    debug: (m, c) => console.debug(`[${prefix}] ${m}${fmt(c)}`),
    info: (m, c) => console.info(`[${prefix}] ${m}${fmt(c)}`),
    warn: (m, c) => {
      console.warn(`[${prefix}] ${m}${fmt(c)}`);
      if (pluginWarnSink) {
        pluginWarnSink.push({ plugin: prefix, message: m, context: { ...baseContext, ...(c ?? {}) } });
      }
    },
    error: (m, c, e) => {
      console.error(`[${prefix}] ${m}${fmt(c)}`, e ? `\n${e.stack || e.message}` : '');
      if (pluginErrorSink) {
        pluginErrorSink.push({
          plugin: prefix,
          message: m,
          context: { ...baseContext, ...(c ?? {}) },
          error: e instanceof Error ? e.message : null,
        });
      }
    },
    child: (extra) => captureLogger(prefix, { ...baseContext, ...extra }),
  };
}
globalThis.__quilltap_logger_factory = (pluginName) => captureLogger(pluginName);

// v4's classifier + fallback trigger — the REAL modules, from the pin.
const { classifyRefusal } = await import(
  pathToFileURL(resolve(V4, 'lib/services/dangerous-content/refusal.ts')).href
);
const { classifyFallbackTrigger } = await import(
  pathToFileURL(resolve(V4, 'lib/llm/fallback/engine.ts')).href
);

// The image recorder's `thrownFields` (P4.D225), widened with `name` — the
// classifier's typed arm reads it, and Google's `ApiError` sets it.
function thrownFields(e) {
  if (!(e instanceof Error)) return { message: String(e), name: null };
  const out = { message: e.message, name: typeof e.name === 'string' ? e.name : null };
  if (e.code !== undefined && e.code !== null) out.code = e.code;
  const nested = e.error && typeof e.error === 'object' ? e.error.code : undefined;
  if (nested !== undefined && nested !== null) out.errorCode = nested;
  if (typeof e.providerReason === 'string') out.providerReason = e.providerReason;
  const status =
    typeof e.statusCode === 'number' ? e.statusCode : typeof e.status === 'number' ? e.status : undefined;
  if (status !== undefined) out.status = status;
  return out;
}

const TOOL = {
  type: 'function',
  function: {
    name: 'lookup',
    description: 'Look something up.',
    parameters: { type: 'object', properties: { q: { type: 'string' } }, required: ['q'] },
  },
};
// A 1×1 PNG — enough for `hasImageAttachments` to route the send to fetch.
const PNG_1x1 =
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==';

function paramsFor(provider, mode, c) {
  const params = {
    messages: [{ role: 'user', content: 'hi' }],
    model: provider === 'google' ? 'gemini-2.5-flash' : 'test-model',
    maxTokens: 64,
    webSearchEnabled: false,
  };
  if (mode === 'stream_tools') params.tools = [TOOL];
  if (typeof c.requestTimeoutMs === 'number') params.requestTimeoutMs = c.requestTimeoutMs;
  if (mode === 'send_vision') {
    params.messages = [
      {
        role: 'user',
        content: 'what is this',
        attachments: [{ id: 'a1', filename: 'x.png', mimeType: 'image/png', data: PNG_1x1 }],
      },
    ];
  }
  return params;
}

function modesFor(provider, c) {
  if (c.modes && Array.isArray(c.modes[provider])) return c.modes[provider];
  return provider === 'openrouter'
    ? ['stream', 'stream_tools', 'send', 'send_vision']
    : ['stream', 'send'];
}

// undici's abort behaviour: a pending `fetch` rejects with the signal's reason.
function hang(input, init) {
  const signal = init?.signal ?? (input instanceof Request ? input.signal : undefined);
  return new Promise((_, reject) => {
    if (!signal) return; // no budget: hangs forever (never posed)
    const fire = () => reject(signal.reason);
    if (signal.aborted) fire();
    else signal.addEventListener('abort', fire, { once: true });
  });
}

const cases = JSON.parse(readFileSync(args.cases, 'utf8'));
const lines = [];
for (const c of cases) {
  for (const provider of c.providers) {
    const spec = PROVIDERS[provider];
    if (!spec) throw new Error(`unknown provider ${provider}`);
    for (const mode of modesFor(provider, c)) {
      let fetchCalls = 0;
      const origFetch = globalThis.fetch;
      globalThis.fetch = async (input, init) => {
        fetchCalls++;
        if (c.transport === 'fetch-throws') throw new TypeError('fetch failed');
        if (c.transport === 'hang') return hang(input, init);
        const headers = c.contentType ? { 'content-type': c.contentType } : {};
        // An empty body on a non-2xx is posed as `null` (the P4.118 rows). On a
        // 2xx it stays `''` — an EMPTY stream: undici hands every fetched 2xx a
        // non-null `response.body`, and a null one would make the SDKs' stream
        // readers throw `… no body` where the real wire yields zero bytes
        // (P4.141's `ok_empty_body_json`).
        const is2xx = c.status >= 200 && c.status < 300;
        return new Response(c.body === '' && !is2xx ? null : c.body, {
          status: c.status,
          statusText: c.statusText,
          headers,
        });
      };
      let thrownValue = null;
      let outcome = 'ok';
      let okResult = null;
      pluginErrorSink = [];
      pluginWarnSink = [];
      try {
        const inst = await spec.make();
        const params = paramsFor(provider, mode, c);
        if (mode.startsWith('stream')) {
          // A posed non-2xx never yields; draining is the generator's throw.
          let chunks = 0;
          let content = '';
          for await (const chunk of inst.streamMessage(params, 'test-key')) {
            chunks++;
            if (typeof chunk?.content === 'string') content += chunk.content;
          }
          okResult = { chunks, content };
        } else {
          const r = await inst.sendMessage(params, 'test-key');
          okResult = { content: r?.content ?? null };
        }
      } catch (e) {
        outcome = 'thrown';
        thrownValue = e;
      } finally {
        globalThis.fetch = origFetch;
      }
      const verdict = thrownValue ? classifyRefusal({ error: thrownValue }) : { refused: false };
      const trigger = thrownValue ? classifyFallbackTrigger(thrownValue) : null;
      const row = {
        case: c.case,
        provider: spec.id,
        mode,
        status: c.status ?? null,
        statusText: c.statusText ?? null,
        contentType: c.contentType ?? null,
        body: c.body ?? null,
        fetchCalls,
        outcome,
        thrown: thrownValue ? thrownFields(thrownValue) : null,
        refusal: verdict,
        trigger,
      };
      if (outcome === 'ok') row.okResult = okResult;
      if (pluginErrorSink.length > 0) row.pluginErrorLog = pluginErrorSink;
      if (pluginWarnSink.length > 0) row.pluginWarnLog = pluginWarnSink;
      pluginErrorSink = null;
      pluginWarnSink = null;
      lines.push(JSON.stringify(row));
    }
  }
}

clearInterval(keepAlive);
writeFileSync(args.out, lines.join('\n') + '\n');
console.error(`wrote ${lines.length} row(s) → ${args.out}`);
