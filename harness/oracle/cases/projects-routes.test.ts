/**
 * @jest-environment node
 *
 * P4.6k PROJECTS route-surface ORACLE: drives v4's REAL projects route handlers
 * over a FRESH copy of the committed groups-projects fixture per case, and emits
 * each response body (+ post-mutation table dumps) so the Rust ports
 * (`api::projects::*`) can be diffed byte-for-byte.
 *
 * Unit-2 coverage: list (O(n²) _count), detail (rich roster + empty), roster
 * list, list-chats (activity sort + its createdAt fallback + pagination),
 * get-state, mount-points
 * (dangling filtered + empty); create (default injection), update, delete
 * (chats/files nulled, mount links UNTOUCHED), roster add/remove, chat
 * add/remove, set/reset-state, tool-settings, mount link/unlink.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-gp-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/projects-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/groups-projects.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_GP_MAIN=$V5W/crates/quilltap-web/tests/fixtures/groups-projects-main.db \
 *   QT_FIXTURE_GP_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/groups-projects-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-projects-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- projects-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  /** P4.D256: the planted wear-ledger rows. */
  p4d256Ledger: { rows: unknown[][] };
  /** P4.D256: the planted `files` rows (an item's own / a foreign picture). */
  p4d256Files: { rows: unknown[][]; own: string; foreign: string };
}

const IOTA = 'a3000000-0000-4000-8000-000000000001';
const LAMBDA = 'a3000000-0000-4000-8000-000000000002';
const KAPPA = 'a3000000-0000-4000-8000-000000000003';
const ARIA = 'a1000000-0000-4000-8000-000000000001';
const BRAM = 'a1000000-0000-4000-8000-000000000002';
/** P4.D63: the archived character (fixture extension). */
const EDDA = 'a1000000-0000-4000-8000-000000000005';
const GAMMA_EXTRA_MP = 'b0000000-0000-4000-8000-000000000001';
const IOTA_DANGLING_MP = 'b0000000-0000-4000-8000-0000000000df';
const CHAT_A = 'c1000000-0000-4000-8000-000000000001';
/** P4.D140: the never-spoken-in project chat (`lastMessageAt` NULL). */
const CHAT_B = 'c1000000-0000-4000-8000-000000000002';
const CLOAK = 'aa000000-0000-4000-8000-000000000001';
const ENSEMBLE = 'aa000000-0000-4000-8000-000000000002';
const BG_FILE = 'f0000001-0000-4000-8000-000000000001'; // legacy image, projectId null
const LAMBDA_FILE_1 = 'f0000002-0000-4000-8000-000000000002'; // projectId = Lambda
const MISSING_FILE = 'f0000009-0000-4000-8000-000000000009';
/** P4.148: a well-formed character uuid with no row behind it. */
const MISSING_CHARACTER = 'a1000000-0000-4000-8000-0000000000ff';

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'GET',
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

/** Dump project slim rows + chats/files projectId + project links (baked ids). */
async function dumpProjectTables(): Promise<unknown> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const main = getRawDatabase() as unknown as { prepare: (s: string) => { all: () => unknown } };
  const mount = getRawMountIndexDatabase() as unknown as {
    prepare: (s: string) => { all: () => unknown };
  };
  return {
    projects: main.prepare('SELECT id, name FROM projects ORDER BY id').all(),
    chats: main.prepare('SELECT id, projectId FROM chats ORDER BY id').all(),
    files: main.prepare('SELECT id, projectId FROM files ORDER BY id').all(),
    links: mount
      .prepare('SELECT projectId, mountPointId FROM project_doc_mount_links ORDER BY projectId, mountPointId')
      .all(),
  };
}

/**
 * P4.D256 — the wear ledger on this case's fresh copy: the table from v4's own
 * migration statements (`WARDROBE_WEAR_STATS_DDL`) and `spec.p4d256Ledger.rows`
 * (the groups-projects pair predates the table; the Rust side runs
 * `ensure_wear_ledger_on` + the same rows). `dumpLedger` is the table after.
 */
async function plantLedger(rows: unknown[][]): Promise<void> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { WARDROBE_WEAR_STATS_DDL } = (await import(
    '@/lib/database/backends/sqlite/wardrobe-wear-stats-ddl'
  )) as { WARDROBE_WEAR_STATS_DDL: readonly string[] };
  const raw = getRawDatabase() as unknown as {
    exec: (s: string) => void;
    prepare: (s: string) => { run: (...a: unknown[]) => unknown };
  };
  for (const statement of WARDROBE_WEAR_STATS_DDL) raw.exec(statement);
  const insert = raw.prepare(
    'INSERT INTO "wardrobe_wear_stats" ("id", "itemId", "wearerCharacterId", "wearCount", "firstWornAt", "lastWornAt", "lastWornChatId", "createdAt", "updatedAt") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)',
  );
  for (const r of rows) insert.run(...r);
}

/** P4.D256 — the PUT cases' pictures (raw `files` rows, mirrored on the Rust side). */
async function plantFiles(rows: unknown[][]): Promise<void> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const raw = getRawDatabase() as unknown as {
    prepare: (s: string) => { run: (...a: unknown[]) => unknown };
  };
  const insert = raw.prepare(
    'INSERT INTO "files" ("id", "userId", "sha256", "originalFilename", "mimeType", "size", "linkedTo", "source", "category", "tags", "createdAt", "updatedAt") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)',
  );
  for (const r of rows) insert.run(...r);
}

async function dumpLedger(): Promise<unknown> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const raw = getRawDatabase() as unknown as { prepare: (s: string) => { all: () => unknown } };
  return raw
    .prepare('SELECT "id", "itemId", "wearerCharacterId", "wearCount" FROM "wardrobe_wear_stats" ORDER BY "id"')
    .all();
}

interface CaseSpec {
  name: string;
  run: () => Promise<{ status: number; body: unknown; tables?: unknown; logs?: unknown }>;
}

/**
 * P4.142 (G1) — the links plant: `doc_mount_file_links.originalMimeType`
 * renamed on this case's fresh copy (v4's raw mount-index handle), then `call`
 * with every ERROR/WARN recorded off the `Logger` prototype as `{level, message,
 * fields}` (`module`/`error` omitted — the mail-tools plant recipe). The column
 * is named by the files repository's `queryLinks` and the links' `queryJoined`,
 * never by the project overlay's batch reads, so the project still resolves.
 */
