/**
 * @jest-environment node
 *
 * Tier-3 (mocked-`streamMessage`) ORACLE for the primary-stream / recovery /
 * provider-failover services (Phase-3 Unit-3 wave 3):
 *   - `runPrimaryStream` / `makePreservePartialOnError` / `findPreviousResponseId`
 *     (lib/services/chat-message/primary-stream.service.ts)
 *   - `attemptRequestLimitRecovery` (recovery.service.ts)
 *   - `attemptEmptyResponseRecovery` (provider-failover.service.ts)
 *
 * Drives v4's REAL services over the committed corpus
 * (harness/oracle/fixtures/primary-stream-tier3.json) against a REAL main-DB
 * fixture, with ONLY the `streamMessage` seam mocked (the wave-2.3 contract) —
 * plus the `resolveProviderForDangerousContent` seam (a repo-reading +
 * key-decryption host concern the Rust side injects as `DangerousContentRouter`).
 *
 * The `streamMessage` mock:
 *   - finds the call's `originalMessage` MARKER inside any user message (the
 *     oracle plants it in the primary/failover messages; the recovery service
 *     embeds it in its recovery user message too), resolves the streamLabel, and
 *     pops the next ATTEMPT in order for that label (so a tool-unsupported retry /
 *     empty retry / the primary-then-recovery pair each get a different sequence);
 *   - RECORDS the exact `provider|model|temperature|messages` canned key it
 *     answered (emitted as `kind:"canned"` rows) so the Rust `CannedStreamingProvider`
 *     replays exactly those — a prompt/params divergence surfaces as a canned-miss;
 *   - yields the attempt's chunks (content / reasoning / done) and, on an `error`
 *     chunk, throws AFTER yielding the chunks before it (v4 streams throw mid-stream).
 *
 * A recording controller/encoder captures every enqueued SSE frame, decoded back
 * to its single-key JSON — the `events` trace the Rust `RecordingSink` is diffed
 * against.
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-primary-stream.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-primary-stream-fixture.ts
 *   QT_FIXTURE_PRIMARY_STREAM=/tmp/qt-primary-stream.db \
 *   QT_ORACLE_OUT=/tmp/oracle-primary-stream.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- primary-stream-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

// Inlined canonicalizer (same as the other tier-2/3 oracles).
function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}
function canonicalizeRows(opts: {
  table: string;
  columns: string[];
  rawRows: Array<Record<string, unknown>>;
  orderBy?: string;
}): { table: string; columns: string[]; rows: Array<Record<string, unknown>> } {
  const { table, columns, rawRows, orderBy = 'id' } = opts;
  const rows = rawRows
    .map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of columns) out[col] = canonValue(r[col]);
      return out;
    })
    .sort((a, b) => {
      const av = String(a[orderBy] ?? '');
      const bv = String(b[orderBy] ?? '');
      return av < bv ? -1 : av > bv ? 1 : 0;
    });
  return { table, columns, rows };
}

interface ChunkSpec {
  content?: string;
  reasoning?: string;
  done?: boolean;
  usage?: { promptTokens: number; completionTokens: number; totalTokens: number } | null;
  error?: string;
}
interface ProfileSpec {
  id: string;
  userId: string;
  name: string;
  provider: string;
  modelName: string;
  baseUrl: string | null;
}
interface CharacterSpec {
  id: string;
  name: string;
  aliases: string[];
}
interface AttachedFileSpec {
  filename: string;
  mimeType: string;
  size: number;
}
interface CallSpec {
  name: string;
  kind: 'primary' | 'recovery' | 'failover' | 'findPrevResponseId';
  chatId?: string;
  participantId?: string;
  preGeneratedMessageId?: string;
  hasTools?: boolean;
  streamLabel?: string;
  attachedFiles?: AttachedFileSpec[];
  originalMessage?: string;
  errorMessage?: string;
  contentWasFlaggedDangerous?: boolean;
  dangerMode?: string;
  provider?: string;
  existingMessages?: Array<Record<string, unknown>>;
  expectThrow?: boolean;
}
interface Spec {
  testPepperBase64: string;
  sentinel: string;
  profile: ProfileSpec;
  uncensoredProfile: ProfileSpec;
  userId: string;
  character: CharacterSpec;
  streams: Record<string, ChunkSpec[][]>;
  calls: CallSpec[];
}

// A minimal ConnectionProfile shape the services read.
function toConnectionProfile(p: ProfileSpec): Record<string, unknown> {
  return {
    id: p.id,
    userId: p.userId,
    name: p.name,
    provider: p.provider,
    modelName: p.modelName,
    baseUrl: p.baseUrl,
  };
}

// A recording SSE controller: decode each enqueued `data: {...}\n\n` frame back
// to its single-key JSON object (comment/keep-alive lines are ignored).
function makeRecordingController(events: unknown[]): {
  controller: { enqueue: (b: Uint8Array) => void; close: () => void };
  encoder: TextEncoder;
} {
  const decoder = new TextDecoder();
  const encoder = new TextEncoder();
  const controller = {
    enqueue(bytes: Uint8Array) {
      const text = decoder.decode(bytes);
      for (const line of text.split('\n')) {
        const trimmed = line.trimEnd();
        if (!trimmed.startsWith('data: ')) continue;
        events.push(JSON.parse(trimmed.slice('data: '.length)));
      }
    },
    close() {},
  };
  return { controller, encoder };
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'primary-stream-tier3.json'), 'utf8')
  ) as Spec;

  const fixture = process.env.QT_FIXTURE_PRIMARY_STREAM;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_PRIMARY_STREAM must point at the seed fixture');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-primary-stream-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'primary-stream.db');
  copyFileSync(fixture, workMain);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // Recorded canned-stream entries, keyed by the exact call key (must match
  // `quilltap_core::model::stream::canned_stream_key`). `sequences` is an ORDERED
  // list of chunk-sequences: a key hit more than once (the tool-unsupported retry
  // re-issues the SAME messages/model/temperature) records each call in order, so
  // the Rust side replays them from a per-key queue.
  const cannedRecorded = new Map<
    string,
    {
      provider: string;
      model: string;
      temperature: number | null;
      messages: Array<{ role: string; content: string }>;
      sequences: ChunkSpec[][];
    }
  >();

  // Per-label attempt cursor.
  const attemptCursor = new Map<string, number>();

  // Restore the real DB stack past jest.setup's global mocks.
  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories')
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));

  // Mock ONLY streamMessage; keep the real encode* helpers + safeEnqueue.
  jest.doMock('@/lib/services/chat-message/streaming.service', () => {
    const actual = jest.requireActual('@/lib/services/chat-message/streaming.service');
    return {
      __esModule: true,
      ...actual,
      streamMessage: async function* streamMessage(options: {
        messages: Array<{ role: string; content: string }>;
        connectionProfile: { provider: string; modelName: string };
        modelParams: Record<string, unknown>;
      }) {
        const messages = options.messages.map((m) => ({ role: m.role, content: m.content }));
        // Resolve the label: which call's marker appears in any user message?
        let label: string | undefined;
        for (const call of spec.calls) {
          if (!call.originalMessage || !call.streamLabel) continue;
          if (messages.some((m) => m.content.includes(call.originalMessage as string))) {
            label = call.streamLabel;
            break;
          }
        }
        if (!label) throw new Error(`streamMessage mock: no marker matched in ${JSON.stringify(messages)}`);

        const attempts = spec.streams[label];
        if (!attempts) throw new Error(`streamMessage mock: no streams for label ${label}`);
        const idx = attemptCursor.get(label) ?? 0;
        attemptCursor.set(label, idx + 1);
        const chunks = attempts[idx];
        if (!chunks) throw new Error(`streamMessage mock: exhausted attempts for label ${label} (idx ${idx})`);

        // Record the canned key (dedup).
        const provider = options.connectionProfile.provider;
        const model = options.connectionProfile.modelName;
        const temperature =
          (options.modelParams.temperature as number | undefined) ?? null;
        const key = `${provider}|${model}|${temperature ?? '-'}|${JSON.stringify(messages)}`;
        const entry = cannedRecorded.get(key);
        if (entry) {
          entry.sequences.push(chunks);
        } else {
          cannedRecorded.set(key, { provider, model, temperature, messages, sequences: [chunks] });
        }

        for (const chunk of chunks) {
          if (chunk.error) throw new Error(chunk.error);
          if (chunk.reasoning) {
            yield { reasoningContent: chunk.reasoning };
          } else if (chunk.done) {
            yield {
              done: true,
              usage: chunk.usage ?? undefined,
              cacheUsage: undefined,
              attachmentResults: undefined,
              rawResponse: undefined,
            };
          } else {
            yield { content: chunk.content };
          }
        }
      },
    };
  });

  // Mock the dangerous-content routing seam (the Rust side injects it).
  jest.doMock('@/lib/services/dangerous-content/provider-routing.service', () => {
    const actual = jest.requireActual(
      '@/lib/services/dangerous-content/provider-routing.service'
    );
    return {
      __esModule: true,
      ...actual,
      resolveProviderForDangerousContent: async () => ({
        rerouted: true,
        connectionProfile: toConnectionProfile(spec.uncensoredProfile),
        apiKey: 'uncensored-key',
        reason: 'canned uncensored reroute',
      }),
    };
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const {
    runPrimaryStream,
    makePreservePartialOnError,
    findPreviousResponseId,
  } = await import('@/lib/services/chat-message/primary-stream.service');
  const { attemptRequestLimitRecovery } = await import(
    '@/lib/services/chat-message/recovery.service'
  );
  const { attemptEmptyResponseRecovery } = await import(
    '@/lib/services/chat-message/provider-failover.service'
  );

  await initializeDatabase();
  const repos = getRepositories();

  const lines: string[] = [];

  const freshStreamingState = () => ({
    fullResponse: '',
    effectiveProfile: toConnectionProfile(spec.profile),
    effectiveApiKey: 'primary-key',
    usage: null,
    cacheUsage: null,
    attachmentResults: null,
    rawResponse: undefined,
    thoughtSignature: undefined,
    reasoningContent: undefined,
    reasoningSegments: undefined,
    reasoningFlushedLen: 0,
    nextTurnSeq: 0,
    hasStartedStreaming: false,
  });

  const userMessages = (marker: string) => [
    { role: 'system', content: `You are ${spec.character.name}.` },
    { role: 'user', content: marker },
  ];

  for (const call of spec.calls) {
    const events: unknown[] = [];
    const { controller, encoder } = makeRecordingController(events);
    let result: unknown = null;
    let threw: string | null = null;

    if (call.kind === 'findPrevResponseId') {
      result = findPreviousResponseId(call.provider as never, call.existingMessages as never) ?? null;
    } else if (call.kind === 'primary') {
      const streaming = freshStreamingState();
      const character = { id: spec.character.id, name: spec.character.name, aliases: spec.character.aliases };
      const characterParticipant = { id: call.participantId as string };
      const preserve = makePreservePartialOnError({
        repos,
        chatId: call.chatId as string,
        character: character as never,
        characterParticipant,
        streaming: streaming as never,
        preGeneratedAssistantMessageId: call.preGeneratedMessageId as string,
      });
      try {
        const psResult = await runPrimaryStream({
          repos,
          chatId: call.chatId as string,
          userId: spec.userId,
          chat: { isPaused: false } as never,
          character: character as never,
          characterParticipant,
          userParticipantId: null,
          isMultiCharacter: false,
          formattedMessages: userMessages(call.originalMessage as string) as never,
          modelParams: { temperature: 1.0, maxTokens: 4096 },
          actualTools: call.hasTools ? [{ function: { name: 'noop' } }] : [],
          useNativeWebSearch: false,
          preGeneratedAssistantMessageId: call.preGeneratedMessageId as string,
          attachedFiles: (call.attachedFiles ?? []) as never,
          originalMessage: call.originalMessage,
          connectionProfile: toConnectionProfile(spec.profile) as never,
          streaming: streaming as never,
          controller: controller as never,
          encoder,
          preservePartialOnError: preserve,
        });
        result = { earlyReturn: psResult.earlyReturn ?? null, fullResponse: streaming.fullResponse };
      } catch (e) {
        threw = e instanceof Error ? e.message : String(e);
        result = { threw, fullResponse: streaming.fullResponse };
      }
    } else if (call.kind === 'recovery') {
      const rr = await attemptRequestLimitRecovery({
        controller: controller as never,
        encoder,
        character: { id: spec.character.id, name: spec.character.name } as never,
        connectionProfile: toConnectionProfile(spec.profile) as never,
        apiKey: 'primary-key',
        attachedFiles: (call.attachedFiles ?? []) as never,
        originalMessage: call.originalMessage,
        error: new Error(call.errorMessage as string),
        repos,
        chatId: call.chatId as string,
        userId: spec.userId,
        characterParticipantId: call.participantId as string,
      });
      result = {
        success: rr.success,
        messageId: rr.messageId ?? null,
        isStaticFallback: rr.isStaticFallback,
        response: rr.response ?? null,
      };
    } else if (call.kind === 'failover') {
      const streaming = freshStreamingState();
      const flags = await attemptEmptyResponseRecovery({
        state: streaming as never,
        toolMessagesLength: 0,
        contentWasFlaggedDangerous: !!call.contentWasFlaggedDangerous,
        dangerSettings: {
          mode: call.dangerMode,
          uncensoredTextProfileId:
            call.dangerMode === 'AUTO_ROUTE' ? spec.uncensoredProfile.id : undefined,
        } as never,
        connectionProfile: toConnectionProfile(spec.profile) as never,
        formattedMessages: userMessages(call.originalMessage as string) as never,
        modelParams: { temperature: 1.0, maxTokens: 4096 },
        actualTools: [],
        useNativeWebSearch: false,
        userId: spec.userId,
        chatId: call.chatId as string,
        character: { id: spec.character.id, name: spec.character.name } as never,
        controller: controller as never,
        encoder,
        preGeneratedAssistantMessageId: undefined,
      });
      result = {
        uncensoredRetryAttempted: flags.uncensoredRetryAttempted,
        sameProviderRetryAttempted: flags.sameProviderRetryAttempted,
        fullResponse: streaming.fullResponse,
        effectiveProfileId: streaming.effectiveProfile.id,
      };
    }

    lines.push(JSON.stringify({ kind: 'result', call: call.name, result }));
    lines.push(JSON.stringify({ kind: 'events', call: call.name, events }));
  }

  for (const entry of cannedRecorded.values()) {
    lines.push(JSON.stringify({ kind: 'canned', ...entry }));
  }

  const dumpTable = async (table: string, orderBy: string) => {
    const columns = (
      (await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>
    ).map((c) => c.name);
    const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };

  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('chat_messages', 'id')) }));
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('chats', 'id')) }));

  await closeDatabase();

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`primary-stream oracle wrote ${outPath}\n`);
}

test('primary-stream tier-3 oracle', async () => {
  await main();
});
