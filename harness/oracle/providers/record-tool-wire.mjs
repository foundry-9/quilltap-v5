/**
 * Tool-wire fixture recorder (wave 4 / W4.7c).
 *
 * Drives a v4 provider plugin's REAL tool-wire methods — `formatTools`,
 * `parseToolCalls`, `hasTextToolMarkers`, `parseTextToolCalls`,
 * `stripTextToolMarkers` — over a committed corpus and emits NDJSON the Rust
 * `model::tool_wire` differential (`tool_wire_equivalence`) diffs against. Each
 * line carries BOTH the input and v4's result, so the Rust side reads the input
 * from the recording, recomputes, and diffs the result — single-authored corpus.
 *
 * The corpus (CATALOG / PARSE_CASES / TEXT_CASES) is authored here. CATALOG holds
 * a representative slice of the REAL b.3 tool catalog (verbatim tool JSON) plus
 * synthetic edge tools exercising the `?? {}` / `?? []` / `?? ''` defaults.
 *
 * Run FROM the plugin directory (imports resolve from the plugin's own
 * node_modules — the record-stream-fixtures.mjs precedent), Node 24:
 *
 *   cd ~/source/quilltap-server/plugins/dist/qtap-plugin-<name>
 *   node <V5>/harness/oracle/providers/record-tool-wire.mjs \
 *     --provider <name> --out /tmp/tool-wire-<name>.ndjson
 *
 * `regenerate-tool-wire.sh` drives all four representative providers (one per
 * tool_format branch + the google special) and concatenates into the committed
 * `fixtures/tool-wire/tool-wire.recorded.ndjson`.
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
  anthropic: async () => (await import(pathToFileURL(resolve('index.ts')))).default,
  openai: async () => (await import(pathToFileURL(resolve('index.ts')))).default,
  google: async () => (await import(pathToFileURL(resolve('index.ts')))).default,
  deepseek: async () => (await import(pathToFileURL(resolve('index.ts')))).default,
};

// Assemble the Claude-Code namespaced close tag without writing the literal
// sequence (it collides with this file's own authoring format).
const P = 'parameter';
const ANTML = 'antml' + ':' + P;

// --- CATALOG: real b.3 tools (verbatim) + synthetic edge tools ---------------
const CATALOG = [
  {
    type: 'function',
    function: {
      name: 'rng',
      description: 'Roll dice, flip coins, or spin the bottle for random outcomes.',
      parameters: {
        type: 'object',
        properties: {
          type: { type: 'string', enum: ['dice', 'coin', 'bottle'] },
          rolls: { type: 'number', default: 1 },
        },
        required: ['type'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'search',
      description: 'Search memories, conversations, documents, and knowledge.',
      parameters: {
        type: 'object',
        properties: { query: { type: 'string' }, limit: { type: 'number' } },
        required: ['query'],
      },
    },
  },
  // Edge: no description, no parameters (exercise `?? ''` / `?? {}` / `?? []`).
  { type: 'function', function: { name: 'edge_bare' } },
  // Edge: parameters present but no properties / required.
  { type: 'function', function: { name: 'edge_no_props', description: 'x', parameters: { type: 'object' } } },
  // Edge: NOT a function-shaped tool (dropped by every formatTools).
  { name: 'edge_not_a_function', description: 'bogus' },
];

// --- PARSE_CASES: raw responses per provider ---------------------------------
const PARSE_CASES = {
  anthropic: [
    { case: 'content-tooluse', response: { content: [{ type: 'text', text: 'hi' }, { type: 'tool_use', id: 't1', name: 'search', input: { query: 'x' } }] } },
    { case: 'content-no-id', response: { content: [{ type: 'tool_use', name: 'rng', input: {} }] } },
    { case: 'content-multi', response: { content: [{ type: 'tool_use', id: 'a', name: 'rng', input: { type: 'dice' } }, { type: 'tool_use', id: 'b', name: 'search', input: { query: 'q', limit: 5 } }] } },
    { case: 'no-content', response: { stop_reason: 'end_turn' } },
    { case: 'content-not-array', response: { content: 'nope' } },
    { case: 'int-valued-input', response: { content: [{ type: 'tool_use', id: 't', name: 'rng', input: { rolls: 2.0 } }] } },
    // Recorded rawResponse (from W4.7b anthropic-thinking-tools).
    { case: 'recorded', response: { id: 'msg_02', type: 'message', role: 'assistant', content: [{ type: 'thinking', thinking: 'I should call the tool.', signature: 'AbC123' }, { type: 'text', text: 'Let me look.' }, { type: 'tool_use', id: 'toolu_1', name: 'get_weather', input: { location: 'Paris' } }], model: 'claude-opus-4-6', stop_reason: 'tool_use', usage: { input_tokens: 100, output_tokens: 25 } } },
  ],
  openai: [
    { case: 'message-tool_calls', response: { choices: [{ message: { tool_calls: [{ id: 'c1', type: 'function', function: { name: 'search', arguments: '{"query":"x"}' } }] } }] } },
    { case: 'direct-tool_calls', response: { tool_calls: [{ id: 'd1', type: 'function', function: { name: 'rng', arguments: '{"type":"dice","rolls":3}' } }] } },
    { case: 'camel-toolCalls', response: { toolCalls: [{ id: 'e1', type: 'function', function: { name: 'rng', arguments: '{}' } }] } },
    { case: 'delta-tool_calls', response: { choices: [{ delta: { tool_calls: [{ id: 'f1', type: 'function', function: { name: 'search', arguments: '{"query":"y"}' } }] } }] } },
    { case: 'incomplete-args', response: { tool_calls: [{ id: 'g1', type: 'function', function: { name: 'search', arguments: '{"query":"z"' } }] } },
    { case: 'non-function-type', response: { tool_calls: [{ id: 'h1', type: 'custom', function: { name: 'x', arguments: '{}' } }] } },
    { case: 'int-valued-args', response: { tool_calls: [{ id: 'i1', type: 'function', function: { name: 'rng', arguments: '{"rolls":2.0,"nested":{"n":5.0}}' } }] } },
    { case: 'no-id', response: { tool_calls: [{ type: 'function', function: { name: 'rng', arguments: '{}' } }] } },
    // Recorded rawResponse (from W4.7b openai-tools) — kept partial (tool_calls block).
    { case: 'recorded', response: { id: 'resp_3', object: 'chat.completion', model: 'gpt-5', choices: [{ index: 0, message: { role: 'assistant', content: '', tool_calls: [{ id: 'fc_1', type: 'function', function: { name: 'get_weather', arguments: '{"city":"Paris"}' } }] }, finish_reason: 'tool_calls' }] } },
  ],
  deepseek: [
    // Recorded rawResponse (from W4.7b deepseek-tools) — escaped-quote args.
    { case: 'recorded', response: { choices: [{ index: 0, message: { role: 'assistant', content: '', tool_calls: [{ id: 'call_abc', type: 'function', function: { name: 'get_weather', arguments: '{"location":"Paris, \\"FR\\""}' } }] }, finish_reason: 'tool_calls' }] } },
    { case: 'message-tool_calls', response: { choices: [{ message: { tool_calls: [{ id: 'j1', type: 'function', function: { name: 'search', arguments: '{"query":"a"}' } }] } }] } },
  ],
  google: [
    { case: 'functionCall-parts', response: { candidates: [{ content: { parts: [{ functionCall: { name: 'search', args: { query: 'x' } } }] } }] } },
    { case: 'functionCalls-branch', response: { functionCalls: [{ name: 'rng', args: { type: 'dice', rolls: 6 } }] } },
    { case: 'no-args', response: { candidates: [{ content: { parts: [{ functionCall: { name: 'edge_bare' } }] } }] } },
    { case: 'no-candidates', response: { text: 'plain' } },
    { case: 'int-valued-args', response: { candidates: [{ content: { parts: [{ functionCall: { name: 'rng', args: { rolls: 2.0 } } }] } }] } },
  ],
};

// --- TEXT_CASES: shared across providers -------------------------------------
const TEXT_CASES = [
  { case: 'plain-no-markers', text: 'Just a normal response with no tool calls at all.' },
  { case: 'function_calls-claude', text: 'Sure. <function_calls><invoke name="search"><' + P + ' name="query">hello world</' + P + '></invoke></function_calls> done.' },
  { case: 'function_calls-deepseek-num', text: '<function_calls><invoke name="rng"><' + P + ' name="rolls" string="false">3</' + P + '><' + P + ' name="type" string="true">dice</' + P + '></invoke></function_calls>' },
  { case: 'function_calls-deepseek-bool', text: '<function_calls><invoke name="do"><' + P + ' name="flag" string="false">true</' + P + '></invoke></function_calls>' },
  { case: 'antml-params', text: '<function_calls><invoke name="search"><' + ANTML + ' name="query">alpha</' + ANTML + '></invoke></function_calls>' },
  { case: 'tool_call-generic', text: 'x <tool_call><name>search</name><arguments><query>abc</query><limit>5</limit></arguments></tool_call> y' },
  { case: 'function_call-format', text: '<function_call name="generate_image"><param name="prompt">a cat</param></function_call>' },
  { case: 'tool_use-bare-json', text: '<tool_use>{"name":"search","input":{"query":"json here"}}</tool_use>' },
  { case: 'tool_use-xml-children', text: '<tool_use><name>rng</name><arguments><type>coin</type></arguments></tool_use>' },
  { case: 'tool_use-attributed', text: '<tool_use name="search"><arguments>{"query":"attr"}</arguments></tool_use>' },
  { case: 'tool_use-json-args', text: '<tool_use><name>rng</name><input>{"rolls":2}</input></tool_use>' },
  { case: 'bare-invoke-kimi', text: 'text <invoke name="search"><' + P + ' name="query">kimi</' + P + '></invoke> more' },
  { case: 'alias-normalize', text: '<function_calls><invoke name="memory_search"><' + P + ' name="query">mem</' + P + '></invoke></function_calls>' },
  { case: 'search-first-value-fallback', text: '<function_calls><invoke name="search"><' + P + ' name="something">fallback</' + P + '></invoke></function_calls>' },
  { case: 'strip-whitespace', text: 'Line one.\n\n\n<function_calls><invoke name="rng"><' + P + ' name="type">dice</' + P + '></invoke></function_calls>\n\n\nLine   two.' },
  { case: 'multi-format-dedup', text: '<function_calls><invoke name="search"><' + P + ' name="query">first</' + P + '></invoke></function_calls> and <tool_call><name>rng</name><arguments><type>coin</type></arguments></tool_call>' },
  { case: 'unicode-content', text: 'Café ☕ <function_calls><invoke name="search"><' + P + ' name="query">naïve 日本</' + P + '></invoke></function_calls> tail' },
];

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
  const plugin = await make();
  const lines = [];
  const emit = (kind, caseName, input, result) => {
    lines.push(JSON.stringify({ kind, provider, case: caseName, input, result }));
  };

  // formatTools over the shared catalog.
  emit('formatTools', 'catalog', CATALOG, plugin.formatTools(CATALOG));

  // parseToolCalls per provider case.
  for (const c of PARSE_CASES[provider] || []) {
    emit('parseToolCalls', c.case, c.response, plugin.parseToolCalls(c.response));
  }

  // Text markers over the shared text corpus.
  for (const c of TEXT_CASES) {
    emit('hasMarkers', c.case, c.text, plugin.hasTextToolMarkers(c.text));
    emit('parseText', c.case, c.text, plugin.parseTextToolCalls(c.text));
    emit('stripText', c.case, c.text, plugin.stripTextToolMarkers(c.text));
  }

  writeFileSync(outPath, lines.join('\n') + '\n');
  console.error(`wrote ${lines.length} line(s) for ${provider} → ${outPath}`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
