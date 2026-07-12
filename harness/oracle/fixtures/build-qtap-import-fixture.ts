/**
 * Tier-2 fixture builder — the shared starting state for v4's `executeImport`
 * over the committed `lorian-and-riya.qtap` seed (P4.4u4).
 *
 * The import spans TWO databases, so this builds TWO fixtures, both EMPTY (the
 * import provisions each character's vault and writes the files itself):
 *   - the MAIN db (QT_FIXTURE_QTAPIMPORT_MAIN) holding the slim `characters`,
 *     `wardrobe_items`, and `memories` tables, created by v4's own
 *     `ensureCollection(...)` so their DDL is production-identical;
 *   - the MOUNT-INDEX db (QT_FIXTURE_QTAPIMPORT_MOUNT) holding the document-store
 *     tables, materialized via v4's generated DDL (idempotent CREATE TABLE; they
 *     must pre-exist because the Rust port never issues DDL). Same recipe as
 *     build-characters-create-fixture.ts.
 *
 * `doc_mount_chunks` IS materialized so v4's post-write reindexSingleFile runs
 * cleanly; the differential pins chunkCount and excludes that table.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_QTAPIMPORT_MAIN=/tmp/qt-qtapimport-main.db \
 *   QT_FIXTURE_QTAPIMPORT_MOUNT=/tmp/qt-qtapimport-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-qtap-import-fixture.ts
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
  const specPath = join(here, 'qtap-import-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_QTAPIMPORT_MAIN;
  const mountOut = process.env.QT_FIXTURE_QTAPIMPORT_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error(
      'QT_FIXTURE_QTAPIMPORT_MAIN and QT_FIXTURE_QTAPIMPORT_MOUNT must both point at the .db files to write'
    );
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-qtapimport-fixture-build-'));
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
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { WardrobeItemSchema } = await import('@/lib/schemas/wardrobe.types');
  const { MemorySchema } = await import('@/lib/schemas/memory.types');
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

  // MAIN db: the slim tables the import writes (vault-managed columns stay
  // NULL/default on characters).
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('wardrobe_items', WardrobeItemSchema);
  await ensureCollection('memories', MemorySchema);

  // MOUNT-INDEX db: materialize every store table the create/provision/write path
  // touches, via v4's own generated DDL.
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
    for (const sql of generateDDL(name, schema as never)) {
      midb.exec(sql);
    }
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built qtap-import fixtures: main=${mainOut} mount=${mountOut}\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`qtap-import fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
