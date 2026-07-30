/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the enclave `step()` + schedule-tick capstone (U4.4; v4
 * `lib/background-jobs/handlers/autonomous-room-turn.ts`
 * `handleAutonomousRoomTurn` + `autonomous-room-schedule-tick.ts`
 * `handleAutonomousRoomScheduleTick`).
 *
 * Drives v4's REAL turn/tick handlers over the committed corpus
 * (harness/oracle/fixtures/enclave-step-tier3.json) against the REAL three-DB
 * fixture (main + mount-index + llm-logs), UNFORKED (no `QUILLTAP_JOB_CHILD`) —
 * so v4's behavior here is direct-write, which is exactly what v5's in-process
 * `step()` pins (the U4.4 encoded design decision).
 *
 * Mocked (each matching a Rust seam):
 *   - `createLLMProvider` — BOTH model boundaries at the provider level (the
 *     W4.11b relocation): `streamMessage` (the turn's primary stream; canned by
 *     the per-call label, RECORDING the exact provider|model|temperature|messages
 *     canned key) and `sendMessage` (the cheap-LLM distill + the 9c summary
 *     fold; routed by prompt kind, recorded the same way). The service-level
 *     `streamMessage` wrapper therefore runs REAL — including its terminal
 *     CHAT_MESSAGE `logLLMCall`, which the ambient `runWithAutonomousRunId`
 *     scope tags with the run id (the B-proof: the Rust side threads the same
 *     LogContext explicitly).
 *   - the buildContext feeders / post-office / subsystem seams — the
 *     orchestrator-oracle mock set, unchanged.
 *   - `ensureProcessorRunning` + the job host (`childCrashed: true` pre-seeded)
 *     so no forked child ever claims the enqueued turn jobs.
 *
 * `logLLMCall` runs REAL (`SQLITE_LLM_LOGS_PATH` = a copy of the seeded
 * llm-logs fixture — the pre-seeded rows are the token-accounting substrate).
 *
 * The wall clock is a per-call re-anchored +1 ms/read mock
 * ([[chain-depth-frozen-clock-artifact]]); `Math.random` is frozen to 0;
 * `crypto.randomUUID` is NOT pinned — minted ids verify through the shared
 * first-seen remap in the Rust harness (the enclave-lifecycle precedent).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-enclave-step-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-enclave-step-mount.db \
 *   QT_FIXTURE_LLMLOGS_OUT=/tmp/qt-enclave-step-llmlogs.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-enclave-step-fixture.ts
 *   TZ=UTC \
 *   QT_FIXTURE_ENCLAVE_STEP_MAIN=/tmp/qt-enclave-step-main.db \
 *   QT_FIXTURE_ENCLAVE_STEP_MOUNT=/tmp/qt-enclave-step-mount.db \
 *   QT_FIXTURE_ENCLAVE_STEP_LLMLOGS=/tmp/qt-enclave-step-llmlogs.db \
 *   QT_ORACLE_OUT=/tmp/oracle-enclave-step.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$V5/harness/oracle/cases" -- enclave-step-tier3
 */

