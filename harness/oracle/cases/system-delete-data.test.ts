/**
 * @jest-environment node
 *
 * P4.9G3 delete-all-data ORACLE (tier-2 DB-state): drives v4's REAL
 * `GET/POST /api/v1/system/tools?action=delete-data-preview|delete-data` route
 * handlers (which compose the REAL `lib/backup/restore/delete-service.ts`) over a
 * FRESH copy of the committed `system-data-*` fixture family per case, and emits
 *
 *   { name, status, body, counts }
 *
 * where `counts` is a row-count map over EVERY table in all three partitions
 * (`main.<table>` / `mount.<table>` / `llm.<table>`, enumerated from
 * `sqlite_master`) taken AFTER the case ran. The full map — not just the tables
 * the wipe is expected to touch — is what catches a repo whose v5 delete cascades
 * differently, and it pins the tables the wipe must NOT touch
 * (`instance_settings` above all).
 *
 * Cases: pristine (no op — the preview baseline), preview (must equal pristine),
 * the full delete, the delete run twice (idempotence), and the wrong-sentinel
 * refusal (400, no writes).
 *
 * [P4.D293] Every case also records `retire` — what the wipe's ONE retire call
 * (`retireMemories(ids, {reason: 'restore-replace'})`, `delete-service.ts:169`)
 * announced: `memoriesPublishes` (every `publishRealtime('memories')`, counted
 * through a pass-through mock of `@/lib/realtime/bus`) and `logs` (the
 * `RETIRE_LOGGED` lines, level + message — their BYTES are P4.D288's). And
 * `delete_data_clears_cursors` plants extraction cursors — CHAT_1's three
 * through v4's REAL `setCursor` plus one for a chat id the fixture lacks
 * (foreign keys off) — so `memory_extraction_cursors` in the truncate list is
 * observable: the chat delete's cascade takes CHAT_1's, only the truncate takes
 * the orphan.
 *
 * The session + startup-state mocks mirror `system-jobs-routes.test.ts`; the
 * background-job processor is pinned/no-oped so nothing starts a child.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-sysdel-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures" "$TMPO/lib"
 *   cp "$V5W/harness/oracle/cases/system-delete-data.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/lib/v4-migrations.ts" "$TMPO/lib/"
 *   cp "$V5W/harness/oracle/fixtures/system-data.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/system-data-main.db \
 *   QT_FIXTURE_SD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/system-data-mount.db \
 *   QT_FIXTURE_SD_LLM=$V5W/crates/quilltap-web/tests/fixtures/system-data-llmlogs.db \
 *   QT_ORACLE_OUT=/tmp/oracle-system-delete-data.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- system-delete-data
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { plantChatWardrobeHistoryTable } from '../lib/v4-migrations';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

const PINNED_STATUS = {
  running: false,
  processing: false,
  inFlight: 0,
  childCrashed: false,
};

function mockRequest(url: string, method: string, body: unknown): unknown {
  return {
    method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

/** [P4.D293] `publishRealtime` topics seen in the current case. */
let publishedTopics: string[] = [];

/** [P4.D293] The retire path's lines a case records (level + message). */
const RETIRE_LOGGED = new Set([
  'Retired memories',
  'Refreshed Commonplace mirror after retiring a digest',
  'Commonplace mirror refresh after retiring a digest failed',
]);

async function withRetireLogs<T>(
  body: () => Promise<T>,
): Promise<{ out: T; logs: Array<Record<string, unknown>> }> {
  const logs: Array<Record<string, unknown>> = [];
  const { Logger } = await import('@/lib/logger');
  const levels = ['error', 'warn', 'info', 'debug'] as const;
  const originals = Object.fromEntries(levels.map((l) => [l, Logger.prototype[l]]));
  for (const level of levels) {
    const original = originals[level];
    Logger.prototype[level] = function (this: unknown, message: string, ...rest: unknown[]) {
      if (RETIRE_LOGGED.has(message)) {
        const line: Record<string, unknown> = { level, message };
        // The mirror refresh's `written` — what R-AF's cadence moves.
        const context = rest[0] as Record<string, unknown> | undefined;
        if (message === 'Refreshed Commonplace mirror after retiring a digest') {
          line.written = context?.written;
        }
        logs.push(line);
      }
      return (original as (...a: unknown[]) => void).call(this, message, ...rest);
    } as never;
  }
  try {
    return { out: await body(), logs };
  } finally {
    for (const level of levels) Logger.prototype[level] = originals[level] as never;
  }
}

const CHAT_1 = 'c1000000-0000-4000-8000-000000000001';
/** [P4.D293] An orphan cursor — its chat is not in the fixture. */
const ORPHAN_CURSOR_SQL =
  'INSERT INTO "memory_extraction_cursors" ("id", "chatId", "pass", "messageId", "createdAt", "updatedAt") ' +
  "VALUES ('c10000ee-0000-4000-8000-0000000000ee:turn', 'c10000ee-0000-4000-8000-0000000000ee', 'turn', " +
  "'d10000ee-0000-4000-8000-0000000000ee', '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')";

