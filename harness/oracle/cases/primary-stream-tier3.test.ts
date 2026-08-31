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
interface ApiKeySpec {
  id: string;
  provider: string;
  label: string;
  keyValue: string;
}
interface ProfileSpec {
  id: string;
  userId: string;
  name: string;
  provider: string;
  modelName: string;
  baseUrl: string | null;
  apiKeyId?: string | null;
  modelClass?: string | null;
  fallbackProfileId?: string | null;
  allowTierFallback?: boolean;
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
  kind: 'primary' | 'recovery' | 'failover' | 'hardFailover' | 'findPrevResponseId';
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
  isDangerousRouted?: boolean;
}
interface Spec {
  testPepperBase64: string;
  sentinel: string;
  profile: ProfileSpec;
  uncensoredProfile: ProfileSpec;
  understudyProfile: ProfileSpec;
  tierSpareProfile: ProfileSpec;
  apiKeys: ApiKeySpec[];
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
    // P4.D135: the chain reads these off the profile it is handed. The primary
    // names an understudy and opts in to the tier pick; the others carry the
    // neutral defaults, so B's own understudy is never followed (chains do not
    // recurse) and only the primary can draft a spare.
    apiKeyId: p.apiKeyId ?? null,
    modelClass: p.modelClass ?? null,
    fallbackProfileId: p.fallbackProfileId ?? null,
    allowTierFallback: p.allowTierFallback ?? false,
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
  // W4.11b: a fresh llm-logs DB so the un-mocked CHAT_MESSAGE `logLLMCall` — which
  // fires inside the REAL `streamMessage` wrapper — lands real rows to dump/diff.
  process.env.SQLITE_LLM_LOGS_PATH = join(scratch, 'data', 'llm-logs.db');
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

