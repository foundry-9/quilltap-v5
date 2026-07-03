/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the chat-message orchestrator (Phase-3 Unit-3 wave 3, the
 * FINAL unit; v4 `processMessage` + `executeTurnChain`, driven through the public
 * `handleSendMessage` entry point in
 * `lib/services/chat-message/orchestrator.service.ts`).
 *
 * Drives v4's REAL `handleSendMessage` over the committed corpus
 * (harness/oracle/fixtures/orchestrator-tier3.json) against the REAL two-DB
 * fixture, mocking ONLY the model boundaries + the out-of-scope subsystems the
 * Rust port injects as seams (each matching the Rust seam exactly):
 *
 *   - `streamMessage` (the primary stream): rule-match by the call's stream label
 *     (the `originalMessage` marker planted in a user message) + RECORD the exact
 *     `provider|model|temperature|messages` canned key answered (the primary-stream
 *     oracle's approach) so the Rust `CannedStreamingProvider` replays them;
 *   - `createLLMProvider` (the summary-check cheap-LLM): returns the corpus canned
 *     summary response + records the canned key;
 *   - `generateEmbeddingForUser`: canned (unused by the corpus — no memory search);
 *   - buildContext's unported feeders (tiered-mount-pool / memory-recap /
 *     frozen-archive / instance-settings / system-prompt-compiler / keyword
 *     distillation) → the same no-op values the Rust `NoopSeams` / `BuildContextSeams`
 *     defaults produce;
 *   - the post-office writers (core-whisper / commonplace / suparna / mailbox /
 *     host / prospero) → no-ops;
 *   - the orchestrator's own subsystem seams (agent-mode / danger reroute / RNG
 *     auto-detect / carina markup / cost estimate / async compression / logLLMCall)
 *     → recorders / no-ops matching the Rust `OrchestratorSeams` + finalizer seams;
 *   - the background-job processor auto-start → no-op (the enqueued PENDING rows are
 *     state under test).
 *
 * The wall clock is frozen to the corpus `frozenNowMs` (buildContext's
 * timestamp-in-prompt must match the Rust injected `now_ms` so the canned stream
 * key matches); all minted DB timestamps are normalized to `<ts>` in the harness.
 *
 * `handleSendMessage` returns a ReadableStream of SSE bytes; the oracle drains it,
 * decoding each `data: {…}` frame to JSON — the ordered event trace the Rust
 * `RecordingSink` is diffed against.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-orch-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-orch-mount.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-orchestrator-fixture.ts
 *   QT_FIXTURE_ORCH_MAIN=/tmp/qt-orch-main.db QT_FIXTURE_ORCH_MOUNT=/tmp/qt-orch-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-orchestrator.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- orchestrator-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

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
interface CallSpec {
  name: string;
  kind: 'single' | 'chain';
  chatId: string;
  content: string;
  continueMode: boolean;
  streamLabel: string;
  cheapLLMSettings: boolean;
  summaryCheck: boolean;
  respondingParticipant?: string;
  expectThrow?: boolean;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  frozenNowMs: number;
  chatSettings: { id: string };
  calls: CallSpec[];
  streams: Record<string, ChunkSpec[][]>;
  summaryCompletion: {
    response: string;
    usage: { promptTokens: number; completionTokens: number; totalTokens: number };
  };
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'orchestrator-tier3.json'), 'utf8')
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_ORCH_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_ORCH_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_ORCH_MAIN / QT_FIXTURE_ORCH_MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-orch-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'orch-main.db');
  const workMount = join(scratch, 'orch-mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // Recorded canned entries (streaming + summary completion), keyed by the exact
  // call key (must match `quilltap_core::model::stream::canned_stream_key` /
  // `canned_completion_key`).
  const cannedStreams = new Map<
    string,
    { provider: string; model: string; temperature: number | null; messages: unknown[]; sequences: ChunkSpec[][] }
  >();
  const cannedCompletions = new Map<
    string,
    { provider: string; model: string; temperature: number | null; messages: unknown[]; response: string; usage: unknown }
  >();
  const attemptCursor = new Map<string, number>();
  const compressionTriggers: Array<{ chatId: string; participantId: string | undefined }> = [];
  const costTracks: Array<Record<string, unknown>> = [];
  // The current call's stream label (steered per-call so the streamMessage mock
  // pops the right attempt sequence).
  let currentLabel: string | undefined;

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));

  // ---- streamMessage (the primary stream) ----
  jest.doMock('@/lib/services/chat-message/streaming.service', () => {
    const actual = jest.requireActual('@/lib/services/chat-message/streaming.service');
    return {
      __esModule: true,
      ...actual,
      // The tool build is a wave-4 subsystem the Rust orchestrator seams out
      // (empty tool slate → no tool instructions in the system prompt). Mirror
      // that on the v4 side so `toolInstructions` stays undefined and the prompt
      // bytes match the Rust `build_context` input.
      buildTools: async () => ({ tools: [], modelSupportsNativeTools: true, useNativeWebSearch: false }),
      streamMessage: async function* streamMessage(options: {
        messages: Array<{ role: string; content: string }>;
        connectionProfile: { provider: string; modelName: string };
        modelParams: Record<string, unknown>;
      }) {
        const messages = options.messages.map((m) => ({ role: m.role, content: m.content }));
        const label = currentLabel;
        if (!label) throw new Error('streamMessage mock: no current label');
        const attempts = spec.streams[label];
        if (!attempts) throw new Error(`streamMessage mock: no streams for ${label}`);
        const idx = attemptCursor.get(label) ?? 0;
        attemptCursor.set(label, idx + 1);
        const chunks = attempts[idx];
        if (!chunks) throw new Error(`streamMessage mock: exhausted attempts for ${label} (idx ${idx})`);

        const provider = options.connectionProfile.provider;
        const model = options.connectionProfile.modelName;
        const temperature = (options.modelParams.temperature as number | undefined) ?? null;
        const key = `${provider}|${model}|${temperature ?? '-'}|${JSON.stringify(messages)}`;
        const entry = cannedStreams.get(key);
        if (entry) entry.sequences.push(chunks);
        else cannedStreams.set(key, { provider, model, temperature, messages, sequences: [chunks] });

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

  // ---- createLLMProvider (the summary-check cheap-LLM) ----
  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async (provider: string) => ({
        sendMessage: async (
          params: { messages: Array<{ role: string; content: string }>; model: string; temperature?: number },
          _apiKey: string
        ) => {
          const messages = params.messages.map((m) => ({ role: m.role, content: m.content }));
          const usage = spec.summaryCompletion.usage;
          const key = `${provider}|${params.model}|${params.temperature ?? '-'}|${JSON.stringify(messages)}`;
          if (!cannedCompletions.has(key)) {
            cannedCompletions.set(key, {
              provider,
              model: params.model,
              temperature: params.temperature ?? null,
              messages,
              response: spec.summaryCompletion.response,
              usage,
            });
          }
          return { content: spec.summaryCompletion.response, finishReason: 'stop', usage };
        },
      }),
    };
  });

  // ---- embeddings (unused by the corpus; canned to a fixed vector) ----
  jest.doMock('@/lib/embedding/embedding-service', () => {
    const actual = jest.requireActual('@/lib/embedding/embedding-service');
    return {
      __esModule: true,
      ...actual,
      generateEmbeddingForUser: async () => ({
        embedding: new Float32Array([0.1, 0.2, 0.3]),
        model: 'canned',
        provider: 'canned',
        dimensions: 3,
      }),
    };
  });

  // ---- API-key requirement → false (host-side seam) ----
  // The Rust port treats API-key acquisition as a host-side seam: the participant
  // resolver returns no key and the orchestrator never blocks on `requiresApiKey`.
  // Mirror that on the v4 side so the key check does not short-circuit (the
  // fixture stores no key).
  jest.doMock('@/lib/plugins/provider-validation', () => {
    const actual = jest.requireActual('@/lib/plugins/provider-validation');
    return { __esModule: true, ...actual, requiresApiKey: () => false };
  });

  // ---- api-key + llm-logging ----
  jest.doMock('@/lib/services/api-key.service', () => {
    const actual = jest.requireActual('@/lib/services/api-key.service');
    return {
      __esModule: true,
      ...actual,
      getApiKeyForCheapLLMSelection: async () => 'test-key',
      getApiKeyForProfile: async () => 'test-key',
    };
  });
  jest.doMock('@/lib/services/llm-logging.service', () => {
    const actual = jest.requireActual('@/lib/services/llm-logging.service');
    return { __esModule: true, ...actual, logLLMCall: async () => undefined };
  });

  // ---- buildMessageContext → the REAL wrapper; mock ONLY the K file-loader ----
  // v4's `buildMessageContext` (context-builder.service.ts) is now ported as
  // `quilltap_core::services::message_context`, so the oracle drives the REAL
  // wrapper (whisper pre-filters / opaque-anywhere / normalization, the ported
  // `buildContext`, `formatMessagesForProvider`, and the multi-character scene
  // block). The ONLY thing mocked inside it is section K, the unported wave-4 file
  // subsystem (`loadChatFilesForLLM` + `processFileAttachmentFallback` +
  // `formatFallbackAsMessagePrefix`) — mirrored by the Rust
  // `NoopMessageContextSeams` (empty prefix / no attachments). The corpus keeps
  // message attachments empty, so `collectLanternImageFileIdsForCharacter` returns
  // `[]` and these are never reached; they are mocked defensively so the real file
  // subsystem is never touched.
  jest.doMock('@/lib/chat-files-v2', () => {
    const actual = jest.requireActual('@/lib/chat-files-v2');
    return { __esModule: true, ...actual, loadChatFilesForLLM: async () => [] };
  });
  jest.doMock('@/lib/chat/file-attachment-fallback', () => {
    const actual = jest.requireActual('@/lib/chat/file-attachment-fallback');
    return {
      __esModule: true,
      ...actual,
      processFileAttachmentFallback: async () => ({ type: 'unsupported' }),
      formatFallbackAsMessagePrefix: () => '',
    };
  });

  // ---- buildContext feeders → the Rust NoopSeams / BuildContextSeams defaults ----
  jest.doMock('@/lib/mount-index/tiered-mount-pool', () => {
    const actual = jest.requireActual('@/lib/mount-index/tiered-mount-pool');
    return {
      __esModule: true,
      ...actual,
      resolveTieredMountPool: async (opts: { characterMountPointId?: string | null }) => ({
        characterMountPointId: opts.characterMountPointId ?? null,
        groupMountPointIds: [],
        projectMountPointIds: [],
        globalMountPointId: null,
      }),
    };
  });
  jest.doMock('@/lib/memory/memory-recap', () => {
    const actual = jest.requireActual('@/lib/memory/memory-recap');
    return { __esModule: true, ...actual, generateMemoryRecap: async () => ({ content: '' }) };
  });
  jest.doMock('@/lib/memory/frozen-archive-cache', () => {
    const actual = jest.requireActual('@/lib/memory/frozen-archive-cache');
    return { __esModule: true, ...actual, getOrComputeFrozenArchive: async () => [] };
  });
  jest.doMock('@/lib/instance-settings', () => {
    const actual = jest.requireActual('@/lib/instance-settings');
    return {
      __esModule: true,
      ...actual,
      getMemoryRecallSettings: async () => ({ scopePolicy: 'BALANCED', expandRelated: false }),
    };
  });
  jest.doMock('@/lib/services/system-prompt-compiler/compiler', () => {
    const actual = jest.requireActual('@/lib/services/system-prompt-compiler/compiler');
    return { __esModule: true, ...actual, getCompiledIdentityStack: () => null };
  });
  jest.doMock('@/lib/memory/cheap-llm-tasks', () => {
    const actual = jest.requireActual('@/lib/memory/cheap-llm-tasks');
    return { __esModule: true, ...actual, extractMemorySearchKeywords: async () => ({ success: false }) };
  });

  // ---- post-office writers → no-ops ----
  jest.doMock('@/lib/services/aurora-notifications/core-whisper', () => {
    const actual = jest.requireActual('@/lib/services/aurora-notifications/core-whisper');
    return {
      __esModule: true,
      ...actual,
      resolveCoreWhisperConfig: () => ({ enabled: false }),
      postCoreWhisper: async () => null,
      assembleCorePacket: async () => null,
    };
  });
  jest.doMock('@/lib/services/commonplace-notifications/writer', () => {
    const actual = jest.requireActual('@/lib/services/commonplace-notifications/writer');
    return { __esModule: true, ...actual, postCommonplaceWhisper: async () => null };
  });
  jest.doMock('@/lib/services/suparna-notifications/writer', () => {
    const actual = jest.requireActual('@/lib/services/suparna-notifications/writer');
    return { __esModule: true, ...actual, postSuparnaMailWhisper: async () => null };
  });
  jest.doMock('@/lib/post-office/mailbox', () => {
    const actual = jest.requireActual('@/lib/post-office/mailbox');
    return { __esModule: true, ...actual, collectUnalertedMail: async () => [], markAlerted: async () => undefined };
  });
  jest.doMock('@/lib/post-office/surface-operator-mail', () => {
    const actual = jest.requireActual('@/lib/post-office/surface-operator-mail');
    return { __esModule: true, ...actual, surfaceOperatorMailForChat: async () => undefined };
  });
  jest.doMock('@/lib/services/host-notifications/writer', () => {
    const actual = jest.requireActual('@/lib/services/host-notifications/writer');
    return {
      __esModule: true,
      ...actual,
      postHostTimestampAnnouncement: async () => undefined,
      postHostOffSceneCharactersAnnouncement: async () => null,
    };
  });
  // Prospero context re-injection (cadence whisper) → no-op posts.
  jest.doMock('@/lib/services/prospero-notifications/writer', () => {
    const actual = jest.requireActual('@/lib/services/prospero-notifications/writer');
    return {
      __esModule: true,
      ...actual,
      loadProsperoProjectContext: async () => null,
      loadProsperoGeneralContext: async () => null,
      postProsperoContextAnnouncement: async () => null,
      postProsperoGroupContextWhisper: async () => null,
    };
  });
  jest.doMock('@/lib/services/librarian-notifications/writer', () => {
    const actual = jest.requireActual('@/lib/services/librarian-notifications/writer');
    return {
      __esModule: true,
      ...actual,
      postLibrarianUploadAnnouncement: async () => null,
      // The summary fold's Librarian announcement is a default-no-op
      // `ContextSummarySeams` on the Rust side (tracked deferral).
      postLibrarianSummaryAnnouncement: async () => undefined,
    };
  });
  // The summary fold's cross-subsystem side effects — all Rust `ContextSummarySeams`
  // default no-ops (tracked deferrals): the vault mirror, the relevant-conversations
  // refresh, and the `context-summary` system-event write.
  jest.doMock('@/lib/file-storage/conversation-summary-vault-bridge', () => {
    const actual = jest.requireActual('@/lib/file-storage/conversation-summary-vault-bridge');
    return {
      __esModule: true,
      ...actual,
      writeConversationSummaryToVaults: async () => undefined,
      computeConversationStats: () => ({ messageCount: 0, firstMessageAt: null, lastMessageAt: null }),
    };
  });
  jest.doMock('@/lib/services/commonplace-notifications/relevant-conversations-refresh', () => {
    const actual = jest.requireActual(
      '@/lib/services/commonplace-notifications/relevant-conversations-refresh'
    );
    return { __esModule: true, ...actual, refreshRelevantConversationsOnFold: async () => undefined };
  });
  jest.doMock('@/lib/services/system-events.service', () => {
    const actual = jest.requireActual('@/lib/services/system-events.service');
    return {
      __esModule: true,
      ...actual,
      createContextSummaryEvent: async () => undefined,
      createTitleGenerationEvent: async () => undefined,
    };
  });

  // ---- orchestrator's own subsystem seams ----
  jest.doMock('@/lib/services/chat-message/compression-cache.service', () => {
    const actual = jest.requireActual('@/lib/services/chat-message/compression-cache.service');
    return {
      __esModule: true,
      ...actual,
      triggerAsyncCompression: (opts: { chatId: string; participantId?: string }) => {
        compressionTriggers.push({ chatId: opts.chatId, participantId: opts.participantId });
      },
    };
  });
  jest.doMock('@/lib/services/chat-message/rng-pattern-detector.service', () => {
    const actual = jest.requireActual('@/lib/services/chat-message/rng-pattern-detector.service');
    return { __esModule: true, ...actual, detectAndConvertRngPatterns: () => [] };
  });
  jest.doMock('@/lib/services/cost-estimation.service', () => {
    const actual = jest.requireActual('@/lib/services/cost-estimation.service');
    return {
      __esModule: true,
      ...actual,
      estimateMessageCost: async (
        provider: string,
        modelName: string,
        promptTokens: number,
        completionTokens: number,
        userId: string
      ) => {
        costTracks.push({ provider, modelName, promptTokens, completionTokens, userId });
        return { cost: null, source: 'unavailable' };
      },
    };
  });
  jest.doMock('@/lib/services/carina/markup-runner', () => {
    const actual = jest.requireActual('@/lib/services/carina/markup-runner');
    // The corpus content never contains carina markup, so the real runner is a
    // no-op; keep it real (it parses and finds nothing).
    return { __esModule: true, ...actual };
  });
  jest.doMock('@/lib/background-jobs/processor', () => {
    const actual = jest.requireActual('@/lib/background-jobs/processor');
    return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
  });
  // handleSendMessage's post-cycle scene-state + conversation-render triggers are
  // wave-4 subsystems (the Scriptorium / scene tracker) the Rust orchestrator does
  // not drive — no-op them so their enqueues do not appear in the diffed dump.
  jest.doMock('@/lib/services/chat-message/memory-trigger.service', () => {
    const actual = jest.requireActual('@/lib/services/chat-message/memory-trigger.service');
    return {
      __esModule: true,
      ...actual,
      triggerSceneStateTracking: async () => undefined,
      triggerConversationRender: async () => undefined,
    };
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { handleSendMessage } = await import('@/lib/services/chat-message/orchestrator.service');

  await initializeDatabase();
  const repos = getRepositories();

  // Freeze the wall clock so buildContext's timestamp-in-prompt matches the Rust
  // injected now_ms (→ the canned stream key matches). Minted DB timestamps are
  // normalized to <ts> in the harness.
  // The wall clock is frozen to `frozenNowMs` so buildContext's timestamp-in-prompt
  // matches the Rust injected `now_ms` (→ the canned stream key matches). BUT it
  // must ADVANCE by 1ms per read, not stand still: v4 stamps every message's
  // `createdAt` from `Date.now()` and `getMessages` sorts `ORDER BY createdAt ASC`,
  // so a truly-frozen clock collapses every minted message to one `createdAt`,
  // making the sort a tie whose order decides `calculateTurnStateFromHistory`'s
  // `lastSpeakerId`. The Rust side stamps `createdAt` from a REAL (monotonic) clock,
  // so its history always orders the latest ASSISTANT last; a frozen v4 clock can
  // instead resolve a non-continue chat's USER row (which carries the user
  // participant id) as most-recent, flipping `lastSpeakerId` to the user and
  // driving the turn manager's cycle-wrap to re-pick the sole LLM character to
  // max depth — a pure harness artifact (real timestamps are distinct). Advancing
  // 1ms per read reproduces the monotonic real-clock ordering on the v4 side, so
  // both sides agree; the +Nms drift is invisible (no case injects a prompt
  // timestamp, and every minted DB timestamp is normalized to `<ts>`). See the
  // chain-depth cases + `[[chain-depth-frozen-clock-artifact]]`.
  const RealDate = Date;
  let tick = 0;
  const nowMs = () => spec.frozenNowMs + tick++;
  const FakeDate = class extends RealDate {
    constructor(...args: unknown[]) {
      if (args.length === 0) super(nowMs());
      // @ts-expect-error variadic forwarding
      else super(...args);
    }
    static now(): number {
      return nowMs();
    }
  } as DateConstructor;
  (global as { Date: DateConstructor }).Date = FakeDate;

  // Freeze Math.random to 0 so the turn manager's weighted next-speaker pick
  // (`Math.random() * totalWeight`) is deterministic, matching the Rust side's
  // injected `random01 = 0.0`.
  const realRandom = Math.random;
  Math.random = () => 0;

  const decoder = new TextDecoder();
  function decodeFrames(bytes: Uint8Array, sink: unknown[]): void {
    const text = decoder.decode(bytes);
    for (const line of text.split('\n')) {
      const t = line.trim();
      if (t.startsWith('data:')) {
        const body = t.slice('data:'.length).trim();
        if (body) sink.push(JSON.parse(body));
      }
    }
  }

  const lines: string[] = [];

  for (const call of spec.calls) {
    currentLabel = call.streamLabel;

    const events: unknown[] = [];
    let threw = false;
    try {
      const stream = await handleSendMessage(repos, call.chatId, spec.userId, {
        content: call.continueMode ? undefined : call.content,
        continueMode: call.continueMode ? true : undefined,
        respondingParticipantId: call.respondingParticipant,
      } as never);
      // Drain the ReadableStream.
      const reader = stream.getReader();
      // eslint-disable-next-line no-constant-condition
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        if (value) decodeFrames(value, events);
      }
    } catch (err) {
      threw = true;
      lines.push(
        JSON.stringify({ kind: 'threw', call: call.name, message: err instanceof Error ? err.message : String(err) })
      );
    }
    lines.push(JSON.stringify({ kind: 'events', call: call.name, events, threw }));

    // Let the fire-and-forget background triggers settle before the next call.
    await new Promise((resolve) => setTimeout(resolve, 200));
  }

  // Restore the real Date + Math.random before dumping (be tidy).
  (global as { Date: DateConstructor }).Date = RealDate;
  Math.random = realRandom;

  for (const s of cannedStreams.values()) lines.push(JSON.stringify({ kind: 'cannedStream', ...s }));
  for (const c of cannedCompletions.values()) lines.push(JSON.stringify({ kind: 'cannedCompletion', ...c }));
  for (const t of compressionTriggers) lines.push(JSON.stringify({ kind: 'compression', ...t }));
  for (const c of costTracks) lines.push(JSON.stringify({ kind: 'cost', ...c }));

  const dumpTable = async (table: string, orderBy: string) => {
    const columns = ((await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>).map(
      (c) => c.name
    );
    const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };

  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('chats', 'id')) }));
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('chat_messages', 'id')) }));
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('background_jobs', 'id')) }));

  closeMountIndexSQLiteClient();
  await closeDatabase();

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`orchestrator oracle wrote ${outPath}\n`);
}

test('orchestrator tier-3 oracle', async () => {
  await main();
});
