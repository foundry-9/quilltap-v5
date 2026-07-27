/**
 * @jest-environment node
 *
 * P4.9E3B tools-inventory ORACLE: drives v4's REAL `GET /api/v1/tools`
 * (`app/api/v1/tools/route.ts`) over a FRESH copy of the committed
 * chat-dialogs fixture per case, and emits each response body so the Rust port
 * (`services::tools_inventory`) diffs byte-for-byte (including per-tool KEY
 * ORDER — `parameters` lands before `available`).
 *
 * The `websearch_configured` case sets `SERPER_API_KEY` for its run only —
 * v4's `isWebSearchConfigured()` reads it at call time. No plugin loader runs
 * under jest, so `toolRegistry.getAllPlugins()` is empty and the output is
 * built-ins only (v5's standing no-plugin-runtime reality — if plugin rows
 * ever appear here, the differential fails loudly and the divergence must be
 * pinned, not filtered).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-cd-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/chat-dialogs-tools.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/chat-dialogs-web.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/chat-dialogs-main.db \
 *   QT_FIXTURE_CD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/chat-dialogs-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-tools-inventory.ndjson TZ=UTC \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- chat-dialogs-tools
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

const TOOLS_CHAT_PROJ = 'c2000000-0000-4000-8000-000000000004';
const TOOLS_CHAT_NOSTORES = 'c2000000-0000-4000-8000-000000000005';
const TOOLS_CHAT_BARE = 'c2000000-0000-4000-8000-000000000006';
const MISSING_ID = '99999999-9999-4999-8999-999999999999';

const RealDate = Date;

function mockRequest(url: string): unknown {
  return {
    method: 'GET',
    url,
    nextUrl: new URL(url),
    headers: new Headers(),
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

interface CaseSpec {
  name: string;
  query: string;
  serperKey?: boolean;
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'ti-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  if (c.serperKey) process.env.SERPER_API_KEY = 'synthetic-serper-key';
  else delete process.env.SERPER_API_KEY;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();

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
    const route = (await import('@/app/api/v1/tools/route')) as never as Record<
      string,
      (...a: unknown[]) => Promise<unknown>
    >;
    const resp = (await route.GET(
      mockRequest(`http://localhost/api/v1/tools${c.query}`),
    )) as { status: number; body?: unknown; json?: () => Promise<unknown> };
    const body =
      resp.body !== undefined && typeof resp.body === 'object'
        ? resp.body
        : await (resp as { json: () => Promise<unknown> }).json();
    return { name: c.name, status: resp.status, body };
  } finally {
    global.Date = RealDate;
    delete process.env.SERPER_API_KEY;
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) {
    throw new Error(
      `chat-dialogs-tools oracle must run under TZ=UTC (getTimezoneOffset=${offset})`,
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ti-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cases: CaseSpec[] = [
    // No chat: 40 built-ins, no availability keys at all.
    { name: 'no_chat', query: '' },
    // Schemas without a chat: parameters on the 37 mapped ids; the three photo
    // tools stay bare (the v4 map quirk).
    { name: 'no_chat_schemas', query: '?includeSchemas=true' },
    // Fully provisioned: image profile + project + stores + allowWebSearch +
    // multi-character — but no search provider configured, so search_web reads
    // the "No search provider configured…" arm.
    { name: 'chat_proj', query: `?chatId=${TOOLS_CHAT_PROJ}` },
    // The same chat with SERPER_API_KEY set: search_web goes available.
    {
      name: 'chat_proj_websearch_configured',
      query: `?chatId=${TOOLS_CHAT_PROJ}`,
      serperKey: true,
    },
    // Project without stores + a no-web-search profile + single character:
    // the doc_* store arm, the connection-profile arm, the whisper arm.
    { name: 'chat_nostores', query: `?chatId=${TOOLS_CHAT_NOSTORES}` },
    // Bare room with Wren (both wardrobe flags OFF), no profile, no project:
    // the wardrobe arms + project arms — and doc_copy_file STILL available
    // (missing from v4's switch).
    { name: 'chat_bare', query: `?chatId=${TOOLS_CHAT_BARE}` },
    // A missing chat: the context stays null — no availability keys (same
    // shape as no_chat).
    { name: 'chat_missing', query: `?chatId=${MISSING_ID}` },
    // Schemas + availability together — pins per-tool key order
    // (`parameters` before `available`).
    {
      name: 'chat_schemas',
      query: `?chatId=${TOOLS_CHAT_PROJ}&includeSchemas=true`,
    },
  ];

  const lines: string[] = [];
  for (const c of cases) {
    lines.push(JSON.stringify(await runCase(spec, c, scratch, fixtures)));
  }
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(`wrote ${lines.length} chat-dialogs-tools oracle rows to ${outPath}\n`);
}

test('chat-dialogs-tools oracle', async () => {
  await main();
});
