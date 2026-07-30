/**
 * Google request-LOGIC recorder (wave 4 / W4.7c part 2).
 *
 * The genai SDK reframes `{model, contents, config}` into the wire body (and
 * reorders keys), so the google request body is the SDK's serialization — not
 * something the sans-IO builder produces. This recorder instead captures the
 * Quilltap-side request LOGIC that the SDK consumes, verified against v4's REAL
 * plugin:
 *
 *   - `contents` / `systemInstruction` / `shouldDisableTools` — from the plugin's
 *     `formatMessagesForGoogle` (called directly via bracket access; the wire
 *     `contents` are SDK-reordered so we read the plugin's own output).
 *   - `wireTools` — the `tools` array as it lands ON THE WIRE (the SDK passes
 *     `functionDeclarations` through faithfully, so the sanitized schema
 *     [`sanitizeSchemaForGoogle`, module-private] is observable here).
 *
 * Run FROM the plugin dir, Node 24, tsx:
 *   cd ~/source/quilltap-server/plugins/dist/qtap-plugin-google
 *   node <V5>/harness/oracle/providers/record-google-request.mjs --out <ndjson>
 *
 * Line shape: { case, input, contents, systemInstruction, shouldDisableTools, wireTools }.
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

function makeResponse() {
  const enc = new TextEncoder();
  const body =
    'data: {"candidates":[{"content":{"parts":[{"text":""}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":1}}\n\n';
  const stream = new ReadableStream({
    start(c) {
      c.enqueue(enc.encode(body));
      c.close();
    },
  });
  return new Response(stream, { status: 200, headers: { 'content-type': 'text/event-stream' } });
}

const SYS = { role: 'system', content: 'You are a helpful assistant.' };
const USER = { role: 'user', content: 'Hello there.' };
const ASSISTANT_TOOLCALL = {
  role: 'assistant',
  content: 'Let me look.',
  toolCalls: [{ id: 'call_1', type: 'function', function: { name: 'search', arguments: '{"query":"x"}' } }],
  thoughtSignature: 'sig-abc',
};
const TOOL_RESULT = { role: 'tool', toolCallId: 'call_1', content: '{"result":"ok"}' };
// A tool whose JSON-Schema carries UNSUPPORTED fields + lowercase types — proves
// sanitizeSchemaForGoogle (field strip + type uppercase + recursion).
const DIRTY_TOOL = {
  name: 'lookup',
  description: 'Look things up.',
  parameters: {
    type: 'object',
    $schema: 'http://json-schema.org/draft-07/schema#',
    additionalItems: true,
    properties: {
      q: { type: 'string', default: 'x', examples: ['a'] },
      n: { type: 'integer' },
      nested: { type: 'object', properties: { deep: { type: 'array', items: { type: 'string' } } }, oneOf: [] },
    },
    required: ['q'],
  },
};
const CLEAN_TOOL = {
  name: 'search',
  description: 'Search.',
  parameters: { type: 'object', properties: { query: { type: 'string' } }, required: ['query'] },
};

// P4.21 — attachment bags (mirror `loadChatFilesForLLM`'s shape).
const IMG_ATT = {
  id: 'att-img-1',
  filepath: '/api/v1/files/att-img-1',
  filename: 'photo.png',
  mimeType: 'image/png',
  size: 68,
  data: 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5CYII=',
};
const IMG_ATT_2 = {
  id: 'att-img-2',
  filepath: '/api/v1/files/att-img-2',
  filename: 'sketch.jpg',
  mimeType: 'image/jpeg',
  size: 18,
  data: '/9j/4AAQSkZJRgABAQ==',
};
const IMG_NO_DATA = { id: 'att-nodata-1', filename: 'missing.png', mimeType: 'image/png', size: 10 };

const CASES = [
  { name: 'plain', params: { model: 'gemini-2.5-flash', messages: [SYS, USER], temperature: 0.5, maxTokens: 1000, topP: 0.9 } },
  { name: 'tools-sanitize', params: { model: 'gemini-2.5-flash', messages: [SYS, USER], tools: [DIRTY_TOOL], temperature: 0.5, maxTokens: 1000, topP: 0.9 } },
  { name: 'web-search', params: { model: 'gemini-2.5-flash', messages: [SYS, USER], webSearchEnabled: true, temperature: 0.5, maxTokens: 1000, topP: 0.9 } },
  { name: 'thought-sig', params: { model: 'gemini-3-pro-preview', messages: [SYS, USER, ASSISTANT_TOOLCALL, TOOL_RESULT], tools: [CLEAN_TOOL], temperature: 0.5, maxTokens: 1000, topP: 0.9 } },
  { name: 'thinking-legacy-disable', params: { model: 'gemini-3-pro-preview', messages: [SYS, USER, { role: 'assistant', content: 'legacy no sig' }], tools: [CLEAN_TOOL], temperature: 0.5, maxTokens: 1000, topP: 0.9 } },
  // P4.21 — inlineData parts, the consecutive-user merge concatenating
  // attachments, and the no-data failure arm (part absent from contents).
  { name: 'image-attachment', params: { model: 'gemini-2.5-flash', messages: [SYS, { role: 'user', content: 'What is in this image?', attachments: [IMG_ATT] }], temperature: 0.5, maxTokens: 1000, topP: 0.9 } },
  { name: 'multi-attachment', params: { model: 'gemini-2.5-flash', messages: [SYS, { role: 'user', content: 'Compare these.', attachments: [IMG_ATT, IMG_ATT_2] }], temperature: 0.5, maxTokens: 1000, topP: 0.9 } },
  { name: 'merged-user-attachments', params: { model: 'gemini-2.5-flash', messages: [SYS, { role: 'user', content: 'First image.', attachments: [IMG_ATT] }, { role: 'user', content: 'Second image.', attachments: [IMG_ATT_2] }], temperature: 0.5, maxTokens: 1000, topP: 0.9 } },
  { name: 'attachment-no-data', params: { model: 'gemini-2.5-flash', messages: [SYS, { role: 'user', content: 'What is in this image?', attachments: [IMG_NO_DATA] }], temperature: 0.5, maxTokens: 1000, topP: 0.9 } },
];

async function main() {
  const args = parseArgs();
  const outPath = args.out;
  if (!outPath) {
    console.error('usage: --out <ndjson>');
    process.exit(1);
  }
  const { GoogleProvider } = await import(pathToFileURL(resolve('provider.ts')));
  const inst = new GoogleProvider();

  const lines = [];
  for (const c of CASES) {
    const params = { webSearchEnabled: false, ...c.params };
    const hasTools =
      (params.tools && params.tools.length > 0) || params.webSearchEnabled === true;
    // Direct call to the plugin's message formatter (private → bracket access).
    const fm = inst['formatMessagesForGoogle'](params.messages, params.model, hasTools);

    // Capture the wire `tools` (functionDeclarations pass through the SDK).
    let wireTools = null;
    const origFetch = globalThis.fetch;
    globalThis.fetch = async (url, init) => {
      if (wireTools === null && init && init.body) {
        try {
          const body = typeof init.body === 'string' ? init.body : new TextDecoder().decode(init.body);
          wireTools = JSON.parse(body).tools ?? null;
        } catch {
          /* ignore */
        }
      }
      return makeResponse();
    };
    try {
      for await (const _ of inst.streamMessage(params, 'test-key')) { /* discard */ }
    } catch {
      /* request captured before any parse error */
    } finally {
      globalThis.fetch = origFetch;
    }

    // P4.21: `attachmentResults` recorded only when non-empty, so pre-P4.21
    // vectors stay byte-identical.
    const ar = fm.attachmentResults;
    lines.push(
      JSON.stringify({
        case: c.name,
        input: params,
        contents: fm.contents,
        systemInstruction: fm.systemInstruction ?? null,
        shouldDisableTools: fm.shouldDisableTools,
        wireTools,
        ...(ar && ((ar.sent && ar.sent.length) || (ar.failed && ar.failed.length))
          ? { attachmentResults: ar }
          : {}),
      })
    );
  }

  writeFileSync(outPath, lines.join('\n') + '\n');
  console.error(`wrote ${lines.length} google case(s) → ${outPath}`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