async function withLinksPlant(
  call: () => Promise<{ status: number; body: unknown }>,
): Promise<{ status: number; body: unknown; logs: unknown }> {
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('raw mount-index handle unavailable');
  midb.exec('ALTER TABLE "doc_mount_file_links" RENAME COLUMN "originalMimeType" TO "originalMimeType_x"');
  const { Logger } = await import('@/lib/logger');
  const logs: Array<Record<string, unknown>> = [];
  for (const level of ['error', 'warn'] as const) {
    const original = Logger.prototype[level];
    Logger.prototype[level] = function (
      this: unknown,
      message: string,
      context?: Record<string, unknown>,
      ...rest: unknown[]
    ) {
      const fields: Array<[string, string]> = [];
      for (const [k, v] of Object.entries(context ?? {})) {
        if (k === 'module' || k === 'error') continue;
        if (typeof v === 'string' || typeof v === 'number' || typeof v === 'boolean') {
          fields.push([k, String(v)]);
        }
      }
      logs.push({ level, message, fields });
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }
  const out = await call();
  return { ...out, logs };
}

/**
 * P4.148 — record every INFO/WARN/ERROR the call logs off the `Logger`
 * prototype as `{level, message, fields, error}`: `fields` are the context's
 * string/number/boolean values in key order (`module`/`error` omitted), and
 * `error` the line's error message — a context `error` string, or the Error
 * passed as the third argument (`logger.error(msg, ctx, err)`). No plant.
 */
async function withLogs(
  call: () => Promise<{ status: number; body: unknown }>,
): Promise<{ status: number; body: unknown; logs: unknown }> {
  const { Logger } = await import('@/lib/logger');
  const logs: Array<Record<string, unknown>> = [];
  const originals = {
    info: Logger.prototype.info,
    warn: Logger.prototype.warn,
    error: Logger.prototype.error,
  };
  for (const level of ['info', 'warn', 'error'] as const) {
    const original = originals[level];
    Logger.prototype[level] = function (
      this: unknown,
      message: string,
      context?: Record<string, unknown>,
      ...rest: unknown[]
    ) {
      const fields: Array<[string, string]> = [];
      for (const [k, v] of Object.entries(context ?? {})) {
        if (k === 'module' || k === 'error') continue;
        if (typeof v === 'string' || typeof v === 'number' || typeof v === 'boolean') {
          fields.push([k, String(v)]);
        }
      }
      const ctxError = context?.error;
      const third = rest[0];
      const error =
        typeof ctxError === 'string'
          ? ctxError
          : third instanceof Error
            ? third.message
            : null;
      logs.push({ level, message, fields, error });
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }
  try {
    const out = await call();
    return { ...out, logs };
  } finally {
    Logger.prototype.info = originals.info;
    Logger.prototype.warn = originals.warn;
    Logger.prototype.error = originals.error;
  }
}

async function loadRoute(path: string): Promise<Record<string, (...a: unknown[]) => Promise<unknown>>> {
  return (await import(path)) as never;
}
async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec);
  const work = mkdtempSync(join(scratch, 'gp-'));
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
  try {
    const out = await c.run();
    return {
      name: c.name,
      status: out.status,
      body: out.body,
      ...(out.tables !== undefined ? { tables: out.tables } : {}),
      ...(out.logs !== undefined ? { logs: out.logs } : {}),
    };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'groups-projects.json'), 'utf8'),
  ) as Spec;
  const fixtures = {
    main: process.env.QT_FIXTURE_GP_MAIN ?? '',
    mount: process.env.QT_FIXTURE_GP_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-gp-proj-oracle-'));
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const B = 'http://localhost/api/v1/projects';
  const idRoute = '@/app/api/v1/projects/[id]/route';
  const mpRoute = '@/app/api/v1/projects/[id]/mount-points/route';
  const p = (id: string) => ({ params: Promise.resolve({ id }) });

  const cases: CaseSpec[] = [
    // --- Reads ---
    { name: 'list', run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).GET(mockRequest(B))) },
    { name: 'get_iota', run: async () => respond(await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${IOTA}`), p(IOTA))) },
    { name: 'get_kappa', run: async () => respond(await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${KAPPA}`), p(KAPPA))) },
    { name: 'list_characters', run: async () => respond(await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${IOTA}?action=list-characters`), p(IOTA))) },
    { name: 'list_chats', run: async () => respond(await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${IOTA}?action=list-chats`), p(IOTA))) },
    { name: 'list_chats_page', run: async () => respond(await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${IOTA}?action=list-chats&limit=1&offset=0`), p(IOTA))) },
    // P4.D140 / v4 `735d9408c`: the activity sort's fallback is `createdAt`,
    // NEVER `updatedAt`. Chat B has never been spoken in; push its `updatedAt`
    // past Chat A's activity and the OLD `lastMessageAt ?? updatedAt` spelling
    // would float it to the top, while `chatActivityAt` leaves it below. The
    // committed fixture cannot express this on its own (both chats were created
    // the same day), so the case mutates its own copy — replayed identically on
    // the Rust side.
    {
      name: 'list_chats_activity_fallback',
      run: async () => {
        const { rawQuery } = await import('@/lib/database/manager');
        await rawQuery('UPDATE "chats" SET "updatedAt" = ? WHERE "id" = ?', [
          '2026-12-01T00:00:00.000Z',
          CHAT_B,
        ]);
        return respond(
          await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${IOTA}?action=list-chats`), p(IOTA)),
        );
      },
    },
    // P4.D143 (v4 `c43d3b1b4`): the project chats row carries the DERIVED
    // `conciergeState` + `dangerCategories`, never the raw label. Both project
    // chats painted at once — Chat A Vouched over a TRUE label, Chat B
    // Uncensored over a FALSE one — plus a Flagged pass with categories. The
    // labels are set the wrong way round on the operator rows, so a row that
    // leaked `isDangerousChat` would be visibly wrong, not accidentally right.
    // `list_chats` above is the Monitored arm.
    {
      name: 'list_chats_operator_states',
      run: async () => {
        const { rawQuery } = await import('@/lib/database/manager');
        await rawQuery('UPDATE "chats" SET "isDangerousChat" = 1 WHERE "id" = ?', [CHAT_A]);
        await rawQuery('UPDATE "chats" SET "isDangerousChat" = 0 WHERE "id" = ?', [CHAT_B]);
        // P4.D226 (v4 `4d370a90f`): the states the rows derive from. P4.D227
        // (`3b463d6b1`, #76): the legacy `conciergeOverride` column is DROPPED
        // at this pin, so the plant no longer writes it.
        await rawQuery(
          'UPDATE "chats" SET "conciergeMode" = ?, "conciergeModeSetBy" = ?, "conciergeModeReason" = ? WHERE "id" = ?',
          ['locked', 'operator', 'migration', CHAT_A],
        );
        await rawQuery(
          'UPDATE "chats" SET "conciergeMode" = ?, "conciergeModeSetBy" = ?, "conciergeModeReason" = ? WHERE "id" = ?',
          ['unmoderated', 'operator', 'manual', CHAT_B],
        );
        return respond(
          await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${IOTA}?action=list-chats`), p(IOTA)),
        );
      },
    },
    {
      name: 'list_chats_flagged_categories',
      run: async () => {
        const { rawQuery } = await import('@/lib/database/manager');
        await rawQuery('UPDATE "chats" SET "isDangerousChat" = 1, "dangerCategories" = ? WHERE "id" = ?', [
          JSON.stringify(['Violence', 'Substance Use']),
          CHAT_A,
        ]);
        await rawQuery(
          'UPDATE "chats" SET "conciergeMode" = ?, "conciergeModeSetBy" = ?, "conciergeModeReason" = ? WHERE "id" = ?',
          ['unmoderated', 'concierge', 'classifier', CHAT_A],
        );
        return respond(
          await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${IOTA}?action=list-chats`), p(IOTA)),
        );
      },
    },
    { name: 'get_state', run: async () => respond(await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${IOTA}?action=get-state`), p(IOTA))) },
    { name: 'background_iota', run: async () => respond(await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${IOTA}?action=get-background`), p(IOTA))) },
    { name: 'background_kappa', run: async () => respond(await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${KAPPA}?action=get-background`), p(KAPPA))) },
    // P4.70: the latest-chat BRANCH of handleGetBackground, which neither
    // standing arm reaches — Iota is stored in the retired 'project' mode
    // (which the GET's normalize folds to 'theme') and Kappa in 'theme'. The
    // PUT is the only way into 'latest_chat' without touching the committed
    // fixture, and it is the surviving mode, so this is one arm, not a new
    // fixture: set the mode, then read the background back. Iota owns two
    // chats, one carrying `storyBackgroundImageId` and one not, so the FILTER
    // is exercised and `sourceChatId` must name the right one. (The
    // sort-by-updatedAt-desc is NOT exercised — one candidate — recorded
    // rather than faked.)
    {
      name: 'background_iota_latest_chat',
      run: async () => {
        await (await loadRoute(idRoute)).PUT(
          mockRequest(`${B}/${IOTA}`, { backgroundDisplayMode: 'latest_chat' }),
          p(IOTA),
        );
        return respond(
          await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${IOTA}?action=get-background`), p(IOTA)),
        );
      },
    },
    { name: 'aesthetic_get_lantern', run: async () => respond(await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${IOTA}?action=aesthetic&kind=lantern`), p(IOTA))) },
    { name: 'aesthetic_get_aurora', run: async () => respond(await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${IOTA}?action=aesthetic&kind=aurora`), p(IOTA))) },
    { name: 'aesthetic_get_empty', run: async () => respond(await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${KAPPA}?action=aesthetic&kind=lantern`), p(KAPPA))) },
    { name: 'mount_points_iota', run: async () => respond(await (await loadRoute(mpRoute)).GET(mockRequest(`${B}/${IOTA}/mount-points`), p(IOTA))) },
    { name: 'mount_points_kappa', run: async () => respond(await (await loadRoute(mpRoute)).GET(mockRequest(`${B}/${KAPPA}/mount-points`), p(KAPPA))) },
    // --- Mutations ---
    {
      name: 'create',
      // The value arm of `color`/`icon` (a real colour + icon). Its sibling
      // `create_no_colour` below is the `|| null` arm (P4.146, dogfood #136).
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, { name: 'Mu', description: 'A new project', allowAnyCharacter: true, characterRoster: [ARIA], color: '#abcdef', icon: 'rocket' }),
      )),
    },
    // ---- P4.D114 / v4 bug 98 (`c93ec7ff`) ----
    // `createProjectSchema` moved out of `route.ts` into `schemas.ts` and the
    // four presentational fields became `.nullable().optional()`. The create
    // dialogs send `description || null` for a blank field, so the old plain
    // `.optional()` refused the whole project over an empty description.
    // MEASURED, old schema vs new: ONLY the four null legs moved. Every other
    // arm below is unchanged by the fix and exists because v5's hand-rolled
    // create validated NOTHING but the name.
    {
      // The bug-98 body itself, with a real colour/icon (the value arm); its
      // sibling `create_blank_description_no_colour` is the `|| null` arm.
      name: 'create_blank_description',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, { name: 'Nu', description: null, instructions: null, color: '#abcdef', icon: 'rocket' }),
      )),
    },
    {
      // `.min(1)` is on the RAW string — no `.trim()` in this schema, so a
      // whitespace-only name is length 3 and PASSES. v5's old
      // `name.trim().is_empty()` guard refused it.
      name: 'create_whitespace_name',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, { name: '   ', color: '#abcdef', icon: 'rocket' }),
      )),
    },
    {
      // `color`/`icon` null — bug 98's other two legs (v4 REFUSED this body
      // before `c93ec7ff`). Since P4.146 (dogfood #136) the whole echo is a
      // live comparand, `"color": null, "icon": null` included.
      name: 'create_null_color_and_icon',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, { name: 'Xi', color: null, icon: null }),
      )),
    },
    {
      name: 'create_missing_name',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, {}),
      )),
    },
    {
      name: 'create_empty_name',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, { name: '' }),
      )),
    },
    {
      name: 'create_name_over_max',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, { name: 'x'.repeat(101) }),
      )),
    },
    {
      // Zod ≥ 4.5.4 (v4 `6e1a64ea6`) measures the name's `.max(100)` in CODE
      // POINTS once the UTF-16 count overflows it: 51 top hats are 102 units but
      // 51 code points, so v4 now ACCEPTS (this row was
      // `create_name_astral_over_max` under 4.4.3's UTF-16 rule).
      name: 'create_name_astral_within_max',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, { name: '\u{1F3A9}'.repeat(51), color: '#abcdef', icon: 'rocket' }),
      )),
    },
    {
      name: 'create_name_wrong_type',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, { name: 42 }),
      )),
    },
    {
      name: 'create_description_over_max',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, { name: 'P', description: 'x'.repeat(2001) }),
      )),
    },
    {
      name: 'create_instructions_over_max',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, { name: 'P', instructions: 'x'.repeat(10001) }),
      )),
    },
    {
      name: 'create_icon_over_max',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, { name: 'P', icon: 'x'.repeat(51) }),
      )),
    },
    {
      name: 'create_bad_color',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, { name: 'P', color: 'blue' }),
      )),
    },
    {
      name: 'create_roster_non_uuid',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, { name: 'P', characterRoster: ['not-a-uuid'] }),
      )),
    },
    {
      name: 'create_roster_null',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, { name: 'P', characterRoster: null }),
      )),
    },
    {
      name: 'create_allow_any_wrong_type',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, { name: 'P', allowAnyCharacter: 'yes' }),
      )),
    },
    {
      name: 'create_allow_any_null',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, { name: 'P', allowAnyCharacter: null }),
      )),
    },
    {
      name: 'create_non_object_body',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, 'hello'),
      )),
    },
    {
      // Non-strict `z.object`: the unknown key is stripped, not refused.
      name: 'create_unknown_key_stripped',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, { name: 'Omicron', color: '#abcdef', icon: 'rocket', notAField: 'should vanish' }),
      )),
    },
    {
      // P4.D246 (v4 `9753d0eb2`): `allowAnyCharacter: z.boolean().prefault(true)`
      // — a project created with the flag ABSENT is OPEN. Stated by itself on a
      // minimal `{name}` body rather than read off a validation row's echo. (Since
      // P4.146 the echo's `color: null, icon: null` — v4's `|| null` on an
      // absent key — is compared too.)
      name: 'create_flag_absent_defaults_open',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, { name: 'Pi' }),
      )),
    },
    // ---- P4.146 (dogfood #136): the `|| null` arm, compared whole ----
    // v4's create route stores `color: validatedData.color || null` (and the
    // same for `icon`), so a body with NO colour/icon writes an explicit
    // `null` into `properties.json` and echoes it. Siblings of the three
    // value-arm rows above (each kept: it pins the value arm).
    {
      name: 'create_no_colour',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, { name: 'Mu', description: 'A new project', allowAnyCharacter: true, characterRoster: [ARIA] }),
      )),
    },
    {
      name: 'create_blank_description_no_colour',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, { name: 'Nu', description: null, instructions: null }),
      )),
    },
    {
      name: 'create_whitespace_name_no_colour',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, { name: '   ' }),
      )),
    },
    {
      // MEASURED: an empty colour never reaches `|| null` — `HexColorSchema`
      // refuses `''` first, so this is the flat 400.
      name: 'create_empty_colour_400',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, { name: 'Rho', color: '' }),
      )),
    },
    {
      // An empty ICON passes `z.string().max(50)` and `'' || null` stores null.
      name: 'create_empty_icon',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, { name: 'Sigma', icon: '' }),
      )),
    },
    {
      // The PUT entry point: `updateProjectSchema`'s `color` is
      // `.nullable().optional()`, the repository copies the null verbatim into
      // the read-modify-write, and the parse keeps it.
      name: 'update_null_color',
      run: async () => respond(await (await loadRoute(idRoute)).PUT(mockRequest(`${B}/${IOTA}`, { color: null, icon: null }), p(IOTA))),
    },
    {
      // The read wire after a nulling PUT: `projectList` spreads the parsed bag,
      // so Iota's row carries `color: null` and `icon: null`.
      name: 'list_after_null_color',
      run: async () => {
        await (await loadRoute(idRoute)).PUT(mockRequest(`${B}/${IOTA}`, { color: null, icon: null }), p(IOTA));
        return respond(await (await loadRoute('@/app/api/v1/projects/route')).GET(mockRequest(B)));
      },
    },
    { name: 'update', run: async () => respond(await (await loadRoute(idRoute)).PUT(mockRequest(`${B}/${IOTA}`, { name: 'Iota Renamed', backgroundDisplayMode: 'theme' }), p(IOTA))) },
    // P4.55 (the merge-verb silent-keep sweep): v4 runs
    // `updateProjectSchema.parse(body)` and hands the repository the PARSED
    // data, so an invalid field is a 400 `Validation error` (the ZodError
    // escapes into `handleRouteError`) and an unknown key is STRIPPED, never
    // written. v5 passed the raw body straight through with no validation.
    // Status + `error` message compared; the `details` issues array is the
    // recorded no-details divergence class.
    {
      name: 'update_invalid_type',
      run: async () =>
        respond(await (await loadRoute(idRoute)).PUT(mockRequest(`${B}/${IOTA}`, { allowAnyCharacter: 'yes' }), p(IOTA))),
    },
    {
      // `z.string().min(1).max(100)` — the over-max leg.
      name: 'update_over_max',
      run: async () =>
        respond(await (await loadRoute(idRoute)).PUT(mockRequest(`${B}/${IOTA}`, { name: 'x'.repeat(101) }), p(IOTA))),
    },
    {
      // `backgroundDisplayMode` is `.optional()` but NOT `.nullable()`, so an
      // explicit null refuses where `description: null` (below) clears.
      name: 'update_null_non_nullable',
      run: async () =>
        respond(await (await loadRoute(idRoute)).PUT(mockRequest(`${B}/${IOTA}`, { backgroundDisplayMode: null }), p(IOTA))),
    },
    // [P4.D146 / v4 70505745a] The update enum narrows to
    // ['latest_chat','theme']: the two retired modes are REFUSED at the write
    // gate (the coercion lives in the properties schema, for values already on
    // disk — a write must not be able to put one there afresh). The surviving
    // mode still passes, so the narrowing is not a blanket refusal.
    {
      name: 'update_retired_mode_project',
      run: async () =>
        respond(await (await loadRoute(idRoute)).PUT(mockRequest(`${B}/${IOTA}`, { backgroundDisplayMode: 'project' }), p(IOTA))),
    },
    {
      name: 'update_retired_mode_static',
      run: async () =>
        respond(await (await loadRoute(idRoute)).PUT(mockRequest(`${B}/${IOTA}`, { backgroundDisplayMode: 'static' }), p(IOTA))),
    },
    {
      name: 'update_surviving_mode_latest_chat',
      run: async () =>
        respond(await (await loadRoute(idRoute)).PUT(mockRequest(`${B}/${IOTA}`, { backgroundDisplayMode: 'latest_chat' }), p(IOTA))),
    },
    {
      // The unknown key is STRIPPED by `z.object`, not refused: the request
      // succeeds and the echo must not carry it. The dump proves nothing of it
      // reached the row either.
      name: 'update_unknown_key_stripped',
      run: async () => {
        const r = await (await loadRoute(idRoute)).PUT(
          mockRequest(`${B}/${IOTA}`, { name: 'Iota Stripped', notAField: 'should vanish' }),
          p(IOTA),
        );
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpProjectTables() };
      },
    },
    {
      // P4.55, the P4.D85 cleared-null residue: `description` is
      // `.nullable()`, so this CLEARS it. v4's store-backed `update` answers
      // `_update`'s in-memory merge overlaid, v5 re-reads — this arm settles
      // whether the two echoes agree on a cleared store-resident key.
      name: 'update_clear_description',
      run: async () =>
        respond(await (await loadRoute(idRoute)).PUT(mockRequest(`${B}/${IOTA}`, { description: null }), p(IOTA))),
    },
    // ── P4.D246 (v4 `9753d0eb2`): the PUT answers the ENRICHED project ──
    // `handlePutDefault` now runs the GET's `enrichProject` over the stored
    // project after the write, so the echo carries `characterRoster` as display
    // entries (not ids) and `_count`. The four standing PUT rows above pin it
    // on Iota's rich roster; these two state the shape where the standing rows
    // cannot: an EMPTY roster (Kappa — `characterRoster: []` + `_count` with
    // `characters: 0`), and the "same shape as GET" claim made a row (a PUT on
    // Iota followed by the GET, both bodies recorded; the Rust side blanks the
    // minted `updatedAt` and asserts the two agree).
    {
      name: 'update_on_empty_roster',
      run: async () =>
        respond(await (await loadRoute(idRoute)).PUT(mockRequest(`${B}/${KAPPA}`, { description: 'Kappa, re-described' }), p(KAPPA))),
    },
    {
      name: 'update_then_get_agree',
      run: async () => {
        const mod = await loadRoute(idRoute);
        const put = await respond(await mod.PUT(mockRequest(`${B}/${IOTA}`, { name: 'Iota Agreed' }), p(IOTA)));
        const get = await respond(await mod.GET(mockRequest(`${B}/${IOTA}`), p(IOTA)));
        return { status: put.status, body: { put: put.body, get: get.body } };
      },
    },
    {
      name: 'delete',
      run: async () => {
        const r = await (await loadRoute(idRoute)).DELETE(mockRequest(`${B}/${IOTA}`), p(IOTA));
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpProjectTables() };
      },
    },
    {
      name: 'add_character',
      run: async () => {
        const r = await (await loadRoute(idRoute)).POST(mockRequest(`${B}/${KAPPA}?action=add-character`, { characterId: BRAM }), p(KAPPA));
        const { status, body } = await respond(r);
        return { status, body };
      },
    },
    {
      // P4.D63 (v4 `d553f72a`): an archived character cannot join a roster.
      name: 'add_character_archived',
      run: async () => {
        const r = await (await loadRoute(idRoute)).POST(mockRequest(`${B}/${KAPPA}?action=add-character`, { characterId: EDDA }), p(KAPPA));
        const { status, body } = await respond(r);
        return { status, body };
      },
    },
    {
      name: 'remove_character',
      run: async () => {
        const r = await (await loadRoute(idRoute)).DELETE(mockRequest(`${B}/${IOTA}?action=remove-character`, { characterId: ARIA }), p(IOTA));
        return respond(r);
      },
    },
    {
      name: 'add_chat',
      run: async () => {
        const r = await (await loadRoute(idRoute)).POST(mockRequest(`${B}/${KAPPA}?action=add-chat`, { chatId: CHAT_A }), p(KAPPA));
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpProjectTables() };
      },
    },
    {
      name: 'remove_chat',
      run: async () => {
        const r = await (await loadRoute(idRoute)).DELETE(mockRequest(`${B}/${IOTA}?action=remove-chat`, { chatId: CHAT_A }), p(IOTA));
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpProjectTables() };
      },
    },
    {
      name: 'aesthetic_set',
      run: async () => {
        const mod = await loadRoute(idRoute);
        const url = `${B}/${IOTA}?action=aesthetic&kind=aurora`;
        const put = await mod.PUT(mockRequest(url, { content: '  A brand new aurora palette.  ' }), p(IOTA));
        const { status, body } = await respond(put);
        // Readback via GET to prove the write landed (trimmed).
        const rb = await mod.GET(mockRequest(url), p(IOTA));
        const readback = ((await (rb as { json: () => Promise<{ content?: string }> }).json()).content) ?? '';
        return { status, body: { ...(body as object), readback } };
      },
    },
    {
      name: 'aesthetic_clear',
      run: async () => {
        const mod = await loadRoute(idRoute);
        const url = `${B}/${IOTA}?action=aesthetic&kind=lantern`;
        const put = await mod.PUT(mockRequest(url, { content: '   ' }), p(IOTA));
        const { status, body } = await respond(put);
        const rb = await mod.GET(mockRequest(url), p(IOTA));
        const readback = ((await (rb as { json: () => Promise<{ content?: string }> }).json()).content) ?? '';
        return { status, body: { ...(body as object), readback } };
      },
    },
    { name: 'set_state', run: async () => respond(await (await loadRoute(idRoute)).PUT(mockRequest(`${B}/${IOTA}?action=set-state`, { state: { mood: 'tense', turns: 9 } }), p(IOTA))) },
    { name: 'reset_state', run: async () => respond(await (await loadRoute(idRoute)).DELETE(mockRequest(`${B}/${IOTA}?action=reset-state`), p(IOTA))) },
    { name: 'tool_settings', run: async () => respond(await (await loadRoute(idRoute)).POST(mockRequest(`${B}/${IOTA}?action=update-tool-settings`, { defaultDisabledTools: ['web_search'], defaultDisabledToolGroups: ['danger'] }), p(IOTA))) },
    // --- Wardrobe (Unit 4) ---
    { name: 'wardrobe_list', run: async () => respond(await (await loadRoute('@/app/api/v1/projects/[id]/wardrobe/route')).GET(mockRequest(`${B}/${IOTA}/wardrobe`), p(IOTA))) },
    { name: 'wardrobe_get', run: async () => respond(await (await loadRoute('@/app/api/v1/projects/[id]/wardrobe/[itemId]/route')).GET(mockRequest(`${B}/${IOTA}/wardrobe/${CLOAK}`), { params: Promise.resolve({ id: IOTA, itemId: CLOAK }) })) },
    { name: 'wardrobe_create', run: async () => respond(await (await loadRoute('@/app/api/v1/projects/[id]/wardrobe/route')).POST(mockRequest(`${B}/${IOTA}/wardrobe`, { title: 'Rain Boots', description: 'For puddles.', imagePrompt: 'yellow rubber boots', types: ['footwear'], isDefault: false }), p(IOTA))) },
    { name: 'wardrobe_update', run: async () => respond(await (await loadRoute('@/app/api/v1/projects/[id]/wardrobe/[itemId]/route')).PUT(mockRequest(`${B}/${IOTA}/wardrobe/${CLOAK}`, { title: 'Weathered Cloak', description: null }), { params: Promise.resolve({ id: IOTA, itemId: CLOAK }) })) },
    // ── P4.D120 / v4 `d25dacc1` ────────────────────────────────────────
    // The project wardrobe list used to pass a hard-coded `true` and let the
    // client filter; the flag is server-side now. A fresh fixture holds no
    // archived garment, so each of these archives the Cloak with a first PUT.
    {
      name: 'wardrobe_list_hides_an_archived_garment',
      run: async () => {
        const item = await loadRoute('@/app/api/v1/projects/[id]/wardrobe/[itemId]/route');
        await item.PUT(mockRequest(`${B}/${IOTA}/wardrobe/${CLOAK}`, { archived: true }), { params: Promise.resolve({ id: IOTA, itemId: CLOAK }) });
        return respond(await (await loadRoute('@/app/api/v1/projects/[id]/wardrobe/route')).GET(mockRequest(`${B}/${IOTA}/wardrobe`), p(IOTA)));
      },
    },
    {
      name: 'wardrobe_list_shows_an_archived_garment_with_the_flag',
      run: async () => {
        const item = await loadRoute('@/app/api/v1/projects/[id]/wardrobe/[itemId]/route');
        await item.PUT(mockRequest(`${B}/${IOTA}/wardrobe/${CLOAK}`, { archived: true }), { params: Promise.resolve({ id: IOTA, itemId: CLOAK }) });
        return respond(await (await loadRoute('@/app/api/v1/projects/[id]/wardrobe/route')).GET(mockRequest(`${B}/${IOTA}/wardrobe?includeArchived=true`), p(IOTA)));
      },
    },
    {
      name: 'wardrobe_update_archives',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/[id]/wardrobe/[itemId]/route')).PUT(mockRequest(`${B}/${IOTA}/wardrobe/${CLOAK}`, { archived: true }), { params: Promise.resolve({ id: IOTA, itemId: CLOAK }) })),
    },
    {
      // The NEW 404, reachable only with `archived` in the body: without it the
      // route never does the extra O(folder) read that finds nothing.
      name: 'wardrobe_update_archived_missing_item_404',
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/[id]/wardrobe/[itemId]/route')).PUT(mockRequest(`${B}/${IOTA}/wardrobe/eeeeeeee-eeee-4eee-8eee-eeeeeeeeee01`, { archived: true }), { params: Promise.resolve({ id: IOTA, itemId: 'eeeeeeee-eeee-4eee-8eee-eeeeeeeeee01' }) })),
    },
    {
      name: 'wardrobe_delete',
      run: async () => {
        const mod = await loadRoute('@/app/api/v1/projects/[id]/wardrobe/[itemId]/route');
        const r = await mod.DELETE(mockRequest(`${B}/${IOTA}/wardrobe/${ENSEMBLE}`), { params: Promise.resolve({ id: IOTA, itemId: ENSEMBLE }) });
        const { status, body } = await respond(r);
        // Re-list to prove the remaining wardrobe (only the Cloak survives).
        const list = await (await loadRoute('@/app/api/v1/projects/[id]/wardrobe/route')).GET(mockRequest(`${B}/${IOTA}/wardrobe`), p(IOTA));
        const remaining = ((await (list as { json: () => Promise<{ wardrobeItems?: unknown[] }> }).json()).wardrobeItems ?? []).map((i: { id: string; title: string }) => ({ id: i.id, title: i.title }));
        return { status, body: { ...(body as object), remaining } };
      },
    },
    // P4.D256 (v4 `3ee3b1342`): the project tier over a planted ledger — the
    // factory's `?action=wear-history` (its 404 runs before the ledger read),
    // the list's `wear` totals, and the DELETE dropping the item's rows only.
    {
      name: 'wardrobe_wear_history',
      run: async () => {
        await plantLedger(spec.p4d256Ledger.rows);
        return respond(await (await loadRoute('@/app/api/v1/projects/[id]/wardrobe/[itemId]/route')).GET(mockRequest(`${B}/${IOTA}/wardrobe/${CLOAK}?action=wear-history`), { params: Promise.resolve({ id: IOTA, itemId: CLOAK }) }));
      },
    },
    {
      name: 'wardrobe_wear_history_missing_item',
      run: async () => {
        await plantLedger(spec.p4d256Ledger.rows);
        return respond(await (await loadRoute('@/app/api/v1/projects/[id]/wardrobe/[itemId]/route')).GET(mockRequest(`${B}/${IOTA}/wardrobe/eeeeeeee-eeee-4eee-8eee-eeeeeeeeee01?action=wear-history`), { params: Promise.resolve({ id: IOTA, itemId: 'eeeeeeee-eeee-4eee-8eee-eeeeeeeeee01' }) }));
      },
    },
    {
      name: 'wardrobe_list_with_ledger',
      run: async () => {
        await plantLedger(spec.p4d256Ledger.rows);
        return respond(await (await loadRoute('@/app/api/v1/projects/[id]/wardrobe/route')).GET(mockRequest(`${B}/${IOTA}/wardrobe`), p(IOTA)));
      },
    },
    {
      name: 'wardrobe_delete_with_ledger',
      run: async () => {
        await plantLedger(spec.p4d256Ledger.rows);
        const mod = await loadRoute('@/app/api/v1/projects/[id]/wardrobe/[itemId]/route');
        const r = await mod.DELETE(mockRequest(`${B}/${IOTA}/wardrobe/${ENSEMBLE}`), { params: Promise.resolve({ id: IOTA, itemId: ENSEMBLE }) });
        const { status, body } = await respond(r);
        return { status, body: { ...(body as object), ledger: await dumpLedger() } };
      },
    },
    // P4.D256 (v4 `7c8572869`): the PUT's `imageFileId` — one of the item's
    // own pictures (echoed), a foreign one (400), `null` (cleared), not a uuid
    // (the middleware's `Validation error`).
    ...([
      ['wardrobe_update_image_file_id_own', () => spec.p4d256Files.own],
      ['wardrobe_update_image_file_id_foreign', () => spec.p4d256Files.foreign],
      ['wardrobe_update_image_file_id_null', () => null],
      ['wardrobe_update_image_file_id_not_a_uuid', () => 'nope'],
    ] as Array<[string, () => string | null]>).map(([name, pick]) => ({
      name,
      run: async () => {
        await plantFiles(spec.p4d256Files.rows);
        return respond(await (await loadRoute('@/app/api/v1/projects/[id]/wardrobe/[itemId]/route')).PUT(mockRequest(`${B}/${IOTA}/wardrobe/${CLOAK}`, { imageFileId: pick() }), { params: Promise.resolve({ id: IOTA, itemId: CLOAK }) }));
      },
    })),
    { name: 'mount_link', run: async () => respond(await (await loadRoute(mpRoute)).POST(mockRequest(`${B}/${IOTA}/mount-points`, { mountPointId: GAMMA_EXTRA_MP }), p(IOTA))) },
    {
      name: 'mount_unlink',
      run: async () => {
        const r = await (await loadRoute(mpRoute)).DELETE(mockRequest(`${B}/${IOTA}/mount-points`, { mountPointId: IOTA_DANGLING_MP }), p(IOTA));
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpProjectTables() };
      },
    },
    // --- Files (Unit 5 / P4.6n): list-files two-branch + add/remove ---
    // Iota: STORE-BACKED branch (its official store carries assets/logo.png).
    { name: 'list_files_iota', run: async () => respond(await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${IOTA}?action=list-files`), p(IOTA))) },
    // P4.142 (G1): Branch A's store read is `docMountFiles.findByMountPointId`, a
    // fallback — a broken links table lists `[]` with v4's `Error finding files
    // by mount point ID` line (200), never a 500.
    {
      name: 'list_files_iota_links_plant',
      run: async () =>
        withLinksPlant(async () =>
          respond(await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${IOTA}?action=list-files`), p(IOTA))),
        ),
    },
    // Lambda: LEGACY branch (official-store link removed; two legacy files).
    { name: 'list_files_lambda', run: async () => respond(await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${LAMBDA}?action=list-files`), p(LAMBDA))) },
    // Kappa: empty (legacy branch, no files).
    { name: 'list_files_kappa', run: async () => respond(await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${KAPPA}?action=list-files`), p(KAPPA))) },
    {
      name: 'add_file',
      run: async () => {
        const r = await (await loadRoute(idRoute)).POST(mockRequest(`${B}/${KAPPA}?action=add-file`, { fileId: BG_FILE }), p(KAPPA));
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpProjectTables() };
      },
    },
    { name: 'add_file_missing', run: async () => respond(await (await loadRoute(idRoute)).POST(mockRequest(`${B}/${KAPPA}?action=add-file`, { fileId: MISSING_FILE }), p(KAPPA))) },
    {
      name: 'remove_file',
      run: async () => {
        const r = await (await loadRoute(idRoute)).DELETE(mockRequest(`${B}/${LAMBDA}?action=remove-file`, { fileId: LAMBDA_FILE_1 }), p(LAMBDA));
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpProjectTables() };
      },
    },
    // P4.23: the corrupted-store 503 envelope arms. Malformed bytes planted
    // into IOTA's official store's properties.json through the REAL
    // writeDatabaseDocument; the hydrating findById then throws
    // ProjectStoreUnavailableError and the middleware answers the deliberate,
    // contextful 503 (`context.ts:176-205`). The PUT arm proves the WRITE
    // route refuses on the same READ-path throw (it hydrates before writing).
    {
      name: 'get_store_corrupt',
      run: async () => {
        const { rawQuery } = await import('@/lib/database/manager');
        const rows = (await rawQuery(
          `SELECT officialMountPointId AS mp FROM projects WHERE id = '${IOTA}'`,
        )) as Array<{ mp: string | null }>;
        const mp = rows[0]?.mp;
        if (!mp) throw new Error('iota has no officialMountPointId');
        const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
        await writeDatabaseDocument(mp, 'properties.json', '{');
        // P4.148: the catch's `[Projects v1] Error fetching project` line.
        return withLogs(async () =>
          respond(await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${IOTA}`), p(IOTA))),
        );
      },
    },
    {
      name: 'update_store_corrupt',
      run: async () => {
        const { rawQuery } = await import('@/lib/database/manager');
        const rows = (await rawQuery(
          `SELECT officialMountPointId AS mp FROM projects WHERE id = '${IOTA}'`,
        )) as Array<{ mp: string | null }>;
        const mp = rows[0]?.mp;
        if (!mp) throw new Error('iota has no officialMountPointId');
        const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
        await writeDatabaseDocument(mp, 'properties.json', '{');
        return respond(
          await (await loadRoute(idRoute)).PUT(
            mockRequest(`${B}/${IOTA}`, { name: 'Iota Should Not Rename' }),
            p(IOTA),
          ),
        );
      },
    },
    // P4.D246 (v4 `9753d0eb2`): the PUT's post-write enrichment FAILING. A
    // roster member's vault keystone is deleted through the REAL
    // deleteDatabaseDocument (Aria is on Iota's roster), so `enrichProject`'s
    // `characters.findById` throws `CharacterVaultUnavailableError` — the
    // overlay runs OUTSIDE the repository's safeQuery — AFTER
    // `repos.projects.update` committed. `handlePutDefault` has no local
    // try/catch, so the throw reaches the middleware's contextful 503; the
    // dump proves the rename LANDED anyway. (The GET's local catch would have
    // answered its fixed 500 here — this arm is what tells the two apart.)
    {
      name: 'update_enrich_store_corrupt',
      run: async () => {
        const { rawQuery } = await import('@/lib/database/manager');
        const rows = (await rawQuery(
          `SELECT characterDocumentMountPointId AS mp FROM characters WHERE id = '${ARIA}'`,
        )) as Array<{ mp: string | null }>;
        const mp = rows[0]?.mp;
        if (!mp) throw new Error('aria has no characterDocumentMountPointId');
        const { deleteDatabaseDocument } = await import('@/lib/mount-index/database-store');
        await deleteDatabaseDocument(mp, 'properties.json');
        const r = await (await loadRoute(idRoute)).PUT(
          mockRequest(`${B}/${IOTA}`, { name: 'Iota Renamed Behind A Broken Vault' }),
          p(IOTA),
        );
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpProjectTables() };
      },
    },
    // ---- P4.148 (P4.D246 item 16): v4's `z.uuid()` gate on the six
    // add/remove bodies, AFTER the project's existence check. A malformed id
    // answers `400 {error: 'Validation error', details: [invalid_format/uuid]}`
    // and the handler logs nothing (its INFO follows the write).
    ...([
      ['add_character_bad_uuid', 'POST', 'add-character', 'characterId'],
      ['remove_character_bad_uuid', 'DELETE', 'remove-character', 'characterId'],
      ['add_chat_bad_uuid', 'POST', 'add-chat', 'chatId'],
      ['remove_chat_bad_uuid', 'DELETE', 'remove-chat', 'chatId'],
      ['add_file_bad_uuid', 'POST', 'add-file', 'fileId'],
      ['remove_file_bad_uuid', 'DELETE', 'remove-file', 'fileId'],
    ] as const).map(([name, method, action, field]) => ({
      name,
      run: async () =>
        withLogs(async () =>
          respond(
            await (await loadRoute(idRoute))[method](
              mockRequest(`${B}/${KAPPA}?action=${action}`, { [field]: 'not-a-uuid' }),
              p(KAPPA),
            ),
          ),
        ),
    })),
    // P4.148 (order item 9): the Scenarios/ ensure FAILING on create. A
    // trigger planted on this case's copy refuses every `doc_mount_folders`
    // INSERT, so the create lands (a project store has no folders) and the
    // best-effort ensure throws — v4 logs INFO `Project created` THEN WARN
    // `Failed to ensure project Scenarios folder on create {projectId, error}`.
    {
      name: 'create_scenarios_ensure_fails',
      run: async () => {
        const { getRawMountIndexDatabase } = await import(
          '@/lib/database/backends/sqlite/mount-index-client'
        );
        const midb = getRawMountIndexDatabase();
        if (!midb) throw new Error('raw mount-index handle unavailable');
        midb.exec(
          "CREATE TRIGGER qt_p4148_no_folders BEFORE INSERT ON doc_mount_folders BEGIN SELECT RAISE(ABORT, 'planted: folder inserts refused'); END",
        );
        return withLogs(async () =>
          respond(
            await (await loadRoute('@/app/api/v1/projects/route')).POST(
              mockRequest(B, { name: 'Tau', color: '#abcdef', icon: 'rocket' }),
            ),
          ),
        );
      },
    },
    // P4.148 (R-F, order item 10): a roster member whose `tags` cell is NULL
    // reads `tags: []` on the enriched roster (v4 `char.tags || []`).
    {
      name: 'get_iota_null_tags',
      run: async () => {
        const { rawQuery } = await import('@/lib/database/manager');
        await rawQuery('UPDATE "characters" SET "tags" = NULL WHERE "id" = ?', [ARIA]);
        return respond(await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${IOTA}`), p(IOTA)));
      },
    },
    // P4.155 (R-C): v4's OTHER two `char.tags || []` sites — the roster list
    // (`roster.ts:38`) and the list-chats participants (`chats.ts:69`) — over
    // the same NULL `tags` cell. Both read `[]`. REGRESSION PINS, not
    // red-first: they were GREEN on unported core, because v5's character read
    // materializes a NULL `tags` cell as `[]` before any of the three sites.
    {
      name: 'list_characters_null_tags',
      run: async () => {
        const { rawQuery } = await import('@/lib/database/manager');
        await rawQuery('UPDATE "characters" SET "tags" = NULL WHERE "id" = ?', [ARIA]);
        return respond(
          await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${IOTA}?action=list-characters`), p(IOTA)),
        );
      },
    },
    {
      name: 'list_chats_null_tags',
      run: async () => {
        const { rawQuery } = await import('@/lib/database/manager');
        await rawQuery('UPDATE "characters" SET "tags" = NULL WHERE "id" = ?', [ARIA]);
        return respond(
          await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${IOTA}?action=list-chats`), p(IOTA)),
        );
      },
    },
    // P4.148 (order item 11): a roster naming a well-formed uuid with no
    // character behind it — `_count.characters` is the RAW roster length (2),
    // the enriched roster and list-characters carry the one real member.
    {
      name: 'roster_missing_character',
      run: async () => {
        const mod = await loadRoute(idRoute);
        const put = await respond(
          await mod.PUT(
            mockRequest(`${B}/${IOTA}`, { characterRoster: [ARIA, MISSING_CHARACTER] }),
            p(IOTA),
          ),
        );
        const get = await respond(await mod.GET(mockRequest(`${B}/${IOTA}`), p(IOTA)));
        const list = await respond(
          await mod.GET(mockRequest(`${B}/${IOTA}?action=list-characters`), p(IOTA)),
        );
        return { status: get.status, body: { put: put.body, get: get.body, list: list.body } };
      },
    },
  ];

  // P4.163 (R-E): the `[Projects v1]` census — every mutation v5 serves records
  // the lines v4 logs on its success path (the INFO lines P4.148 / P4.D246 left
  // absent), through the same `withLogs` spy. Wrapped here so each case body
  // stays as it was.
  const WITH_LOGS = new Set([
    'delete',
    'add_chat',
    'remove_chat',
    'tool_settings',
    'aesthetic_set',
    'aesthetic_clear',
    'mount_link',
    'mount_unlink',
    'add_file',
    'remove_file',
    'wardrobe_create',
    'wardrobe_update',
    // The `94fbb1ae3` boot-hardness unification's review: the archive arm's
    // line carries v4's conditional `archivedAt` (a minted stamp here).
    'wardrobe_update_archives',
    'wardrobe_delete',
    // P4.D256: the ledger-present delete — no ledger WARN, the success INFO.
    'wardrobe_delete_with_ledger',
  ]);
  for (const c of cases) {
    if (WITH_LOGS.has(c.name)) {
      const inner = c.run;
      c.run = () => withLogs(inner as never) as never;
    }
  }

  const outLines: string[] = [];
  for (const c of cases) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`projects-routes oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('projects-routes oracle', async () => {
  await main();
});

// Remove the OS-temp scratch dir(s) once the oracle has written its NDJSON.
const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
