/**
 * @jest-environment node
 *
 * Tier-3-style ORACLE for W4.1c sub-unit c.2 — v4's `processToolCalls`
 * (lib/services/chat-message/tool-execution.service.ts).
 *
 * Drives v4's REAL `processToolCalls` over the committed corpus
 * (harness/oracle/fixtures/tool-execution-process-tier3.json) with ONLY
 * `executeToolCallWithContext` mocked (the W4.1d tool-executor dispatch — both
 * differential sides inject the SAME canned per-call results, keyed by
 * `name|JSON.stringify(args)|callId`). No DB is touched (processToolCalls only
 * emits SSE frames + builds the ToolMessage slate), so no fixture / initializeDb.
 *
 * The SSE controller is a recording controller: each `data: {…}` frame is decoded
 * to JSON, so the emitted frame trace can be diffed against the Rust RecordingSink.
 * Per batch we also emit the returned `toolMessages` + `generatedImagePaths`.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-toolexec-process.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- tool-execution-process-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

interface CannedResult {
  toolName: string;
  success: boolean;
  result: unknown;
  error?: string;
  message?: string;
  metadata?: { provider?: string; model?: string; expandedPrompt?: string };
}
interface ToolCallSpec {
  name: string;
  arguments: Record<string, unknown>;
  callId?: string;
}
interface Batch {
  name: string;
  statusContext?: { characterName: string; characterId: string };
  toolCalls: ToolCallSpec[];
  cannedResults: CannedResult[];
}
interface Spec {
  batches: Batch[];
}

function keyOf(tc: ToolCallSpec): string {
  return `${tc.name}|${JSON.stringify(tc.arguments)}|${tc.callId ?? '-'}`;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'tool-execution-process-tier3.json'), 'utf8')
  ) as Spec;
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  // One global canned map across the whole corpus (keys are unique per corpus).
  const canned = new Map<string, CannedResult>();
  for (const batch of spec.batches) {
    batch.toolCalls.forEach((tc, i) => canned.set(keyOf(tc), batch.cannedResults[i]));
  }

  jest.resetModules();
  // Fully replace the tool-executor module (no `...actual`) so its heavy real
  // deps never load — processToolCalls only needs `executeToolCallWithContext`.
  jest.doMock('@/lib/chat/tool-executor', () => ({
    __esModule: true,
    executeToolCallWithContext: async (toolCall: ToolCallSpec) => {
      const hit = canned.get(keyOf(toolCall));
      if (!hit) throw new Error(`no canned result for ${keyOf(toolCall)}`);
      return hit;
    },
    detectToolCalls: () => [],
  }));

  const { processToolCalls, createToolContext } = await import(
    '@/lib/services/chat-message/tool-execution.service'
  );

  // Recording SSE controller: decode each enqueued `data: {json}\n\n` frame.
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
    };
  }
  const encoder = new TextEncoder();

  const toolContext = createToolContext('chat-1', 'user-1', 'char-1', 'pp-1');

  const batches: Array<{
    name: string;
    frames: unknown[];
    toolMessages: unknown[];
    generatedImagePaths: unknown[];
  }> = [];

  for (const batch of spec.batches) {
    const frames: unknown[] = [];
    const controller = makeController(frames);
    const result = await processToolCalls(
      batch.toolCalls as never,
      toolContext,
      controller as never,
      encoder,
      batch.statusContext
    );
    batches.push({
      name: batch.name,
      frames,
      toolMessages: result.toolMessages as unknown[],
      generatedImagePaths: result.generatedImagePaths as unknown[],
    });
  }

  fs.writeFileSync(outPath, JSON.stringify({ case: 'tool-execution-process-tier3', batches }) + '\n');
}

test('tool-execution processToolCalls tier-3 oracle', async () => {
  await main();
});
