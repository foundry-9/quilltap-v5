/**
 * Request-envelope recorder (wave 4 / W4.7c part 2).
 *
 * Drives a v4 provider plugin's REAL `streamMessage` and INTERCEPTS the outgoing
 * `fetch` to capture the exact request envelope (method / url / headers / raw
 * body string) the plugin (or its SDK) builds — the oracle the Rust
 * `model::request_builder` differential (`request_builder_equivalence`) diffs
 * against. A canned wire response is returned so the generator completes cleanly;
 * we only keep the captured request.
 *
 * Run FROM the plugin directory (imports resolve from the plugin's node_modules;
 * the record-stream-fixtures.mjs precedent), Node 24 under `npx tsx`:
 *
 *   cd ~/source/quilltap-server/plugins/dist/qtap-plugin-<name>
 *   node <V5>/harness/oracle/providers/record-request-envelopes.mjs \
 *     --provider <name> --out /tmp/req-<name>.ndjson
 *
 * `regenerate-request-envelopes.sh` drives every provider and concatenates into
 * `fixtures/request-envelopes/request-envelopes.recorded.ndjson`.
 *
 * Line shape: { provider, case, method, url, body } (body = raw request string).
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

function makeResponse(bodyText) {
  const enc = new TextEncoder();
  const stream = new ReadableStream({
    start(controller) {
      controller.enqueue(enc.encode(bodyText));
      controller.close();
    },
  });
  return new Response(stream, { status: 200, headers: { 'content-type': 'text/event-stream' } });
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

function casesFor(provider) {
  const base = { messages: [SYS, USER], model: 'MODEL', temperature: 0.5, maxTokens: 1000, topP: 0.9 };
  const cases = [];
  const add = (name, params) => cases.push({ name, params });

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
  } else if (provider === 'ollama') {
    add('plain', { ...base, model: 'llama3' });
    add('tools-stop', { ...base, model: 'llama3', tools: [TOOL], stop: ['DONE'] });
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
  if (!provider || !outPath) {
    console.error('usage: --provider <name> --out <ndjson>');
    process.exit(1);
  }
  const make = PROVIDERS[provider];
  if (!make) {
    console.error(`unknown provider ${provider}`);
    process.exit(1);
  }

  const lines = [];
  for (const c of casesFor(provider)) {
    let captured = null;
    const origFetch = globalThis.fetch;
    globalThis.fetch = async (url, init) => {
      if (!captured) {
        const u = typeof url === 'string' ? url : (url && url.url) || String(url);
        const method = (init && init.method) || (url && url.method) || 'GET';
        let body = (init && init.body) || (url && url.body) || null;
        if (body && typeof body !== 'string') {
          try { body = new TextDecoder().decode(body); } catch { body = String(body); }
        }
        captured = { method, url: u, body };
      }
      return makeResponse(cannedWire(provider));
    };
    try {
      const inst = await make();
      const params = { webSearchEnabled: false, ...c.params };
      // Drain the generator (fires the request, then parses the canned wire).
      // eslint-disable-next-line no-unused-vars
      for await (const _chunk of inst.streamMessage(params, 'test-api-key')) { /* discard */ }
    } catch (e) {
      // The request is captured before any parse error; keep going.
    } finally {
      globalThis.fetch = origFetch;
    }
    lines.push(
      JSON.stringify({
        provider,
        case: c.name,
        input: c.params,
        ...(captured || { error: 'no request captured' }),
      })
    );
  }

  writeFileSync(outPath, lines.join('\n') + '\n');
  console.error(`wrote ${lines.length} line(s) for ${provider} → ${outPath}`);
}

main().catch((e) => { console.error(e); process.exit(1); });