/**
 * [P4.D293, R-AF] A CONSOLIDATED digest held by Lorian (`a1…01`) about Riya
 * (`a1…02`), so the wipe's retirement carries a digest bucket and its
 * announce refreshes the Commonplace mirror. v4 announces INSIDE
 * `retireMemories`, BEFORE the wipe deletes the characters; v5 announces after
 * the writer commits. Raw SQL both sides run verbatim.
 */
const DIGEST_SQL =
  'INSERT INTO "memories" ("id", "characterId", "aboutCharacterId", "content", "summary", ' +
  '"source", "tier", "consolidatedFrom", "consolidatedAt", "createdAt", "updatedAt") VALUES ' +
  "('ae0000d1-0000-4000-8000-0000000000d1', 'a1000000-0000-4000-8000-000000000001', " +
  "'a1000000-0000-4000-8000-000000000002', 'Lorian has come to trust Riya.', 'trust', " +
  "'CONSOLIDATED', 'hot', '[]', '2026-03-02T00:00:00.000Z', '2026-03-02T00:00:00.000Z', " +
  "'2026-03-02T00:00:00.000Z')";

async function plantDigest(): Promise<void> {
  const { rawQuery } = await import('@/lib/database/manager');
  await rawQuery(DIGEST_SQL);
}

async function plantCursors(): Promise<void> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  const cursors = (getRepositories() as unknown as Record<string, any>).memoryExtractionCursors;
  await cursors.setCursor(CHAT_1, 'other-fold', 'd1000000-0000-4000-8000-000000000001');
  await cursors.setCursor(CHAT_1, 'episode-fold', 'd1000000-0000-4000-8000-000000000002');
  await cursors.setCursor(CHAT_1, 'turn', 'd1000000-0000-4000-8000-0000000000ff');
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const raw = getRawDatabase() as unknown as { exec: (sql: string) => void };
  raw.exec('PRAGMA foreign_keys = OFF');
  raw.exec(ORPHAN_CURSOR_SQL);
  raw.exec('PRAGMA foreign_keys = ON');
}

function applyMocks(userId: string): void {
  jest.doMock('@/lib/realtime/bus', () => {
    const actual = jest.requireActual('@/lib/realtime/bus');
    return {
      __esModule: true,
      ...actual,
      publishRealtime: (topic: string, ...rest: unknown[]) => {
        publishedTopics.push(topic);
        return (actual.publishRealtime as (...a: unknown[]) => unknown)(topic, ...rest);
      },
    };
  });
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/background-jobs/processor', () => ({
    __esModule: true,
    getProcessorStatus: () => ({ ...PINNED_STATUS }),
    startProcessor: () => {},
    stopProcessor: () => {},
    wakeProcessor: () => {},
    ensureProcessorRunning: () => {},
  }));
  jest.doMock('@/lib/background-jobs', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/background-jobs'),
    ensureProcessorRunning: () => {},
    getProcessorStatus: () => ({ ...PINNED_STATUS }),
  }));
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: userId } }),
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

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  if (resp.status === 204) return { status: 204, body: null };
  return { status: resp.status, body: await resp.json() };
}

const TOOLS_ROUTE = '@/app/api/v1/system/tools/route';
const B = 'http://localhost/api/v1';

async function toolsGet(action: string): Promise<{ status: number; body: unknown }> {
  const route = (await import(TOOLS_ROUTE)) as never as Record<
    string,
    (...a: unknown[]) => Promise<unknown>
  >;
  return respond(await route.GET(mockRequest(`${B}/system/tools?action=${action}`, 'GET', {})));
}

async function toolsPost(
  action: string,
  body: unknown,
): Promise<{ status: number; body: unknown }> {
  const route = (await import(TOOLS_ROUTE)) as never as Record<
    string,
    (...a: unknown[]) => Promise<unknown>
  >;
  return respond(await route.POST(mockRequest(`${B}/system/tools?action=${action}`, 'POST', body)));
}

/** Row counts for every table in one partition handle, prefixed `<tag>.<table>`. */
function countPartition(
  db: { prepare: (sql: string) => { all: () => unknown[]; get: () => unknown } } | null,
  tag: string,
  out: Record<string, number>,
): void {
  if (!db) return;
  const tables = db
    .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
    .all() as Array<{ name: string }>;
  for (const { name } of tables) {
    const row = db.prepare(`SELECT COUNT(*) AS n FROM "${name}"`).get() as { n: number };
    out[`${tag}.${name}`] = row.n;
  }
}

async function countAll(): Promise<Record<string, number>> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawLLMLogsDatabase } = await import('@/lib/database/backends/sqlite/llm-logs-client');
  const out: Record<string, number> = {};
  countPartition(getRawDatabase() as never, 'main', out);
  countPartition(getRawMountIndexDatabase() as never, 'mount', out);
  countPartition(getRawLLMLogsDatabase() as never, 'llm', out);
  return out;
}

interface CaseSpec {
  name: string;
  run: () => Promise<{ status: number; body: unknown }>;
}

