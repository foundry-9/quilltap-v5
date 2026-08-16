/**
 * Request-envelope recorder (wave 4 / W4.7c part 2; the non-streaming leg is
 * P4.11 unit 1).
 *
 * Drives a v4 provider plugin's REAL `streamMessage` (`--mode stream`, the
 * default) or its REAL `sendMessage` (`--mode send`) and INTERCEPTS the outgoing
 * `fetch` to capture the exact request envelope (method / url / headers / raw
 * body string) the plugin (or its SDK) builds — the oracle the Rust
 * `model::request_builder` differential (`request_builder_equivalence`) diffs
 * against. A canned wire response is returned so the drive completes cleanly;
 * we only keep the captured request.
 *
 * **Why both modes.** `RequestInput.stream` selects which v4 method a v5 build
 * must reproduce. Until P4.11 the corpus recorded ONLY `streamMessage`, so every
 * builder could (and did) hard-code `stream: true` and stay green while every
 * non-streaming caller in production got an SSE body it could not parse
 * (dogfood finding #23). Recording both halves is what makes that class of
 * outage impossible to hide.
 *
 * Run FROM the plugin directory (imports resolve from the plugin's node_modules;
 * the record-stream-fixtures.mjs precedent), Node 24 under `npx tsx`:
 *
 *   cd ~/source/quilltap-server/plugins/dist/qtap-plugin-<name>
 *   node <V5>/harness/oracle/providers/record-request-envelopes.mjs \
 *     --provider <name> --mode send --out /tmp/req-<name>-send.ndjson
 *
 * `regenerate-request-envelopes.sh` drives every provider in BOTH modes and
 * concatenates into
 * `fixtures/request-envelopes/request-envelopes.recorded.ndjson`.
 *
 * Line shape: { provider, case, mode, input, method, url, body, headers } (body =
 * raw request string; headers = the outbound request headers, lowercased names —
 * P4.44 item 3). A line with no `mode` predates P4.11 and means `"stream"`; a
 * line with no `headers` predates P4.44. The Rust differential compares only the
 * headers v5 MODELS (User-Agent, HTTP-Referer/X-Title, content-type, auth,
 * anthropic-version) — a subset, since the SDKs add plumbing headers a single
 * reqwest transport neither sends nor should.
 */

import { writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';

function parseArgs() {
  const args = process.argv.slice(2);
  const out = {};
  for (let i = 0; i < args.length; i += 2) out[args[i].replace(/^--/, '')] = args[i + 1];
  return out;
}

const PROVIDERS = {
  anthropic: async () => new (await import(pathToFileURL(resolve('provider.ts')))).AnthropicProvider(),
  deepseek: async () => new (await import(pathToFileURL(resolve('provider.ts')))).DeepSeekProvider(),
  'z-ai': async () => new (await import(pathToFileURL(resolve('provider.ts')))).ZAIProvider(),
  openrouter: async () => new (await import(pathToFileURL(resolve('provider.ts')))).OpenRouterProvider(),
  ollama: async () => new (await import(pathToFileURL(resolve('provider.ts')))).OllamaProvider('http://localhost:11434'),
  openai: async () => new (await import(pathToFileURL(resolve('provider.ts')))).OpenAIProvider(),
  grok: async () => new (await import(pathToFileURL(resolve('provider.ts')))).GrokProvider(),
  google: async () => new (await import(pathToFileURL(resolve('provider.ts')))).GoogleProvider(),
  // The bundled plugin only re-exports @quilltap/plugin-utils' canonical class;
  // the base url matches v5's OPENAI_COMPATIBLE manifest so the recorded url is
  // the one `build_request` produces. (Added P4.11 unit 2 — this provider had
  // NO corpus coverage at all, in either mode.)
  'openai-compatible': async () =>
    new (await import(pathToFileURL(resolve('provider.ts')))).OpenAICompatibleProvider({
      baseUrl: 'http://localhost:8080/v1',
    }),
};

// A minimal SSE stream body that lets each decoder's streamMessage complete
// without throwing (we only care about the captured REQUEST). Anthropic needs a
// message_start/stop; chat-completions needs a [DONE]; responses needs a
// completed event; google needs a candidates part; ollama needs a done line.
function cannedWire(provider) {
  switch (provider) {
    case 'anthropic':
      return (
        'event: message_start\ndata: {"type":"message_start","message":{"id":"m","type":"message","role":"assistant","model":"x","content":[],"stop_reason":null,"usage":{"input_tokens":1,"output_tokens":1}}}\n\n' +
        'event: message_stop\ndata: {"type":"message_stop"}\n\n'
      );
    case 'openai':
    case 'grok':
      return 'event: response.completed\ndata: {"type":"response.completed","response":{"id":"r","output":[],"output_text":"","usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2}}}\n\n';
    case 'google':
      return 'data: {"candidates":[{"content":{"parts":[{"text":""}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":1}}\n\n';
    case 'ollama':
      return '{"message":{"content":""},"done":true,"prompt_eval_count":1,"eval_count":1}\n';
    default: // chat-completions
      return 'data: {"choices":[{"delta":{"content":""}}]}\n\ndata: [DONE]\n\n';
  }
}

// The non-streaming counterpart: a minimal, well-formed JSON body per wire
// family so each plugin's `sendMessage` returns instead of throwing. (The
// interceptor captures the request BEFORE any parse, so a throw would still
// record — but a clean drive keeps SDK retry logic out of the picture.)
function cannedJson(provider) {
  switch (provider) {
    case 'anthropic':
      return '{"id":"m","type":"message","role":"assistant","model":"x","content":[{"type":"text","text":""}],"stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":1,"output_tokens":1}}';
    case 'openai':
    case 'grok':
      return '{"id":"r","object":"response","created_at":1,"model":"x","status":"completed","output":[],"output_text":"","usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2}}';
    case 'google':
      return '{"candidates":[{"content":{"parts":[{"text":""}],"role":"model"},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":1,"totalTokenCount":2}}';
    case 'ollama':
      return '{"message":{"role":"assistant","content":""},"done":true,"prompt_eval_count":1,"eval_count":1}';
    case 'openrouter':
      // The @openrouter/sdk parses the WIRE (snake_case) chat.completion shape
      // through its zod response schema; the old camelCase stand-in failed
      // response validation AFTER the capture, which silently discarded every
      // send-mode `attachmentResults` (found by P4.21's results capture).
      return '{"id":"c","object":"chat.completion","created":1,"model":"x","system_fingerprint":"fp","choices":[{"index":0,"message":{"role":"assistant","content":""},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}';
    default: // chat-completions
      return '{"id":"c","object":"chat.completion","created":1,"model":"x","choices":[{"index":0,"message":{"role":"assistant","content":""},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}';
  }
}

function tryDecode(body) {
  try {
    return new TextDecoder().decode(body);
  } catch {
    return String(body);
  }
}

// Normalize whatever a fetch call carried as `headers` (a Headers instance, a
// plain object, or an array of [k, v] pairs — the SDKs and the raw-fetch paths
// each use a different shape) into a plain object with LOWERCASED names. HTTP
// header names are case-insensitive, so the differential compares case-folded.
function headersToObject(h) {
  const out = {};
  if (!h) return out;
  if (typeof h.forEach === 'function' && typeof h.entries === 'function') {
    for (const [k, v] of h.entries()) out[String(k).toLowerCase()] = String(v);
    return out;
  }
  if (Array.isArray(h)) {
    for (const pair of h) {
      if (pair && pair.length >= 2) out[String(pair[0]).toLowerCase()] = String(pair[1]);
    }
    return out;
  }
  if (typeof h === 'object') {
    for (const [k, v] of Object.entries(h)) out[String(k).toLowerCase()] = String(v);
    return out;
  }
  return out;
}

function makeResponse(bodyText, contentType) {
  const enc = new TextEncoder();
  const stream = new ReadableStream({
    start(controller) {
      controller.enqueue(enc.encode(bodyText));
      controller.close();
    },
  });
  return new Response(stream, { status: 200, headers: { 'content-type': contentType } });
}

// ---- Case corpus (shared shapes; provider-specific params via `only`) --------
const SYS = { role: 'system', content: 'You are a helpful assistant.' };
const USER = { role: 'user', content: 'Hello there.' };
const TOOL = {
  type: 'function',
  function: { name: 'search', description: 'Search.', parameters: { type: 'object', properties: { query: { type: 'string' } }, required: ['query'] } },
};
const ASSISTANT_TOOLCALL = {
  role: 'assistant',
  content: 'Let me look.',
  toolCalls: [{ id: 'call_1', type: 'function', function: { name: 'search', arguments: '{"query":"x"}' } }],
  reasoningContent: 'thinking about it',
  thoughtSignature: 'sig-abc',
};
const TOOL_RESULT = { role: 'tool', toolCallId: 'call_1', content: 'result text' };

// ---- Attachment shapes (P4.21 — the corpus's first attachment vectors; the
// pre-P4.21 corpus had ZERO, which is how dogfood #37 survived a
// differential-verified port). The bags mirror what v4's `loadChatFilesForLLM`
// builds: {id, filepath, filename, mimeType, size, data}.
const IMG_ATT = {
  id: 'att-img-1',
  filepath: '/api/v1/files/att-img-1',
  filename: 'photo.png',
  mimeType: 'image/png',
  size: 68,
  data: 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==',
};
const IMG_ATT_2 = {
  id: 'att-img-2',
  filepath: '/api/v1/files/att-img-2',
  filename: 'sketch.jpg',
  mimeType: 'image/jpeg',
  size: 18,
  data: '/9j/4AAQSkZJRgABAQ==',
};
// Data never loaded — v4's `File data not loaded` / filtered-out arm.
const IMG_NO_DATA = { id: 'att-nodata-1', filename: 'missing.png', mimeType: 'image/png', size: 10 };
// Not in any provider's supportedMimeTypes.
const TIFF_ATT = { id: 'att-tiff-1', filename: 'scan.tiff', mimeType: 'image/tiff', size: 9, data: 'AAAA' };
const PDF_ATT = {
  id: 'att-pdf-1',
  filename: 'doc.pdf',
  mimeType: 'application/pdf',
  size: 17,
  data: 'JVBERi0xLjQKJcTl8g==',
};
// base64-looking text data (no newline, all base64 chars) — v4's anthropic
// text/plain arm DECODES it before the document block.
const TXT_ATT_B64 = { id: 'att-txt-1', filename: 'notes.txt', mimeType: 'text/plain', size: 11, data: 'aGVsbG8gd29ybGQ=' };
// Raw text data (contains a newline) — v4 sends it as-is.
const TXT_ATT_RAW = { id: 'att-txt-2', filename: 'raw.txt', mimeType: 'text/plain', size: 17, data: 'line one\nline two' };
// Base64-LOOKING but strictly-invalid data (newline-free, base64 charset, bad
// length). Node's Buffer.from never throws. v4 bug 34's decodeBase64Text
// round-trips (decode, re-encode, compare) so this ships VERBATIM ("hello")
// instead of the old mojibake — the mangled-b64 anthropic vectors flip.
const TXT_ATT_MANGLED = { id: 'att-txt-3', filename: 'word.txt', mimeType: 'text/plain', size: 5, data: 'hello' };
// "x=1" — base64 charset, decode stops at "=" → 0 bytes; v4 bug 34 round-trips
// so it ships VERBATIM ("x=1") instead of "". Absent before; v4's tests cover it.
const TXT_ATT_X1 = { id: 'att-txt-4', filename: 'expr.txt', mimeType: 'text/plain', size: 3, data: 'x=1' };
// A text file missing its data — v4 bug 33 makes Grok's `File data not loaded`
// arm reachable for text (it now passes the widened isHandledMimeType gate).
const TXT_ATT_NO_DATA = { id: 'att-txt-5', filename: 'empty.txt', mimeType: 'text/plain', size: 0 };
// A binary (zip) that is NOT image/text/pdf — reaches v4 bug 33's new suffixed
// Grok rejection ("... supports: <images>, text/*").
const ZIP_ATT = { id: 'att-zip-1', filename: 'archive.zip', mimeType: 'application/zip', size: 8, data: 'UEsDBBQA' };
const USER_IMG = { role: 'user', content: 'What is in this image?', attachments: [IMG_ATT] };

/// `modes` restricts a case to a subset of `stream` / `send` (default: both).
function casesFor(provider) {
  const base = { messages: [SYS, USER], model: 'MODEL', temperature: 0.5, maxTokens: 1000, topP: 0.9 };
  const cases = [];
  const add = (name, params, modes) => cases.push({ name, params, modes: modes ?? ['stream', 'send'] });

  if (provider === 'anthropic') {
    add('plain', { ...base, model: 'claude-opus-4-6', messages: [SYS, USER] });
    add('sampling-rejected', { ...base, model: 'claude-sonnet-5', messages: [SYS, USER] });
    add('thinking', { ...base, model: 'claude-opus-4-6', profileParameters: { thinkingBudget: 2048 } });
    add('tools-stop', { ...base, model: 'claude-opus-4-6', tools: [{ name: 'search', description: 'Search.', input_schema: { type: 'object', properties: { query: { type: 'string' } }, required: ['query'] } }], stop: ['STOP', 'END'] });
    add('caching', { ...base, model: 'claude-opus-4-6', profileParameters: { enableCacheBreakpoints: true, cacheStrategy: 'system_and_long_context' } });
    add('tool-roundtrip', { ...base, model: 'claude-opus-4-6', messages: [SYS, USER, ASSISTANT_TOOLCALL, TOOL_RESULT] });
    // P4.d10 tier-2 item 6 (the 8ee56f6e corpus bank): the EXACT new-generation
    // prefix boundary. claude-opus-4-7 is the first new-gen opus (sampling
    // rejected, adaptive thinking); a DATED claude-opus-4-8 snapshot is still
    // new-gen; claude-opus-4-6 with a thinking budget stays the classic
    // fixed-budget shape (covered by 'thinking' above).
    add('boundary-first-new-gen-4-7', { ...base, model: 'claude-opus-4-7', messages: [SYS, USER] });
    add('boundary-dated-new-gen-4-8', { ...base, model: 'claude-opus-4-8-20260215', messages: [SYS, USER] });
    add('boundary-new-gen-thinking', { ...base, model: 'claude-opus-4-8', profileParameters: { thinkingBudget: 2048 } });
    // P4.21 — attachment vectors (image + PDF + both text-decode arms + the
    // failure arms + the cache-control-on-image-block interaction).
    add('image-attachment', { ...base, model: 'claude-opus-4-6', messages: [SYS, USER_IMG] });
    add('pdf-attachment', { ...base, model: 'claude-opus-4-6', messages: [SYS, { role: 'user', content: 'Summarize this document.', attachments: [PDF_ATT] }] });
    add('text-attachment-b64', { ...base, model: 'claude-opus-4-6', messages: [SYS, { role: 'user', content: 'Read this.', attachments: [TXT_ATT_B64] }] });
    add('text-attachment-raw', { ...base, model: 'claude-opus-4-6', messages: [SYS, { role: 'user', content: 'Read this.', attachments: [TXT_ATT_RAW] }] });
    add('text-attachment-mangled-b64', { ...base, model: 'claude-opus-4-6', messages: [SYS, { role: 'user', content: 'Read this.', attachments: [TXT_ATT_MANGLED] }] });
    // v4 bug 34: base64-looking-but-not "x=1" now ships verbatim (was "").
    add('text-attachment-x-equals-1', { ...base, model: 'claude-opus-4-6', messages: [SYS, { role: 'user', content: 'Read this.', attachments: [TXT_ATT_X1] }] });
    add('attachment-no-data', { ...base, model: 'claude-opus-4-6', messages: [SYS, { role: 'user', content: 'What is in this image?', attachments: [IMG_NO_DATA] }] });
    add('multi-attachment', { ...base, model: 'claude-opus-4-6', messages: [SYS, { role: 'user', content: 'Compare these.', attachments: [IMG_ATT, PDF_ATT] }] });
    add('image-attachment-caching', { ...base, model: 'claude-opus-4-6', messages: [SYS, USER_IMG], profileParameters: { enableCacheBreakpoints: true, cacheStrategy: 'system_and_long_context' } });
    add('unsupported-attachment', { ...base, model: 'claude-opus-4-6', messages: [SYS, { role: 'user', content: 'What is this?', attachments: [TIFF_ATT] }] });
  } else if (provider === 'deepseek') {
    add('plain', { ...base, model: 'deepseek-chat' });
    add('tools', { ...base, model: 'deepseek-chat', tools: [TOOL], toolChoice: 'auto' });
    add('reasoning-echo', { ...base, model: 'deepseek-chat', messages: [SYS, USER, ASSISTANT_TOOLCALL, TOOL_RESULT] });
    add('cache-key', { ...base, model: 'deepseek-chat', cacheKey: 'char-42' });
    add('thinking-strip', { ...base, model: 'deepseek-chat', profileParameters: { thinking: 'enabled' } });
    add('profile-params', { ...base, model: 'deepseek-chat', profileParameters: { frequency_penalty: 0.5, presence_penalty: 0.2, reasoning_effort: 'high' } });
    // P4.D83 (v4 `93ed8abf`): the NEUTRALITY gate for the collapse onto the
    // shared `applyProfileParameters`. The skip rules (null, the empty string,
    // a key not on the allow-list) and the `thinking` object reshape are the
    // whole behaviour the hand-rolled loop had; if the collapse changed any of
    // them these bytes move.
    add('profile-params-skips', { ...base, model: 'deepseek-chat', profileParameters: { frequency_penalty: '', presence_penalty: null, logprobs: true, top_logprobs: 3, reasoning_effort: 'high', model: 'HIJACKED', messages: [], stream: false, tools: [] } });
    add('profile-params-thinking-object', { ...base, model: 'deepseek-chat', profileParameters: { thinking: { type: 'disabled' } } });
    // P4.21 — DeepSeek DROPS attachments (body shows plain string content).
    add('image-attachment', { ...base, model: 'deepseek-chat', messages: [SYS, USER_IMG] });
  } else if (provider === 'z-ai') {
    add('plain', { ...base, model: 'glm-4.6' });
    add('web-search', { ...base, model: 'glm-4.6', webSearchEnabled: true });
    add('tools-cache', { ...base, model: 'glm-4.6', tools: [TOOL], toolChoice: 'auto', cacheKey: 'char-7' });
    add('reasoning-default', { ...base, model: 'glm-5.2', tools: [TOOL] });
    // P4.D83: the `reasoning_effort` MODEL GATE — v4 refuses to forward the key
    // to a model that does not honour it (glm-4.6), and forwards it on one that
    // does (glm-5.2, where it also suppresses the default `high`). v5 forwarded
    // it to every GLM; no vector could see that because none crossed a profile
    // effort with an old model.
    add('profile-effort-unsupported-model', { ...base, model: 'glm-4.6', profileParameters: { reasoning_effort: 'high', thinking: 'disabled' } });
    add('profile-effort-supported-model', { ...base, model: 'glm-5.2', profileParameters: { reasoning_effort: 'low' } });
    add('profile-params-skips', { ...base, model: 'glm-5.2', profileParameters: { thinking: '', do_sample: null, reasoning_effort: 'medium', model: 'HIJACKED', messages: [], stream: false, tools: [] } });
    // P4.21 — the vision-model gate: a vision model gets image_url parts; a
    // non-vision model still switches to a parts ARRAY but reports the failure.
    add('image-attachment-vision', { ...base, model: 'glm-4.6v', messages: [SYS, USER_IMG] });
    add('image-attachment-non-vision', { ...base, model: 'glm-4.6', messages: [SYS, USER_IMG] });
    add('attachment-no-data', { ...base, model: 'glm-4.6v', messages: [SYS, { role: 'user', content: 'What is in this image?', attachments: [IMG_NO_DATA] }] });
  } else if (provider === 'openrouter') {
    add('tools', { ...base, model: 'openai/gpt-4o', tools: [TOOL] });
    add('web-search', { ...base, model: 'openai/gpt-4o', tools: [TOOL], webSearchEnabled: true });
    add('stop', { ...base, model: 'openai/gpt-4o', tools: [TOOL], stop: ['X'] });
    // P4.11 unit 2: OpenRouter is the provider whose `sendMessage` differs by
    // more than the flag — it goes through the @openrouter/sdk (a Speakeasy zod
    // codegen) instead of the raw-fetch chat-completions path `streamMessage`
    // uses for tool/vision requests. These cases pin every branch v4's
    // `sendMessage` can take, so the non-streaming builder is BUILT to the
    // recording rather than reasoned out of the SDK source.
    add('cache-key', { ...base, model: 'openai/gpt-4o', tools: [TOOL], cacheKey: 'char-5' });
    // Two SEPARATE tool shapes, because the SDK treats them differently:
    // an assistant turn's `tool_calls` are SILENTLY STRIPPED (the message ships
    // as {content, role}), while a `tool`-role result fails the SDK's input
    // validation outright and NO request is made. Both are v4 behaviours the
    // port carries; the second records as a loud `error` line.
    add('assistant-toolcall', { ...base, model: 'openai/gpt-4o', tools: [TOOL], messages: [SYS, USER, ASSISTANT_TOOLCALL] });
    add('assistant-toolcall-empty', { ...base, model: 'openai/gpt-4o', tools: [TOOL], messages: [SYS, USER, { ...ASSISTANT_TOOLCALL, content: '' }] });
    add('tool-roundtrip', { ...base, model: 'openai/gpt-4o', tools: [TOOL], messages: [SYS, USER, ASSISTANT_TOOLCALL, TOOL_RESULT] });
    add('response-format-schema', {
      ...base,
      model: 'openai/gpt-4o',
      tools: [TOOL],
      responseFormat: { type: 'json_schema', jsonSchema: { name: 'out', strict: true, schema: { type: 'object', properties: { a: { type: 'string' } } } } },
    });
    add('response-format-object', { ...base, model: 'openai/gpt-4o', tools: [TOOL], responseFormat: { type: 'json_object' } });
    add('fallback-models', { ...base, model: 'openai/gpt-4o', tools: [TOOL], profileParameters: { fallbackModels: ['anthropic/claude-3'] } });
    add('provider-prefs', {
      ...base,
      model: 'openai/gpt-4o',
      tools: [TOOL],
      profileParameters: { providerPreferences: { order: ['openai', 'azure'], allowFallbacks: false, requireParameters: true, dataCollection: 'deny', ignore: ['bad'], only: ['good'] } },
    });
    add('reasoning-effort', { ...base, model: 'openai/gpt-4o', tools: [TOOL], profileParameters: { reasoningEffort: 'high' } });
    // SEND-ONLY. v4's `sendMessage` is one path regardless of tools (and a
    // cheap-LLM call carries none), but its `streamMessage` routes a
    // no-tools/no-images request through the SDK's `callModel()` OpenResponses
    // shape instead of the raw chat-completions fetch v5's streaming builder
    // ports — an UNPORTED streaming path, deferred loud in the P4.11 record.
    // Recording a streaming vector here would pin that gap as if this lane had
    // closed it; recording only `send` keeps the claim honest.
    add('no-tools', { ...base, model: 'openai/gpt-4o' }, ['send']);
    // P4.21 — vision. Streaming: an image attachment forces v4 off the SDK
    // callModel path onto streamViaChatCompletions even with NO tools, so a
    // stream vector here is recordable (unlike the plain no-tools case above).
    // Send: the @openrouter/sdk re-emits the content-parts array — recorded, not
    // reasoned. The no-data arm is SEND-ONLY: with no usable image v4's
    // streaming falls back to the unported SDK path.
    add('image-attachment', { ...base, model: 'openai/gpt-4o', messages: [SYS, USER_IMG] });
    add('image-attachment-tools', { ...base, model: 'openai/gpt-4o', tools: [TOOL], messages: [SYS, USER_IMG] });
    add('attachment-no-data', { ...base, model: 'openai/gpt-4o', messages: [SYS, { role: 'user', content: 'What is in this image?', attachments: [IMG_NO_DATA] }] }, ['send']);
    // v4 bug 31 — the NON-STREAMING vision send path (sendViaChatCompletions),
    // send-only: an image escapes the SDK to a direct chat-completions POST, a
    // THIRD body shape whose assignment order IS wire order. Each vector crosses
    // an image with one feature so the new arm is actually exercised (every
    // pre-bug-31 openrouter vector is image-free). The streaming half of these
    // combos is already ported (streamViaChatCompletions); the send arm is new.
    add('image-cache-key', { ...base, model: 'openai/gpt-4o', cacheKey: 'char-5', messages: [SYS, USER_IMG] }, ['send']);
    add('image-web-search', { ...base, model: 'openai/gpt-4o', webSearchEnabled: true, messages: [SYS, USER_IMG] }, ['send']);
    add('image-response-format-schema', { ...base, model: 'openai/gpt-4o', responseFormat: { type: 'json_schema', jsonSchema: { name: 'out', strict: true, schema: { type: 'object', properties: { a: { type: 'string' } } } } }, messages: [SYS, USER_IMG] }, ['send']);
    add('image-fallback-models', { ...base, model: 'openai/gpt-4o', profileParameters: { fallbackModels: ['anthropic/claude-3'] }, messages: [SYS, USER_IMG] }, ['send']);
    add('image-provider-prefs', { ...base, model: 'openai/gpt-4o', profileParameters: { enableZDR: true, providerPreferences: { order: ['openai', 'azure'], allowFallbacks: false, requireParameters: true, ignore: ['bad'], only: ['good'] } }, messages: [SYS, USER_IMG] }, ['send']);
    add('image-reasoning-effort', { ...base, model: 'openai/gpt-4o', profileParameters: { reasoningEffort: 'high' }, messages: [SYS, USER_IMG] }, ['send']);
    add('image-tool-roundtrip', { ...base, model: 'openai/gpt-4o', tools: [TOOL], messages: [SYS, USER_IMG, ASSISTANT_TOOLCALL, TOOL_RESULT] }, ['send']);
    // Stream-only result-string pins (the send drive does not surface the
    // SDK-path response's attachmentResults): the non-image arm and the
    // missing-data arm, each alongside a body the raw-fetch path records.
    add('pdf-attachment-tools', { ...base, model: 'openai/gpt-4o', tools: [TOOL], messages: [SYS, { role: 'user', content: 'Read this.', attachments: [PDF_ATT] }] }, ['stream']);
    add('mixed-image-no-data-tools', { ...base, model: 'openai/gpt-4o', tools: [TOOL], messages: [SYS, { role: 'user', content: 'What is here?', attachments: [IMG_ATT, IMG_NO_DATA] }] }, ['stream']);
  } else if (provider === 'ollama') {
    add('plain', { ...base, model: 'llama3' });
    add('tools-stop', { ...base, model: 'llama3', tools: [TOOL], stop: ['DONE'] });
    // P4.21 — Ollama DROPS attachments (body shows plain string content).
    add('image-attachment', { ...base, model: 'llama3', messages: [SYS, USER_IMG] });
    // P4.D78 (v4 d9c5a1c7) — the thinking switch + the context window. `think`
    // is ALWAYS on the body (false when off, which every pre-existing vector
    // above now shows); `options.num_ctx` appears only when the profile bag
    // coerces to a finite positive number.
    add('thinking-on', { ...base, model: 'qwen3:8b', profileParameters: { enable_thinking: true } });
    add('thinking-on-string', { ...base, model: 'qwen3:8b', profileParameters: { enable_thinking: 'true' } });
    add('thinking-off-explicit', { ...base, model: 'qwen3:8b', profileParameters: { enable_thinking: false } });
    // Truthy but not `true`/`'true'` → off (v4's strict comparison).
    add('thinking-truthy-string-off', { ...base, model: 'qwen3:8b', profileParameters: { enable_thinking: 'yes' } });
    add('thinking-numeric-one-off', { ...base, model: 'qwen3:8b', profileParameters: { enable_thinking: 1 } });
    add('num-ctx-number', { ...base, model: 'qwen3:8b', profileParameters: { num_ctx: 40960 } });
    add('num-ctx-string', { ...base, model: 'qwen3:8b', profileParameters: { num_ctx: '32768' } });
    add('num-ctx-fractional-floored', { ...base, model: 'qwen3:8b', profileParameters: { num_ctx: 8192.9 } });
    // Every arm that leaves the key OFF the wire: zero, negative, unparseable,
    // null, boolean (v4 coerces only the number and string arms).
    add('num-ctx-zero-omitted', { ...base, model: 'qwen3:8b', profileParameters: { num_ctx: 0 } });
    add('num-ctx-negative-omitted', { ...base, model: 'qwen3:8b', profileParameters: { num_ctx: -1 } });
    add('num-ctx-garbage-omitted', { ...base, model: 'qwen3:8b', profileParameters: { num_ctx: 'lots' } });
    add('num-ctx-null-omitted', { ...base, model: 'qwen3:8b', profileParameters: { num_ctx: null } });
    add('num-ctx-true-omitted', { ...base, model: 'qwen3:8b', profileParameters: { num_ctx: true } });
    add('thinking-and-num-ctx-with-tools', { ...base, model: 'qwen3:8b', tools: [TOOL], stop: ['DONE'], profileParameters: { enable_thinking: true, num_ctx: 16384 } });
  } else if (provider === 'openai-compatible') {
    add('plain', { ...base, model: 'local-model' });
    add('stop-cache', { ...base, model: 'local-model', stop: ['END'], cacheKey: 'char-1' });
    add('tool-roundtrip', { ...base, model: 'local-model', messages: [SYS, USER, ASSISTANT_TOOLCALL, TOOL_RESULT] });
    // P4.21 — the OpenAI-compatible base DROPS attachments.
    add('image-attachment', { ...base, model: 'local-model', messages: [SYS, USER_IMG] });
  } else if (provider === 'openai') {
    add('plain', { ...base, model: 'gpt-4o' });
    add('first-call', { ...base, model: 'gpt-4o', messages: [SYS, USER] });
    add('chained', { ...base, model: 'gpt-4o', previousResponseId: 'resp_prev', messages: [SYS, { role: 'user', content: 'First.' }, { role: 'assistant', content: 'Ok.' }, { role: 'user', content: 'Second.' }] });
    add('tools-websearch', { ...base, model: 'gpt-4o', tools: [TOOL], webSearchEnabled: true, stop: ['S'] });
    add('cache-key', { ...base, model: 'gpt-4o', cacheKey: 'char-9' });
    add('reasoning-model', { ...base, model: 'gpt-5', profileParameters: { reasoningEffort: 'medium', reasoningSummary: true } });
    add('reasoning-cache-retention', { ...base, model: 'gpt-5.1', cacheKey: 'char-11' });
    // P4.21 — Responses-API input_image vectors + the failure arms + the
    // empty-content asymmetry (image only, no empty text part) + chaining
    // (extractLastUserMessage keeps the image parts).
    add('image-attachment', { ...base, model: 'gpt-4o', messages: [SYS, USER_IMG] });
    add('multi-attachment', { ...base, model: 'gpt-4o', messages: [SYS, { role: 'user', content: 'Compare these.', attachments: [IMG_ATT, IMG_ATT_2] }] });
    add('attachment-no-data', { ...base, model: 'gpt-4o', messages: [SYS, { role: 'user', content: 'What is in this image?', attachments: [IMG_NO_DATA] }] });
    add('unsupported-attachment', { ...base, model: 'gpt-4o', messages: [SYS, { role: 'user', content: 'What is this?', attachments: [TIFF_ATT] }] });
    add('image-attachment-empty-content', { ...base, model: 'gpt-4o', messages: [SYS, { role: 'user', content: '', attachments: [IMG_ATT] }] });
    add('image-attachment-chained', { ...base, model: 'gpt-4o', previousResponseId: 'resp_prev', messages: [SYS, { role: 'user', content: 'First.' }, { role: 'assistant', content: 'Ok.' }, USER_IMG] });
  } else if (provider === 'grok') {
    add('plain', { ...base, model: 'grok-4' });
    add('web-search', { ...base, model: 'grok-4', webSearchEnabled: true });
    add('tools-stop-cache', { ...base, model: 'grok-4', tools: [TOOL], stop: ['S'], cacheKey: 'char-3' });
    // P4.21 — Grok input_image.
    add('image-attachment', { ...base, model: 'grok-4', messages: [SYS, USER_IMG] });
    // v4 bug 33: the gate is now isHandledMimeType, so text/* is SENT inline
    // (base64-round-tripped, bug 34) and PDF reaches the honest Files-API
    // refusal; a binary reaches the new ", text/*"-suffixed rejection. The old
    // (now-lying) `unsupported-attachment` text case is renamed `text-attachment`.
    add('text-attachment', { ...base, model: 'grok-4', messages: [SYS, { role: 'user', content: 'Read this file.', attachments: [TXT_ATT_RAW] }] });
    // A Grok text file carrying REAL base64 (Grok has no charset pre-guard — its
    // decodeBase64Text round-trip is otherwise pinned nowhere).
    add('text-attachment-b64', { ...base, model: 'grok-4', messages: [SYS, { role: 'user', content: 'Read this.', attachments: [TXT_ATT_B64] }] });
    add('text-attachment-no-data', { ...base, model: 'grok-4', messages: [SYS, { role: 'user', content: 'Read this.', attachments: [TXT_ATT_NO_DATA] }] });
    add('pdf-attachment', { ...base, model: 'grok-4', messages: [SYS, { role: 'user', content: 'Summarize this.', attachments: [PDF_ATT] }] });
    add('binary-attachment', { ...base, model: 'grok-4', messages: [SYS, { role: 'user', content: 'What is this?', attachments: [ZIP_ATT] }] });
  } else if (provider === 'google') {
    add('plain', { ...base, model: 'gemini-2.5-flash' });
    add('tools', { ...base, model: 'gemini-2.5-flash', tools: [{ name: 'search', description: 'Search.', parameters: { type: 'object', properties: { query: { type: 'string' } }, required: ['query'] } }] });
    add('web-search', { ...base, model: 'gemini-2.5-flash', webSearchEnabled: true });
    add('thought-sig', { ...base, model: 'gemini-3-pro-preview', tools: [{ name: 'search', description: 'Search.', parameters: { type: 'object', properties: { query: { type: 'string' } }, required: ['query'] } }], messages: [SYS, USER, ASSISTANT_TOOLCALL, TOOL_RESULT] });
    add('stop', { ...base, model: 'gemini-2.5-flash', stop: ['STOP'] });
    // P4.21 — inlineData parts + the consecutive-user merge (attachments concat).
    add('image-attachment', { ...base, model: 'gemini-2.5-flash', messages: [SYS, USER_IMG] });
    add('multi-attachment', { ...base, model: 'gemini-2.5-flash', messages: [SYS, { role: 'user', content: 'Compare these.', attachments: [IMG_ATT, IMG_ATT_2] }] });
    add('merged-user-attachments', { ...base, model: 'gemini-2.5-flash', messages: [SYS, { role: 'user', content: 'First image.', attachments: [IMG_ATT] }, { role: 'user', content: 'Second image.', attachments: [IMG_ATT_2] }] });
    add('attachment-no-data', { ...base, model: 'gemini-2.5-flash', messages: [SYS, { role: 'user', content: 'What is in this image?', attachments: [IMG_NO_DATA] }] });
  }
  return cases;
}

async function main() {
  const args = parseArgs();
  const provider = args.provider;
  const outPath = args.out;
  const mode = args.mode || 'stream';
  if (!provider || !outPath) {
    console.error('usage: --provider <name> [--mode stream|send] --out <ndjson>');
    process.exit(1);
  }
  if (mode !== 'stream' && mode !== 'send') {
    console.error(`unknown mode ${mode} (expected "stream" or "send")`);
    process.exit(1);
  }
  const make = PROVIDERS[provider];
  if (!make) {
    console.error(`unknown provider ${provider}`);
    process.exit(1);
  }

  const lines = [];
  let missing = 0;
  for (const c of casesFor(provider).filter((c) => c.modes.includes(mode))) {
    let captured = null;
    let failure = null;
    const origFetch = globalThis.fetch;
    globalThis.fetch = async (url, init) => {
      if (!captured) {
        const u = typeof url === 'string' ? url : (url && url.url) || String(url);
        const method = (init && init.method) || (url && url.method) || 'GET';
        let body = (init && init.body) || null;
        if (body && typeof body !== 'string') {
          // The @openrouter/sdk sends its body as a ReadableStream; drain it.
          // (We never forward the request, so consuming the stream is safe.)
          body = typeof body.getReader === 'function'
            ? await new Response(body).text()
            : tryDecode(body);
        }
        // Undici `Request` objects carry the body on the request, not in init.
        if (body === null && url && typeof url.text === 'function') {
          try { body = await url.clone().text(); } catch { /* leave null */ }
        }
        // Outbound headers (P4.44 item 3): from init.headers for a plain fetch,
        // or off the undici Request object. Lowercased names (HTTP header names
        // are case-insensitive; the SDKs differ from the raw-fetch paths).
        const headers = headersToObject(
          (init && init.headers) || (url && url.headers) || null
        );
        captured = { method, url: u, body, headers };
      }
      return mode === 'send'
        ? makeResponse(cannedJson(provider), 'application/json')
        : makeResponse(cannedWire(provider), 'text/event-stream');
    };
    // P4.21: capture `attachmentResults` off the response (send) / the chunks
    // (stream) so the sent/failed outcome — including every failure STRING —
    // is recorded rather than transcribed. Only kept on the line when
    // non-empty, so every pre-P4.21 vector stays byte-identical.
    let attachmentResults = null;
    try {
      const inst = await make();
      const params = { webSearchEnabled: false, ...c.params };
      if (mode === 'send') {
        const resp = await inst.sendMessage(params, 'test-api-key');
        attachmentResults = (resp && resp.attachmentResults) || null;
      } else {
        // Drain the generator (fires the request, then parses the canned wire).
        for await (const chunk of inst.streamMessage(params, 'test-api-key')) {
          if (chunk && chunk.attachmentResults) attachmentResults = chunk.attachmentResults;
        }
      }
    } catch (e) {
      // The request is captured before any parse error; keep going. But a case
      // that never reached `fetch` is a LOUD line, not a silent absence — a
      // missing capture reading as "no case" is exactly how dogfood #23 hid.
      failure = e instanceof Error ? e.message : String(e);
    } finally {
      globalThis.fetch = origFetch;
    }
    if (!captured) missing += 1;
    lines.push(
      JSON.stringify({
        provider,
        case: c.name,
        mode,
        input: c.params,
        // A refusal line: the plugin (or its SDK) rejected the params before any
        // HTTP call. Only the first line of the thrown message is kept — the
        // zod dumps are multi-KB and SDK-version-specific, and the CONTRACT the
        // Rust side checks is "v4 refuses here", not the diagnostic wording.
        ...(captured || {
          refused: true,
          error: (failure ?? 'drive completed without fetch').split('\n')[0],
        }),
        ...(attachmentResults &&
        ((attachmentResults.sent && attachmentResults.sent.length) ||
          (attachmentResults.failed && attachmentResults.failed.length))
          ? { attachmentResults }
          : {}),
      })
    );
  }

  writeFileSync(outPath, lines.join('\n') + '\n');
  console.error(
    `wrote ${lines.length} line(s) for ${provider} [${mode}] → ${outPath}` +
      (missing > 0 ? ` — ⚠ ${missing} case(s) captured NO request` : '')
  );
}

main().catch((e) => { console.error(e); process.exit(1); });
