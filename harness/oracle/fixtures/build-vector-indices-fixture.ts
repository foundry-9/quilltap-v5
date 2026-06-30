/**
 * Tier-2 fixture builder for the standalone two-table `vector_indices` repo.
 *
 * Build-from-empty: the seed has NO rows — only the two table DDLs, materialized
 * by v4's REAL `VectorIndicesRepository.ensureInitialized()` (triggered by a
 * harmless read). That private method calls:
 *   - ensureCollection('vector_indices', VectorIndexMetaSchema)   (metadata table)
 *   - ensureCollection('vector_entries',  VectorEntryRowSchema)   (entries table)
 *   - registerBlobColumns('vector_entries', ['embedding'])        (BLOB column)
 * so the on-disk shape (column order, affinities, the registered embedding BLOB)
 * is identical to production. The encrypted seed-only DB is written under the
 * throwaway test pepper, exactly like build-folders-remap-fixture.ts.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-vector-indices-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-vector-indices-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'vector-indices-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the fixture .db to write');
  }

  // Fresh output: drop any prior fixture so we never seed on top of stale state.
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  // Throwaway data dir absorbs v4's operational scaffolding (instance lock,
  // startup physical backup, sibling DBs). The MAIN db lands at SQLITE_PATH.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-vector-indices-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  // Env MUST be set before importing v4 config/manager modules.
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE; // writable path uses journal_mode = TRUNCATE
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { VectorIndicesRepository } = await import(
    '@/lib/database/repositories/vector-indices.repository'
  );

  await initializeDatabase();

  // A harmless read triggers ensureInitialized() -> ensureCollection for both
  // tables + registerBlobColumns('vector_entries', ['embedding']).
  const repo = new VectorIndicesRepository();
  await repo.findMetaByCharacterId('00000000-0000-4000-8000-000000000000');

  await closeDatabase();

  process.stderr.write(`built vector_indices fixture: ${out} (empty seed, two tables)\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`vector-indices fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
