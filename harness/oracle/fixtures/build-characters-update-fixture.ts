/**
 * Tier-2 fixture builder — the shared starting state for v4's
 * `CharactersRepository.update` (Phase-2, the store-backed capstone sub-unit 4a).
 *
 * Unlike the create fixture (which starts EMPTY), this fixture must hold a
 * fully-formed character + vault for the update ops to mutate. So the builder runs
 * v4's REAL `repos.characters.create(character)` once, baking the slim row + the
 * provisioned vault (mount point, folders, managed files, prompt, scenario) into
 * the two output DBs. Both the oracle and the Rust port then operate on a COPY of
 * this SAME baked fixture, so the minted ids/timestamps match and only the
 * update-created rows differ per side; the character id is read back from the
 * fixture (SELECT id FROM characters) by both.
 *
 *   - MAIN db (QT_FIXTURE_CHARUPD_MAIN): the slim `characters` table + the row.
 *   - MOUNT-INDEX db (QT_FIXTURE_CHARUPD_MOUNT): the store tables + the vault.
 *
 * The mount-index store tables are materialized via v4's generated DDL before the
 * create (they must pre-exist for the Rust port, which never issues DDL).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARUPD_MAIN=/tmp/qt-charupd-main.db \
 *   QT_FIXTURE_CHARUPD_MOUNT=/tmp/qt-charupd-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-characters-update-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  character: Record<string, unknown>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'characters-update-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_CHARUPD_MAIN;
  const mountOut = process.env.QT_FIXTURE_CHARUPD_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error(
      'QT_FIXTURE_CHARUPD_MAIN and QT_FIXTURE_CHARUPD_MOUNT must both point at the .db files to write'
    );
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-charupd-fixture-build-'));
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

  // Bake the character + vault by running v4's REAL create.
  const repos = getRepositories();
  const created = await repos.characters.create(spec.character as never);

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built characters update fixtures: main=${mainOut} mount=${mountOut} (character ${created.id})\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`characters update fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
