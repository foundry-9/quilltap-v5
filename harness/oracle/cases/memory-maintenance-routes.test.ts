/**
 * @jest-environment node
 *
 * P4.D297 ORACLE for the Memories API's maintenance verbs (v4
 * `app/api/v1/memories/route.ts` at the `83334b9a5` pin): POST
 * `?action=maintain`, GET `?action=maintenance-status`, POST `?action=repair`,
 * GET/POST `?action=maintenance-config`, and the RETIRED actions
 * (`housekeep` GET/POST, `housekeep-sweep`, `housekeeping-config` GET/POST,
 * `consolidate` — v4's `Unknown action` 400) — v4's REAL route handlers over a
 * FRESH copy of the committed memories-{main,mount}.db pair per case, the
 * maintenance ledger reset per case.
 *
 * The one seam: a dry run's `runMaintenance` reads the fixed clock
 * `2026-10-08T12:00:00.000Z` (an omitted `ctx.now` filled — protection decays
 * with the wall clock, so two sides a millisecond apart would score apart).
 * A case may first plant SQL (`plantSql`) or a RAW stored
 * `instance_settings['memoryMaintenance']` value (`stored` — L-7's two
 * shapes: no `phases`, `phases: null`).
 *
 * Each record: `{name, status, body, firstBody?, lines, jobs?, storedAfter?}`
 * — `lines` are the captured `[Memories API]`, `[Maintenance]`,
 * `memory:maintenance*` and `memory:repair` lines plus the two façade tags
 * (`[MemorySettings] Resolved memory settings`, `[InstanceSettings]
 * memoryMaintenance failed to parse — using defaults`), context in v4's key
 * order; `jobs` = every `background_jobs` row after the call.
 *
 * Run (Node 24, from the v4 checkout or the round's pin — a /tmp mirror):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; M=${M:-/tmp/qt-maintenance-routes-oracle}
 *   rm -rf $M && mkdir -p $M && cp -R $V5W/harness/oracle/* $M/
 *   cd ~/source/quilltap-server
 *   rm -f ${QT_OUT:-/tmp/qt-maintenance-routes.ndjson}
 *   QT_FIXTURE_MEM_MAIN=$V5W/crates/quilltap-web/tests/fixtures/memories-main.db \
 *   QT_FIXTURE_MEM_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/memories-mount.db \
 *   QT_ORACLE_OUT=${QT_OUT:-/tmp/qt-maintenance-routes.ndjson} \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$M/cases" -- memory-maintenance-routes
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
  method: 'GET' | 'POST';
  action: string;
  body?: unknown;
  query?: Record<string, string>;
  /** A RAW `instance_settings['memoryMaintenance']` value written before the request. */
  stored?: string;
  plantSql?: string[];
  /** Repeat the request (the widen row). */
  twice?: boolean;
  /** Dump `background_jobs` after the call. */
  jobs?: boolean;
}

const RUN_NOW = new Date('2026-10-08T12:00:00.000Z');
const TAGS = new Set([
  '[MemorySettings] Resolved memory settings',
  '[InstanceSettings] memoryMaintenance failed to parse — using defaults',
]);