  // W4.11b: relocate the model mock BELOW the wrapper. The REAL service-level
  // `streamMessage` (streaming.service.ts) now runs — and with it its terminal
  // CHAT_MESSAGE `logLLMCall` (:407, gated `if (userId)`) — while ONLY the
  // provider it constructs (`createLLMProvider().streamMessage`) is canned. The
  // recorded canned key is IDENTICAL to the old service-level key (the wrapper's
  // provider param = `connectionProfile.provider`, `params.model` =
  // `connectionProfile.modelName`, `params.temperature` = `modelParams.temperature`,
  // `params.messages` role/content = the formatted messages), so the Rust
  // `QueuedStreamingProvider` replay keys are unchanged.
  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async (providerName: string, _baseUrl?: string) => ({
        streamMessage: async function* streamMessage(
          params: {
            messages: Array<{ role: string; content: string }>;
            model: string;
            temperature?: number;
          },
          _apiKey: string
        ) {
          const messages = params.messages.map((m) => ({ role: m.role, content: m.content }));
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

          // Record the canned key (dedup) — provider from `createLLMProvider`,
          // model/temperature from the provider params.
          const temperature = params.temperature ?? null;
          const key = `${providerName}|${params.model}|${temperature ?? '-'}|${JSON.stringify(messages)}`;
          const entry = cannedRecorded.get(key);
          if (entry) {
            entry.sequences.push(chunks);
          } else {
            cannedRecorded.set(key, { provider: providerName, model: params.model, temperature, messages, sequences: [chunks] });
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
                rawProviderUsage: undefined,
                attachmentResults: undefined,
                rawResponse: undefined,
              };
            } else {
              yield { content: chunk.content };
            }
          }
        },
      }),
    };
  });

  // W4.11b: run the REAL `logLLMCall` so the wrapper's CHAT_MESSAGE rows land in
  // the llm-logs DB (dumped below). Recovery's `streamMessage` passes no userId, so
  // it does not log (faithful).
  jest.doMock('@/lib/services/llm-logging.service', () =>
    jest.requireActual('@/lib/services/llm-logging.service')
  );

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

  // W4.11b: the wrapper logs `durationMs = Date.now() - startTime`. Freeze
  // `Date.now` so it is deterministically 0 — matching the Rust port's hard-coded
  // `Some(0.0)` (a real stream clock is a spine-injected follow-up, per the W4.7e3
  // note). `new Date()` (the repos' minted createdAt/updatedAt) is UNTOUCHED — and
  // those columns are placeholdered in the dumps anyway.
  Date.now = () => Date.parse('2020-01-01T00:00:00.000Z');

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
          // P4.D135: no `primary` case is danger-routed, and none carries an
          // image — the fallback shapes get their own `hardFailover` cases
          // below, where the flags matter.
          isDangerousRouted: false,
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
        // v4's `restreamInto` logs the CHAT_MESSAGE row against this id (the
        // wrapper passes NO `characterId`, so those rows carry `characterId=NULL`).
        preGeneratedAssistantMessageId: call.preGeneratedMessageId,
        // P4.D135: the THIRD recovery. Present `repos` + `fallbackContext` is
        // what enables it; absent keeps the pre-4.10 two-step behaviour, which
        // is what every pre-existing case here still exercises when its second
        // step produced content.
        repos,
        fallbackContext: { dangerous: false, needsVision: false, needsTools: false },
      });
      result = {
        uncensoredRetryAttempted: flags.uncensoredRetryAttempted,
        sameProviderRetryAttempted: flags.sameProviderRetryAttempted,
        chainFallbackAttempted: flags.chainFallbackAttempted,
        chainAttempts: flags.chainAttempts.map((a) => ({
          profileId: a.profileId,
          profileName: a.profileName,
          provider: a.provider,
          modelName: a.modelName,
          trigger: a.trigger,
          error: a.error,
        })),
        fullResponse: streaming.fullResponse,
        effectiveProfileId: streaming.effectiveProfile.id,
      };
    } else if (call.kind === 'hardFailover') {
      // P4.D135: `runPrimaryStream`'s catch-all, which since `65f5021c8` walks
      // the chain before it rethrows. Driven through the REAL `runPrimaryStream`
      // rather than `attemptHardErrorFailover` directly, because the two things
      // worth pinning are exactly the ones only the whole path shows: that the
      // chain runs AFTER the tool-unsupported and request-limit branches have
      // declined, and that an exhausted chain's summary reaches the rethrown
      // error's message.
      const streaming = freshStreamingState();
      const character = {
        id: spec.character.id,
        name: spec.character.name,
        aliases: spec.character.aliases,
      };
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
          actualTools: [],
          useNativeWebSearch: false,
          preGeneratedAssistantMessageId: call.preGeneratedMessageId as string,
          attachedFiles: (call.attachedFiles ?? []) as never,
          originalMessage: call.originalMessage,
          connectionProfile: toConnectionProfile(spec.profile) as never,
          isDangerousRouted: !!call.isDangerousRouted,
          streaming: streaming as never,
          controller: controller as never,
          encoder,
          preservePartialOnError: preserve,
        });
        result = {
          earlyReturn: psResult.earlyReturn ?? null,
          fullResponse: streaming.fullResponse,
          effectiveProfileId: streaming.effectiveProfile.id,
          // The swap's buffer reset is only measurable against a DIRTY state:
          // a failed attempt that left reasoning behind before it died.
          reasoningContent: streaming.reasoningContent ?? null,
          reasoningSegmentCount: (streaming.reasoningSegments ?? []).length,
        };
      } catch (e) {
        threw = e instanceof Error ? e.message : String(e);
        result = {
          threw,
          fullResponse: streaming.fullResponse,
          effectiveProfileId: streaming.effectiveProfile.id,
          reasoningContent: streaming.reasoningContent ?? null,
          reasoningSegmentCount: (streaming.reasoningSegments ?? []).length,
        };
      }
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

  // W4.11b: dump the CHAT_MESSAGE `llm_logs` rows the REAL wrapper wrote. The
  // wrapper's `logLLMCall` is fire-and-forget (`.catch`, not awaited), so drain the
  // pending synchronous writes before reading. `durationMs` is 0 on both sides
  // (frozen `Date.now` here; hard-coded `Some(0.0)` in the port).
  await new Promise((r) => setTimeout(r, 300));
  const { getRawLLMLogsDatabase } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const lldb = getRawLLMLogsDatabase();
  if (!lldb) throw new Error('llm-logs DB handle unavailable (degraded open?)');
  const llColumns = (
    lldb.pragma('table_info(llm_logs)') as Array<{ name: string }>
  ).map((c) => c.name);
  const llRawRows = lldb.prepare('SELECT * FROM llm_logs').all() as Array<
    Record<string, unknown>
  >;
  const llRows = llRawRows
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
  process.stderr.write(`primary-stream oracle wrote ${outPath}\n`);
}

test('primary-stream tier-3 oracle', async () => {
  await main();
});
