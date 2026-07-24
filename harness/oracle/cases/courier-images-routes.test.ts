/**
 * @jest-environment node
 *
 * P4.6ab COURIER + CHAT-IMAGES route-surface ORACLE: drives v4's REAL Salon
 * courier/image route handlers over a FRESH copy of the committed
 * courier-images fixture per case, and emits each response body (+ post-mutation
 * single-table dumps) so the Rust ports (`api::chat_media::*`) diff byte-for-byte.
 *
 * Cases: photo-albums (read), files-list (read), save-image (character-vault +
 * user-persona attribution; response envelope — the write internals are already
 * proven by `photo_tools_equivalence`), add-tool-result (user + character
 * initiated), resolve-external-turn (the settle; the fixture resolves NO
 * connection profile → the three triggers are skipped, exactly as v4), the
 * AT-CADENCE resolve (`resolve_cadence` — CHAT_CADENCE resolves a connection
 * profile at interchange 11, so the triggers FIRE: the memory-extraction
 * enqueue + the summary-check fold + `runFoldEpisodePass`, pinning the fold's
 * AND the episode pass's cheap-LLM calls in `llm_logs`; the four
 * cross-subsystem fold arms — Librarian re-post / vault mirror /
 * relevant-conversations refresh / cost events — are jest-mocked to no-ops,
 * matching the Rust `FoldEpisodePassSeams`, and the `checkAndGenerate…`
 * wrapper forces `awaitFold: true` so v4's fire-and-forget fold settles
 * inside the case, matching v5's awaited settle — same committed state),
 * cancel-external-turn, chat-file delete, and the error arms.
 *
 * The clock is frozen to NOW_MS for the mutating cases (save-image `keptAt`,
 * resolve `lastMessageAt`/`updatedAt`/`resolvedAt`) so the minted timestamps are
 * pinned (the Rust side passes the same constant). The save-image side-effect
 * seams (mount-chunk cache / embedding scheduler) are jest.mocked to no-ops
 * (mirroring the Rust `NoSideEffects`); QUILLTAP_JOB_CHILD stays UNSET.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-ci-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/courier-images-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/courier-images-web.json" "$TMPO/fixtures/"
 *   cp "$V5W/crates/quilltap-web/tests/fixtures/courier-images-main.db.meta.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CI_MAIN=$V5W/crates/quilltap-web/tests/fixtures/courier-images-main.db \
 *   QT_FIXTURE_CI_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/courier-images-mount.db \
 *   QT_FIXTURE_CI_LLMLOGS=$V5W/crates/quilltap-web/tests/fixtures/courier-images-llmlogs.db \
 *   QT_FIXTURE_CI_META=$V5W/crates/quilltap-web/tests/fixtures/courier-images-main.db.meta.json \
 *   QT_ORACLE_OUT=/tmp/oracle-courier-images.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- courier-images-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}
interface Meta {
  loraVault: string;
  riyaVault: string;
  projMp: string;
  img1FileId: string;
}

// Pinned entity ids (shared verbatim with the builder + the Rust differential).
const GENERAL_MP = 'b0000000-0000-4000-8000-0000000000a0';
const CHAT_PENDING = 'c1000000-0000-4000-8000-000000000001';
const CHAT_IMG = 'c1000000-0000-4000-8000-000000000002';
const MSG_PENDING = 'd1000000-0000-4000-8000-000000000001';
const MSG_SETTLED = 'd1000000-0000-4000-8000-000000000002';
const MSG_IMG = 'd2000000-0000-4000-8000-000000000001';
const CF_NOTES = 'f1000000-0000-4000-8000-000000000001';
const CHAT_CADENCE = 'c1000000-0000-4000-8000-000000000004';
const MSG_CAD_PENDING = 'd4000000-0000-4000-8000-000000000022';

// Shared frozen clock (matches NOW_MS in the Rust test).
const NOW_MS = 1_775_779_200_000; // 2026-04-10T00:00:00.000Z

// The at-cadence resolve's canned cheap-LLM responses, routed by the fixed
// prompt openings (the memory-pipeline-jobs oracle precedent). The episode
// response is an explicit `[]` — the pass RUNS (its cheap-LLM call is the
// pinned evidence) but extracts nothing, keeping the gate/memory-write
// machinery (separately proven by the context-summary + memory-pipeline
// families) out of this route family's corpus.
const FOLD_RESPONSE =
  'Riya and Lora spent the evening decoding the harbor lantern signals, ' +
  'entry by entry, into Lora’s ledger.';
const FOLD_USAGE = { promptTokens: 111, completionTokens: 22, totalTokens: 133 };
const EPISODE_RESPONSE = '[]';
const EPISODE_USAGE = { promptTokens: 99, completionTokens: 3, totalTokens: 102 };
// The fold tail also generates a title from the fresh summary (v4
// `generateTitleFromSummary`) — canned so the title write + its
// TITLE_GENERATION llm_logs row are pinned on both sides (an uncanned title
// call would fail silently in v4 but write v5's ruled failed-call error row).
const TITLE_RESPONSE = 'The Lantern Ledger';
const TITLE_USAGE = { promptTokens: 55, completionTokens: 6, totalTokens: 61 };

// Canned cheap-LLM calls recorded during `resolve_cadence`, keyed exactly as
// the Rust `CannedCompletionProvider` lookup; emitted in the case payload's
// `canned` field. The trigger promises v4 fire-and-forgets are captured here
// so the case can await quiescence before dumping tables.
const cannedRecorded = new Map<
  string,
  {
    provider: string;
    model: string;
    temperature: number | null;
    messages: Array<{ role: string; content: string }>;
    response: string;
    usage: { promptTokens: number; completionTokens: number; totalTokens: number };
  }
>();
const capturedTriggerPromises: Array<Promise<unknown>> = [];

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

function applyMocks(spec: Spec): void {
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories'),
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store'),
  );
  // jest.setup stubs the character-vault bridge to a single "mock-vault-mount";
  // un-mock it so each character resolves its own real vault (photo-albums +
  // save-image attribution). See [[p4.6i-characters-remainder-server]].
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge'),
  );
  // Side-effect seams saveImageToAlbum touches → no-ops (mirror the Rust NoSideEffects).
  jest.doMock('@/lib/mount-index/mount-chunk-cache', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/mount-index/mount-chunk-cache'),
    invalidateMountPoint: () => {},
  }));
  jest.doMock('@/lib/mount-index/embedding-scheduler', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/mount-index/embedding-scheduler'),
    enqueueEmbeddingJobsForMountPoint: async () => {},
  }));
  // The chat GET handler pulls the markdown renderer (unified/ESM) — mock it away
  // (the chat GET body isn't under test here; the port omits renderedHtml).
  jest.doMock('@/lib/services/markdown-renderer.service', () => ({
    __esModule: true,
    renderMarkdownToHtml: async () => null,
    canPreRenderMessage: () => false,
  }));
  // ── The at-cadence resolve's cheap-LLM mock set (the enclave-step /
  //    memory-pipeline oracle precedent). Inert for every other case: no other
  //    case reaches createLLMProvider or the fold. ──────────────────────────
  // The provider boundary: route the summary fold + the fold-episode pass by
  // their fixed prompt openings, recording each call keyed exactly as the Rust
  // CannedCompletionProvider lookup.
  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async (provider: string, _baseUrl?: string) => ({
        sendMessage: async (
          params: {
            messages: Array<{ role: string; content: string }>;
            model: string;
            temperature?: number;
          },
          _apiKey: string,
        ) => {
          const messages = params.messages.map((m) => ({ role: m.role, content: m.content }));
          const system = messages.find((m) => m.role === 'system')?.content ?? '';
          let response: string;
          let usage: { promptTokens: number; completionTokens: number; totalTokens: number };
          if (system.startsWith('You are updating an existing summary')) {
            response = FOLD_RESPONSE;
            usage = FOLD_USAGE;
          } else if (system.startsWith('You are consolidating a batch')) {
            response = EPISODE_RESPONSE;
            usage = EPISODE_USAGE;
          } else if (system.startsWith('Generate a literary title')) {
            response = TITLE_RESPONSE;
            usage = TITLE_USAGE;
          } else {
            throw new Error('canned completion: unclassifiable system prompt');
          }
          const temperature = (params.temperature as number | undefined) ?? null;
          const key = `${provider}|${params.model}|${temperature ?? '-'}|${JSON.stringify(messages)}`;
          if (!cannedRecorded.has(key)) {
            cannedRecorded.set(key, {
              provider,
              model: params.model,
              temperature,
              messages,
              response,
              usage,
            });
          }
          return { content: response, finishReason: 'stop', usage };
        },
      }),
    };
  });
  // API-key seams (host-side on the Rust port).
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
  // logLLMCall runs REAL — the fold + episode SUMMARIZATION llm_logs rows are
  // the diffed evidence.
  jest.doMock('@/lib/services/llm-logging.service', () =>
    jest.requireActual('@/lib/services/llm-logging.service'),
  );
  jest.doMock('@/lib/llm/pricing-fetcher', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/llm/pricing-fetcher'),
    getPricingCache: async () => ({ providers: {} }),
  }));
  // The four cross-subsystem fold arms → no-ops, matching the Rust
  // `FoldEpisodePassSeams` (the orchestrator-oracle mock set; wiring them live
  // on the in-loop paths is the standing tracked deferral).
  jest.doMock('@/lib/services/librarian-notifications/writer', () => {
    const actual = jest.requireActual('@/lib/services/librarian-notifications/writer');
    return {
      __esModule: true,
      ...actual,
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
  // Embeddings: canned (unused while the episode response is `[]`; safety net).
  jest.doMock('@/lib/embedding/embedding-service', () => {
    const actual = jest.requireActual('@/lib/embedding/embedding-service');
    return {
      __esModule: true,
      ...actual,
      generateEmbeddingForUser: async () => ({
        embedding: new Float32Array([1, 0, 0, 0]),
        model: 'canned',
        provider: 'canned',
        dimensions: 4,
      }),
    };
  });
  // Force `awaitFold: true` on the summary check and capture its promise: v4's
  // route path fire-and-forgets `checkAndGenerateSummaryIfNeeded`, whose fold
  // is ITSELF fire-and-forget (`generateContextSummaryAsync`) — awaiting the
  // fold reaches the same committed state (v4's own autonomous-room arm) and
  // lets the case dump AFTER quiescence, matching v5's awaited settle.
  jest.doMock('@/lib/chat/context-summary', () => {
    const actual = jest.requireActual('@/lib/chat/context-summary');
    return {
      __esModule: true,
      ...actual,
      checkAndGenerateSummaryIfNeeded: (...args: unknown[]) => {
        const p = actual.checkAndGenerateSummaryIfNeeded(
          ...args.slice(0, 7),
          { awaitFold: true },
        ) as Promise<unknown>;
        capturedTriggerPromises.push(p);
        return p;
      },
    };
  });
  // Capture the three fire-and-forgotten trigger promises themselves (memory
  // extraction's enqueue must also settle before the background_jobs dump).
  jest.doMock('@/lib/services/chat-message/memory-trigger.service', () => {
    const actual = jest.requireActual('@/lib/services/chat-message/memory-trigger.service');
    const capture =
      (fn: (...a: unknown[]) => Promise<unknown>) =>
      (...a: unknown[]) => {
        const p = fn(...a);
        capturedTriggerPromises.push(p);
        return p;
      };
    return {
      __esModule: true,
      ...actual,
      triggerTurnMemoryExtraction: capture(actual.triggerTurnMemoryExtraction),
      triggerChatDangerClassification: capture(actual.triggerChatDangerClassification),
      triggerContextSummaryCheck: capture(actual.triggerContextSummaryCheck),
    };
  });
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: spec.userId } }),
  }));
  jest.doMock('@/lib/startup/startup-state', () => {
    const actual = jest.requireActual('@/lib/startup/startup-state');
    return {
      __esModule: true,
      ...actual,
      startupState: {
        ...actual.startupState,
        isReady: () => true,
        waitForReady: async () => true,
        isPepperResolved: () => true,
        getPepperState: () => 'resolved',
        getPhase: () => 'ready',
        isLockedMode: () => false,
      },
    };
  });
}

async function readMessageRow(chatId: string, messageId: string): Promise<unknown> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  const repos = getRepositories();
  const messages = await repos.chats.getMessages(chatId);
  return (messages as Array<{ id: string }>).find((m) => m.id === messageId) ?? null;
}

async function readChatFlags(chatId: string): Promise<unknown> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  const repos = getRepositories();
  const chat = await repos.chats.findById(chatId);
  const c = chat as { isPaused?: boolean; courierCheckpoints?: unknown; lastMessageAt?: string | null } | null;
  return c
    ? { isPaused: c.isPaused ?? false, courierCheckpoints: c.courierCheckpoints ?? null, lastMessageAt: c.lastMessageAt ?? null }
    : null;
}

async function readChatFileIds(chatId: string): Promise<unknown> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const main = getRawDatabase() as unknown as { prepare: (s: string) => { all: (...a: unknown[]) => unknown } };
  return main
    .prepare(
      "SELECT id, originalFilename FROM files WHERE EXISTS (SELECT 1 FROM json_each(files.linkedTo) WHERE value = ?) ORDER BY id",
    )
    .all(chatId);
}

/**
 * The at-cadence chat's summary-bearing columns. `updatedAt` is deliberately
 * omitted: the fold stamps it from the wall clock on the Rust side (v4's is
 * frozen), and the substance — the summary text, anchors, fold cursor — is
 * what the differential pins.
 */
