/**
 * @jest-environment node
 *
 * P4.D297 tier-2 ORACLE — the maintenance run's LLM-free phases (fold
 * near-duplicates, demote, retire; the mirror with nothing touched) through
 * v4's REAL `runMaintenance` (`lib/memory/maintenance/run.ts` at the
 * `83334b9a5` pin), in the PARENT (in-process: every write lands as it is
 * made, the run announces itself).
 *
 * Over a COPY of the maintenance fixture pair (`build-maintenance-fixture.ts`)
 * the runs (`memory-maintenance-tier2-runs.json`) run IN ORDER, each with
 * `ctx.now` = the spec's clock (v4's own seam — no calendar time bomb). A run
 * may first plant instance settings through v4's real setters. Only the
 * embedding call is pinned: `generateEmbeddingForUser` (the fold's survivor
 * re-embed) answers a vector derived from the text and RECORDS each text, so
 * the Rust side replays the same vectors and a text-byte divergence is a
 * canned miss.
 *
 * Output rows: `{kind:'run', name, report, lines}` per run (`lines` = every
 * `module: 'memory:maintenance*'` line, plus the façade's `[MemorySettings]
 * Resolved memory settings` — P4.D288's line, asserted by TAG only);
 * `{kind:'embedding', text, vector}`; `{kind:'table', rows}` — `memories`
 * after every run.
 *
 * Run (Node 24, from the v4 checkout or the round's pin — a /tmp mirror):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; M=${M:-/tmp/qt-maintenance-tier2-oracle}
 *   rm -rf $M && mkdir -p $M && cp -R $V5W/harness/oracle/* $M/
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=${QT_MAIN:-/tmp/qt-maintenance-main.db} \
 *   QT_FIXTURE_MOUNT_OUT=${QT_MOUNT:-/tmp/qt-maintenance-mount.db} \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-maintenance-fixture.ts
 *   rm -f ${QT_OUT:-/tmp/qt-maintenance-tier2.ndjson}
 *   QT_FIXTURE_MAINTENANCE_MAIN=${QT_MAIN:-/tmp/qt-maintenance-main.db} \
 *   QT_FIXTURE_MAINTENANCE_MOUNT=${QT_MOUNT:-/tmp/qt-maintenance-mount.db} \
 *   QT_ORACLE_OUT=${QT_OUT:-/tmp/qt-maintenance-tier2.ndjson} \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$M/cases" -- memory-maintenance-tier2
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

/** A deterministic 8-dim unit vector for an embedded text (FNV-1a seeded). */
export function cannedVector(text: string): number[] {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  let a = h;
  const next = () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296 - 0.5;
  };
  const raw = Array.from({ length: 8 }, next);
  const n = Math.sqrt(raw.reduce((s, v) => s + v * v, 0));
  return raw.map((v) => Math.fround(v / n));
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'memory-maintenance-tier2.json'), 'utf8'));
  const { runs } = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'memory-maintenance-tier2-runs.json'), 'utf8')) as {
    runs: Array<{ name: string; holder: string; plant?: Record<string, Record<string, unknown>>; ctx?: { timeBudgetMs?: number }; request: Record<string, unknown> }>;
  };
  const fixtureMain = process.env.QT_FIXTURE_MAINTENANCE_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_MAINTENANCE_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_MAINTENANCE_MAIN / _MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-maintenance-tier2-oracle-'));
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

    const embeddings = new Map<string, number[]>();
    jest.resetModules();
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
      '@/lib/file-storage/manager',
    ]) {
      jest.doMock(real, () => jest.requireActual(real));
    }
    jest.doMock('@/lib/embedding/embedding-service', () => {
      const actual = jest.requireActual('@/lib/embedding/embedding-service');
      return {
        __esModule: true,
        ...actual,
        generateEmbeddingForUser: async (text: string) => {
          const vec = cannedVector(text);
          embeddings.set(text, vec);
          return { embedding: new Float32Array(vec), model: 'canned', dimensions: vec.length, provider: 'canned' };
        },
      };
    });
    // The parent's job host must not claim anything mid-run.
    (globalThis as Record<string, unknown>).__quilltapJobHost = { shuttingDown: true };

    const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
    const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
    const { runMaintenance } = await import('@/lib/memory/maintenance/run');
    const settings = await import('@/lib/instance-settings');
    const { Logger } = await import('@/lib/logger');

    let sink: Array<{ level: string; message: string; context: unknown }> | null = null;
    for (const level of ['error', 'warn', 'info', 'debug'] as const) {
      const proto = Logger.prototype as unknown as Record<string, (...a: unknown[]) => void>;
      const original = proto[level];
      proto[level] = function (this: { context?: Record<string, unknown> }, message: string, context?: Record<string, unknown>, ...rest: unknown[]) {
        const { service: _s, environment: _e, ...own } = this.context ?? {};
        const merged = { ...own, ...(context ?? {}) };
        const mod = typeof merged.module === 'string' ? merged.module : '';
        if (sink && typeof message === 'string' && (mod.startsWith('memory:maintenance') || message === '[MemorySettings] Resolved memory settings')) {
          sink.push({ level, message, context: JSON.parse(JSON.stringify(merged)) });
        }
        return original.call(this, message, context, ...rest);
      };
    }

    await initializeDatabase();
    const holderId = (key: string): string => (key === 'unknown' ? spec.unknownId : spec.holders[key].id);
    const lines: string[] = [];
    for (const run of runs) {
      if (run.plant?.maintenance) await settings.setMemoryMaintenanceSettings(run.plant.maintenance as never);
      if (run.plant?.consolidation) await settings.setMemoryConsolidationSettings(run.plant.consolidation as never);
      sink = [];
      const report = await runMaintenance(
        { characterId: holderId(run.holder), ...run.request } as never,
        { now: new Date(spec.now), ...(run.ctx ?? {}) },
      );
      // `void` effects (the retirement announce, the realtime bus) settle before the next run.
      for (let i = 0; i < 10; i++) await new Promise((r) => setImmediate(r));
      lines.push(JSON.stringify({ kind: 'run', name: run.name, report, lines: sink }));
      sink = null;
    }
    for (const [text, vector] of embeddings) lines.push(JSON.stringify({ kind: 'embedding', text, vector }));
    const hex = (v: unknown) => (v instanceof Uint8Array ? Buffer.from(v).toString('hex') : v);
    const rows = ((await rawQuery('SELECT * FROM memories ORDER BY characterId, id')) as Array<Record<string, unknown>>).map((r) => {
      const out: Record<string, unknown> = {};
      for (const [k, v] of Object.entries(r)) out[k] = hex(v);
      return out;
    });
    lines.push(JSON.stringify({ kind: 'table', table: 'memories', rows }));
    closeMountIndexSQLiteClient();
    await closeDatabase();
    fs.writeFileSync(outPath, lines.join('\n') + '\n');
    process.stderr.write(`memory maintenance tier-2 oracle wrote ${outPath}\n`);
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}

test('memory maintenance tier-2 oracle', async () => {
  await main();
}, 180000);
