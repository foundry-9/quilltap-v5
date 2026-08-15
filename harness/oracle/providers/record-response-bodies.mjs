/**
 * Response-body recorder (P4.13 unit 4 — dogfood finding #24's close-out).
 *
 * Drives a v4 provider plugin's REAL `sendMessage` with the network mocked
 * UNDERNEATH the provider SDK: `globalThis.fetch` returns a corpus response
 * body, so the SDK's own unwrap/normalization (e.g. the openai SDK's
 * `addOutputText`, the @openrouter/sdk's inbound zod rename, the genai SDK's
 * `.text` accessor) runs INSIDE the oracle loop, and the recorded output is
 * the exact `LLMResponse` v4 builds from that body. Feeding a body straight to
 * `buildLLMResponse` would have passed #24 green — both sides reading the same
 * SDK-synthesized key that never exists on the wire.
 *
 * ⚠ CAPTURE TIERS. Every body in this corpus is currently **doc-derived
 * (`synthetic: true`)** — it proves v4/v5 PARSE AGREEMENT on those bytes, not
 * that the bytes match a live provider's wire. Replacing a family's bodies
 * with real captures (run the same script against a body captured from a live
 * call) upgrades that family's trust; record the upgrade in `_meta`.
 *
 * Run FROM the plugin directory (imports resolve from the plugin's
 * node_modules; the record-request-envelopes.mjs precedent), Node 24:
 *
 *   cd ~/source/quilltap-server/plugins/dist/qtap-plugin-<name>
 *   node <V5>/harness/oracle/providers/record-response-bodies.mjs \
 *     --provider <name> --out /tmp/resp-<name>.ndjson
 *
 * `regenerate-response-bodies.sh` drives every provider and concatenates into
 * `fixtures/response-bodies/response-bodies.recorded.ndjson`.
 *
 * Line shape: { provider, case, synthetic, body, response, error }
 *   body     = the raw response-body STRING fetch returned (the wire bytes)
 *   response = JSON.parse(JSON.stringify(<LLMResponse>)) (undefined dropped,
 *              exactly what survives v4's own serialization boundaries)
 *   error    = the thrown message when sendMessage refused/threw (loud line —
 *              a missing capture reading as "no case" is how #23/#24 hid)
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
  'openai-compatible': async () =>
    new (await import(pathToFileURL(resolve('provider.ts')))).OpenAICompatibleProvider({
      baseUrl: 'http://localhost:8080/v1',
    }),
};

// ---- The response-body corpus (all doc-derived; synthetic: true) -----------
// Each case: { name, model, body } — body is the STRING fetch hands the SDK.
// Variants exercise every arm v5's `response_parse` implements per family.
function bodiesFor(provider) {
  const j = (o) => JSON.stringify(o);
  switch (provider) {
    case 'deepseek':
      return [
        { name: 'plain', model: 'deepseek-chat', body: j({ id: 'c1', object: 'chat.completion', created: 1700000000, model: 'deepseek-chat', choices: [{ index: 0, message: { role: 'assistant', content: 'Hello from DeepSeek.' }, finish_reason: 'stop' }], usage: { prompt_tokens: 12, completion_tokens: 8, total_tokens: 20 } }) },
        { name: 'reasoning-toolcalls', model: 'deepseek-reasoner', body: j({ id: 'c2', object: 'chat.completion', created: 1700000001, model: 'deepseek-reasoner', choices: [{ index: 0, message: { role: 'assistant', content: 'Checking.', reasoning_content: 'let me think about this', tool_calls: [{ id: 'call_ds_1', type: 'function', function: { name: 'search', arguments: '{"query":"x"}' } }] }, finish_reason: 'tool_calls' }], usage: { prompt_tokens: 15, completion_tokens: 9, total_tokens: 24 } }) },
        { name: 'cache-hit', model: 'deepseek-chat', body: j({ id: 'c3', object: 'chat.completion', created: 1700000002, model: 'deepseek-chat', choices: [{ index: 0, message: { role: 'assistant', content: 'Cached reply.' }, finish_reason: 'stop' }], usage: { prompt_tokens: 12, completion_tokens: 8, total_tokens: 20, prompt_cache_hit_tokens: 5, prompt_cache_miss_tokens: 7 } }) },
        { name: 'no-usage', model: 'deepseek-chat', body: j({ id: 'c4', object: 'chat.completion', created: 1700000003, model: 'deepseek-chat', choices: [{ index: 0, message: { role: 'assistant', content: 'No usage block.' }, finish_reason: 'stop' }] }) },
      ];
    case 'z-ai':
      return [
        { name: 'plain', model: 'glm-4.6', body: j({ id: 'z1', object: 'chat.completion', created: 1700000000, model: 'glm-4.6', choices: [{ index: 0, message: { role: 'assistant', content: 'Hello from Z.AI.' }, finish_reason: 'stop' }], usage: { prompt_tokens: 10, completion_tokens: 6, total_tokens: 16 } }) },
        { name: 'reasoning-toolcalls', model: 'glm-4.6', body: j({ id: 'z2', object: 'chat.completion', created: 1700000001, model: 'glm-4.6', choices: [{ index: 0, message: { role: 'assistant', content: 'On it.', reasoning_content: 'reasoning text', tool_calls: [{ id: 'call_z_1', type: 'function', function: { name: 'search', arguments: '{"query":"y"}' } }] }, finish_reason: 'tool_calls' }], usage: { prompt_tokens: 14, completion_tokens: 7, total_tokens: 21 } }) },
        { name: 'cached-tokens', model: 'glm-4.6', body: j({ id: 'z3', object: 'chat.completion', created: 1700000002, model: 'glm-4.6', choices: [{ index: 0, message: { role: 'assistant', content: 'Cache-aware.' }, finish_reason: 'stop' }], usage: { prompt_tokens: 12, completion_tokens: 5, total_tokens: 17, prompt_tokens_details: { cached_tokens: 4 } } }) },
      ];
    case 'openai-compatible':
      return [
        { name: 'plain', model: 'local-model', body: j({ id: 'oc1', object: 'chat.completion', created: 1700000000, model: 'local-model', choices: [{ index: 0, message: { role: 'assistant', content: 'Hello from a compatible host.' }, finish_reason: 'stop' }], usage: { prompt_tokens: 9, completion_tokens: 5, total_tokens: 14 } }) },
        { name: 'no-usage', model: 'local-model', body: j({ id: 'oc2', object: 'chat.completion', created: 1700000001, model: 'local-model', choices: [{ index: 0, message: { role: 'assistant', content: 'Bare-bones reply.' }, finish_reason: 'length' }] }) },
      ];
    case 'openrouter':
      // The OpenRouter REST wire (snake_case; the @openrouter/sdk's inbound
      // zod validates then renames camelCase before v4's buildLLMResponse
      // reads it). The SDK's ChatResult REQUIRES `system_fingerprint`
      // (nullable) and its ChatUsage carries cached tokens ONLY under
      // `prompt_tokens_details` — v4's cacheUsage extraction reads
      // `usage.cachedTokens`, a key the SDK never materializes, so the
      // recorded cacheUsage stays absent even on the cached case (a v4 quirk
      // the corpus pins; do not "fix" it here).
      return [
        { name: 'plain', model: 'openai/gpt-4o', body: j({ id: 'gen-1', model: 'openai/gpt-4o', object: 'chat.completion', created: 1700000000, system_fingerprint: null, choices: [{ logprobs: null, finish_reason: 'stop', index: 0, message: { role: 'assistant', content: 'Hello from OpenRouter.', refusal: null, reasoning: null } }], usage: { prompt_tokens: 10, completion_tokens: 7, total_tokens: 17 } }) },
        { name: 'reasoning', model: 'openai/gpt-4o', body: j({ id: 'gen-2', model: 'openai/gpt-4o', object: 'chat.completion', created: 1700000001, system_fingerprint: null, choices: [{ logprobs: null, finish_reason: 'stop', index: 0, message: { role: 'assistant', content: 'Reasoned reply.', refusal: null, reasoning: 'the model reasoning text' } }], usage: { prompt_tokens: 11, completion_tokens: 8, total_tokens: 19 } }) },
        { name: 'cached', model: 'openai/gpt-4o', body: j({ id: 'gen-3', model: 'openai/gpt-4o', object: 'chat.completion', created: 1700000002, system_fingerprint: null, choices: [{ logprobs: null, finish_reason: 'stop', index: 0, message: { role: 'assistant', content: 'Cache-aware reply.', refusal: null, reasoning: null } }], usage: { prompt_tokens: 12, completion_tokens: 6, total_tokens: 18, prompt_tokens_details: { cached_tokens: 3 }, cost: 0.001 } }) },
        // v4 bug 31 — the raw-wire VISION path (sendViaChatCompletions). Driven
        // WITH an image, so sendMessage escapes the SDK and reads this body
        // DIRECTLY (snake_case, message.reasoning, finish_reason || 'stop', and
        // prompt_tokens_details.cached_tokens MATERIALIZED into cacheUsage —
        // where the SDK `cached` case above leaves it absent). Same wire shape,
        // a different LLMResponse. `raw` is the wire body, not the SDK object.
        { name: 'vision', model: 'openai/gpt-4o', attachments: [{ id: 'att-img-1', filename: 'photo.png', mimeType: 'image/png', size: 68, data: 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==' }], body: j({ id: 'gen-4', model: 'openai/gpt-4o', object: 'chat.completion', created: 1700000003, choices: [{ finish_reason: 'stop', index: 0, message: { role: 'assistant', content: 'I see an image.', reasoning: 'looked closely' } }], usage: { prompt_tokens: 15, completion_tokens: 8, total_tokens: 23, prompt_tokens_details: { cached_tokens: 5 } } }) },
      ];
    case 'ollama':
      return [
        { name: 'plain', model: 'llama3.2', body: j({ model: 'llama3.2', created_at: '2026-07-23T00:00:00Z', message: { role: 'assistant', content: 'Hello from Ollama.' }, done: true, total_duration: 100, prompt_eval_count: 9, eval_count: 5 }) },
        { name: 'not-done', model: 'llama3.2', body: j({ model: 'llama3.2', created_at: '2026-07-23T00:00:01Z', message: { role: 'assistant', content: 'Cut off mid-' }, done: false, prompt_eval_count: 9, eval_count: 4 }) },
        // P4.D78 (v4 d9c5a1c7) — the two reasoning channels on the non-streaming
        // path. `thinking` is Ollama's native field (it parsed the template);
        // inline `<think>` blocks are what leaks into `content` when it did not.
        // reasoningContent is `native + inline` and is attached ONLY when the
        // concatenation is non-empty, so `plain`/`not-done` above stay bare.
        { name: 'native-thinking', model: 'qwen3:8b', body: j({ model: 'qwen3:8b', created_at: '2026-08-15T00:00:00Z', message: { role: 'assistant', content: 'The answer.', thinking: 'native musing' }, done: true, prompt_eval_count: 7, eval_count: 3 }) },
        { name: 'inline-think', model: 'qwen3:8b', body: j({ model: 'qwen3:8b', created_at: '2026-08-15T00:00:01Z', message: { role: 'assistant', content: '<think>inline musing</think>\n\nHello' }, done: true, prompt_eval_count: 8, eval_count: 4 }) },
        { name: 'both-channels', model: 'qwen3:8b', body: j({ model: 'qwen3:8b', created_at: '2026-08-15T00:00:02Z', message: { role: 'assistant', content: '<think>inline musing</think>\n\nHello', thinking: 'native musing. ' }, done: true, prompt_eval_count: 3, eval_count: 4 }) },
        // The swallowed-opening-tag shape, live-reproduced by v4 against a
        // Qwen3 GGUF in no-think mode (the shape that corrupted JSON tasks).
        { name: 'orphan-close', model: 'qwen3:8b', body: j({ model: 'qwen3:8b', created_at: '2026-08-15T00:00:03Z', message: { role: 'assistant', content: 'Okay, the user wants a JSON array of animals. Commas, quotes, brackets. Done.\n</think>\n\n["cat", "dog", "heron"]' }, done: true, prompt_eval_count: 10, eval_count: 20 }) },
        { name: 'unterminated-think', model: 'qwen3:8b', body: j({ model: 'qwen3:8b', created_at: '2026-08-15T00:00:04Z', message: { role: 'assistant', content: '<think>cut off mid-' }, done: false, prompt_eval_count: 5, eval_count: 2 }) },
        // An empty `thinking` string must NOT materialize reasoningContent.
        { name: 'empty-thinking-omitted', model: 'qwen3:8b', body: j({ model: 'qwen3:8b', created_at: '2026-08-15T00:00:05Z', message: { role: 'assistant', content: 'Plain.', thinking: '' }, done: true, prompt_eval_count: 2, eval_count: 2 }) },
        // A non-string `thinking` / `content` is ignored by v4's typeof guards.
        { name: 'non-string-channels', model: 'qwen3:8b', body: j({ model: 'qwen3:8b', created_at: '2026-08-15T00:00:06Z', message: { role: 'assistant', content: null, thinking: 42 }, done: true, prompt_eval_count: 1, eval_count: 1 }) },
      ];
    case 'openai':
      // The Responses API wire. NOTE: no top-level `output_text` — that key is
      // SDK-SYNTHESIZED (`addOutputText`); the whole point of this corpus is
      // that the SDK adds it inside the oracle loop (#24's lesson).
      return [
        { name: 'plain', model: 'gpt-4o', body: j({ id: 'resp_1', object: 'response', created_at: 1700000000, status: 'completed', model: 'gpt-4o', output: [{ type: 'message', id: 'msg_1', status: 'completed', role: 'assistant', content: [{ type: 'output_text', text: 'Hello from OpenAI.', annotations: [] }] }], parallel_tool_calls: true, usage: { input_tokens: 11, input_tokens_details: { cached_tokens: 0 }, output_tokens: 6, output_tokens_details: { reasoning_tokens: 0 }, total_tokens: 17 } }) },
        { name: 'reasoning-summary', model: 'gpt-5', body: j({ id: 'resp_2', object: 'response', created_at: 1700000001, status: 'completed', model: 'gpt-5', output: [{ type: 'reasoning', id: 'rs_1', summary: [{ type: 'summary_text', text: 'summed-up reasoning' }] }, { type: 'message', id: 'msg_2', status: 'completed', role: 'assistant', content: [{ type: 'output_text', text: 'Considered answer.', annotations: [] }] }], parallel_tool_calls: true, usage: { input_tokens: 20, input_tokens_details: { cached_tokens: 0 }, output_tokens: 12, output_tokens_details: { reasoning_tokens: 6 }, total_tokens: 32 } }) },
        { name: 'function-call', model: 'gpt-4o', body: j({ id: 'resp_3', object: 'response', created_at: 1700000002, status: 'completed', model: 'gpt-4o', output: [{ type: 'message', id: 'msg_3', status: 'completed', role: 'assistant', content: [{ type: 'output_text', text: 'Let me check.', annotations: [] }] }, { type: 'function_call', id: 'fc_1', call_id: 'call_abc', name: 'search', arguments: '{"query":"x"}', status: 'completed' }], parallel_tool_calls: true, usage: { input_tokens: 18, input_tokens_details: { cached_tokens: 0 }, output_tokens: 9, output_tokens_details: { reasoning_tokens: 0 }, total_tokens: 27 } }) },
        { name: 'cached-tokens', model: 'gpt-4o', body: j({ id: 'resp_4', object: 'response', created_at: 1700000003, status: 'completed', model: 'gpt-4o', output: [{ type: 'message', id: 'msg_4', status: 'completed', role: 'assistant', content: [{ type: 'output_text', text: 'Warm cache.', annotations: [] }] }], parallel_tool_calls: true, usage: { input_tokens: 14, input_tokens_details: { cached_tokens: 4 }, output_tokens: 5, output_tokens_details: { reasoning_tokens: 0 }, total_tokens: 19 } }) },
        { name: 'incomplete-max-tokens', model: 'gpt-4o', body: j({ id: 'resp_5', object: 'response', created_at: 1700000004, status: 'incomplete', incomplete_details: { reason: 'max_output_tokens' }, model: 'gpt-4o', output: [{ type: 'message', id: 'msg_5', status: 'incomplete', role: 'assistant', content: [{ type: 'output_text', text: 'Truncat', annotations: [] }] }], parallel_tool_calls: true, usage: { input_tokens: 10, input_tokens_details: { cached_tokens: 0 }, output_tokens: 3, output_tokens_details: { reasoning_tokens: 0 }, total_tokens: 13 } }) },
      ];
    case 'grok':
      return [
        { name: 'plain', model: 'grok-4', body: j({ id: 'resp_g1', object: 'response', created_at: 1700000000, status: 'completed', model: 'grok-4', output: [{ type: 'message', id: 'msg_g1', status: 'completed', role: 'assistant', content: [{ type: 'output_text', text: 'Hello from Grok.', annotations: [] }] }], parallel_tool_calls: true, usage: { input_tokens: 9, input_tokens_details: { cached_tokens: 0 }, output_tokens: 5, output_tokens_details: { reasoning_tokens: 0 }, total_tokens: 14 } }) },
        { name: 'function-call', model: 'grok-4', body: j({ id: 'resp_g2', object: 'response', created_at: 1700000001, status: 'completed', model: 'grok-4', output: [{ type: 'function_call', id: 'fc_g1', call_id: 'call_gk', name: 'search', arguments: '{"query":"z"}', status: 'completed' }], parallel_tool_calls: true, usage: { input_tokens: 12, input_tokens_details: { cached_tokens: 0 }, output_tokens: 4, output_tokens_details: { reasoning_tokens: 0 }, total_tokens: 16 } }) },
      ];
    case 'anthropic':
      return [
        { name: 'plain', model: 'claude-sonnet-4-5', body: j({ id: 'msg_a1', type: 'message', role: 'assistant', model: 'claude-sonnet-4-5', content: [{ type: 'text', text: 'Hello from Anthropic.' }], stop_reason: 'end_turn', stop_sequence: null, usage: { input_tokens: 9, output_tokens: 6 } }) },
        { name: 'thinking', model: 'claude-sonnet-4-5', body: j({ id: 'msg_a2', type: 'message', role: 'assistant', model: 'claude-sonnet-4-5', content: [{ type: 'thinking', thinking: 'quiet pondering', signature: 'sig-think-1' }, { type: 'text', text: 'A considered answer.' }], stop_reason: 'end_turn', stop_sequence: null, usage: { input_tokens: 15, output_tokens: 11 } }) },
        { name: 'tool-use', model: 'claude-sonnet-4-5', body: j({ id: 'msg_a3', type: 'message', role: 'assistant', model: 'claude-sonnet-4-5', content: [{ type: 'text', text: 'Let me check.' }, { type: 'tool_use', id: 'toolu_1', name: 'search', input: { query: 'x' } }], stop_reason: 'tool_use', stop_sequence: null, usage: { input_tokens: 17, output_tokens: 9 } }) },
        { name: 'cache-usage', model: 'claude-sonnet-4-5', body: j({ id: 'msg_a4', type: 'message', role: 'assistant', model: 'claude-sonnet-4-5', content: [{ type: 'text', text: 'From the warm cache.' }], stop_reason: 'end_turn', stop_sequence: null, usage: { input_tokens: 8, output_tokens: 5, cache_creation_input_tokens: 3, cache_read_input_tokens: 2 } }) },
      ];
    case 'google':
      return [
        { name: 'plain', model: 'gemini-2.5-flash', body: j({ candidates: [{ content: { parts: [{ text: 'Hello from Google.' }], role: 'model' }, finishReason: 'STOP', index: 0 }], usageMetadata: { promptTokenCount: 8, candidatesTokenCount: 5, totalTokenCount: 13 }, modelVersion: 'gemini-2.5-flash' }) },
        { name: 'thought-parts', model: 'gemini-2.5-flash', body: j({ candidates: [{ content: { parts: [{ text: 'internal thought', thought: true }, { text: 'Spoken answer.', thoughtSignature: 'sig-g-1' }], role: 'model' }, finishReason: 'STOP', index: 0 }], usageMetadata: { promptTokenCount: 12, candidatesTokenCount: 9, totalTokenCount: 21, thoughtsTokenCount: 4 }, modelVersion: 'gemini-2.5-flash' }) },
        { name: 'function-call', model: 'gemini-2.5-flash', body: j({ candidates: [{ content: { parts: [{ functionCall: { name: 'search', args: { query: 'x' } } }], role: 'model' }, finishReason: 'STOP', index: 0 }], usageMetadata: { promptTokenCount: 10, candidatesTokenCount: 6, totalTokenCount: 16 }, modelVersion: 'gemini-2.5-flash' }) },
        { name: 'cached-content', model: 'gemini-2.5-flash', body: j({ candidates: [{ content: { parts: [{ text: 'Cached context answer.' }], role: 'model' }, finishReason: 'STOP', index: 0 }], usageMetadata: { promptTokenCount: 14, candidatesTokenCount: 5, totalTokenCount: 19, cachedContentTokenCount: 4 }, modelVersion: 'gemini-2.5-flash' }) },
      ];
    default:
      return [];
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
  return new Response(stream, { status: 200, headers: { 'content-type': 'application/json' } });
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

  const SYS = { role: 'system', content: 'You are a helpful assistant.' };
  const USER = { role: 'user', content: 'Hello there.' };

  const lines = [];
  for (const c of bodiesFor(provider)) {
    let response = null;
    let failure = null;
    const origFetch = globalThis.fetch;
    globalThis.fetch = async () => makeResponse(c.body);
    // A case carrying `attachments` rides them on the (single) user message —
    // for OpenRouter this routes sendMessage onto v4 bug 31's raw-wire vision
    // path (sendViaChatCompletions), whose LLMResponse differs from the SDK
    // path. `visionPath` tells the harness to use the raw-wire parse flavor.
    const visionPath = Array.isArray(c.attachments) && c.attachments.length > 0;
    try {
      const inst = await make();
      const user = visionPath ? { ...USER, attachments: c.attachments } : USER;
      const params = {
        messages: [SYS, user],
        model: c.model,
        temperature: 0.5,
        maxTokens: 1000,
        webSearchEnabled: false,
      };
      const r = await inst.sendMessage(params, 'test-api-key');
      // What survives v4's own serialization boundary (undefined dropped,
      // getters materialized as own enumerable props only).
      response = JSON.parse(JSON.stringify(r));
    } catch (e) {
      failure = e instanceof Error ? e.message : String(e);
    } finally {
      globalThis.fetch = origFetch;
    }
    lines.push(
      JSON.stringify({
        provider,
        case: c.name,
        synthetic: true,
        model: c.model,
        body: c.body,
        response,
        error: failure,
        ...(visionPath ? { visionPath: true } : {}),
      }),
    );
    if (failure) console.error(`  [${provider}/${c.name}] sendMessage threw: ${failure}`);
  }
  writeFileSync(outPath, lines.join('\n') + '\n');
  console.error(`recorded ${lines.length} response bodies for ${provider} -> ${outPath}`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
