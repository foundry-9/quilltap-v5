/**
 * @jest-environment node
 *
 * P4.9E3B chat-export + outfit-summary ORACLE: drives v4's REAL chat GET route
 * (`app/api/v1/chats/[id]/route.ts` → `handlers/get.ts` `?action=export` /
 * `?action=outfit-summary`) over a FRESH copy of the committed chat-dialogs
 * fixture per case, and emits each response — the export's status + headers +
 * RAW BODY TEXT (the JSONL bytes are the wire payload) and the summary's JSON
 * body — so the Rust port (`services::chat_export` +
 * `api::chat_outfits::chat_outfit_summary`) diffs byte-for-byte.
 *
 * **Must run under `TZ=UTC`** (the export filename + `send_date` embed
 * `Date.getTime()` values).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-cd-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/chat-dialogs-export.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/chat-dialogs-web.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/chat-dialogs-main.db \
 *   QT_FIXTURE_CD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/chat-dialogs-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-chat-export.ndjson TZ=UTC \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- chat-dialogs-export
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

const EXPORT_CHAT = 'c2000000-0000-4000-8000-000000000001';
const NOCHAR_CHAT = 'c2000000-0000-4000-8000-000000000002';
const MERGE_SOURCE = 'c2000000-0000-4000-8000-000000000008';
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
  jest.doMock('@/lib/services/markdown-renderer.service', () => ({
    __esModule: true,
    renderMarkdownToHtml: async () => null,
    canPreRenderMessage: () => false,
  }));
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
  action: 'export' | 'outfit-summary';
  chatId: string;
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'cd-'));
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

  // The ticking frozen clock (chain-depth-frozen-clock-artifact): nothing in
  // these READ paths mints a stamp that lands in the output, but the house
  // convention keeps every oracle deterministic.
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
    const route = (await import('@/app/api/v1/chats/[id]/route')) as never as Record<
      string,
      (...a: unknown[]) => Promise<unknown>
    >;
    const resp = (await route.GET(
      mockRequest(`http://localhost/api/v1/chats/${c.chatId}?action=${c.action}`),
      { params: Promise.resolve({ id: c.chatId }) },
    )) as {
      status: number;
      body?: unknown;
      text?: () => Promise<string>;
      headers: { get?: (k: string) => string | null } & Record<string, unknown>;
    };
    // The jest next/server shim exposes the raw body string on `.body` (no
    // `.text()`); a real NextResponse has `.text()`. Handle both.
    const bodyText =
      typeof resp.text === 'function' ? await resp.text() : String(resp.body ?? '');
    const header = (k: string): string | null => {
      const h = resp.headers as { get?: (k: string) => string | null } | Record<string, string>;
      if (typeof (h as { get?: unknown }).get === 'function') {
        return (h as { get: (k: string) => string | null }).get(k);
      }
      const rec = h as Record<string, string>;
      return rec[k] ?? rec[k.toLowerCase()] ?? null;
    };
    const out: Record<string, unknown> = {
      name: c.name,
      status: resp.status,
      contentType: header('Content-Type'),
      contentDisposition: header('Content-Disposition'),
    };
    if (c.action === 'export' && resp.status === 200) {
      out.bodyText = bodyText; // the JSONL bytes, compared verbatim
    } else {
      // JSON responses: the shim keeps the parsed object on `.body`; a real
      // NextResponse yields text.
      out.body =
        resp.body !== undefined && typeof resp.body === 'object'
          ? resp.body
          : JSON.parse(bodyText);
    }
    return out;
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
      `chat-dialogs-export oracle must run under TZ=UTC (getTimezoneOffset=${offset})`,
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cd-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cases: CaseSpec[] = [
    // The multi-character transcript: attribution map, the void participant's
    // primary-name fallback, the unattributed USER line ('Friday'), the swipe
    // group, the SYSTEM-role drop, the TOOL row, `extra` from rawResponse, and
    // the sillyTavernMetadata header.
    { name: 'export_multi', action: 'export', chatId: EXPORT_CHAT },
    { name: 'export_no_character', action: 'export', chatId: NOCHAR_CHAT },
    { name: 'export_chat_missing', action: 'export', chatId: MISSING_ID },
    // Pip's composite expands to coat+boots (boots dropped from `top` by the
    // slot filter), the unresolvable hat id vanishes, and Nora's bare entry
    // keeps its four empty arrays.
    { name: 'outfit_summary_rich', action: 'outfit-summary', chatId: MERGE_SOURCE },
    { name: 'outfit_summary_empty', action: 'outfit-summary', chatId: EXPORT_CHAT },
    { name: 'outfit_summary_chat_missing', action: 'outfit-summary', chatId: MISSING_ID },
  ];

  const lines: string[] = [];
  for (const c of cases) {
    lines.push(JSON.stringify(await runCase(spec, c, scratch, fixtures)));
  }
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(`wrote ${lines.length} chat-dialogs-export oracle rows to ${outPath}\n`);
}

test('chat-dialogs-export oracle', async () => {
  await main();
});
