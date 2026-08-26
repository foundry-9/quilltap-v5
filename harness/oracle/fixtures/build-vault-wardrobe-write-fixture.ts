/**
 * Tier-2 fixture builder — the shared mount-index sibling DB for the character
 * vault WARDROBE WRITE projection (projectVaultWardrobe).
 *
 * Seeds the pinned store, materializes the doc-store tables (including
 * `doc_mount_chunks` so v4's post-write reindex runs cleanly), and seeds the
 * legacy `wardrobe.json` so the projection's cleanup of it is exercised. Both the
 * oracle and the Rust port then run the SAME projection sequence against a copy of
 * this fixture and diff the resulting tables.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-vault-wardrobe-write-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-vault-wardrobe-write-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  store: { id: string };
  seedWardrobeJson?: string;
  /** P4.D119: hand-kept files seeded into the folder before any projection, so
   * the `preserveFileNames` sweep exemption has something real to preserve. */
  seedFolderFiles?: Array<{ relativePath: string; content: string }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'vault-wardrobe-write-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the mount-index fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-vault-wardrobe-write-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { DocMountPointsRepository } = await import(
    '@/lib/database/repositories/doc-mount-points.repository'
  );
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
  } = await import('@/lib/schemas/mount-index.types');
  const { DocMountFileLinksRepository } = await import(
    '@/lib/database/repositories/doc-mount-file-links.repository'
  );
  const { sha256OfString } = await import('@/lib/utils/sha256');

  await initializeDatabase();

  const points = new DocMountPointsRepository();
  const PINNED_TS = '2026-02-01T00:00:00.000Z';
  await points.create(
    {
      name: 'Character Files: Wardrobe Vault',
      basePath: '',
      mountType: 'database',
      storeType: 'documents',
      includePatterns: [],
      excludePatterns: [],
      enabled: true,
      lastScannedAt: null,
      scanStatus: 'idle',
      lastScanError: null,
      conversionStatus: 'idle',
      conversionError: null,
      fileCount: 0,
      chunkCount: 0,
      totalSizeBytes: 0,
    } as never,
    { id: spec.store.id, createdAt: PINNED_TS, updatedAt: PINNED_TS },
  );

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) {
      midb.exec(sql);
    }
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

  // Seed the legacy wardrobe.json so the projection's cleanup of it is exercised.
  if (spec.seedWardrobeJson !== undefined) {
    const links = new DocMountFileLinksRepository();
    const content = spec.seedWardrobeJson;
    await links.linkDocumentContent({
      mountPointId: spec.store.id,
      relativePath: 'wardrobe.json',
      fileName: 'wardrobe.json',
      folderId: null,
      fileType: 'json',
      content,
      contentSha256: sha256OfString(content),
      plainTextLength: content.length,
      fileSizeBytes: Buffer.byteLength(content, 'utf-8'),
    });
  }

  // [P4.D119 / v4 b86bb1a5] Hand-kept folder files (the dressing-instructions
  // file, deliberately mis-cased) seeded through the raw document-store write,
  // so the projection's `preserveFileNames` exemption is tested against a file
  // the projection never wrote.
  if (spec.seedFolderFiles?.length) {
    const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
    const { ensureFolderPath } = await import('@/lib/mount-index/folder-paths');
    for (const f of spec.seedFolderFiles) {
      const folder = f.relativePath.slice(0, f.relativePath.lastIndexOf('/'));
      if (folder) await ensureFolderPath(spec.store.id, folder);
      await writeDatabaseDocument(spec.store.id, f.relativePath, f.content);
    }
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(`built vault-wardrobe-write fixture: ${out}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