// The IANA zone must be pinned in the INVOCATION env (`TZ=UTC npx jest …`) —
// an in-process assignment does not reliably re-zone croner's Date math under
// jest (empirically: a stale system zone leaked into the stale-slot advance).
process.env.TZ = 'UTC';

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
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
  done?: boolean;
  usage?: { promptTokens: number; completionTokens: number; totalTokens: number } | null;
  error?: string;
}
interface CallSpec {
  name: string;
  kind: 'turn' | 'tick';
  chatId?: string;
  userId: string;
  runId?: string;
  jobId?: string;
  jobCreatedAt?: string;
  streamLabel?: string | null;
  anchorMs: number;
}
interface Spec {
  testPepperBase64: string;
  calls: CallSpec[];
  streams: Record<string, ChunkSpec[][]>;
  cheapCompletion: { response: string; usage: unknown };
  foldCompletion: { response: string; usage: unknown };
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'enclave-step-tier3.json'), 'utf8'),
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_ENCLAVE_STEP_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_ENCLAVE_STEP_MOUNT;
  const fixtureLl = process.env.QT_FIXTURE_ENCLAVE_STEP_LLMLOGS;
  if (
    !fixtureMain || !existsSync(fixtureMain) ||
    !fixtureMount || !existsSync(fixtureMount) ||
    !fixtureLl || !existsSync(fixtureLl)
  ) {
    throw new Error(
      'QT_FIXTURE_ENCLAVE_STEP_MAIN / _MOUNT / _LLMLOGS must point at the seed fixtures',
    );
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-enclave-step-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'main.db');
  const workMount = join(scratch, 'mount.db');
  const workLl = join(scratch, 'llm-logs.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);
  copyFileSync(fixtureLl, workLl);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.SQLITE_LLM_LOGS_PATH = workLl;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // Neutralize the job-processor host BEFORE any service import (the enqueued
  // AUTONOMOUS_ROOM_TURN rows are state under test — nothing may claim them).
  (globalThis as Record<string, unknown>).__quilltapJobHost = {
    child: null, spawning: false, shuttingDown: false, childCrashed: true,
    restartTimestamps: [], signalHandlersInstalled: true, invalidationListeners: new Set(),
  };

  // Recorded canned entries, keyed exactly as the Rust `canned_stream_key` /
  // `canned_completion_key`.
  const cannedStreams = new Map<
    string,
    { provider: string; model: string; temperature: number | null; messages: unknown[]; sequences: ChunkSpec[][] }
  >();
  const cannedCompletions = new Map<
    string,
    { provider: string; model: string; temperature: number | null; messages: unknown[]; response: string; usage: unknown }
  >();
  const attemptCursor = new Map<string, number>();
  let currentLabel: string | undefined | null;

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));

  // Pricing fetch seam: empty cache (checkModelSupportsTools falls through to
  // the static fallback table — matching the Rust never-called PricingFetch).
  jest.doMock('@/lib/llm/pricing-fetcher', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/llm/pricing-fetcher'),
    getPricingCache: async () => ({ providers: {} }),
  }));

  // ---- BOTH model boundaries at the provider level (the W4.11b relocation).
  // The service-level streamMessage wrapper runs REAL — its terminal
  // CHAT_MESSAGE logLLMCall (tagged with the ambient autonomous run id) is
  // part of the diffed llm_logs state. sendMessage serves the cheap-LLM calls:
  // the buildContext keyword distill and the 9c summary fold, routed by the
  // system-prompt kind (the answer-confirmation oracle precedent).
  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async (providerName: string, _baseUrl?: string) => ({
        supportsWebSearch: false,
        streamMessage: async function* streamMessage(
          params: {
            messages: Array<{ role: string; content: string }>;
            model: string;
            temperature?: number;
          },
          _apiKey: string,
        ) {
          const messages = params.messages.map((m) => ({ role: m.role, content: m.content }));
          const label = currentLabel;
          if (!label) throw new Error('streamMessage mock: no current label');
          const attempts = spec.streams[label];
          if (!attempts) throw new Error(`streamMessage mock: no streams for ${label}`);
          const idx = attemptCursor.get(label) ?? 0;
          attemptCursor.set(label, idx + 1);
          const chunks = attempts[idx];
          if (!chunks) throw new Error(`streamMessage mock: exhausted attempts for ${label}`);

          const temperature = (params.temperature as number | undefined) ?? null;
          const key = `${providerName}|${params.model}|${temperature ?? '-'}|${JSON.stringify(messages)}`;
          const entry = cannedStreams.get(key);
          if (entry) entry.sequences.push(chunks);
          else cannedStreams.set(key, { provider: providerName, model: params.model, temperature, messages, sequences: [chunks] });

          for (const chunk of chunks) {
            if (chunk.error) throw new Error(chunk.error);
            if (chunk.done) {
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
        sendMessage: async (
          params: { messages: Array<{ role: string; content: string }>; model: string; temperature?: number },
          _apiKey: string,
        ) => {
          const messages = params.messages.map((m) => ({ role: m.role, content: m.content }));
          const system = messages.find((m) => m.role === 'system')?.content ?? '';
          // Route by prompt kind: the summary fold vs the keyword distill.
          const isFold = system.includes('summar');
          const canned = isFold ? spec.foldCompletion : spec.cheapCompletion;
          const temperature = (params.temperature as number | undefined) ?? null;
          const key = `${providerName}|${params.model}|${temperature ?? '-'}|${JSON.stringify(messages)}`;
          if (!cannedCompletions.has(key)) {
            cannedCompletions.set(key, {
              provider: providerName,
              model: params.model,
              temperature,
              messages,
              response: canned.response,
              usage: canned.usage,
            });
          }
          return { content: canned.response, finishReason: 'stop', usage: canned.usage };
        },
      }),
    };
  });

  // ---- embeddings (unused: no memories seeded) ----
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

  // ---- API-key seams (host-side on the Rust port) ----
  jest.doMock('@/lib/plugins/provider-validation', () => {
    const actual = jest.requireActual('@/lib/plugins/provider-validation');
    return { __esModule: true, ...actual, requiresApiKey: () => false };
  });
  jest.doMock('@/lib/services/api-key.service', () => {
    const actual = jest.requireActual('@/lib/services/api-key.service');
    return {
      __esModule: true,
      ...actual,
      getApiKeyForCheapLLMSelection: async () => 'test-key',
      getApiKeyForProfile: async () => 'test-key',
    };
  });
  // logLLMCall runs REAL (the llm_logs rows are diffed state).
  jest.doMock('@/lib/services/llm-logging.service', () =>
    jest.requireActual('@/lib/services/llm-logging.service'),
  );

  // ---- the K file-loader seam (the orchestrator-oracle mock set) ----
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

  // ---- `runPreContextPreCompute` runs REAL (P4.20) ----
  // It used to be stubbed inert here: a W4.11a-era mock written when v5's
  // orchestrator ported only the compression half of the pre-compute service and
  // had no proactive-recall port at all. P4.19 landed that port
  // (`services/pre_compute.rs`) and the stub was never retired, so from P4.19 on
  // this oracle suppressed a call v4 really makes — v5 ran the proactive distill,
  // v4 (as mocked) did not, and the family read RED at `llm_logs` with an extra
  // MEMORY_EXTRACTION row that was blamed on a v4 guard bail. There is no bail:
  // instrumenting `proactiveRecallTask` in a pinned v4 worktree showed it running
  // to the distill on the Fold-room turn with the SAME 1-message window v5 uses
  // (and bailing at `characterMessages.length === 0` in the other 20 rooms, where
  // the responder has never spoken). The oracle, not v4, was the divergence — so
  // the stub is gone and the real service runs: 13 MEMORY_EXTRACTION rows on both
  // sides. See the P4.20 lane record in status-log.md.
  jest.doMock('@/lib/services/system-prompt-compiler/compiler', () => {
    const actual = jest.requireActual('@/lib/services/system-prompt-compiler/compiler');
    return { __esModule: true, ...actual, getCompiledIdentityStack: () => null };
  });
  jest.doMock('@/lib/post-office/surface-operator-mail', () => {
    const actual = jest.requireActual('@/lib/post-office/surface-operator-mail');
    return { __esModule: true, ...actual, surfaceOperatorMailForChat: async () => undefined };
  });
  jest.doMock('@/lib/services/librarian-notifications/writer', () => {
    const actual = jest.requireActual('@/lib/services/librarian-notifications/writer');
    return {
      __esModule: true,
      ...actual,
      postLibrarianUploadAnnouncement: async () => null,
      postLibrarianSummaryAnnouncement: async () => undefined,
    };
  });
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
      '@/lib/services/commonplace-notifications/relevant-conversations-refresh',
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
  jest.doMock('@/lib/services/chat-message/compression-cache.service', () => {
    const actual = jest.requireActual('@/lib/services/chat-message/compression-cache.service');
    return { __esModule: true, ...actual, triggerAsyncCompression: () => undefined };
  });
  jest.doMock('@/lib/services/cost-estimation.service', () => {
    const actual = jest.requireActual('@/lib/services/cost-estimation.service');
    return {
      __esModule: true,
      ...actual,
      estimateMessageCost: async () => ({ cost: null, source: 'unavailable' }),
    };
  });
  jest.doMock('@/lib/background-jobs/processor', () => {
    const actual = jest.requireActual('@/lib/background-jobs/processor');
    return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
  });
  // handleSendMessage's post-cycle scene-state + conversation-render triggers
  // are unported wave-4 subsystems the Rust step does not drive.
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
  const { handleAutonomousRoomTurn } = await import(
    '@/lib/background-jobs/handlers/autonomous-room-turn'
  );
  const { handleAutonomousRoomScheduleTick } = await import(
    '@/lib/background-jobs/handlers/autonomous-room-schedule-tick'
  );

  // Initialize the REAL provider registry (the built plugins/dist bundles) so
  // buildTools reshapes the slate to the provider's native form — matching the
  // Rust manifest registry (the Round-3 unification precedent).
  {
    const nodeRequire = createRequire(join(process.cwd(), 'noop.js'));
    const PLUGIN_DIRS = [
      'anthropic', 'openai', 'google', 'grok', 'deepseek',
      'z-ai', 'openrouter', 'ollama', 'openai-compatible',
    ];
    const { initializeProviderRegistry } = await import('@/lib/plugins/provider-registry');
    const providers = PLUGIN_DIRS.map((d) => {
      const m = nodeRequire(join(process.cwd(), 'plugins', 'dist', `qtap-plugin-${d}`, 'index.js'));
      return m.plugin || m.default?.plugin || m.default;
    });
    await initializeProviderRegistry(providers);
  }

  await initializeDatabase();

  // Per-call re-anchored +1 ms/read Date mock (the enclave-lifecycle idiom).
  const RealDate = Date;
  let mockClock = 0;
  const nowMs = () => mockClock++;
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
  const realRandom = Math.random;
  Math.random = () => 0;

  const lines: string[] = [];

  for (const call of spec.calls) {
    mockClock = call.anchorMs;
    currentLabel = call.streamLabel ?? null;
    let threw: string | null = null;
    try {
      if (call.kind === 'turn') {
        await handleAutonomousRoomTurn({
          id: call.jobId,
          userId: call.userId,
          type: 'AUTONOMOUS_ROOM_TURN',
          status: 'PROCESSING',
          payload: { chatId: call.chatId, runId: call.runId },
          priority: 0,
          attempts: 1,
          maxAttempts: 1,
          scheduledAt: call.jobCreatedAt,
          createdAt: call.jobCreatedAt,
          updatedAt: call.jobCreatedAt,
        } as never);
      } else {
        await handleAutonomousRoomScheduleTick({
          id: `tick-${call.name}`,
          userId: call.userId,
          type: 'AUTONOMOUS_ROOM_SCHEDULE_TICK',
          status: 'PROCESSING',
          payload: {},
          priority: -1,
          attempts: 1,
          maxAttempts: 1,
          scheduledAt: new RealDate(call.anchorMs).toISOString(),
          createdAt: new RealDate(call.anchorMs).toISOString(),
          updatedAt: new RealDate(call.anchorMs).toISOString(),
        } as never);
      }
    } catch (err) {
      threw = err instanceof Error ? err.message : String(err);
    }
    lines.push(
      JSON.stringify({
        kind: 'call',
        call: call.name,
        threw,
        clockReads: mockClock - call.anchorMs,
      }),
    );
    // Let any fire-and-forget promise settle before the next call.
    await new Promise((resolve) => setTimeout(resolve, 200));
  }

  (global as { Date: DateConstructor }).Date = RealDate;
  Math.random = realRandom;

  for (const s of cannedStreams.values()) lines.push(JSON.stringify({ kind: 'cannedStream', ...s }));
  for (const c of cannedCompletions.values()) lines.push(JSON.stringify({ kind: 'cannedCompletion', ...c }));

  const dumpTable = async (table: string, orderBy: string) => {
    const columns = ((await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>).map(
      (c) => c.name,
    );
    const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };

  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('chats', 'id')) }));
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('chat_messages', 'id')) }));
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('background_jobs', 'id')) }));

  // llm_logs: RAW rows (the Rust harness normalizes BOTH sides — the remaps
  // need the raw messageId/autonomousRunId values).
  const { getRawLLMLogsDatabase } = await import('@/lib/database/backends/sqlite/llm-logs-client');
  const lldb = getRawLLMLogsDatabase();
  if (!lldb) throw new Error('llm-logs DB handle unavailable');
  const llColumns = (lldb.pragma('table_info(llm_logs)') as Array<{ name: string }>).map(
    (c) => c.name,
  );
  const llRawRows = lldb.prepare('SELECT * FROM llm_logs').all() as Array<Record<string, unknown>>;
  lines.push(
    JSON.stringify({
      kind: 'table',
      ...canonicalizeRows({ table: 'llm_logs', columns: llColumns, rawRows: llRawRows }),
    }),
  );

  closeMountIndexSQLiteClient();
  await closeDatabase();

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`enclave-step oracle wrote ${outPath}\n`);
}

test('enclave step tier-3 oracle', async () => {
  await main();
});
