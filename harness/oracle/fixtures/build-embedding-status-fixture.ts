/**
 * Tier-2 fixture builder — materializes the shared starting DB for the
 * `embedding_status` repo (Phase-2) from the committed plaintext spec
 * (`embedding-status-tier2.json`).
 *
 * Same shape as build-tfidf-vocabulary-fixture.ts: the `embedding_status` table
 * is created by v4's OWN `ensureCollection('embedding_status',
 * EmbeddingStatusSchema)` so the DDL (column set/order, the enum/TEXT columns,
 * the nullable columns) is identical to production by construction. Seed rows are
 * inserted via the real `EmbeddingStatusRepository.create` with id + createdAt
 * pinned (CreateOptions). NOTE: that repo OVERRIDES create and always mints
 * `updatedAt = now`, so seed `updatedAt` is live wall-clock — that's expected and
 * gets placeholdered by the tier-2 harness on both sides.
 *
 * The output file is the SEED-ONLY starting state. The op sequence under test is
 * applied later, by `cases/embedding-status-tier2.ts` (oracle) and the Rust
 * harness, each on its own fresh copy.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-es-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-embedding-status-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedRow {
  id: string;
  userId: string;
  entityType: string;
  entityId: string;
  profileId: string;
  status: string;
  embeddedAt: string | null;
  error: string | null;
  createdAt: string;
  updatedAt: string;
}

interface Spec {
  testPepperBase64: string;
  seed: SeedRow[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'embedding-status-tier2.json');
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
  const scratch = mkdtempSync(join(tmpdir(), 'qt-es-fixture-build-'));
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
  const { EmbeddingStatusRepository } = await import(
    '@/lib/database/repositories/embedding-status.repository'
  );
  const { EmbeddingStatusSchema } = await import('@/lib/schemas/types');

  await initializeDatabase();
  await ensureCollection('embedding_status', EmbeddingStatusSchema);

  const repo = new EmbeddingStatusRepository();
  for (const row of spec.seed) {
    await repo.create(
      {
        userId: row.userId,
        entityType: row.entityType,
        entityId: row.entityId,
        profileId: row.profileId,
        status: row.status,
        embeddedAt: row.embeddedAt,
        error: row.error,
      } as never,
      { id: row.id, createdAt: row.createdAt }
    );
  }

  await closeDatabase();

  process.stderr.write(
    `built embedding_status fixture: ${out} (${spec.seed.length} seed rows)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
