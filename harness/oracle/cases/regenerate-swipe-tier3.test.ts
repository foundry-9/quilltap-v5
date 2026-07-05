/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for regenerate-swipe (v4
 * `lib/services/chat-message/regenerate-swipe.service.ts`
 * `regenerateMessageAsSwipe`), the sibling entry point to `processMessage`.
 *
 * Drives v4's REAL `regenerateMessageAsSwipe` over the committed corpus against
 * the REAL two-DB fixture, mocking ONLY the model boundaries + the out-of-scope
 * subsystems the Rust port injects as seams (each matching the Rust seam):
 *   - `createLLMProvider().sendMessage` — the single non-streaming generation:
 *     returns the corpus canned completion + RECORDS the exact
 *     `provider|model|temperature|messages` key (so the Rust
 *     `CannedCompletionProvider` replays it — the rebuilt continue-mode prompt
 *     bytes are proven);
 *   - `generateEmbeddingForUser` — canned (buildContext memory search is
 *     memory-free here: the memories sit on a "ghost" characterId, not Bertie);
 *   - buildContext's unported feeders + the buildMessageContext K file-loader →
 *     the same no-op values the Rust `NoopSeams` / `NoopMessageContextSeams`
 *     produce.
 * The memory cascade (`deleteMemoriesBySourceMessageWithVectors`) touches no LLM
 * — it runs against the REAL DB + vector store.
 *
 * The wall clock ADVANCES +1ms per read (frozen base = `frozenNowMs`) so
 * buildContext's timestamp-in-prompt matches the Rust injected `now_ms` while the
 * `getMessages` `ORDER BY createdAt` tie is broken the same way the Rust real
 * clock breaks it (see `[[chain-depth-frozen-clock-artifact]]`).
 *
 * Run (Node 24, from the v4 checkout; `.claude` worktree copy workaround):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; TMPO=/tmp/qt-oracle-run
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_REGEN_MAIN=/tmp/qt-regen-main.db QT_FIXTURE_REGEN_MOUNT=/tmp/qt-regen-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-regenerate-swipe.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- regenerate-swipe-tier3
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
}): { table: string; columns: string[]; rows: Array<Record<string, unknown>> } {
  const { table, columns, rawRows } = opts;
  const rows = rawRows.map((r) => {
    const out: Record<string, unknown> = {};
    for (const col of columns) out[col] = canonValue(r[col]);
    return out;
  });
  return { table, columns, rows };
}

interface CallSpec {
  name: string;
  chatId: string;
  userId: string;
  targetMessageId: string;
  expectThrow?: boolean;
}
interface Spec {
  testPepperBase64: string;
  frozenNowMs: number;
  completion: { response: string; usage: { promptTokens: number; completionTokens: number; totalTokens: number } };
  calls: CallSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'regenerate-swipe-tier3.json'), 'utf8')
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_REGEN_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_REGEN_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_REGEN_MAIN / QT_FIXTURE_REGEN_MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-regen-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'regen-main.db');
  const workMount = join(scratch, 'regen-mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cannedCompletions = new Map<
    string,
    { provider: string; model: string; temperature: number | null; messages: unknown[]; response: string; usage: unknown }
  >();

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  // jest.setup mocks the vector store globally; un-mock it so the memory
  // cascade's `getCharacterVectorStore().save()` actually removes the vector
  // entries against the real DB (else load() is empty → nothing removed → the
  // vectors leak, diverging from the Rust path).
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store')
  );

  // ---- createLLMProvider (the single non-streaming generation) ----
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
          const key = `${provider}|${params.model}|${params.temperature ?? '-'}|${JSON.stringify(messages)}`;
          if (!cannedCompletions.has(key)) {
            cannedCompletions.set(key, {
              provider,
              model: params.model,
              temperature: params.temperature ?? null,
              messages,
              response: spec.completion.response,
              usage: spec.completion.usage,
            });
          }
          return { content: spec.completion.response, finishReason: 'stop', usage: spec.completion.usage };
        },
      }),
    };
  });

  // ---- embeddings (buildContext memory search — memory-free corpus) ----
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

  // ---- buildMessageContext → the REAL wrapper; mock ONLY the K file-loader ----
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
  // Commonplace recall fold reader (buildContext trailing recall) → empty.
  jest.doMock('@/lib/services/commonplace-notifications/writer', () => {
    const actual = jest.requireActual('@/lib/services/commonplace-notifications/writer');
    return { __esModule: true, ...actual, postCommonplaceWhisper: async () => null };
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { regenerateMessageAsSwipe } = await import(
    '@/lib/services/chat-message/regenerate-swipe.service'
  );

  await initializeDatabase();
  const repos = getRepositories();

  // The wall clock ADVANCES +1ms per read (frozen base). See the header + the
  // chain-depth artifact note.
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
  const realRandom = Math.random;
  Math.random = () => 0;

  const lines: string[] = [];

  for (const call of spec.calls) {
    let threw = false;
    let message = '';
    try {
      const chat = await repos.chats.findById(call.chatId);
      if (!chat) throw new Error(`chat not found: ${call.chatId}`);
      const allMessages = await repos.chats.getMessages(call.chatId);
      const targetMessage = allMessages.find((m: { id?: string }) => m.id === call.targetMessageId);
      if (!targetMessage) throw new Error(`target message not found: ${call.targetMessageId}`);
      await regenerateMessageAsSwipe({
        repos,
        userId: call.userId,
        chat,
        targetMessage,
        allMessages,
        activeUserParticipantId: null,
      } as never);
    } catch (err) {
      threw = true;
      message = err instanceof Error ? err.message : String(err);
    }
    lines.push(JSON.stringify({ kind: 'call', call: call.name, threw, message }));
    // Let any fire-and-forget settle.
    await new Promise((resolve) => setTimeout(resolve, 100));
  }

  (global as { Date: DateConstructor }).Date = RealDate;
  Math.random = realRandom;

  for (const c of cannedCompletions.values()) lines.push(JSON.stringify({ kind: 'cannedCompletion', ...c }));

  const dumpTable = async (table: string) => {
    const columns = ((await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>).map(
      (c) => c.name
    );
    const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
    return canonicalizeRows({ table, columns, rawRows });
  };

  for (const table of ['chats', 'chat_messages', 'memories', 'vector_indices', 'vector_entries']) {
    lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable(table)) }));
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`regenerate-swipe oracle wrote ${outPath}\n`);
}

test('regenerate-swipe tier-3 oracle', async () => {
  await main();
});
