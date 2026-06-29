/**
 * Tier-2 fixture builder — materializes the shared starting DB for the
 * `tfidf_vocabulary` repo (Phase-2) from the committed plaintext spec
 * (`tfidf-vocabulary-tier2.json`).
 *
 * Same shape as build-image-profiles-fixture.ts: the `tfidf_vocabularies` table
 * is created by v4's OWN `ensureCollection('tfidf_vocabularies',
 * TfidfVocabularySchema)` so the DDL (column set/order, the REAL number columns,
 * the boolean column registration) is identical to production by construction.
 * Seed rows are inserted via the real `TfidfVocabularyRepository.create` with id
 * + timestamps pinned (CreateOptions), so the starting state is fully
 * deterministic.
 *
 * The output file is the SEED-ONLY starting state. The op sequence under test is
 * applied later, by `cases/tfidf-vocabulary-tier2.ts` (oracle) and the Rust
 * harness, each on its own fresh copy.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-tv-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-tfidf-vocabulary-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedRow {
  id: string;
  profileId: string;
  userId: string;
  vocabulary: string;
  idf: string;
  avgDocLength: number;
  vocabularySize: number;
  includeBigrams: boolean;
  fittedAt: string;
  createdAt: string;
  updatedAt: string;
}

interface Spec {
  testPepperBase64: string;
  seed: SeedRow[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'tfidf-vocabulary-tier2.json');
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
  const scratch = mkdtempSync(join(tmpdir(), 'qt-tv-fixture-build-'));
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
  const { TfidfVocabularyRepository } = await import(
    '@/lib/database/repositories/tfidf-vocabulary.repository'
  );
  const { TfidfVocabularySchema } = await import('@/lib/schemas/types');

  await initializeDatabase();
  await ensureCollection('tfidf_vocabularies', TfidfVocabularySchema);

  const repo = new TfidfVocabularyRepository();
  for (const row of spec.seed) {
    await repo.create(
      {
        profileId: row.profileId,
        userId: row.userId,
        vocabulary: row.vocabulary,
        idf: row.idf,
        avgDocLength: row.avgDocLength,
        vocabularySize: row.vocabularySize,
        includeBigrams: row.includeBigrams,
        fittedAt: row.fittedAt,
      } as never,
      { id: row.id, createdAt: row.createdAt, updatedAt: row.updatedAt }
    );
  }

  await closeDatabase();

  process.stderr.write(
    `built tfidf_vocabularies fixture: ${out} (${spec.seed.length} seed rows)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
