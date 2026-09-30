/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the text-tool loop (v4 `runTextToolPass`,
 * lib/services/chat-message/text-tool-loop.service.ts, W4.1f).
 *
 * Drives v4's REAL `runTextToolPass` over the committed corpus
 * (harness/oracle/fixtures/text-tool-loop-tier3.json). The pass writes NOTHING to
 * the DB (`processToolCalls` only emits SSE frames + builds the ToolMessage slate;
 * the only DB write in the whole pass is the preserve closure, injected here as a
 * recording no-op). P4.121: a fresh main + llm-logs DB pair is initialized so v4's
 * REAL `streamMessage` funnel's CHAT_MESSAGE rows can be dumped and diffed; the
 * boundaries pinned to match the Rust seams are:
 *
 *   - `createLLMProvider().streamMessage` (BENEATH the real ./streaming.service
 *     funnel, P4.121) → each case's scripted chunk sequence in call order, RECORDING the exact `provider|model|temperature|messages` key
 *     (a `canned` row the Rust `QueuedStreamingProvider` replays) + the `stop`
 *     forwarded on that continuation (per-case, to prove stopSequences forwarding).
 *     The whole of streaming.service (incl. its CHAT_MESSAGE logLLMCall) is REAL.
 *   - `executeToolCallWithContext` (@/lib/chat/tool-executor) → canned per-call
 *     results keyed by `name|JSON.stringify(args)|callId`, mirroring the Rust
 *     `CannedToolRunner`. The REAL `processToolCalls` runs beneath it.
 *   - the STRATEGY → simple-json / text-block use the REAL ported strategy
 *     functions from `@/lib/tools`; the provider-text-markers cases use a synthetic
 *     `<<T:name:argsJson>>` strategy (trivially identical in TS + Rust) to exercise
 *     the strategy-agnostic engine (dedup nudge, multi-iteration, cap, failure,
 *     empty-segment assembly, stopSequences).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-text-tool-loop.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- text-tool-loop-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { createRequire } from 'node:module';

// Inlined canonicalizer (same as the other tier-2/3 oracles).
function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

// The REAL DeepSeek provider plugin — its `hasTextToolMarkers` /
// `parseTextToolCalls` / `stripTextToolMarkers` (the composite XML functions).
// W4.7c swaps the provider-text-markers cases from the old synthetic
// `<<T:...>>` strategy onto this real plugin (the Rust side uses
// `ProviderTextMarkersStrategy::built_in("DEEPSEEK")`).
const nodeRequire = createRequire(import.meta.url);
const deepseekPlugin = (() => {
  const m = nodeRequire(join(process.cwd(), 'plugins', 'dist', 'qtap-plugin-deepseek', 'index.js'));
  return m.plugin || m.default?.plugin || m.default;
})();

interface ToolCallSpec {
  name: string;
  arguments: Record<string, unknown>;
  callId?: string;
}
interface CannedTool {
  name: string;
  arguments: Record<string, unknown>;
  callId?: string;
  success: boolean;
  result: unknown;
  error?: string;
  message?: string;
}
interface ChunkSpec {
  content?: string;
  done?: boolean;
  rawResponse?: unknown;
  usage?: { promptTokens: number; completionTokens: number; totalTokens: number };
  error?: string;
}
interface CaseSpec {
  name: string;
  chatId: string;
  characterId: string;
  characterName: string;
  provider: string;
  model: string;
  temperature: number;
  strategy: 'simple-json' | 'text-block' | 'provider';
  stopSequences?: string[];
  initialFullResponse: string;
  formattedMessages: Array<{ role: string; content: string; name?: string }>;
  cannedTools: CannedTool[];
  streams: ChunkSpec[][];
  expectThrow?: boolean;
}
interface Spec {
  userId: string;
  cases: CaseSpec[];
}

