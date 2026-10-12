/**
 * @jest-environment node
 *
 * P4.D271 ORACLE for the Memories API's consolidation actions (v4
 * `app/api/v1/memories/route.ts`, `d58548051`, byte-identical at the
 * `01a83539d` pin): GET/POST `?action=consolidation-config`, GET/POST
 * `?action=extraction-mode-config`, POST `?action=consolidate` — v4's REAL
 * route handlers over a FRESH copy of the committed memories-{main,mount}.db
 * pair per case (widened in place to the tier columns by P4.D265, §R.3).
 *
 * Includes the `PARTIAL_DEFAULTS_CLOBBER` rows (R-A): a tuned stored value
 * plus a POST of one other key — v4's `.partial()` parse fills every default,
 * so the stored tuning is LOST (`storedAfter` records it). The dry runs run on
 * Orla (four AUTO rows, none embedded → no candidates, zero model calls).
 *
 * Each record: `{name, status, body, lines, storedAfter?, jobs?}` — `lines`
 * are the captured `[Memories API]` lines (context in v4's key order).
 *
 * Run (Node 24, from the v4 checkout or the round's pin — a /tmp mirror):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; M=/tmp/qt-consolidation-oracle
 *   rm -rf $M && mkdir -p $M && cp -R $V5W/harness/oracle/* $M/
 *   cd ~/source/quilltap-server
 *   rm -f /tmp/qt-consolidation-routes.ndjson
 *   QT_FIXTURE_MEM_MAIN=$V5W/crates/quilltap-web/tests/fixtures/memories-main.db \
 *   QT_FIXTURE_MEM_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/memories-mount.db \
 *   QT_ORACLE_OUT=/tmp/qt-consolidation-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$M/cases" -- memory-consolidation-routes
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
  /** `consolidation` | `extraction` stored before the request, through v4's real setter. */
  stored?: { consolidation?: Record<string, unknown>; extraction?: Record<string, unknown> };
  method: 'GET' | 'POST';
  action: string;
  body?: unknown;
  /** Repeat the request (the dedupe row). */
  twice?: boolean;
}

function mockRequest(method: string, url: string, body?: unknown): unknown {
  return {
    method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

const CAPTURED = new Set([
  '[Memories API] Consolidation settings updated (instance-wide)',
  '[Memories API] Extraction-mode settings updated (instance-wide)',
  '[Memories API] Consolidation dry run (in-process)',
  '[Memories API] Consolidation dry run failed',
  '[Memories API] Consolidation job enqueued',
  '[Memories API] Failed to enqueue consolidation',
]);

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
  for (const level of ['error', 'warn', 'info', 'debug'] as const) {
    const proto = Logger.prototype as unknown as Record<string, (...a: unknown[]) => void>;
    const original = proto[level];
    proto[level] = function (this: unknown, message: string, context?: Record<string, unknown>, ...rest: unknown[]) {
      if (typeof message === 'string' && CAPTURED.has(message)) {
        sink.push({ level, message, context: JSON.parse(JSON.stringify(context ?? {})) });
      }
      return original.call(this, message, context, ...rest);
    };
  }
  await initializeDatabase();
  try {
    const settings = await import('@/lib/instance-settings');
    if (c.stored?.consolidation) await settings.setMemoryConsolidationSettings(c.stored.consolidation as never);
    if (c.stored?.extraction) await settings.setMemoryExtractionModeSettings(c.stored.extraction as never);
    const route = (await import('@/app/api/v1/memories/route')) as Record<string, (...a: unknown[]) => Promise<unknown>>;
    const url = `http://localhost/api/v1/memories?action=${c.action}`;
    const call = async () => {
      const resp = (await route[c.method](mockRequest(c.method, url, c.body))) as { status: number; json: () => Promise<unknown> };
      return { status: resp.status, body: await resp.json() };
    };
    let out = await call();
    const first = out;
    if (c.twice) out = await call();
    const rec: Record<string, unknown> = { name: c.name, status: out.status, body: out.body };
    if (c.twice) rec.firstBody = first.body;
    rec.lines = sink;
    if (c.action === 'consolidation-config' || c.action === 'extraction-mode-config') {
      rec.storedAfter =
        c.action === 'consolidation-config'
          ? await settings.getMemoryConsolidationSettings()
          : await settings.getMemoryExtractionModeSettings();
    }
    if (c.action === 'consolidate') {
      const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
      const db = getRawDatabase() as unknown as { prepare: (s: string) => { all: () => unknown } };
      rec.jobs = db
        .prepare("SELECT userId, type, status, payload, maxAttempts FROM background_jobs WHERE type = 'MEMORY_CONSOLIDATION' ORDER BY createdAt")
        .all();
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
    fs.readFileSync(join(here, '..', 'fixtures', 'memory-consolidation-routes.json'), 'utf8'),
  ) as { cases: CaseSpec[] };
  const fixtures = { main: process.env.QT_FIXTURE_MEM_MAIN ?? '', mount: process.env.QT_FIXTURE_MEM_MOUNT ?? '' };
  for (const [k, v] of Object.entries(fixtures)) if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  const scratch = mkdtempSync(join(tmpdir(), 'qt-consolidation-routes-oracle-'));
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
  process.stderr.write(`wrote ${cases.length} consolidation-routes records to ${outPath}\n`);
}

test('memory consolidation routes oracle', async () => {
  await main();
}, 180000);