/** A no-op "case" so the pristine fixture's counts are on record. */
const NOOP = async () => ({ status: 0, body: null });

function buildCases(): CaseSpec[] {
  return [
    { name: 'pristine_counts', run: NOOP },
    { name: 'delete_data_preview', run: () => toolsGet('delete-data-preview') },
    {
      name: 'delete_data_wrong_confirm',
      run: () => toolsPost('delete-data', { confirm: 'nope' }),
    },
    {
      name: 'delete_data',
      run: () => toolsPost('delete-data', { confirm: 'DELETE_ALL_MY_DATA' }),
    },
    {
      name: 'delete_data_twice',
      run: async () => {
        await toolsPost('delete-data', { confirm: 'DELETE_ALL_MY_DATA' });
        return toolsPost('delete-data', { confirm: 'DELETE_ALL_MY_DATA' });
      },
    },
    {
      // `instance_settings` is the table the wipe must NOT touch. The committed
      // fixture deliberately carries no row there (the concurrency default), so
      // the case WRITES one first (through the concurrency verb) and then wipes:
      // a surviving row is the assertion, not a coincidental 0 == 0.
      name: 'delete_data_keeps_instance_settings',
      run: async () => {
        await toolsPost('job-concurrency', { concurrency: 7 });
        return toolsPost('delete-data', { confirm: 'DELETE_ALL_MY_DATA' });
      },
    },
    // ── P4.D65: the SWEPT leg (the banked P4.D63 unit-8 arm) ──────────────
    // The KEPT leg was already covered by every other wipe case: the committed
    // fixture carries one ARCHIVE-category `files` row, and `main.files` lands
    // on 1 rather than 0 because the default spares it. What no case reached is
    // the explicit opt-in to destroy it — so a port that kept ARCHIVE rows
    // unconditionally, ignoring the option entirely, passed. Here `main.files`
    // must reach ZERO and the summary must report `archiveBundlesKept: false`.
    {
      name: 'delete_data_sweeps_archive_bundles',
      run: () =>
        toolsPost('delete-data', {
          confirm: 'DELETE_ALL_MY_DATA',
          keepArchivedCharacterBundles: false,
        }),
    },
    {
      // …and the explicit TRUE, which must behave exactly as the default does.
      // Without it, the pair above could be satisfied by a port that read the
      // flag but inverted it.
      name: 'delete_data_keeps_archive_bundles_explicitly',
      run: () =>
        toolsPost('delete-data', {
          confirm: 'DELETE_ALL_MY_DATA',
          keepArchivedCharacterBundles: true,
        }),
    },
    {
      // [P4.D293 L-2] the cursor table in the truncate list.
      name: 'delete_data_clears_cursors',
      run: async () => {
        await plantCursors();
        return toolsPost('delete-data', { confirm: 'DELETE_ALL_MY_DATA' });
      },
    },
    {
      // [P4.D293, R-AF] a retired digest: the announce's mirror refresh.
      name: 'delete_data_retires_a_digest',
      run: async () => {
        await plantDigest();
        return toolsPost('delete-data', { confirm: 'DELETE_ALL_MY_DATA' });
      },
    },
    {
      // The preview after a full wipe: every count zero, and it still writes
      // nothing.
      name: 'delete_data_preview_after_wipe',
      run: async () => {
        await toolsPost('delete-data', { confirm: 'DELETE_ALL_MY_DATA' });
        return toolsGet('delete-data-preview');
      },
    },
  ];
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string; llm: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec.userId);

  const work = mkdtempSync(join(scratch, 'sd-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  const llmWork = join(work, 'llm.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  copyFileSync(fixtures.llm, llmWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.SQLITE_LLM_LOGS_PATH = llmWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  await initializeDatabase();
  // [P4.D317] The outfit-history table on every copy (a booted instance's
  // shape; v4's REAL DDL through the round's shared plant — never the seed).
  // At the `e8e17b2d9` pin `clearFormat3Entities` truncates it; on an absent
  // table v4 logs ERROR + DEBUG and moves on. The Rust side runs
  // `ensure_chat_wardrobe_history_on`.
  await plantChatWardrobeHistoryTable();

  try {
    publishedTopics = [];
    const { out, logs } = await withRetireLogs(() => c.run());
    const counts = await countAll();
    return {
      name: c.name,
      status: out.status,
      body: out.body,
      counts,
      retire: {
        memoriesPublishes: publishedTopics.filter((t) => t === 'memories').length,
        logs,
      },
    };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    closeLLMLogsSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'system-data.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_SD_MAIN ?? '',
    mount: process.env.QT_FIXTURE_SD_MOUNT ?? '',
    llm: process.env.QT_FIXTURE_SD_LLM ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-sysdel-oracle-'));
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const outLines: string[] = [];
  for (const c of buildCases()) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`system-delete-data oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('system-delete-data oracle', async () => {
  await main();
});

// Remove the OS-temp scratch dir(s) once the oracle has written its NDJSON.
const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