function toolKey(tc: { name: string; arguments: Record<string, unknown>; callId?: string }): string {
  return `${tc.name}|${JSON.stringify(tc.arguments)}|${tc.callId ?? '-'}`;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'text-tool-loop-tier3.json'), 'utf8')
  ) as Spec;
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  // Global canned tool-result map (unique keys across the corpus).
  const cannedTools = new Map<string, CannedTool>();
  for (const c of spec.cases) for (const t of c.cannedTools) cannedTools.set(toolKey(t), t);

  // Per-case mutable state the mocks read lazily.
  let currentCase: CaseSpec = spec.cases[0];
  let streamCallIndex = 0;
  let currentStops: Array<string[]> = [];
  const cannedRows: Array<{
    provider: string;
    model: string;
    temperature: number | null;
    messages: Array<{ role: string; content: string }>;
    sequences: ChunkSpec[][];
  }> = [];

  // P4.121: a fresh main + llm-logs DB pair so v4's REAL `streamMessage` funnel's
  // CHAT_MESSAGE `logLLMCall` lands rows to dump/diff (the pass itself writes
  // nothing to main).
  const scratch = mkdtempSync(join(tmpdir(), 'qt-ttl-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
  process.env.SQLITE_PATH = join(scratch, 'ttl-main.db');
  process.env.SQLITE_LLM_LOGS_PATH = join(scratch, 'data', 'llm-logs.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  // v4's jest.setup no-ops the whole llm-logging module
  // (`jest-setup-llm-logging-service-mocked`) — run the REAL `logLLMCall`.
  jest.doMock('@/lib/services/llm-logging.service', () =>
    jest.requireActual('@/lib/services/llm-logging.service')
  );

  // The tool-executor: canned per-call (mirrors the Rust CannedToolRunner).
  jest.doMock('@/lib/chat/tool-executor', () => ({
    __esModule: true,
    executeToolCallWithContext: async (toolCall: ToolCallSpec) => {
      const hit = cannedTools.get(toolKey(toolCall));
      if (!hit) throw new Error(`no canned tool result for ${toolKey(toolCall)}`);
      return {
        toolName: hit.name,
        success: hit.success,
        result: hit.result,
        error: hit.error,
        message: hit.message,
      };
    },
    detectToolCalls: () => [],
  }));

  // P4.121 (the W4.11b shape): the model mock sits BELOW v4's REAL
  // `streamMessage` funnel — only `createLLMProvider().streamMessage` is scripted —
  // so the funnel's own CHAT_MESSAGE `logLLMCall` fires for every continuation.
  // The recorded canned key and the forwarded `stop` are the ones the old
  // service-level mock saw (`params.stop` is what the funnel hands the provider).
  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async (providerName: string, _baseUrl?: string) => ({
        streamMessage: async function* (
          params: {
            messages: Array<{ role: string; content: string }>;
            model: string;
            temperature?: number;
            stop?: string[];
          },
          _apiKey: string
        ) {
          const seq = currentCase.streams[streamCallIndex];
          streamCallIndex += 1;
          if (!seq) throw new Error(`no scripted stream #${streamCallIndex - 1} for case ${currentCase.name}`);
          cannedRows.push({
            provider: providerName,
            model: params.model,
            temperature: params.temperature ?? null,
            messages: params.messages.map((m) => ({ role: m.role, content: m.content })),
            sequences: [seq],
          });
          currentStops.push(params.stop ?? []);
          for (const chunk of seq) {
            if (chunk.error) throw new Error(chunk.error);
            yield chunk;
          }
        },
      }),
    };
  });

  // The simple-json / text-block RESPONSE functions live in pseudo-tool.service
  // (the orchestrator imports them from there); only hasTextBlockMarkers is in
  // @/lib/tools.
  const pseudo = await import('@/lib/services/chat-message/pseudo-tool.service');
  const { hasTextBlockMarkers } = await import('@/lib/tools');
  const { createToolContext } = await import('@/lib/services/chat-message/tool-execution.service');
  const { runTextToolPass } = await import('@/lib/services/chat-message/text-tool-loop.service');
  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  await initializeDatabase();

  const decoder = new TextDecoder();
  function makeController(sink: unknown[]) {
    return {
      enqueue: (u: Uint8Array) => {
        const text = decoder.decode(u);
        for (const line of text.split('\n')) {
          const t = line.trim();
          if (t.startsWith('data:')) {
            const body = t.slice('data:'.length).trim();
            if (body) sink.push(JSON.parse(body));
          }
        }
      },
      close: () => {},
      error: () => {},
    };
  }
  const encoder = new TextEncoder();

  const lines: string[] = [];

  for (const c of spec.cases) {
    currentCase = c;
    streamCallIndex = 0;
    currentStops = [];

    const streaming: Record<string, unknown> = {
      fullResponse: c.initialFullResponse,
      effectiveProfile: { id: '60000000-0000-4000-8000-000000000001', provider: c.provider, modelName: c.model, baseUrl: null },
      effectiveApiKey: 'test-key',
      usage: null,
      cacheUsage: null,
      attachmentResults: null,
      rawResponse: null,
      thoughtSignature: undefined,
      reasoningContent: undefined,
      reasoningSegments: undefined,
      reasoningFlushedLen: 0,
      nextTurnSeq: 0,
      hasStartedStreaming: false,
    };

    // Build the strategy exactly as the orchestrator does.
    let strategy: unknown;
    if (c.strategy === 'simple-json') {
      strategy = {
        name: 'simple-json',
        hasMarkers: pseudo.hasSimpleJsonInResponse,
        parse: (r: string) =>
          pseudo.parseSimpleJsonFromResponse(r, {
            provider: c.provider,
            model: c.model,
          }),
        strip: pseudo.stripSimpleJsonFromResponse,
        formatToolResult: pseudo.formatSimpleJsonToolResult,
        stopSequences: pseudo.SIMPLE_JSON_STOP,
      };
    } else if (c.strategy === 'text-block') {
      strategy = {
        name: 'text-block',
        hasMarkers: hasTextBlockMarkers,
        parse: pseudo.parseTextBlocksFromResponse,
        strip: pseudo.stripTextBlockMarkersFromResponse,
        formatToolResult: (toolName: string, content: string) => `[Tool Result: ${toolName}]\n${content}`,
      };
    } else {
      // provider-text-markers: the REAL DeepSeek plugin, wired exactly as the
      // orchestrator does (formatToolResult `[Tool Result: …]`, no stop sequences).
      strategy = {
        name: 'provider-text-markers',
        hasMarkers: (r: string) => deepseekPlugin.hasTextToolMarkers(r),
        parse: (r: string) => deepseekPlugin.parseTextToolCalls(r),
        strip: (r: string) => deepseekPlugin.stripTextToolMarkers(r),
        formatToolResult: (toolName: string, content: string) => `[Tool Result: ${toolName}]\n${content}`,
      };
    }

    const eventSink: unknown[] = [];
    const controller = makeController(eventSink);
    const toolMessages: Array<Record<string, unknown>> = [];
    const generatedImagePaths: unknown[] = [];
    const toolContext = createToolContext(c.chatId, spec.userId, c.characterId, `${c.chatId}-pp`);

    let threw = false;
    try {
      await runTextToolPass({
        chatId: c.chatId,
        userId: spec.userId,
        character: { id: c.characterId, name: c.characterName } as never,
        preGeneratedAssistantMessageId: c.chatId.replace(/^3/, '5'),
        strategy: strategy as never,
        formattedMessages: c.formattedMessages as never,
        modelParams: { temperature: c.temperature },
        continuationTools: [],
        continuationUseNativeWebSearch: false,
        toolContext: toolContext as never,
        streaming: streaming as never,
        toolMessages: toolMessages as never,
        generatedImagePaths: generatedImagePaths as never,
        controller: controller as never,
        encoder,
        preservePartialOnError: async () => {},
      });
    } catch (err) {
      if (!c.expectThrow) throw err;
      threw = true;
    }

    lines.push(
      JSON.stringify({
        kind: 'result',
        case: c.name,
        result: {
          fullResponse: streaming.fullResponse,
          usage: streaming.usage ?? null,
          cacheUsage: streaming.cacheUsage ?? null,
          rawResponse: streaming.rawResponse ?? null,
          thoughtSignature: streaming.thoughtSignature ?? null,
          threw,
          toolMessages: toolMessages.map((m) => ({
            toolName: m.toolName,
            success: m.success,
            content: m.content,
            callId: m.callId ?? null,
            anchorOffset: m.anchorOffset ?? null,
            seq: m.seq ?? null,
          })),
          generatedImagePaths,
        },
      })
    );
    lines.push(JSON.stringify({ kind: 'events', case: c.name, events: eventSink }));
    lines.push(JSON.stringify({ kind: 'stops', case: c.name, stopPerCall: currentStops }));
  }

  for (const row of cannedRows) {
    lines.push(JSON.stringify({ kind: 'canned', ...row }));
  }

  // P4.121: the CHAT_MESSAGE `llm_logs` rows the REAL funnel wrote (fire-and-forget
  // `.catch`, not awaited — drain before reading). id/createdAt/updatedAt are
  // placeholdered; `durationMs` is collapsed to a presence marker Rust-side.
  await new Promise((r) => setTimeout(r, 300));
  const { getRawLLMLogsDatabase } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const lldb = getRawLLMLogsDatabase();
  if (!lldb) throw new Error('llm-logs DB handle unavailable (degraded open?)');
  const llColumns = (lldb.pragma('table_info(llm_logs)') as Array<{ name: string }>).map(
    (c) => c.name
  );
  const llRows = (lldb.prepare('SELECT * FROM llm_logs').all() as Array<Record<string, unknown>>)
    .map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of llColumns) out[col] = canonValue(r[col]);
      out.id = '<id>';
      out.createdAt = '<ts>';
      out.updatedAt = '<ts>';
      return out;
    })
    .sort((a, b) => {
      const sa = JSON.stringify(a);
      const sb = JSON.stringify(b);
      return sa < sb ? -1 : sa > sb ? 1 : 0;
    });
  lines.push(JSON.stringify({ kind: 'llmlogs', columns: llColumns, rows: llRows }));

  await closeDatabase();
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`text-tool-loop oracle wrote ${outPath}\n`);
}

test('text-tool-loop tier-3 oracle', async () => {
  await main();
});
