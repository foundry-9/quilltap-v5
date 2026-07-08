/**
 * P4.1b fixture builder — the SEED state for the image-ingest differential
 * (`image-ingest.json`): both DBs with every table materialized (main `files`
 * + the mount-index store family), the "Quilltap Uploads" mount row (pinned
 * id/timestamps), and the `instance_settings.userUploadsMountPointId` key.
 * NO images are ingested here — the op sequence is the unit under test and
 * runs in the oracle (jest, sharp mocked) and in the Rust harness, each on a
 * fresh copy.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_INGEST_MAIN=/tmp/qt-ingest-main.db \
 *   QT_FIXTURE_INGEST_MOUNT=/tmp/qt-ingest-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-image-ingest-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  uploadsMountPointId: string;
  seedTs: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'image-ingest.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_INGEST_MAIN;
  const mountOut = process.env.QT_FIXTURE_INGEST_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_INGEST_MAIN and QT_FIXTURE_INGEST_MOUNT must be set');
  }
  for (const base of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = base + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ingest-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { FileEntrySchema } = await import('@/lib/schemas/types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
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
  await ensureCollection('users', UserSchema);
  await ensureCollection('files', FileEntrySchema);

  // Materialize the mount-index DDL up front (the Rust side never creates
  // tables) — see [[document-store-oracle-gotchas]].
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }

  const repos = getRepositories();
  // doc_mount_blobs is hand-written by v4 (data BLOB deliberately schema-less);
  // touch it so the repo creates its own table.
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');

  const TS = spec.seedTs;
  await repos.users.create(
    { username: 'ingest-tester', name: 'Ingest User' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );

  await repos.docMountPoints.create(
    {
      name: 'Quilltap Uploads',
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
    { id: spec.uploadsMountPointId, createdAt: TS, updatedAt: TS },
  );
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'userUploadsMountPointId',
    spec.uploadsMountPointId,
  ]);

  await closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(`built image-ingest fixture: ${mainOut} + ${mountOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`image-ingest fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
