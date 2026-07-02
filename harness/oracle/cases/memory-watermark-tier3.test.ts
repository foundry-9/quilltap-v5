/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the memory gate's watermark auto-housekeeping check
 * (v4 `maybeEnqueueHousekeeping` in memory-service.ts +
 * `enqueueMemoryHousekeeping` in queue-service.ts + the in-memory
 * housekeeping-outcome cache).
 *
 * `maybeEnqueueHousekeeping` is module-private, so it is exercised the way it
 * fires in production: through v4's REAL `createMemoryWithGate` (canned
 * embedding, INSERT band, empty vector stores). Beyond the memory-gate
 * oracle's mocks (real DB stack + canned `generateEmbeddingForUser`), exactly
 * one more seam is pinned: `ensureProcessorRunning` → no-op, so the enqueued
 * PENDING job is state, not a race against the in-process job runner (the Rust
 * port defers the runner the same way). The `cached_ineffective_sweep`
 * scenario calls the REAL `recordHousekeepingOutcome` before its gate write —
 * the Rust side records the same outcome through the ported cache.
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-memory-watermark.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-memory-watermark-fixture.ts
 *   QT_FIXTURE_WATERMARK=/tmp/qt-memory-watermark.db \
 *   QT_ORACLE_OUT=/tmp/oracle-memory-watermark.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- memory-watermark-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}
function canonicalizeRows(opts: {
  table: string;
  columns: string[];
  rawRows: Array<Record<string, unknown>>;
  orderBy?: string;
}): { table: string; columns: string[]; rows: Array<Record<string, unknown>> } {
  const { table, columns, rawRows, orderBy = 'id' } = opts;
  const rows = rawRows
    .map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of columns) out[col] = canonValue(r[col]);
      return out;
    })
    .sort((a, b) => {
      const av = String(a[orderBy] ?? '');
      const bv = String(b[orderBy] ?? '');
      return av < bv ? -1 : av > bv ? 1 : 0;
    });
  return { table, columns, rows };
}

interface Scenario {
  name: string;
  userId: string;
  characterId: string;
  recordOutcome?: { deleted: number; totalBefore: number; cap: number };
  candidate: { content: string; summary: string };
  expectJob: boolean;
}
interface Spec {
  testPepperBase64: string;
  cannedEmbeddings: Record<string, number[]>;
  scenarios: Scenario[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'memory-watermark-tier3.json'), 'utf8')
  ) as Spec;

  const fixture = process.env.QT_FIXTURE_WATERMARK;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_WATERMARK must point at the seed fixture');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-memory-watermark-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'watermark-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories')
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store')
  );
  jest.doMock('@/lib/embedding/embedding-service', () => {
    const actual = jest.requireActual('@/lib/embedding/embedding-service');
    const { EmbeddingError } = actual;
    return {
      __esModule: true,
      ...actual,
      generateEmbeddingForUser: async (text: string) => {
        const vec = spec.cannedEmbeddings[text];
        if (!vec) {
          throw new EmbeddingError(
            `no canned embedding registered for input (${text.length} chars)`
          );
        }
        return {
          embedding: new Float32Array(vec),
          model: 'canned',
          dimensions: vec.length,
          provider: 'canned',
        };
      },
    };
  });
  // Keep the in-process job runner OFF: the enqueued PENDING row is the state
  // under test, and the Rust port defers the runner the same way.
  jest.doMock('@/lib/background-jobs/processor', () => {
    const actual = jest.requireActual('@/lib/background-jobs/processor');
    return {
      __esModule: true,
      ...actual,
      ensureProcessorRunning: () => undefined,
    };
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { createMemoryWithGate } = await import('@/lib/memory/memory-service');
  const { recordHousekeepingOutcome } = await import('@/lib/memory/housekeeping-outcome-cache');

  await initializeDatabase();

  for (const scenario of spec.scenarios) {
    if (scenario.recordOutcome) {
      recordHousekeepingOutcome(
        scenario.characterId,
        scenario.recordOutcome.deleted,
        scenario.recordOutcome.totalBefore,
        scenario.recordOutcome.cap
      );
    }
    const outcome = await createMemoryWithGate(
      {
        characterId: scenario.characterId,
        content: scenario.candidate.content,
        summary: scenario.candidate.summary,
        keywords: [],
        tags: [],
        source: 'AUTO',
      },
      { userId: scenario.userId }
    );
    if (outcome.action !== 'INSERT') {
      throw new Error(`scenario ${scenario.name}: expected INSERT, got ${outcome.action}`);
    }
  }

  // Let the fire-and-forget maybeEnqueueHousekeeping promises settle before
  // dumping — the enqueues under test happen inside them.
  await new Promise((resolve) => setTimeout(resolve, 250));

  const dumpTable = async (table: string, orderBy: string) => {
    const columns = (
      (await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>
    ).map((c) => c.name);
    const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };

  const lines = [
    JSON.stringify(await dumpTable('background_jobs', 'payload')),
    JSON.stringify(await dumpTable('memories', 'content')),
    JSON.stringify(await dumpTable('vector_indices', 'id')),
    JSON.stringify(await dumpTable('vector_entries', 'embedding')),
  ];

  await closeDatabase();

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`memory-watermark oracle wrote ${outPath}\n`);
}

test('memory-watermark tier-3 oracle', async () => {
  await main();
});
