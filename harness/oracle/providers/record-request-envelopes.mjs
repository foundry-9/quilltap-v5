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
 * Line shape: { provider, case, mode, input, method, url, body } (body = raw
 * request string). A line with no `mode` predates P4.11 and means `"stream"`.
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
      // The @openrouter/sdk ChatResult shape (camelCase usage keys).
      return '{"id":"c","model":"x","choices":[{"index":0,"message":{"role":"assistant","content":""},"finishReason":"stop"}],"usage":{"promptTokens":1,"completionTokens":1,"totalTokens":2}}';
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
  } else if (provider === 'deepseek') {
    add('plain', { ...base, model: 'deepseek-chat' });
    add('tools', { ...base, model: 'deepseek-chat', tools: [TOOL], toolChoice: 'auto' });
    add('reasoning-echo', { ...base, model: 'deepseek-chat', messages: [SYS, USER, ASSISTANT_TOOLCALL, TOOL_RESULT] });
    add('cache-key', { ...base, model: 'deepseek-chat', cacheKey: 'char-42' });
    add('thinking-strip', { ...base, model: 'deepseek-chat', profileParameters: { thinking: 'enabled' } });
    add('profile-params', { ...base, model: 'deepseek-chat', profileParameters: { frequency_penalty: 0.5, presence_penalty: 0.2, reasoning_effort: 'high' } });
  } else if (provider === 'z-ai') {
    add('plain', { ...base, model: 'glm-4.6' });
    add('web-search', { ...base, model: 'glm-4.6', webSearchEnabled: true });
    add('tools-cache', { ...base, model: 'glm-4.6', tools: [TOOL], toolChoice: 'auto', cacheKey: 'char-7' });
    add('reasoning-default', { ...base, model: 'glm-5.2', tools: [TOOL] });
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
  } else if (provider === 'ollama') {
    add('plain', { ...base, model: 'llama3' });
    add('tools-stop', { ...base, model: 'llama3', tools: [TOOL], stop: ['DONE'] });
  } else if (provider === 'openai-compatible') {
    add('plain', { ...base, model: 'local-model' });
    add('stop-cache', { ...base, model: 'local-model', stop: ['END'], cacheKey: 'char-1' });
    add('tool-roundtrip', { ...base, model: 'local-model', messages: [SYS, USER, ASSISTANT_TOOLCALL, TOOL_RESULT] });
  } else if (provider === 'openai') {
    add('plain', { ...base, model: 'gpt-4o' });
    add('first-call', { ...base, model: 'gpt-4o', messages: [SYS, USER] });
    add('chained', { ...base, model: 'gpt-4o', previousResponseId: 'resp_prev', messages: [SYS, { role: 'user', content: 'First.' }, { role: 'assistant', content: 'Ok.' }, { role: 'user', content: 'Second.' }] });
    add('tools-websearch', { ...base, model: 'gpt-4o', tools: [TOOL], webSearchEnabled: true, stop: ['S'] });
    add('cache-key', { ...base, model: 'gpt-4o', cacheKey: 'char-9' });
    add('reasoning-model', { ...base, model: 'gpt-5', profileParameters: { reasoningEffort: 'medium', reasoningSummary: true } });
    add('reasoning-cache-retention', { ...base, model: 'gpt-5.1', cacheKey: 'char-11' });
  } else if (provider === 'grok') {
    add('plain', { ...base, model: 'grok-4' });
    add('web-search', { ...base, model: 'grok-4', webSearchEnabled: true });
    add('tools-stop-cache', { ...base, model: 'grok-4', tools: [TOOL], stop: ['S'], cacheKey: 'char-3' });
  } else if (provider === 'google') {
    add('plain', { ...base, model: 'gemini-2.5-flash' });
    add('tools', { ...base, model: 'gemini-2.5-flash', tools: [{ name: 'search', description: 'Search.', parameters: { type: 'object', properties: { query: { type: 'string' } }, required: ['query'] } }] });
    add('web-search', { ...base, model: 'gemini-2.5-flash', webSearchEnabled: true });
    add('thought-sig', { ...base, model: 'gemini-3-pro-preview', tools: [{ name: 'search', description: 'Search.', parameters: { type: 'object', properties: { query: { type: 'string' } }, required: ['query'] } }], messages: [SYS, USER, ASSISTANT_TOOLCALL, TOOL_RESULT] });
    add('stop', { ...base, model: 'gemini-2.5-flash', stop: ['STOP'] });
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
        captured = { method, url: u, body };
      }
      return mode === 'send'
        ? makeResponse(cannedJson(provider), 'application/json')
        : makeResponse(cannedWire(provider), 'text/event-stream');
    };
    try {
      const inst = await make();
      const params = { webSearchEnabled: false, ...c.params };
      if (mode === 'send') {
        await inst.sendMessage(params, 'test-api-key');
      } else {
        // Drain the generator (fires the request, then parses the canned wire).
        // eslint-disable-next-line no-unused-vars
        for await (const _chunk of inst.streamMessage(params, 'test-api-key')) { /* discard */ }
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
