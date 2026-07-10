/**
 * @jest-environment node
 *
 * P4.6c swipe-GENERATE ORACLE: drives v4's REAL `handleGenerateSwipe` (POST
 * `/api/v1/messages/{id}?action=swipe`, no `swipeIndex`) over a FRESH copy of the
 * committed salon GROUP fixture per case, emitting the response status + body + the
 * MAIN-db `chats` / `chat_messages` dumps + the recorded canned completion key.
 * The Rust port drives `api::salon::message_swipe_generate` + a canned-provider
 * `SwipeGenerateDriver`; the byte-exact regeneration itself is separately proven by
 * `regenerate_swipe_tier3` (this differential proves the ROUTE layer — the ownership
 * gate, the ASSISTANT-only / `systemSender` guards, the 201 `{ message }` shape —
 * and the driver seam over the salon fixture).
 *
 * Model boundaries + the out-of-scope buildContext feeders are mocked to the SAME
 * no-op values the Rust `NoopSeams` / `NoopMessageContextSeams` produce (the
 * `regenerate-swipe-tier3` precedent); the completion RECORDS its
 * `provider|model|temperature|messages` key so the Rust `CannedCompletionProvider`
 * replays it. The wall clock ADVANCES +1ms per read (frozen base) so buildContext's
 * timestamp-in-prompt matches the Rust injected `now_ms` and the `getMessages`
 * `ORDER BY createdAt` tie breaks the same way (`[[chain-depth-frozen-clock-artifact]]`).
 *
 * Cases: `swipe_happy` (an ASSISTANT character message → 201 new swipe),
 * `swipe_not_assistant` (a USER message → 400), `swipe_systemsender` (the Host
 * message → 400), `swipe_not_found` (a bad id → 404).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   … QT_ORACLE_OUT=/tmp/oracle-salon-swipe.ndjson npx jest -- salon-swipe-generate
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
interface CaseSpec {
  name: string;
  messageId: string;
}

const FROZEN_NOW_MS = 1_770_000_000_000;
const COMPLETION = {
  response: 'A fresh retort, reconsidered.',
  usage: { promptTokens: 40, completionTokens: 12, totalTokens: 52 },
};

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}
function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'salon.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_SALON_MAIN ?? '',
    mount: process.env.QT_FIXTURE_SALON_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-salon-swipe-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const GROUP_ASSISTANT = 'd2000000-0000-4000-8000-000000000007'; // ASSISTANT by Aria
  const GROUP_USER = 'd2000000-0000-4000-8000-000000000001'; // USER by Cleo
  const GROUP_HOST = 'd2000000-0000-4000-8000-000000000003'; // ASSISTANT systemSender host
  const cases: CaseSpec[] = [
    { name: 'swipe_happy', messageId: GROUP_ASSISTANT },
    { name: 'swipe_not_assistant', messageId: GROUP_USER },
    { name: 'swipe_systemsender', messageId: GROUP_HOST },
    { name: 'swipe_not_found', messageId: '99999999-9999-4999-8999-999999999999' },
  ];

  const outLines: string[] = [];
  const cannedCompletions = new Map<string, unknown>();

  for (const c of cases) {
    jest.resetModules();
    cannedCompletions.clear();

    const cipherDriverPath = require('node:path').join(
      process.cwd(),
      'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
    );
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/database/repositories', () =>
      jest.requireActual('@/lib/database/repositories'),
    );
    jest.doMock('@/lib/repositories/factory', () =>
      jest.requireActual('@/lib/repositories/factory'),
    );
    jest.doMock('@/lib/embedding/vector-store', () =>
      jest.requireActual('@/lib/embedding/vector-store'),
    );
    jest.doMock('@/lib/services/markdown-renderer.service', () => ({
      __esModule: true,
      renderMarkdownToHtml: async () => null,
      canPreRenderMessage: () => false,
    }));
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

    // The single non-streaming generation — record the canned key.
    jest.doMock('@/lib/llm', () => {
      const actual = jest.requireActual('@/lib/llm');
      return {
        __esModule: true,
        ...actual,
        createLLMProvider: async (provider: string) => ({
          sendMessage: async (
            params: { messages: Array<{ role: string; content: string }>; model: string; temperature?: number },
            _apiKey: string,
          ) => {
            const messages = params.messages.map((m) => ({ role: m.role, content: m.content }));
            const key = `${provider}|${params.model}|${params.temperature ?? '-'}|${JSON.stringify(messages)}`;
            if (!cannedCompletions.has(key)) {
              cannedCompletions.set(key, {
                provider,
                model: params.model,
                temperature: params.temperature ?? null,
                messages,
                response: COMPLETION.response,
                usage: COMPLETION.usage,
              });
            }
            return { content: COMPLETION.response, finishReason: 'stop', usage: COMPLETION.usage };
          },
        }),
      };
    });
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
    jest.doMock('@/lib/services/commonplace-notifications/writer', () => {
      const actual = jest.requireActual('@/lib/services/commonplace-notifications/writer');
      return { __esModule: true, ...actual, postCommonplaceWhisper: async () => null };
    });

    const work = mkdtempSync(join(scratch, 'salon-'));
    const mainWork = join(work, 'main.db');
    const mountWork = join(work, 'mount.db');
    copyFileSync(fixtures.main, mainWork);
    copyFileSync(fixtures.mount, mountWork);
    process.env.SQLITE_PATH = mainWork;
    process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

    const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
    const { closeMountIndexSQLiteClient } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    const { getRawDatabase } = await import('@/lib/database/backends/sqlite');
    await initializeDatabase();

    // Frozen +1ms/read clock (the chain-depth artifact).
    const RealDate = Date;
    let tick = 0;
    const nowMs = () => FROZEN_NOW_MS + tick++;
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

    try {
      const url = `http://localhost/api/v1/messages/${c.messageId}?action=swipe`;
      const params = { params: Promise.resolve({ id: c.messageId }) };
      const { POST } = await import('@/app/api/v1/messages/[id]/route');
      const response = (await POST(mockRequest(url, {}) as never, params as never)) as {
        status: number;
        json: () => Promise<unknown>;
      };
      const status = response.status;
      const body = await response.json();

      const mdb = getRawDatabase();
      if (!mdb) throw new Error('main DB handle unavailable');
      const tables: Record<string, unknown> = {};
      for (const [key, table] of [
        ['chats', 'chats'],
        ['chatMessages', 'chat_messages'],
      ]) {
        const columns = (
          mdb.prepare(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>
        ).map((col) => col.name);
        const rawRows = mdb.prepare(`SELECT * FROM ${table}`).all() as Array<
          Record<string, unknown>
        >;
        const rows = rawRows.map((r) => {
          const out: Record<string, unknown> = {};
          for (const col of columns) out[col] = canonValue(r[col]);
          return out;
        });
        tables[key] = { table, columns, rows };
      }
      const canned = Array.from(cannedCompletions.values());
      outLines.push(JSON.stringify({ name: c.name, status, body, tables, canned }));
    } finally {
      (global as { Date: DateConstructor }).Date = RealDate;
      Math.random = realRandom;
      await closeDatabase();
      closeMountIndexSQLiteClient();
      rmSync(work, { recursive: true, force: true });
    }
  }

  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`salon-swipe-generate oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('salon-swipe-generate oracle', async () => {
  await main();
});
