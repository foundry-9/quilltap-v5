/**
 * Tier-2 oracle case — the `tfidf_vocabulary` repo's `upsertByProfileId` method
 * (Phase-2), in the MINTED-VALUES (remap) form.
 *
 * Proves what state v4 leaves the database in after a fixed sequence of
 * `upsertByProfileId(profileId, data)` calls, so the Rust port can be diffed
 * against it. v4's semantics: find existing by profileId (findByProfileId); if
 * found -> this.update(existing.id, FULL data); else -> this.create(data). Both
 * branches mint nondeterministic values — the CREATE branch mints id + createdAt
 * + updatedAt, the UPDATE branch mints updatedAt and preserves createdAt — so the
 * two ports cannot produce byte-identical raw dumps. They are reconciled by the
 * harness's REMAP normalization (first-seen id remap in natural-key order +
 * createdAt/updatedAt placeholdered).
 *
 * This case therefore emits a RAW dump (no remap, no placeholder), sorted by the
 * natural key `profileId` (an input, unique per row) — the harness applies the
 * SAME normalization to this dump and to the Rust dump, then diffs, so the
 * normalization is provably consistent.
 *
 * Op 2 creates a NEW profileId (P3) and op 3 upserts that SAME profileId,
 * exercising an UPDATE of a row whose id v4 minted — the case the id remap exists
 * for. v4 reads `profileId` from the `data` payload, so each op carries it there.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`), AFTER
 * building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_TFIDF_VOCABULARY_UPSERT=/tmp/qt-tv-upsert-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/tfidf-vocabulary-upsert-tier2.ts \
 *     > /tmp/oracle-tv-upsert.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  copyFileSync,
  existsSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind: 'upsert';
  data: Record<string, unknown> & { profileId: string };
}

interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(
    here,
    '..',
    'fixtures',
    'tfidf-vocabulary-upsert-tier2.json'
  );
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_TFIDF_VOCABULARY_UPSERT;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_TFIDF_VOCABULARY_UPSERT must point at the seed fixture from build-tfidf-vocabulary-upsert-fixture.ts'
    );
  }

  // Work on a fresh copy so the shared seed fixture stays pristine.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-tv-upsert-oracle-'));
  // v4 nests working files under `<dataDir>/data/` (instance lock, sibling DBs).
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'tv-upsert-work.db');
  copyFileSync(fixture, work);

  // Env MUST be set before importing v4 config/manager modules.
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE; // writable path uses journal_mode = TRUNCATE
  // Keep stdout clean for the NDJSON: v4's console logger sends INFO to stdout.
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { TfidfVocabularyRepository } = await import(
    '@/lib/database/repositories/tfidf-vocabulary.repository'
  );

  await initializeDatabase();
  const repo = new TfidfVocabularyRepository();

  for (const op of spec.ops) {
    // v4's signature: upsertByProfileId(profileId, data). The profileId is also a
    // field of `data` (the Omit<…,'id'|timestamps> shape includes it); pass both.
    await repo.upsertByProfileId(op.data.profileId, op.data as never);
  }

  // Read RAW on-disk state through v4's own connected backend. table_info gives
  // schema column order; SELECT * gives the persisted rows (booleans as 0/1, the
  // REAL number columns as numbers, vocabulary/idf as single-encoded JSON text).
  const columns = (
    (await rawQuery('PRAGMA table_info(tfidf_vocabularies)')) as Array<{
      name: string;
    }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM tfidf_vocabularies')) as Array<
    Record<string, unknown>
  >;

  await closeDatabase();

  // RAW dump, sorted by the natural key `profileId` (NOT the random id) so both
  // sides line up row-for-row before the harness remaps ids / placeholders ts.
  const dump = canonicalizeRows({
    table: 'tfidf_vocabularies',
    columns,
    rawRows,
    orderBy: 'profileId',
  });

  process.stdout.write(
    JSON.stringify({ case: 'tfidf-vocabulary-upsert-tier2', ...dump }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(
    `tfidf-vocabulary-upsert-tier2 oracle failed: ${err?.stack ?? err}\n`
  );
  process.exit(1);
});