async function readCadenceChat(chatId: string): Promise<unknown> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  const repos = getRepositories();
  const chat = (await repos.chats.findById(chatId)) as Record<string, unknown> | null;
  if (!chat) return null;
  return {
    title: chat.title,
    isPaused: chat.isPaused ?? false,
    courierCheckpoints: chat.courierCheckpoints ?? null,
    lastMessageAt: chat.lastMessageAt ?? null,
    contextSummary: chat.contextSummary ?? null,
    compactionGeneration: chat.compactionGeneration ?? 0,
    lastSummaryTurn: chat.lastSummaryTurn ?? 0,
    lastFullRebuildTurn: chat.lastFullRebuildTurn ?? 0,
    lastRenameCheckInterchange: chat.lastRenameCheckInterchange ?? 0,
    summaryAnchorMessageIds: chat.summaryAnchorMessageIds ?? [],
  };
}

/** The context-summary chat event(s) the fold appended (minted id/createdAt
 *  are blanked by the harness's shared normalization). */
async function readContextSummaryEvents(chatId: string): Promise<unknown> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  const repos = getRepositories();
  const messages = (await repos.chats.getMessages(chatId)) as Array<Record<string, unknown>>;
  return messages.filter((m) => m.type === 'context-summary');
}

/** background_jobs rows over stable columns (payload parsed so both sides
 *  compare canonical JSON, not string bytes). */
