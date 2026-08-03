/**
 * Tier-3 fixture builder — the seed state for v4's EMBEDDING_REFIT handler
 * (W4.7e2). Bakes, through v4's REAL repositories, into two DBs:
 *   - MAIN (QT_FIXTURE_REFIT_MAIN): characters (real create → vaults), their
 *     memories, the help docs, and a BUILTIN embedding profile with a PINNED id;
 *     plus the (empty) background_jobs + tfidf_vocabularies tables the handler
 *     writes (materialized via generateDDL so the Rust port — which issues no
 *     DDL — can write to them).
 *   - MOUNT-INDEX (QT_FIXTURE_REFIT_MOUNT): the store tables + the vaults.
 *
 * Character ids are minted by the real create; the refit discovers them via
 * findByUserId, so they need not be pinned. The embedding profile id IS pinned so
 * both sides build the refit job with the same profileId. Both the oracle and the
 * Rust port READ a COPY of this SAME baked fixture.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_REFIT_MAIN=/tmp/qt-refit-main.db \
 *   QT_FIXTURE_REFIT_MOUNT=/tmp/qt-refit-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-embedding-refit-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  profileId: string;
  characters: Array<Record<string, unknown>>;
  memories: Array<{ character: string; summary: string; content: string }>;
  helpDocs: Array<{ title: string; path: string; url: string; content: string }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'embedding-refit-tier3.json'), 'utf8')
  ) as Spec;

  const mainOut = process.env.QT_FIXTURE_REFIT_MAIN;
  const mountOut = process.env.QT_FIXTURE_REFIT_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error(
      'QT_FIXTURE_REFIT_MAIN and QT_FIXTURE_REFIT_MOUNT must both point at the .db files to write'
    );
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-refit-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { CharacterSchema, BackgroundJobSchema, TfidfVocabularySchema } = await import(
    '@/lib/schemas/types'
  );
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  await ensureCollection('characters', CharacterSchema);

  // Materialize the mount-index store tables (the Rust port issues no DDL).
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const mountDdl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
  ];
  for (const [name, schema] of mountDdl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }

  // [40319484] `gcOrphanedFileRow` runs inside every content-addressed rewrite
  // and deletes from `doc_mount_blobs` unconditionally, so a mount index that
  // lacks the table now throws `no such table` on the SECOND write to any path.
  // v4's blobs repository creates it lazily from hand-written DDL
  // (`doc-mount-blobs.repository.ts:113`); copied verbatim so the fixture carries
  // exactly the table a real instance has (it is in `fresh_schema.json` too).
  midb.exec(`
    CREATE TABLE IF NOT EXISTS "doc_mount_blobs" (
      "id" TEXT PRIMARY KEY,
      "fileId" TEXT NOT NULL,
      "sha256" TEXT NOT NULL,
      "sizeBytes" INTEGER NOT NULL,
      "storedMimeType" TEXT NOT NULL,
      "data" BLOB NOT NULL,
      "createdAt" TEXT NOT NULL,
      "updatedAt" TEXT NOT NULL,
      FOREIGN KEY ("fileId") REFERENCES "doc_mount_files" ("id") ON DELETE CASCADE
    )
  `);
  midb.exec(
    'CREATE UNIQUE INDEX IF NOT EXISTS "idx_doc_mount_blobs_fileId" ON "doc_mount_blobs" ("fileId")'
  );

  const repos = getRepositories();

  // Characters (real create → vaults). Capture minted ids by name.
  const idByName = new Map<string, string>();
  for (const character of spec.characters) {
    const c = await repos.characters.create(character as never);
    idByName.set((character as { name: string }).name, c.id);
  }

  // Memories keyed to the minted character ids.
  for (const m of spec.memories) {
    const characterId = idByName.get(m.character);
    if (!characterId) throw new Error(`unknown character in memory spec: ${m.character}`);
    await repos.memories.create({
      characterId,
      aboutCharacterId: null,
      chatId: null,
      projectId: null,
      content: m.content,
      summary: m.summary,
      keywords: [],
      tags: [],
      importance: 0.5,
      embedding: null,
      source: 'AUTO',
      witnessedContext: null,
      sourceMessageId: null,
      lastAccessedAt: null,
      reinforcementCount: 1,
      lastReinforcedAt: null,
      relatedMemoryIds: [],
      reinforcedImportance: 0.5,
    } as never);
  }

  // Help docs.
  for (const d of spec.helpDocs) {
    await repos.helpDocs.create({
      title: d.title,
      path: d.path,
      url: d.url,
      content: d.content,
      contentHash: 'seed-hash-' + d.path,
      embedding: null,
    } as never);
  }

  // The BUILTIN embedding profile (PINNED id so both sides know the profileId).
  await repos.embeddingProfiles.create(
    {
      userId: spec.userId,
      name: 'Built-in TF-IDF',
      provider: 'BUILTIN',
      apiKeyId: null,
      baseUrl: null,
      modelName: 'tfidf-bm25-v1',
      dimensions: null,
      truncateToDimensions: null,
      normalizeL2: true,
      isDefault: true,
      tags: [],
    } as never,
    {
      id: spec.profileId,
      createdAt: '2020-01-01T00:00:00.000Z',
      updatedAt: '2020-01-01T00:00:00.000Z',
    }
  );

  // Materialize the (empty) tables the handler writes but the builder does not
  // (the Rust port issues no DDL).
  await ensureCollection('background_jobs', BackgroundJobSchema);
  await ensureCollection('tfidf_vocabularies', TfidfVocabularySchema);

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built embedding-refit fixture: main=${mainOut} mount=${mountOut} ` +
      `(${spec.characters.length} chars, ${spec.memories.length} memories, ${spec.helpDocs.length} help docs)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`embedding-refit fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
