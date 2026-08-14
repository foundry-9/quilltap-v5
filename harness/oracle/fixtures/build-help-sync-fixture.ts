/**
 * P4.1b fixture builder — the pre-sync `help_docs` state for the help-doc-sync
 * differential (`help-sync.json` + the committed help tree at
 * `fixtures/help-sync/help/`).
 *
 * Seeds two rows through v4's REAL `HelpDocsRepository.create` with pinned
 * ids/timestamps (the help-docs-tier2 idiom; embeddings passed as `number[]`
 * so v4's own blob-column registration produces the Float32 LE BLOB):
 *   - `help/getting-started.md` with a STALE contentHash + an embedding →
 *     the sync must UPDATE it and clear the embedding;
 *   - `help/no-frontmatter.md` with the MATCHING contentHash + a deliberately
 *     different title → the sync must leave the row byte-untouched.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_HELP_MAIN=/tmp/qt-help-sync-main.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-help-sync-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedRow {
  id: string;
  title: string;
  path: string;
  url: string;
  content: string;
  contentHash: string;
  embedding: number[] | null;
  createdAt: string;
  updatedAt: string;
}

interface EmbeddingStatusSeedRow {
  id: string;
  userId: string;
  entityType: string;
  entityId: string;
  profileId: string;
  status: string;
  embeddedAt: string | null;
  error: string | null;
  createdAt: string;
}

interface HelpDocChunkSeedRow {
  id: string;
  docId: string;
  chunkIndex: number;
  heading: string | null;
  content: string;
  embedding: number[] | null;
}

interface Spec {
  testPepperBase64: string;
  seedSentinel: string;
  seed: SeedRow[];
  embeddingStatusSeed: EmbeddingStatusSeedRow[];
  helpDocChunkSeed: HelpDocChunkSeedRow[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'help-sync.json'), 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_HELP_MAIN;
  if (!out) throw new Error('QT_FIXTURE_HELP_MAIN must be set');
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-help-sync-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { HelpDocsRepository } = await import(
    '@/lib/database/repositories/help-docs.repository'
  );
  const { EmbeddingStatusRepository } = await import(
    '@/lib/database/repositories/embedding-status.repository'
  );
  const { HelpDocSchema } = await import('@/lib/schemas/help-doc.types');
  const { HelpDocChunkSchema } = await import('@/lib/schemas/help-doc-chunk.types');
  const { EmbeddingStatusSchema } = await import('@/lib/schemas/embedding-job.types');

  await initializeDatabase();
  await ensureCollection('help_docs', HelpDocSchema);
  // P4.D77 (v4 `24633026`): the sync now re-slices each changed doc into
  // `help_doc_chunks` and deletes a pruned doc's chunks. Materialize the table
  // so the write lands rather than rethrowing identically on both sides.
  await ensureCollection('help_doc_chunks', HelpDocChunkSchema);
  // The P4.d6 prune cascades into embedding_status (deleteByEntity). That repo
  // call rethrows if the table is absent, so without this the prune would fail
  // on BOTH sides identically and the differential would prove nothing.
  await ensureCollection('embedding_status', EmbeddingStatusSchema);

  const repo = new HelpDocsRepository();
  for (const row of spec.seed) {
    await repo.create(
      {
        title: row.title,
        path: row.path,
        url: row.url,
        content: row.content,
        contentHash: row.contentHash,
        embedding: row.embedding,
      } as never,
      { id: row.id, createdAt: row.createdAt, updatedAt: row.updatedAt }
    );
  }

  const statusRepo = new EmbeddingStatusRepository();
  for (const row of spec.embeddingStatusSeed) {
    await statusRepo.create(
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

  // P4.D77 — pre-existing section chunks: one on the doc the sync UPDATES (must
  // be replaced), one on the doc it leaves UNCHANGED (must survive byte-exact),
  // one on the doc it PRUNES (must be deleted explicitly — this table has no FK).
  const { HelpDocChunksRepository } = await import(
    '@/lib/database/repositories/help-doc-chunks.repository'
  );
  const chunkRepo = new HelpDocChunksRepository();
  for (const row of spec.helpDocChunkSeed) {
    await chunkRepo.create(
      {
        docId: row.docId,
        chunkIndex: row.chunkIndex,
        heading: row.heading,
        content: row.content,
        embedding: row.embedding,
      } as never,
      { id: row.id, createdAt: spec.seedSentinel, updatedAt: spec.seedSentinel }
    );
  }

  await closeDatabase();
  process.stderr.write(
    `built help-sync fixture: ${out} (${spec.seed.length} help_docs rows, ` +
      `${spec.embeddingStatusSeed.length} embedding_status rows, ` +
      `${spec.helpDocChunkSeed.length} help_doc_chunks rows)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`help-sync fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
