/**
 * Read-differential fixture builder — the shared starting state for v4's
 * `CharactersRepository` findBy* queries (Phase-2, the store-backed capstone
 * sub-unit 4c).
 *
 * Runs v4's REAL `repos.characters.create(...)` for each spec character, baking the
 * slim rows + their provisioned vaults into the two output DBs. Both the oracle and
 * the Rust port then READ a COPY of this SAME baked fixture, so the minted
 * ids/timestamps are identical on both sides (no remap) and the hydrated query
 * results compare exactly (except physicalDescription's minted createdAt/updatedAt).
 *
 *   - MAIN db (QT_FIXTURE_CHARREAD_MAIN): the slim `characters` table + the rows.
 *   - MOUNT-INDEX db (QT_FIXTURE_CHARREAD_MOUNT): the store tables + the vaults.
 *
 * The mount-index store tables are materialized via v4's generated DDL before the
 * creates (they must pre-exist for the Rust port, which never issues DDL).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARREAD_MAIN=/tmp/qt-charread-main.db \
 *   QT_FIXTURE_CHARREAD_MOUNT=/tmp/qt-charread-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-characters-read-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  characters: Array<Record<string, unknown>>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'characters-read-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_CHARREAD_MAIN;
  const mountOut = process.env.QT_FIXTURE_CHARREAD_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error(
      'QT_FIXTURE_CHARREAD_MAIN and QT_FIXTURE_CHARREAD_MOUNT must both point at the .db files to write'
    );
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-charread-fixture-build-'));
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
  await ensureCollection('characters', CharacterSchema);

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

  // Bake each character + vault by running v4's REAL create.
  const repos = getRepositories();
  const created: string[] = [];
  for (const character of spec.characters) {
    const c = await repos.characters.create(character as never);
    created.push(`${(character as { name: string }).name}=${c.id}`);
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built characters read fixtures: main=${mainOut} mount=${mountOut} (${created.join(', ')})\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`characters read fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
