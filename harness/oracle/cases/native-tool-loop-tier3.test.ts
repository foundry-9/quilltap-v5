/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the native tool loop (v4 `runNativeToolLoop`,
 * lib/services/chat-message/native-tool-loop.service.ts, W4.1e).
 *
 * Drives v4's REAL `runNativeToolLoop` over the committed corpus
 * (harness/oracle/fixtures/native-tool-loop-tier3.json) against a MAIN-db-only
 * fixture (the loop's only DB write is the agent-mode `agentTurnCount` bump),
 * with ONLY the model / detection / tool-execution boundaries pinned to match the
 * Rust seams:
 *
 *   - `createLLMProvider().streamMessage` (BENEATH the real ./streaming.service
 *     funnel, P4.121 — its CHAT_MESSAGE rows are dumped and diffed) → returns each case's scripted chunk
 *     sequence in call order, RECORDING the exact
 *     `provider|model|temperature|messages` key it answered (as a `canned` row the
 *     Rust `QueuedStreamingProvider` replays). The rest of streaming.service
 *     (encode-status/safeEnqueue/applyReasoningChunk/flushReasoningSegment/
 *     nextTurnSeq) stays REAL.
 *   - `detectToolCallsInResponse` (tool-execution.service) → canned by the raw
 *     response's `marker` field. `processToolCalls` stays REAL beneath it.
 *   - `executeToolCallWithContext` (@/lib/chat/tool-executor) → canned per-call
 *     results keyed by `name|JSON.stringify(args)|callId`, mirroring the Rust
 *     `CannedToolRunner` (the real handlers are proven by the W4.1d differentials;
 *     this unit proves the LOOP orchestration).
 *
 * Everything else — the iteration control, the ghost-wrap / truncation / agent
 * submit / force-final branches, the anchor/seq stamping, the threaded-slate
 * construction, and the `agentTurnCount` write — is v4's REAL loop.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-ntl.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-native-tool-loop-fixture.ts
 *   QT_FIXTURE_NTL=/tmp/qt-ntl.db QT_ORACLE_OUT=/tmp/oracle-native-tool-loop.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- native-tool-loop-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { createRequire } from 'node:module';

// The REAL Anthropic provider plugin — its `parseToolCalls` (= the ported
// `parseAnthropicToolCalls`), the exact method v4's `detectToolCallsInResponse`
// dispatches to for an ANTHROPIC connection. W4.7c swaps the detection from the
// old marker-canned mock onto this real parse over REAL anthropic rawResponses
// (the Rust side uses `RegistryToolCallDetector::built_in()`).
const nodeRequire = createRequire(import.meta.url);
const anthropicPlugin = (() => {
  const m = nodeRequire(join(process.cwd(), 'plugins', 'dist', 'qtap-plugin-anthropic', 'index.js'));
  return m.plugin || m.default?.plugin || m.default;
})();

// Inlined canonicalizer (same as the other tier-2/3 oracles).
function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

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
}
interface CaseSpec {
  name: string;
  chatId: string;
  characterId: string;
  characterName: string;
  provider: string;
  model: string;
  temperature: number;
  agentMode: { enabled: boolean; maxTurns: number };
  initialFullResponse: string;
  // Real anthropic rawResponse: `content[]` tool_use blocks + `stop_reason`.
  initialRawResponse: unknown;
  formattedMessages: Array<{ role: string; content: string }>;
  cannedTools: CannedTool[];
  streams: ChunkSpec[][];
  dumpChats: boolean;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  chats: Array<{ id: string; participant: string; character: string }>;
  cases: CaseSpec[];
}