async function readBackgroundJobsStable(): Promise<unknown> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const main = getRawDatabase() as unknown as {
    prepare: (s: string) => { all: (...a: unknown[]) => Array<Record<string, unknown>> };
  };
  const rows = main
    .prepare('SELECT type, status, userId, payload FROM background_jobs ORDER BY type')
    .all();
  return rows.map((r) => ({
    ...r,
    payload: typeof r.payload === 'string' ? JSON.parse(r.payload) : (r.payload ?? null),
  }));
}

/** llm_logs rows over stable columns (id / timestamps / durationMs excluded;
 *  JSON columns parsed), sorted canonically — the fold + episode
 *  SUMMARIZATION rows are the fix's pinned evidence. */
async function readLlmLogsStable(): Promise<unknown> {
  const { getRawLLMLogsDatabase } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const db = getRawLLMLogsDatabase() as unknown as {
    prepare: (s: string) => { all: (...a: unknown[]) => Array<Record<string, unknown>> };
  } | null;
  if (!db) return [];
  const parse = (v: unknown): unknown => (typeof v === 'string' ? JSON.parse(v) : (v ?? null));
  const rows = db
    .prepare(
      'SELECT type, userId, chatId, messageId, characterId, autonomousRunId, ' +
        'provider, modelName, request, response, usage FROM llm_logs',
    )
    .all()
    .map((r) => ({ ...r, request: parse(r.request), response: parse(r.response), usage: parse(r.usage) }));
  rows.sort((a, b) => (JSON.stringify(a) < JSON.stringify(b) ? -1 : 1));
  return rows;
}