function mockRequest(method: string, url: string, body?: unknown): unknown {
  return {
    method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

function captured(message: string, mod: string): boolean {
  return (
    message.startsWith('[Memories API]') ||
    message.startsWith('[Maintenance]') ||
    mod.startsWith('memory:maintenance') ||
    mod === 'memory:repair' ||
    TAGS.has(message)
  );
}

function applyMocks(spec: Spec): void {
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  for (const real of [
    '@/lib/database/manager',
    '@/lib/database/repositories',
    '@/lib/repositories/factory',
    '@/lib/embedding/vector-store',
    '@/lib/file-storage/character-vault-bridge',
    // jest.setup.ts mocks the embedding service whole (its default-profile
    // read answers nothing): repair's embedding backfill needs the real one.
    '@/lib/embedding/embedding-service',
  ]) {
    jest.doMock(real, () => jest.requireActual(real));
  }
  jest.doMock('@/lib/background-jobs/host/processor-host', () => ({
    __esModule: true,
    ensureProcessorRunning: () => undefined,
    stopProcessor: () => undefined,
    isProcessorRunning: () => false,
    getProcessorStatus: () => ({ running: false, processing: false, inFlight: 0, childCrashed: false }),
    wakeProcessor: () => undefined,
    notifyChild: () => undefined,
    sendToChild: () => false,
  }));
  jest.doMock('@/lib/memory/maintenance/run', () => {
    const actual = jest.requireActual('@/lib/memory/maintenance/run');
    return {
      __esModule: true,
      ...actual,
      runMaintenance: (request: unknown, ctx: { now?: Date } = {}) => actual.runMaintenance(request, { ...ctx, now: ctx.now ?? RUN_NOW }),
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

async function runCase(spec: Spec, c: CaseSpec, scratch: string, fixtures: { main: string; mount: string }) {
  jest.resetModules();
  applyMocks(spec);
  const work = mkdtempSync(join(scratch, 'case-'));
  copyFileSync(fixtures.main, join(work, 'main.db'));
  copyFileSync(fixtures.mount, join(work, 'mount.db'));
  process.env.SQLITE_PATH = join(work, 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = join(work, 'mount.db');
  mkdirSync(join(work, 'data'), { recursive: true });
  process.env.QUILLTAP_DATA_DIR = work;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
  const { Logger } = await import('@/lib/logger');
  const sink: Array<{ level: string; message: string; context: unknown }> = [];
  (globalThis as Record<string, unknown>).__quilltapJobHost = { shuttingDown: true };
  for (const level of ['error', 'warn', 'info', 'debug'] as const) {
    const proto = Logger.prototype as unknown as Record<string, (...a: unknown[]) => void>;
    const original = proto[level];
    proto[level] = function (this: { context?: Record<string, unknown> }, message: string, context?: Record<string, unknown>, ...rest: unknown[]) {
      const { service: _s, environment: _e, ...own } = this.context ?? {};
      const merged = { ...own, ...(context ?? {}) };
      const mod = typeof merged.module === 'string' ? merged.module : '';
      if (typeof message === 'string' && captured(message, mod)) {
        sink.push({ level, message, context: JSON.parse(JSON.stringify(merged)) });
      }
      return original.call(this, message, context, ...rest);
    };
  }
  await initializeDatabase();
  try {
    const { rawQuery } = await import('@/lib/database/manager');
    const ledger = await import('@/lib/memory/maintenance/ledger');
    ledger.resetMaintenanceLedgerForTests();
    await rawQuery('CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)');
    if (c.stored !== undefined) {
      await rawQuery('INSERT INTO "instance_settings" ("key", "value") VALUES (?, ?) ON CONFLICT("key") DO UPDATE SET "value" = excluded."value"', ['memoryMaintenance', c.stored]);
    }
    for (const sql of c.plantSql ?? []) await rawQuery(sql);
    const route = (await import('@/app/api/v1/memories/route')) as Record<string, (...a: unknown[]) => Promise<unknown>>;
    const qs = new URLSearchParams({ action: c.action, ...(c.query ?? {}) });
    const url = `http://localhost/api/v1/memories?${qs.toString()}`;
    const call = async () => {
      const resp = (await route[c.method](mockRequest(c.method, url, c.body))) as { status: number; json: () => Promise<unknown> };
      return { status: resp.status, body: await resp.json() };
    };
    let out = await call();
    const first = out;
    if (c.twice) out = await call();
    for (let i = 0; i < 10; i++) await new Promise((r) => setImmediate(r));
    const rec: Record<string, unknown> = { name: c.name, status: out.status, body: out.body };
    if (c.twice) rec.firstBody = first.body;
    rec.lines = sink;
    if (c.jobs) {
      const rows = (await rawQuery(
        'SELECT id, userId, type, status, payload, maxAttempts, scheduledAt, createdAt FROM background_jobs ORDER BY type, payload, status',
      )) as Array<Record<string, unknown>>;
      rec.jobs = rows.map((r) => ({
        id: r.id,
        userId: r.userId,
        type: r.type,
        status: r.status,
        payload: r.payload,
        maxAttempts: r.maxAttempts,
        // Whole seconds, not ms: v4's enqueue stamps `scheduledAt` with its own
        // `new Date()` BEFORE the repository stamps `createdAt`, so an immediate
        // job reads -1 ms whenever a millisecond tick lands between the two —
        // a timing coin the Rust side (one clock read) never flips (the
        // `322a5488e` follow-ups unification). `+ 0` folds a -0 into 0.
        scheduledDelayS: Math.round((Date.parse(r.scheduledAt as string) - Date.parse(r.createdAt as string)) / 1000) + 0,
      }));
    }
    if (c.action === 'maintenance-config') {
      const stored = (await rawQuery('SELECT "value" FROM "instance_settings" WHERE "key" = ?', ['memoryMaintenance'])) as Array<{ value: string }>;
      rec.storedAfter = stored[0]?.value ?? null;
    }
    return rec;
  } finally {
    await new Promise((r) => setTimeout(r, 300));
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'memories-web.json'), 'utf8')) as Spec;
  const { cases } = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'memory-maintenance-routes.json'), 'utf8'),
  ) as { cases: CaseSpec[] };
  const fixtures = { main: process.env.QT_FIXTURE_MEM_MAIN ?? '', mount: process.env.QT_FIXTURE_MEM_MOUNT ?? '' };
  for (const [k, v] of Object.entries(fixtures)) if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  const scratch = mkdtempSync(join(tmpdir(), 'qt-maintenance-routes-oracle-'));
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';
  const lines: string[] = [];
  try {
    for (const c of cases) lines.push(JSON.stringify(await runCase(spec, c, scratch, fixtures)));
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`wrote ${cases.length} maintenance-routes records to ${outPath}\n`);
}

test('memory maintenance routes oracle', async () => {
  await main();
}, 180000);