function toolKey(tc: { name: string; arguments: Record<string, unknown>; callId?: string }): string {
  return `${tc.name}|${JSON.stringify(tc.arguments)}|${tc.callId ?? '-'}`;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'native-tool-loop-tier3.json'), 'utf8')
  ) as Spec;

  const fixture = process.env.QT_FIXTURE_NTL;
  if (!fixture || !existsSync(fixture)) throw new Error('QT_FIXTURE_NTL must point at the seed fixture');
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ntl-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'ntl-main.db');
  copyFileSync(fixture, workMain);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  // P4.121: a fresh llm-logs DB so the un-mocked funnel's CHAT_MESSAGE
  // `logLLMCall` lands real rows to dump/diff.
  process.env.SQLITE_LLM_LOGS_PATH = join(scratch, 'data', 'llm-logs.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // Global canned tool-result map (unique keys across the corpus).
  const cannedTools = new Map<string, CannedTool>();
  for (const c of spec.cases) for (const t of c.cannedTools) cannedTools.set(toolKey(t), t);

  // Per-case mutable state the mocks read lazily.
  let currentCase: CaseSpec = spec.cases[0];
  let streamCallIndex = 0;
  const cannedRows: Array<{
    provider: string;
    model: string;
    temperature: number | null;
    messages: Array<{ role: string; content: string }>;
    sequences: ChunkSpec[][];
  }> = [];

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));

  // P4.121: run the REAL `logLLMCall` (v4's jest.setup no-ops the whole
  // llm-logging module — `jest-setup-llm-logging-service-mocked`) so the funnel's
  // CHAT_MESSAGE rows land in the llm-logs DB dumped below.
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

  // Detection: the REAL Anthropic plugin `parseToolCalls` over the REAL anthropic
  // rawResponse (`content[]` tool_use blocks) — W4.7c (keep processToolCalls REAL).
  jest.doMock('@/lib/services/chat-message/tool-execution.service', () => {
    const actual = jest.requireActual('@/lib/services/chat-message/tool-execution.service');
    return {
      __esModule: true,
      ...actual,
      detectToolCallsInResponse: (raw: unknown) => anthropicPlugin.parseToolCalls(raw),
    };
  });

  // P4.121 (the W4.11b shape, as `primary-stream-tier3` / dogfood #129): the model
  // mock sits BELOW v4's REAL `streamMessage` funnel (streaming.service.ts) — only
  // the provider it constructs (`createLLMProvider().streamMessage`) is scripted —
  // so the funnel's own terminal CHAT_MESSAGE `logLLMCall` (:479, gated `if
  // (userId)`) fires for every loop leg and lands REAL rows to dump and diff. The
  // recorded canned key is IDENTICAL to the old service-level one (`provider` =
  // `connectionProfile.provider`, `model`/`temperature` = the params the funnel
  // derives from `modelParams`/the profile, `messages` role/content).
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
          for (const chunk of seq) {
            yield chunk;
          }
        },
      }),
    };
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { createToolContext } = await import('@/lib/services/chat-message/tool-execution.service');
  const { runNativeToolLoop } = await import(
    '@/lib/services/chat-message/native-tool-loop.service'
  );

  await initializeDatabase();
  const repos = getRepositories();

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

    const streaming = {
      fullResponse: c.initialFullResponse,
      effectiveProfile: { id: '60000000-0000-4000-8000-000000000001', provider: c.provider, modelName: c.model, baseUrl: null },
      effectiveApiKey: 'test-key',
      usage: null,
      cacheUsage: null,
      attachmentResults: null,
      rawResponse: c.initialRawResponse,
      thoughtSignature: undefined,
      reasoningContent: undefined,
      reasoningSegments: undefined,
      reasoningFlushedLen: 0,
      nextTurnSeq: 0,
      hasStartedStreaming: false,
    };

    const eventSink: unknown[] = [];
    const controller = makeController(eventSink);
    const toolMessages: Array<Record<string, unknown>> = [];
    const generatedImagePaths: unknown[] = [];
    const toolContext = createToolContext(
      c.chatId,
      spec.userId,
      c.characterId,
      `${c.chatId}-pp`
    );

    await runNativeToolLoop({
      repos,
      chatId: c.chatId,
      userId: spec.userId,
      character: { id: c.characterId, name: c.characterName } as never,
      characterParticipant: { id: `${c.chatId}-pp` } as never,
      preGeneratedAssistantMessageId: c.chatId.replace(/^3/, '5'),
      agentMode: {
        enabled: c.agentMode.enabled,
        maxTurns: c.agentMode.maxTurns,
        enabledSource: 'global',
      } as never,
      formattedMessages: c.formattedMessages as never,
      modelParams: { temperature: c.temperature },
      actualTools: [{ function: { name: 'noop' } }],
      useNativeWebSearch: false,
      toolContext: toolContext as never,
      streaming: streaming as never,
      toolMessages: toolMessages as never,
      generatedImagePaths: generatedImagePaths as never,
      controller: controller as never,
      encoder,
      preservePartialOnError: async () => {},
    });

    lines.push(
      JSON.stringify({
        kind: 'result',
        case: c.name,
        result: {
          fullResponse: streaming.fullResponse,
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
  }

  for (const row of cannedRows) {
    lines.push(JSON.stringify({ kind: 'canned', ...row }));
  }

  // The chats dump (agentTurnCount / updatedAt) for the agent cases that write it.
  const columns = ((await rawQuery('PRAGMA table_info(chats)')) as Array<{ name: string }>).map(
    (r) => r.name
  );
  const rawRows = (await rawQuery('SELECT * FROM chats')) as Array<Record<string, unknown>>;
  const rows = rawRows
    .map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of columns) out[col] = r[col] ?? null;
      return out;
    })
    .sort((a, b) => String(a.id).localeCompare(String(b.id)));
  lines.push(JSON.stringify({ kind: 'table', table: 'chats', columns, rows }));

  // P4.121: the CHAT_MESSAGE `llm_logs` rows the REAL funnel wrote. The funnel's
  // `logLLMCall` is fire-and-forget (`.catch`, not awaited), so drain the pending
  // writes before reading. id/createdAt/updatedAt are placeholdered; `durationMs`
  // is collapsed to a presence marker on the Rust side (`common::dump_llm_logs`).
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
  process.stderr.write(`native-tool-loop oracle wrote ${outPath}\n`);
}

test('native-tool-loop tier-3 oracle', async () => {
  await main();
});
