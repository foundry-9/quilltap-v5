/**
 * Fixture builder for the PUBLIC wardrobe write path (seam #7).
 *
 * Bakes the SHARED starting state: a character with an empty vault, spanning TWO
 * databases (the slim `characters` row in MAIN, the document store in
 * MOUNT-INDEX). Unlike build-characters-create-fixture (which leaves both empty
 * for the create-under-test), this one RUNS v4's REAL `repos.characters.create`
 * so both the oracle and the Rust port start from the same baked character +
 * empty `Wardrobe/` folder, then drive the wardrobe ops-under-test against a copy.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WPUB_MAIN=/tmp/qt-wpub-main.db \
 *   QT_FIXTURE_WPUB_MOUNT=/tmp/qt-wpub-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-vault-wardrobe-public-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  characterId: string;
  character: Record<string, unknown>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'vault-wardrobe-public-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_WPUB_MAIN;
  const mountOut = process.env.QT_FIXTURE_WPUB_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_WPUB_MAIN and QT_FIXTURE_WPUB_MOUNT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wpub-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import('@/lib/database/manager');
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

  // Bake the character + its empty vault via the REAL create (mints the mount
  // point, scaffolds the preset folders incl. an empty Wardrobe/). Pin the
  // character id via CreateOptions so both sides can target it (create ignores
  // any `data.id`).
  const repos = getRepositories();
  const PINNED_TS = '2026-02-01T00:00:00.000Z';
  await repos.characters.create(spec.character as never, {
    id: spec.characterId,
    createdAt: PINNED_TS,
    updatedAt: PINNED_TS,
  } as never);

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(`built vault-wardrobe-public fixtures: main=${mainOut} mount=${mountOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`vault-wardrobe-public fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