interface CaseSpec {
  name: string;
  freezeClock?: boolean;
  run: () => Promise<{ status: number; body: unknown; tables?: unknown; canned?: unknown }>;
}

async function loadRoute(path: string): Promise<Record<string, (...a: unknown[]) => Promise<unknown>>> {
  return (await import(path)) as never;
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

const MSG_ROUTE = '@/app/api/v1/chats/[id]/messages/[messageId]/route';
const CHAT_ROUTE = '@/app/api/v1/chats/[id]/route';
const CHAT_GET_ROUTE = '@/app/api/v1/chats/[id]/route';
const FILES_ROUTE = '@/app/api/v1/chats/[id]/files/route';
const CHATFILE_ROUTE = '@/app/api/v1/chat-files/[id]/route';

async function runCase(
  spec: Spec,
  meta: Meta,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string; llmlogs: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'ci-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  const llmLogsWork = join(work, 'llm-logs.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  copyFileSync(fixtures.llmlogs, llmLogsWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.SQLITE_LLM_LOGS_PATH = llmLogsWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  await initializeDatabase();

  const RealDate = Date;
  if (c.freezeClock) {
    const iso = new RealDate(NOW_MS).toISOString();
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    global.Date = class extends RealDate {
      constructor(...a: unknown[]) {
        if (a.length === 0) super(iso);
        // @ts-expect-error forward variadic args
        else super(...a);
      }
      static now(): number {
        return NOW_MS;
      }
    } as unknown as DateConstructor;
  }

  try {
    const out = await c.run();
    return {
      name: c.name,
      status: out.status,
      body: out.body,
      ...(out.tables !== undefined ? { tables: out.tables } : {}),
      ...(out.canned !== undefined ? { canned: out.canned } : {}),
    };
  } finally {
    global.Date = RealDate;
    await closeDatabase();
    closeMountIndexSQLiteClient();
    closeLLMLogsSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'courier-images-web.json'), 'utf8'),
  ) as Spec;
  const metaPath = process.env.QT_FIXTURE_CI_META ?? '';
  const meta = JSON.parse(fs.readFileSync(metaPath, 'utf8')) as Meta;

  const fixtures = {
    main: process.env.QT_FIXTURE_CI_MAIN ?? '',
    mount: process.env.QT_FIXTURE_CI_MOUNT ?? '',
    llmlogs: process.env.QT_FIXTURE_CI_LLMLOGS ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ci-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const B = 'http://localhost/api/v1';
  const cases: CaseSpec[] = [
    // The exact bytes saveImageToAlbum reads (readImageBuffer) at case time — the
    // Rust canned FileBytesStore returns these so the saved sha256 matches. (The
    // build-time recording can diverge from the case-time read; emit the truth.)
    {
      name: 'image_bytes',
      run: async () => {
        const { readImageBuffer } = await import('@/lib/images-v2');
        const buf = await readImageBuffer(meta.img1FileId);
        return { status: 200, body: { base64: Buffer.from(buf).toString('base64') } };
      },
    },
    // --- Reads ---
    {
      name: 'photo_albums',
      run: async () =>
        respond(
          await (await loadRoute(CHAT_GET_ROUTE)).GET(
            mockRequest(`${B}/chats/${CHAT_IMG}?action=photo-albums`),
            { params: Promise.resolve({ id: CHAT_IMG }) },
          ),
        ),
    },
    {
      name: 'files_list',
      run: async () =>
        respond(
          await (await loadRoute(FILES_ROUTE)).GET(mockRequest(`${B}/chats/${CHAT_IMG}/files`), {
            params: Promise.resolve({ id: CHAT_IMG }),
          }),
        ),
    },
    // --- Save image (character-vault attribution → Riya's vault) ---
    {
      name: 'save_image_riya',
      freezeClock: true,
      run: async () =>
        respond(
          await (await loadRoute(MSG_ROUTE)).POST(
            mockRequest(`${B}/chats/${CHAT_IMG}/messages/${MSG_IMG}?action=save-image`, {
              fileId: meta.img1FileId,
              mountPointId: meta.riyaVault,
              caption: 'A quiet scene',
            }),
            { params: Promise.resolve({ id: CHAT_IMG, messageId: MSG_IMG }) },
          ),
        ),
    },
    // --- Save image (user-persona attribution → Quilltap General) ---
    {
      name: 'save_image_general',
      freezeClock: true,
      run: async () =>
        respond(
          await (await loadRoute(MSG_ROUTE)).POST(
            mockRequest(`${B}/chats/${CHAT_IMG}/messages/${MSG_IMG}?action=save-image`, {
              fileId: meta.img1FileId,
              mountPointId: GENERAL_MP,
            }),
            { params: Promise.resolve({ id: CHAT_IMG, messageId: MSG_IMG }) },
          ),
        ),
    },
    // --- Save image error: fileId not attached to the message ---
    {
      name: 'save_image_not_attached',
      run: async () =>
        respond(
          await (await loadRoute(MSG_ROUTE)).POST(
            mockRequest(`${B}/chats/${CHAT_IMG}/messages/${MSG_IMG}?action=save-image`, {
              fileId: '99999999-9999-4999-8999-999999999999',
              mountPointId: meta.riyaVault,
            }),
            { params: Promise.resolve({ id: CHAT_IMG, messageId: MSG_IMG }) },
          ),
        ),
    },
    // --- Add tool result (user-initiated → Prospero bubble) ---
    {
      name: 'add_tool_result_user',
      freezeClock: true,
      run: async () => {
        const r = await (await loadRoute(CHAT_ROUTE)).POST(
          mockRequest(`${B}/chats/${CHAT_IMG}?action=add-tool-result`, {
            tool: 'generate_image',
            initiatedBy: 'user',
            prompt: 'a lantern-lit alley',
            images: [{ id: meta.img1FileId, filename: 'scene.png' }],
          }),
          { params: Promise.resolve({ id: CHAT_IMG }) },
        );
        return respond(r);
      },
    },
    // --- Add tool result (character-initiated → attached, no Staff sender) ---
    {
      name: 'add_tool_result_character',
      freezeClock: true,
      run: async () => {
        const r = await (await loadRoute(CHAT_ROUTE)).POST(
          mockRequest(`${B}/chats/${CHAT_IMG}?action=add-tool-result`, {
            tool: 'generate_image',
            initiatedBy: 'character',
            prompt: 'a foggy harbor',
            result: { success: true },
            images: [{ id: meta.img1FileId, filename: 'scene.png' }],
          }),
          { params: Promise.resolve({ id: CHAT_IMG }) },
        );
        return respond(r);
      },
    },
    // --- Resolve external turn (the settle; no connection profile → triggers skipped) ---
    {
      name: 'resolve',
      freezeClock: true,
      run: async () => {
        const r = await (await loadRoute(MSG_ROUTE)).POST(
          mockRequest(`${B}/chats/${CHAT_PENDING}/messages/${MSG_PENDING}?action=resolve-external-turn`, {
            replyContent: 'Lora nods and steps forward.',
          }),
          { params: Promise.resolve({ id: CHAT_PENDING, messageId: MSG_PENDING }) },
        );
        const { status, body } = await respond(r);
        return {
          status,
          body,
          tables: {
            message: await readMessageRow(CHAT_PENDING, MSG_PENDING),
            chat: await readChatFlags(CHAT_PENDING),
          },
        };
      },
    },
    // --- Resolve error: message is settled (not awaiting) ---
    {
      name: 'resolve_not_pending',
      run: async () =>
        respond(
          await (await loadRoute(MSG_ROUTE)).POST(
            mockRequest(`${B}/chats/${CHAT_PENDING}/messages/${MSG_SETTLED}?action=resolve-external-turn`, {
              replyContent: 'noop',
            }),
            { params: Promise.resolve({ id: CHAT_PENDING, messageId: MSG_SETTLED }) },
          ),
        ),
    },
    // --- Resolve external turn AT CADENCE: the connection profile resolves,
    //     the triggers FIRE, and post-resolve interchange 11 crosses the fold
    //     gate — pinning the summary fold AND the fold-episode pass (the
    //     courier-path FoldEpisodePassSeams evidence: the fold + episode
    //     SUMMARIZATION rows + the fold tail's TITLE_GENERATION row and title
    //     write + the memory-extraction enqueue). ---
    {
      name: 'resolve_cadence',
      freezeClock: true,
      run: async () => {
        cannedRecorded.clear();
        capturedTriggerPromises.length = 0;
        const r = await (await loadRoute(MSG_ROUTE)).POST(
          mockRequest(
            `${B}/chats/${CHAT_CADENCE}/messages/${MSG_CAD_PENDING}?action=resolve-external-turn`,
            { replyContent: 'Lora pins the final lantern code to the board.' },
          ),
          { params: Promise.resolve({ id: CHAT_CADENCE, messageId: MSG_CAD_PENDING }) },
        );
        const { status, body } = await respond(r);
        // Drain to quiescence: the three trigger promises land first; the
        // forced-awaitFold check promise is pushed while they settle.
        while (capturedTriggerPromises.length > 0) {
          const batch = capturedTriggerPromises.splice(0);
          await Promise.all(batch);
        }
        return {
          status,
          body,
          tables: {
            message: await readMessageRow(CHAT_CADENCE, MSG_CAD_PENDING),
            chat: await readCadenceChat(CHAT_CADENCE),
            contextSummaryEvents: await readContextSummaryEvents(CHAT_CADENCE),
            backgroundJobs: await readBackgroundJobsStable(),
            llmLogs: await readLlmLogsStable(),
          },
          canned: [...cannedRecorded.values()],
        };
      },
    },
    // --- Cancel external turn (delete placeholder + unpause) ---
    {
      name: 'cancel',
      run: async () => {
        const r = await (await loadRoute(MSG_ROUTE)).POST(
          mockRequest(`${B}/chats/${CHAT_PENDING}/messages/${MSG_PENDING}?action=cancel-external-turn`),
          { params: Promise.resolve({ id: CHAT_PENDING, messageId: MSG_PENDING }) },
        );
        const { status, body } = await respond(r);
        return {
          status,
          body,
          tables: {
            message: await readMessageRow(CHAT_PENDING, MSG_PENDING),
            chat: await readChatFlags(CHAT_PENDING),
          },
        };
      },
    },
    // --- Chat-file delete (CF_NOTES) ---
    {
      name: 'file_delete',
      run: async () => {
        const r = await (await loadRoute(CHATFILE_ROUTE)).DELETE(
          mockRequest(`${B}/chat-files/${CF_NOTES}`),
          { params: Promise.resolve({ id: CF_NOTES }) },
        );
        const { status, body } = await respond(r);
        return { status, body, tables: { files: await readChatFileIds(CHAT_IMG) } };
      },
    },
    // --- Chat-file delete error: nonexistent id ---
    {
      name: 'file_delete_missing',
      run: async () =>
        respond(
          await (await loadRoute(CHATFILE_ROUTE)).DELETE(
            mockRequest(`${B}/chat-files/99999999-9999-4999-8999-999999999999`),
            { params: Promise.resolve({ id: '99999999-9999-4999-8999-999999999999' }) },
          ),
        ),
    },
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    const payload = await runCase(spec, meta, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`courier-images-routes oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('courier-images-routes oracle', async () => {
  await main();
});
