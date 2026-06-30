/**
 * Tier-2 fixture builder — the shared starting state for v4's
 * `CharactersRepository.create` (Phase-2, the store-backed capstone sub-unit 3b).
 *
 * A character spans TWO databases, so this builds TWO fixtures, both EMPTY (create
 * provisions the vault and writes the files itself — the whole point):
 *   - the MAIN db (QT_FIXTURE_CHARCREATE_MAIN) holding the slim `characters` table,
 *     created by v4's own `ensureCollection('characters', CharacterSchema)` so its
 *     DDL (incl. the vault-managed columns that stay NULL/default) is identical to
 *     production;
 *   - the MOUNT-INDEX db (QT_FIXTURE_CHARCREATE_MOUNT) holding the document-store
 *     tables, materialized via v4's generated DDL (idempotent CREATE TABLE; they
 *     must pre-exist because the Rust port never issues DDL). Same recipe as
 *     build-groups-tier2-fixture.ts.
 *
 * `doc_mount_chunks` IS materialized so v4's post-write reindexSingleFile runs
 * cleanly; the differential pins the resulting chunkCount and excludes that table.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARCREATE_MAIN=/tmp/qt-charcreate-main.db \
 *   QT_FIXTURE_CHARCREATE_MOUNT=/tmp/qt-charcreate-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-characters-create-fixture.ts
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
  const specPath = join(here, 'characters-create-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_CHARCREATE_MAIN;
  const mountOut = process.env.QT_FIXTURE_CHARCREATE_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error(
      'QT_FIXTURE_CHARCREATE_MAIN and QT_FIXTURE_CHARCREATE_MOUNT must both point at the .db files to write'
    );
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-charcreate-fixture-build-'));
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

  // MAIN db: the slim `characters` table (vault-managed columns stay NULL/default).
  await ensureCollection('characters', CharacterSchema);

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
    `built characters create fixtures: main=${mainOut} mount=${mountOut}\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`characters create fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
