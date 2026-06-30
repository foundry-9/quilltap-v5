/**
 * Tier-2 fixture builder — materializes an EMPTY `memories` table (Phase-2, the
 * memories repo). The op sequence under test (`memories-tier2.json`) starts with
 * its own `create`s, so the seed state is just the schema.
 *
 * The table is created by v4's OWN `ensureCollection('memories', MemorySchema)`,
 * so the DDL (column set/order, the `embedding` BLOB column) is identical to
 * production by construction. The op sequence is applied later, by
 * `cases/memories-tier2.ts` (oracle) and the Rust harness, each on a fresh copy.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-mem-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-memories-fixture.ts
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
  const specPath = join(here, 'memories-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-mem-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { MemorySchema } = await import('@/lib/schemas/types');

  await initializeDatabase();
  await ensureCollection('memories', MemorySchema);
  await closeDatabase();

  process.stderr.write(`built empty memories fixture: ${out}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memories fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
