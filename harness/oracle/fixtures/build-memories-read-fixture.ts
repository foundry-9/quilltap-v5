/**
 * Read-differential fixture builder — the shared starting state for v4's
 * `MemoriesRepository` findBy* / count* queries (Phase-2, the memories repo).
 *
 * Runs v4's REAL `repos.memories.create(data, { id, createdAt, updatedAt })` for
 * each spec memory (ids + timestamps pinned, so the fixture is fully
 * deterministic). Both the oracle and the Rust port then READ a COPY of this SAME
 * baked fixture, so every cell is identical on both sides — the hydrated query
 * results compare exactly, with NO normalization (nothing is mutated, so no
 * minted timestamp ever appears).
 *
 * `memories` is a single MAIN-db table; the `embedding` BLOB column is registered
 * by the real repository on its first `getCollection()`, so the seed `embedding:
 * number[]` lands as a little-endian Float32 BLOB and `null` as SQL NULL — all by
 * v4's own marshaling.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MEMREAD=/tmp/qt-memread.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-memories-read-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  memories: Array<Record<string, unknown> & { id: string; createdAt: string; updatedAt: string }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'memories-read-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_MEMREAD;
  if (!out) {
    throw new Error('QT_FIXTURE_MEMREAD must point at the fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-memread-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { MemoriesRepository } = await import(
    '@/lib/database/repositories/memories.repository'
  );
  const { MemorySchema } = await import('@/lib/schemas/types');

  await initializeDatabase();
  await ensureCollection('memories', MemorySchema);

  const repo = new MemoriesRepository();
  for (const m of spec.memories) {
    const { id, createdAt, updatedAt, ...data } = m;
    await repo.create(data as never, { id, createdAt, updatedAt });
  }

  await closeDatabase();
  process.stderr.write(`built memories read fixture: ${out} (${spec.memories.length} rows)\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memories read fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
