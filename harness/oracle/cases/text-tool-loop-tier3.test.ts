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
 * recording no-op), so there is NO fixture / initializeDatabase — just the three
 * boundaries pinned to match the Rust seams:
 *
 *   - `streamMessage` (./streaming.service) → each case's scripted chunk sequence
 *     in call order, RECORDING the exact `provider|model|temperature|messages` key
 *     (a `canned` row the Rust `QueuedStreamingProvider` replays) + the `stop`
 *     forwarded on that continuation (per-case, to prove stopSequences forwarding).
 *     The rest of streaming.service (encodeStatus/safeEnqueue/applyReasoningChunk)
 *     stays REAL.
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
  formattedMessages: Array<{ role: string; content: string }>;
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

// The synthetic provider-text-markers strategy — a `<<T:name:argsJson>>` scanner,
// trivially identical to the Rust harness's `SyntheticStrategy`. `strip` removes
// every marker span then trims.
const SYN_OPEN = '<<T:';
const SYN_CLOSE = '>>';
function synParse(r: string): Array<{ name: string; arguments: Record<string, unknown> }> {
  const calls: Array<{ name: string; arguments: Record<string, unknown> }> = [];
  let i = 0;
  for (;;) {
    const open = r.indexOf(SYN_OPEN, i);
    if (open === -1) break;
    const close = r.indexOf(SYN_CLOSE, open + SYN_OPEN.length);
    if (close === -1) break;
    const body = r.slice(open + SYN_OPEN.length, close);
    const colon = body.indexOf(':');
    const name = body.slice(0, colon);
    const args = JSON.parse(body.slice(colon + 1)) as Record<string, unknown>;
    calls.push({ name, arguments: args });
    i = close + SYN_CLOSE.length;
  }
  return calls;
}
function synStrip(r: string): string {
  let out = '';
  let i = 0;
  for (;;) {
    const open = r.indexOf(SYN_OPEN, i);
    if (open === -1) {
      out += r.slice(i);
      break;
    }
    const close = r.indexOf(SYN_CLOSE, open + SYN_OPEN.length);
    if (close === -1) {
      out += r.slice(i);
      break;
    }
    out += r.slice(i, open);
    i = close + SYN_CLOSE.length;
  }
  return out.trim();
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

  jest.resetModules();

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

  // streamMessage: scripted sequences in call order; record the key + the stop
  // (keep the rest of streaming.service REAL).
  jest.doMock('@/lib/services/chat-message/streaming.service', () => {
    const actual = jest.requireActual('@/lib/services/chat-message/streaming.service');
    return {
      __esModule: true,
      ...actual,
      streamMessage: async function* (opts: {
        messages: Array<{ role: string; content: string }>;
        connectionProfile: { provider: string; modelName: string };
        modelParams?: { temperature?: number };
        stop?: string[];
      }) {
        const seq = currentCase.streams[streamCallIndex];
        streamCallIndex += 1;
        if (!seq) throw new Error(`no scripted stream #${streamCallIndex - 1} for case ${currentCase.name}`);
        cannedRows.push({
          provider: opts.connectionProfile.provider,
          model: opts.connectionProfile.modelName,
          temperature: opts.modelParams?.temperature ?? null,
          messages: opts.messages.map((m) => ({ role: m.role, content: m.content })),
          sequences: [seq],
        });
        currentStops.push(opts.stop ?? []);
        for (const chunk of seq) {
          if (chunk.error) throw new Error(chunk.error);
          yield chunk;
        }
      },
    };
  });

  // The simple-json / text-block RESPONSE functions live in pseudo-tool.service
  // (the orchestrator imports them from there); only hasTextBlockMarkers is in
  // @/lib/tools.
  const pseudo = await import('@/lib/services/chat-message/pseudo-tool.service');
  const { hasTextBlockMarkers } = await import('@/lib/tools');
  const { createToolContext } = await import('@/lib/services/chat-message/tool-execution.service');
  const { runTextToolPass } = await import('@/lib/services/chat-message/text-tool-loop.service');

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
      effectiveProfile: { id: 'prof-1', provider: c.provider, modelName: c.model, baseUrl: null },
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
      strategy = {
        name: 'provider-text-markers',
        hasMarkers: (r: string) => r.includes(SYN_OPEN),
        parse: synParse,
        strip: synStrip,
        formatToolResult: (toolName: string, content: string) => `[Provider Result: ${toolName}]\n${content}`,
        stopSequences: c.stopSequences && c.stopSequences.length ? c.stopSequences : undefined,
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
        preGeneratedAssistantMessageId: `${c.chatId}-msg`,
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

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`text-tool-loop oracle wrote ${outPath}\n`);
}

test('text-tool-loop tier-3 oracle', async () => {
  await main();
});
