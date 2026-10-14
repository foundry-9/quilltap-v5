/**
 * @jest-environment node
 *
 * P4.D288 (contract C2) ORACLE — the round's two FILLED memory verbs and the
 * retired `system/tools` dedup pair, through v4's REAL route handlers at the
 * `83334b9a5` pin, each case over a FRESH copy of the committed
 * memories-{main,mount}.db pair (it carries an `instance_settings` table):
 *
 *   - GET / POST `/api/v1/memories?action=maintenance-config`
 *     (`app/api/v1/memories/route.ts:864-899`): the defaults; a POST naming
 *     only `phases.demote` (the other two stored toggles kept); a cap out of
 *     bounds (Zod's `validationError` 400 — `{error, details}`); an unknown key
 *     stripped; a sent `perCharacterCapOverrides` replacing the stored map;
 *   - GET `?action=memory-dedup-preview` / POST `?action=memory-dedup` on
 *     `/api/v1/system/tools` — retired by `104e986c7`: v4's `Unknown action`
 *     envelope with the pin's action lists.
 *
 * Each record: `{name, status, body, storedAfter?, infoTags}` — `storedAfter`
 * is `getMemoryMaintenanceSettings()` after a config call; `infoTags` the
 * `[Memories API]` INFO messages the call logged (PRESENCE only — the line's
 * bytes are P4.D297's capture pin, §R.5).
 *
 * Run (Node 24, from the pinned v4 worktree — a /tmp mirror, jest ignores
 * `.claude/`):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; M=/tmp/qt-p4d288-wire-oracle
 *   rm -rf $M && mkdir -p $M && cp -R $V5W/harness/oracle/* $M/
 *   cd ~/source/quilltap-server
 *   rm -f /tmp/qt-p4d288-wire.ndjson
 *   QT_FIXTURE_MEM_MAIN=$V5W/crates/quilltap-web/tests/fixtures/memories-main.db \
 *   QT_FIXTURE_MEM_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/memories-mount.db \
 *   QT_ORACLE_OUT=/tmp/qt-p4d288-wire.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$M/cases" -- p4-d288-wire-contract
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
  route: 'memories' | 'tools';
  method: 'GET' | 'POST';
  action: string;
  /** Stored `memoryMaintenance` value (raw text) before the request. */
  stored?: string;
  body?: unknown;
}

const CASES: CaseSpec[] = [
  { name: 'config_get_defaults', route: 'memories', method: 'GET', action: 'maintenance-config' },
  {
    name: 'config_get_stored_no_phases',
    route: 'memories',
    method: 'GET',
    action: 'maintenance-config',
    stored: JSON.stringify({ enabled: true, perCharacterCap: 900, perCharacterCapOverrides: { c1: 150 } }),
  },
  {
    name: 'config_post_one_toggle',
    route: 'memories',
    method: 'POST',
    action: 'maintenance-config',
    stored: JSON.stringify({ phases: { foldNearDuplicates: false, demote: true, retire: false } }),
    body: { phases: { demote: false } },
  },
  {
    name: 'config_post_cap_out_of_bounds',
    route: 'memories',
    method: 'POST',
    action: 'maintenance-config',
    body: { perCharacterCap: 50 },
  },
  {
    name: 'config_post_unknown_key_stripped',
    route: 'memories',
    method: 'POST',
    action: 'maintenance-config',
    body: { enabled: true, autoMergeSimilarThreshold: 0.9, mergeSimilar: true },
  },
  {
    name: 'config_post_overrides_replace',
    route: 'memories',
    method: 'POST',
    action: 'maintenance-config',
    stored: JSON.stringify({ perCharacterCapOverrides: { a: 150, b: 300 } }),
    body: { perCharacterCapOverrides: { c: 900 }, perCharacterCap: 400 },
  },
  {
    name: 'config_post_bad_phase_type',
    route: 'memories',
    method: 'POST',
    action: 'maintenance-config',
    body: { phases: { retire: 'no' } },
  },
  { name: 'tools_get_memory_dedup_preview', route: 'tools', method: 'GET', action: 'memory-dedup-preview' },
  { name: 'tools_post_memory_dedup', route: 'tools', method: 'POST', action: 'memory-dedup', body: { threshold: 0.8 } },
];

function mockRequest(method: string, url: string, body?: unknown): unknown {
  return {
    method,
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
  for (const real of [
    '@/lib/database/manager',
    '@/lib/database/repositories',
    '@/lib/repositories/factory',
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

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
  const { Logger } = await import('@/lib/logger');
  const infoTags: string[] = [];
  const proto = Logger.prototype as unknown as Record<string, (...a: unknown[]) => void>;
  const original = proto.info;
  proto.info = function (this: unknown, message: string, ...rest: unknown[]) {
    if (typeof message === 'string' && message.startsWith('[Memories API]')) infoTags.push(message);
    return original.call(this, message, ...rest);
  };
  await initializeDatabase();
  try {
    await rawQuery('DELETE FROM "instance_settings" WHERE "key" = ?', ['memoryMaintenance']);
    if (c.stored !== undefined) {
      await rawQuery('INSERT INTO "instance_settings" ("key", "value") VALUES (?, ?)', ['memoryMaintenance', c.stored]);
    }
    const route = (await import(
      c.route === 'memories' ? '@/app/api/v1/memories/route' : '@/app/api/v1/system/tools/route'
    )) as Record<string, (...a: unknown[]) => Promise<unknown>>;
    const path = c.route === 'memories' ? 'memories' : 'system/tools';
    const url = `http://localhost/api/v1/${path}?action=${c.action}`;
    const resp = (await route[c.method](mockRequest(c.method, url, c.body))) as {
      status: number;
      json: () => Promise<unknown>;
    };
    const rec: Record<string, unknown> = { name: c.name, status: resp.status, body: await resp.json() };
    if (c.route === 'memories') {
      const settings = await import('@/lib/instance-settings');
      rec.storedAfter = await settings.getMemoryMaintenanceSettings();
    }
    rec.infoTags = infoTags;
    return rec;
  } finally {
    proto.info = original;
    await new Promise((r) => setTimeout(r, 300));
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'memories-web.json'), 'utf8')) as Spec;
  const fixtures = { main: process.env.QT_FIXTURE_MEM_MAIN ?? '', mount: process.env.QT_FIXTURE_MEM_MOUNT ?? '' };
  for (const [k, v] of Object.entries(fixtures)) if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  const scratch = mkdtempSync(join(tmpdir(), 'qt-p4d288-wire-oracle-'));
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';
  const lines: string[] = [];
  try {
    for (const c of CASES) lines.push(JSON.stringify({ ...(await runCase(spec, c, scratch, fixtures)), request: c }));
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`wrote ${CASES.length} p4-d288 wire records to ${outPath}\n`);
}

test('p4-d288 wire contract oracle', async () => {
  await main();
}, 180000);
