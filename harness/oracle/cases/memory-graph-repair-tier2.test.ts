/**
 * @jest-environment node
 *
 * P4.D297 tier-2 ORACLE — the memory graph repair (v4 `lib/memory/repair.ts`
 * at the `83334b9a5` pin) through v4's REAL route handler (POST
 * `/api/v1/memories?action=repair`), over a COPY of the maintenance fixture
 * pair with the dangling-link plants of `memory-graph-repair-steps.json`:
 * a missing `relatedMemoryIds` target (one row naming it twice; a live
 * cross-holder link kept), a missing `consolidatedFrom` member (a partial
 * digest), a digest whose every member is gone (→ the `graph-repair`
 * retirement), a missing `supersededById` digest on a cold AND a hot row, and
 * a `relatedMemoryIds` that is not JSON (skipped by the scan). The steps run IN
 * ORDER on ONE copy: whole-instance and scoped dry runs, a scoped live run, a
 * whole-instance live run, the clean re-scan, an absent body, the 404, and a
 * failed scan (the memories table renamed away — no fallback: a 500).
 *
 * The fixture holds no embedding profile, so a live run's backfill answers
 * `embeddingSkippedReason: 'no-embedding-profile'` (the routes family covers
 * the queued-jobs arm over the memories-web pair).
 *
 * Output: `{kind:'step', name, status, body, lines}` (`lines` = every captured
 * line: `memory:*` modules, `[Memories API]`, and the repository's scan
 * line); `{kind:'table', rows}` — `memories` before the failed scan;
 * `{kind:'jobs', rows}`.
 *
 * Run (Node 24, from the v4 checkout or the round's pin — a /tmp mirror):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; M=${M:-/tmp/qt-graph-repair-oracle}
 *   rm -rf $M && mkdir -p $M && cp -R $V5W/harness/oracle/* $M/
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=${QT_MAIN:-/tmp/qt-maintenance-main.db} \
 *   QT_FIXTURE_MOUNT_OUT=${QT_MOUNT:-/tmp/qt-maintenance-mount.db} \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-maintenance-fixture.ts
 *   rm -f ${QT_OUT:-/tmp/qt-graph-repair.ndjson}
 *   QT_FIXTURE_MAINTENANCE_MAIN=${QT_MAIN:-/tmp/qt-maintenance-main.db} \
 *   QT_FIXTURE_MAINTENANCE_MOUNT=${QT_MOUNT:-/tmp/qt-maintenance-mount.db} \
 *   QT_ORACLE_OUT=${QT_OUT:-/tmp/qt-graph-repair.ndjson} \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$M/cases" -- memory-graph-repair-tier2
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

function mockRequest(method: string, url: string, body?: unknown): unknown {
  return {
    method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}


function applyMocks(spec: { userId: string }): void {
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

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'memory-maintenance-tier2.json'), 'utf8'));
  const corpus = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'memory-graph-repair-steps.json'), 'utf8'));
  const fixtureMain = process.env.QT_FIXTURE_MAINTENANCE_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_MAINTENANCE_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_MAINTENANCE_MAIN / _MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  const holderId = (key: string): string => (key === 'unknown' ? spec.unknownId : spec.holders[key].id);
  const resolve = (v: unknown): unknown => {
    if (typeof v === 'string') return v.startsWith('@') ? holderId(v.slice(1)) : v;
    if (Array.isArray(v)) return v.map(resolve);
    if (v && typeof v === 'object') return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, resolve(x)]));
    return v;
  };

  const scratch = mkdtempSync(join(tmpdir(), 'qt-graph-repair-oracle-'));
  try {
    mkdirSync(join(scratch, 'data'), { recursive: true });
    copyFileSync(fixtureMain, join(scratch, 'main.db'));
    copyFileSync(fixtureMount, join(scratch, 'mount.db'));
    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.SQLITE_PATH = join(scratch, 'main.db');
    process.env.SQLITE_MOUNT_INDEX_PATH = join(scratch, 'mount.db');
    process.env.SQLITE_LLM_LOGS_PATH = join(scratch, 'llm-logs.db');
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.QUILLTAP_JOB_CHILD;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';
    jest.resetModules();
    applyMocks(spec);
    (globalThis as Record<string, unknown>).__quilltapJobHost = { shuttingDown: true };

    const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
    const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
    const { Logger } = await import('@/lib/logger');
    let sink: Array<{ level: string; message: string; context: unknown }> | null = null;
    for (const level of ['error', 'warn', 'info', 'debug'] as const) {
      const proto = Logger.prototype as unknown as Record<string, (...a: unknown[]) => void>;
      const original = proto[level];
      proto[level] = function (this: { context?: Record<string, unknown> }, message: string, context?: Record<string, unknown>, ...rest: unknown[]) {
        const { service: _s, environment: _e, ...own } = this.context ?? {};
        // v4's third argument (the Error) lands in the entry's own `error`
        // slot; v5 renders it as a trailing `error=<message>` field.
        const err = rest[0] instanceof Error ? { error: (rest[0] as Error).message } : {};
        const merged = { ...own, ...(context ?? {}), ...err };
        const mod = typeof merged.module === 'string' ? merged.module : '';
        if (sink && typeof message === 'string' && (mod.startsWith('memory:') || message.startsWith('[Memories API]') || message === 'Error scanning for dangling memory links' || message === 'Scanned for dangling memory links')) {
          sink.push({ level, message, context: JSON.parse(JSON.stringify(merged)) });
        }
        return original.call(this, message, context, ...rest);
      };
    }
    await initializeDatabase();
    for (const sql of corpus.plants) await rawQuery(sql);
    const route = (await import('@/app/api/v1/memories/route')) as Record<string, (...a: unknown[]) => Promise<unknown>>;
    const hex = (v: unknown) => (v instanceof Uint8Array ? Buffer.from(v).toString('hex') : v);
    const lines: string[] = [];
    for (const raw of corpus.steps) {
      const step = resolve(raw) as { name: string; body?: unknown; sql?: string };
      if (step.sql) {
        const rows = ((await rawQuery('SELECT * FROM memories ORDER BY characterId, id')) as Array<Record<string, unknown>>).map((r) =>
          Object.fromEntries(Object.entries(r).map(([k, v]) => [k, hex(v)])),
        );
        lines.push(JSON.stringify({ kind: 'table', table: 'memories', rows }));
        const jobs = (await rawQuery('SELECT userId, type, status, payload, maxAttempts FROM background_jobs ORDER BY type, payload')) as unknown[];
        lines.push(JSON.stringify({ kind: 'jobs', rows: jobs }));
        await rawQuery(step.sql);
      }
      sink = [];
      const resp = (await route.POST(mockRequest('POST', 'http://localhost/api/v1/memories?action=repair', step.body))) as { status: number; json: () => Promise<unknown> };
      const body = await resp.json();
      for (let i = 0; i < 10; i++) await new Promise((r) => setImmediate(r));
      lines.push(JSON.stringify({ kind: 'step', name: step.name, status: resp.status, body, lines: sink }));
      sink = null;
    }
    closeMountIndexSQLiteClient();
    await closeDatabase();
    fs.writeFileSync(outPath, lines.join('\n') + '\n');
    process.stderr.write(`memory graph repair oracle wrote ${outPath}\n`);
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}

test('memory graph repair tier-2 oracle', async () => {
  await main();
}, 180000);
