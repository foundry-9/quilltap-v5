/**
 * @jest-environment node
 *
 * P4.9E3B search-replace ORACLE: drives v4's REAL standalone route
 * (`app/api/v1/search-replace/route.ts` → `search-replace-service.ts`) over a
 * FRESH copy of the committed chat-dialogs fixture per case, and emits each
 * response body plus post-mutation table dumps (SR_CHAT's chat row + messages,
 * and every memory) so the Rust port (`services::search_replace`) diffs
 * byte-for-byte.
 *
 * The case-asymmetry cases pin v4's search/replace mismatch: the memory COUNT
 * is case-insensitive while the REPLACE is case-sensitive, and the message
 * path is case-sensitive on both sides.
 *
 * **Must run under `TZ=UTC`.**
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-cd-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/chat-dialogs-search-replace.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/chat-dialogs-web.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/chat-dialogs-main.db \
 *   QT_FIXTURE_CD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/chat-dialogs-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-search-replace.ndjson TZ=UTC \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- chat-dialogs-search-replace
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  frozenNowMs: number;
}

const NORA = 'a2000000-0000-4000-8000-000000000001';
const SR_CHAT = 'c2000000-0000-4000-8000-000000000003';
const MISSING_ID = '99999999-9999-4999-8999-999999999999';

const RealDate = Date;

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
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge'),
  );
  jest.doMock('@/lib/mount-index/character-vault', () =>
    jest.requireActual('@/lib/mount-index/character-vault'),
  );
  jest.doMock('@/lib/background-jobs/processor', () => ({
    __esModule: true,
    ensureProcessorRunning: () => {},
    startProcessor: () => {},
    stopProcessor: () => {},
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
}

/** SR_CHAT's chat row + every message event, plus every memory (all columns
 * the execute path can move; `embedding` proves the no-embed invariant). */
async function readTables(): Promise<unknown> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  const repos = getRepositories();
  const chat = (await repos.chats.findById(SR_CHAT)) ?? null;
  const messages = await repos.chats.getMessages(SR_CHAT);
  const memories = ((await repos.memories.findAll()) as Array<Record<string, unknown>>)
    .map((m) => ({
      id: m.id,
      characterId: m.characterId,
      chatId: m.chatId ?? null,
      sourceMessageId: m.sourceMessageId ?? null,
      content: m.content,
      summary: m.summary,
      keywords: m.keywords,
      embedding: m.embedding ?? null,
      updatedAt: m.updatedAt,
    }))
    .sort((a, b) => String(a.id).localeCompare(String(b.id)));
  return { chat, messages, memories };
}

interface CaseSpec {
  name: string;
  action: string;
  body: unknown;
  dump?: boolean;
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'sr-'));
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
  await initializeDatabase();

  // Ticking frozen clock — the execute path mints memory `updatedAt`s.
  let tick = 0;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  global.Date = class extends RealDate {
    constructor(...a: unknown[]) {
      if (a.length === 0) super(spec.frozenNowMs + tick++);
      // @ts-expect-error forward variadic args
      else super(...a);
    }
    static now(): number {
      return spec.frozenNowMs + tick++;
    }
  } as unknown as DateConstructor;

  try {
    const route = (await import('@/app/api/v1/search-replace/route')) as never as Record<
      string,
      (...a: unknown[]) => Promise<unknown>
    >;
    const resp = (await route.POST(
      mockRequest(`http://localhost/api/v1/search-replace?action=${c.action}`, c.body),
    )) as { status: number; body?: unknown; json?: () => Promise<unknown> };
    const body =
      resp.body !== undefined && typeof resp.body === 'object'
        ? resp.body
        : await (resp as { json: () => Promise<unknown> }).json();
    return {
      name: c.name,
      status: resp.status,
      body,
      ...(c.dump ? { tables: await readTables() } : {}),
    };
  } finally {
    global.Date = RealDate;
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) {
    throw new Error(
      `chat-dialogs-search-replace oracle must run under TZ=UTC (getTimezoneOffset=${offset})`,
    );
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'chat-dialogs-web.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_CD_MAIN ?? '',
    mount: process.env.QT_FIXTURE_CD_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-sr-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const chatScope = { type: 'chat', chatId: SR_CHAT };
  const charScope = { type: 'character', characterId: NORA };

  const cases: CaseSpec[] = [
    // Preview: 3 case-sensitive message matches (R1/R2/R4 — R5's capital
    // misses), 4 case-INSENSITIVE memory matches (content/summary/keyword/
    // capital arms).
    {
      name: 'preview_chat_scope',
      action: 'preview',
      body: { scope: chatScope, searchText: 'lantern', replaceText: 'beacon' },
    },
    {
      name: 'preview_character_scope',
      action: 'preview',
      body: { scope: charScope, searchText: 'lantern', replaceText: 'beacon' },
    },
    {
      name: 'preview_messages_only',
      action: 'preview',
      body: { scope: chatScope, searchText: 'lantern', replaceText: 'beacon', includeMemories: false },
    },
    {
      name: 'preview_memories_only',
      action: 'preview',
      body: { scope: chatScope, searchText: 'lantern', replaceText: 'beacon', includeMessages: false },
    },
    {
      name: 'preview_no_match',
      action: 'preview',
      body: { scope: chatScope, searchText: 'gryphon', replaceText: 'beacon' },
    },
    {
      name: 'preview_chat_missing',
      action: 'preview',
      body: { scope: { type: 'chat', chatId: MISSING_ID }, searchText: 'lantern', replaceText: 'beacon' },
    },
    // Execute: 3 messages rewritten, 3 memories (the capital-L row is counted
    // by preview but NOT replaced — the asymmetry), embeddings untouched.
    {
      name: 'execute_chat_scope',
      action: 'execute',
      body: { scope: chatScope, searchText: 'lantern', replaceText: 'beacon' },
      dump: true,
    },
    {
      name: 'execute_character_scope',
      action: 'execute',
      body: { scope: charScope, searchText: 'lantern', replaceText: 'beacon' },
      dump: true,
    },
    {
      name: 'execute_case_asymmetry',
      action: 'execute',
      body: { scope: chatScope, searchText: 'Lantern', replaceText: 'Beacon' },
      dump: true,
    },
    {
      name: 'execute_memories_only',
      action: 'execute',
      body: { scope: chatScope, searchText: 'lantern', replaceText: 'beacon', includeMessages: false },
      dump: true,
    },
    {
      name: 'execute_no_match',
      action: 'execute',
      body: { scope: chatScope, searchText: 'gryphon', replaceText: 'beacon' },
      dump: true,
    },
    // The validation arms.
    {
      name: 'execute_invalid_scope',
      action: 'execute',
      body: { scope: { type: 'project', projectId: SR_CHAT }, searchText: 'x', replaceText: 'y' },
    },
    {
      name: 'preview_empty_search',
      action: 'preview',
      body: { scope: chatScope, searchText: '', replaceText: 'y' },
    },
    {
      name: 'unknown_action',
      action: 'rename',
      body: { scope: chatScope, searchText: 'x', replaceText: 'y' },
    },
    {
      // No ?action= at all — the route's own default-handler copy.
      name: 'missing_action',
      action: '',
      body: { scope: chatScope, searchText: 'x', replaceText: 'y' },
    },
  ];

  const lines: string[] = [];
  for (const c of cases) {
    lines.push(JSON.stringify(await runCase(spec, c, scratch, fixtures)));
  }
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(
    `wrote ${lines.length} chat-dialogs-search-replace oracle rows to ${outPath}\n`,
  );
}

test('chat-dialogs-search-replace oracle', async () => {
  await main();
});
