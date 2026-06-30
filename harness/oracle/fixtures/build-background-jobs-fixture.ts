/**
 * Tier-2 fixture builder — materializes the shared starting DB for the
 * `background_jobs` repo (Phase-2, main DB) from the committed plaintext spec
 * (`background-jobs-tier2.json`).
 *
 * Same shape as build-text-replacement-rules-fixture.ts: the `background_jobs`
 * table is created by v4's OWN `ensureCollection('background_jobs',
 * BackgroundJobSchema)` so the DDL (column set/order, the REAL number columns,
 * the JSON `payload` column) is identical to production by construction. Seed
 * rows are inserted via the real `BackgroundJobsRepository.create` with id +
 * timestamps pinned (CreateOptions), so the starting state is fully
 * deterministic. The seed pins `scheduledAt` far in the past (2020) so the
 * claimable / stuck SET is the same no matter when the differential runs.
 *
 * The output file is the SEED-ONLY starting state. The op sequence under test is
 * applied later, by `cases/background-jobs.ts` (oracle) and the Rust harness,
 * each on its own fresh copy.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-bj-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-background-jobs-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedJob {
  id: string;
  userId: string;
  type: string;
  status: string;
  payload: Record<string, unknown>;
  priority: number;
  attempts: number;
  maxAttempts: number;
  lastError?: string;
  scheduledAt: string;
  createdAt: string;
  updatedAt: string;
}

interface Spec {
  testPepperBase64: string;
  seed: SeedJob[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'background-jobs-tier2.json');
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
  // startup physical backup, sibling llm-logs / mount-index DBs). A unique dir
  // per run avoids stale-lock collisions. The MAIN db still lands at SQLITE_PATH.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-bj-fixture-build-'));
  // v4 nests its working files under `<dataDir>/data/` (instance lock, sibling
  // DBs). Pre-create it so `acquireInstanceLock` can open the lock file.
  mkdirSync(join(scratch, 'data'), { recursive: true });

  // Env MUST be set before importing v4 config/manager modules.
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE; // writable path uses journal_mode = TRUNCATE
  process.env.LOG_LEVEL = 'error'; // keep stdout/stderr quiet for clean runs

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { BackgroundJobsRepository } = await import(
    '@/lib/database/repositories/background-jobs.repository'
  );
  const { BackgroundJobSchema } = await import('@/lib/schemas/job.types');

  await initializeDatabase();
  await ensureCollection('background_jobs', BackgroundJobSchema);

  const repo = new BackgroundJobsRepository();
  for (const row of spec.seed) {
    const data: Record<string, unknown> = {
      userId: row.userId,
      type: row.type,
      status: row.status,
      payload: row.payload,
      priority: row.priority,
      attempts: row.attempts,
      maxAttempts: row.maxAttempts,
      scheduledAt: row.scheduledAt,
    };
    if (row.lastError !== undefined) data.lastError = row.lastError;

    await repo.create(data as never, {
      id: row.id,
      createdAt: row.createdAt,
      updatedAt: row.updatedAt,
    });
  }

  await closeDatabase();

  process.stderr.write(
    `built background_jobs fixture: ${out} (${spec.seed.length} seed rows)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
