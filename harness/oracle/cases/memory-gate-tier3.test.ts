/**
 * @jest-environment node
 *
 * Tier-3 (mocked-embedding) ORACLE for the memory gate (v4 `createMemoryWithGate`
 * / `runMemoryGate`, lib/memory/memory-service.ts + lib/memory/memory-gate.ts).
 *
 * Drives v4's REAL `createMemoryWithGate` over the committed corpus
 * (harness/oracle/fixtures/memory-gate-tier3.json) against a REAL fixture DB, with
 * ONLY `generateEmbeddingForUser` stubbed — mocked to the corpus's canned vectors
 * (keyed by the exact `${summary}\n\n${content}` embedding text), the same vectors
 * the Rust `CannedEmbeddingProvider` injects. `cosineSimilarity` / `EmbeddingError`
 * stay REAL (`requireActual`), so the vector search + the gate's error handling are
 * genuine; only the model call is pinned. Everything downstream is deterministic →
 * the three affected tables (`memories`, `vector_indices`, `vector_entries`) are
 * structural-diffed against `quilltap_core::services::memory_gate` by the Rust
 * harness (minted-values shared-cross-table-id-map remap form).
 *
 * This runs under v4's JEST (not tsx): `generateEmbeddingForUser` is a module
 * export that `jest.mock` replaces, the same seam v4's own tests use. The file
 * lives in the v5 harness tree; v4's jest resolves it via an extra `--roots`, with
 * `@/` mapped to v4. `@jest-environment node` keeps the native Buffers off a jsdom
 * realm boundary (v4's real-binding DB-suite convention).
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-memory-gate-fixture.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-memory-gate-fixture.ts
 *   QT_FIXTURE_GATE=/tmp/qt-memory-gate-fixture.db \
 *   QT_ORACLE_OUT=/tmp/oracle-memory-gate.ndjson \
 *     $N/npx jest --silent --roots "$PWD" --roots "$V5/harness/oracle/cases" -- memory-gate-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

// Inlined from ../lib/tier2.ts: jest (unlike tsx) can't resolve the `.js` ESM
// specifier, and this canonicalizer is small — BLOBs → lowercase hex, nulls
// explicit, everything else as-is, rows sorted by `orderBy` (code-unit string
// order on the canonicalized cell, so a BLOB sorts by hex == the Rust dump's
// BLOB memcmp order).
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

// NOTE on mocking: v4's jest.setup.ts (a setupFilesAfterEach file) GLOBALLY mocks
// the whole DB stack (factory, manager, embedding-service, vector-store) so
// ordinary unit tests never touch a real DB — and because it runs AFTER a test
// file's hoisted `jest.mock`, a file-level `jest.mock` can't override it. This
// oracle needs the opposite: the REAL data layer, only the model call pinned. So
// inside the test body we `jest.resetModules()` + `jest.doMock(...)` (runtime, not
// hoisted → wins over the setup mocks) to restore the real modules, and give the
// embedding-service a real body except `generateEmbeddingForUser`, wired to the
// corpus's canned vectors. See main().

interface Candidate {
  content: string;
  summary: string;
  source?: string;
}
interface Scenario {
  name: string;
  characterId: string;
  expectedAction: string;
  candidate: Candidate;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  scenarios: Scenario[];
  cannedEmbeddings: Record<string, number[]>;
  cannedFailures: string[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'memory-gate-tier3.json'), 'utf8')
  ) as Spec;

  const fixture = process.env.QT_FIXTURE_GATE;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_GATE must point at the seed fixture from build-memory-gate-fixture.ts');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) {
    throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-memory-gate-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'gate-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // Restore the real DB stack for THIS test (overriding jest.setup's global
  // mocks), and give the embedding-service a real body with only
  // generateEmbeddingForUser pinned to the corpus. doMock is not hoisted, so it
  // runs now (after setup) and its factory can close over `spec`.
  jest.resetModules();
  // v4's jest.config maps `better-sqlite3` → a fake in-memory mock (so ordinary
  // suites never touch the native binding). Point it at the REAL
  // better-sqlite3-multiple-ciphers (the sqleet/ChaCha20 driver) so the manager
  // opens the actual encrypted fixture — the same real-binding trick v4's own DB
  // suites use. Resolved by absolute path (it lives under packages/quilltap, and
  // this oracle file is loaded from the v5 tree via --roots, so a bare specifier
  // wouldn't resolve from here).
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
        if (spec.cannedFailures.includes(text)) {
          throw new EmbeddingError(`canned failure for input (${text.length} chars)`);
        }
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

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { createMemoryWithGate } = await import('@/lib/memory/memory-service');

  await initializeDatabase();

  for (const scenario of spec.scenarios) {
    const action = await createMemoryWithGate(
      {
        characterId: scenario.characterId,
        content: scenario.candidate.content,
        summary: scenario.candidate.summary,
        keywords: [],
        tags: [],
        source: (scenario.candidate.source as 'AUTO' | 'MANUAL') ?? 'MANUAL',
      },
      { userId: spec.userId }
    );
    if (action.action !== scenario.expectedAction) {
      throw new Error(
        `scenario ${scenario.name}: expected ${scenario.expectedAction}, got ${action.action}`
      );
    }
  }

  // Let the fire-and-forget `void maybeEnqueueHousekeeping(...)` promises settle
  // (they read chatSettings — absent here → early return — never enqueue) before
  // we close the DB, so no query races the close.
  await new Promise((resolve) => setTimeout(resolve, 100));

  const dumpTable = async (table: string, orderBy: string) => {
    const columns = (
      (await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>
    ).map((c) => c.name);
    const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };

  const memoriesDump = await dumpTable('memories', 'content');
  const indicesDump = await dumpTable('vector_indices', 'id');
  const entriesDump = await dumpTable('vector_entries', 'embedding');

  await closeDatabase();

  const lines = [
    JSON.stringify({ case: 'memory-gate-tier3', ...memoriesDump }),
    JSON.stringify({ case: 'memory-gate-tier3', ...indicesDump }),
    JSON.stringify({ case: 'memory-gate-tier3', ...entriesDump }),
  ];
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`memory-gate oracle wrote ${outPath}\n`);
}

// A jest `test` wrapper so the file runs under `jest` (which discovers *.test.ts).
test('memory-gate tier-3 oracle', async () => {
  await main();
});
