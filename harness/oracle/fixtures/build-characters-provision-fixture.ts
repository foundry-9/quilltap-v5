/**
 * Tier-2 fixture builder — the shared starting state for the
 * PROVISION-ON-THE-FLY seam of v4's `CharactersRepository.update` (Phase-2, the
 * store-backed capstone).
 *
 * Bakes a character WITHOUT a vault: runs v4's REAL `repos.characters.create`
 * once (which provisions a vault), then WIPES it — deletes every mount-index store
 * row and NULLs the character's `characterDocumentMountPointId`. The result is a
 * valid slim `characters` row with no linked vault and no surviving same-name
 * store, so the update op under test reaches the provision-on-the-fly branch and
 * a FRESH vault is minted (the adopt search finds nothing). Both the oracle and
 * the Rust port operate on a COPY of this SAME baked+wiped fixture.
 *
 *   - MAIN db (QT_FIXTURE_CHARPROV_MAIN): the slim `characters` table + the row
 *     (FK nulled).
 *   - MOUNT-INDEX db (QT_FIXTURE_CHARPROV_MOUNT): the (now empty) store tables.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARPROV_MAIN=/tmp/qt-charprov-main.db \
 *   QT_FIXTURE_CHARPROV_MOUNT=/tmp/qt-charprov-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-characters-provision-fixture.ts
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
  const specPath = join(here, 'characters-provision-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_CHARPROV_MAIN;
  const mountOut = process.env.QT_FIXTURE_CHARPROV_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error(
      'QT_FIXTURE_CHARPROV_MAIN and QT_FIXTURE_CHARPROV_MOUNT must both point at the .db files to write'
    );
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-charprov-fixture-build-'));
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
  const storeTables: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
  ];
  for (const [name, schema] of storeTables) {
    for (const sql of generateDDL(name, schema as never)) {
      midb.exec(sql);
    }
  }

  // Bake the character + vault by running v4's REAL create.
  const repos = getRepositories();
  const created = await repos.characters.create(spec.character as never);

  // Now WIPE the vault so the character is vault-less with no surviving same-name
  // store: null the FK on the slim row and clear every mount-index store table.
  await rawQuery('UPDATE characters SET "characterDocumentMountPointId" = NULL WHERE id = ?', [
    created.id,
  ]);
  for (const [name] of storeTables) {
    midb.exec(`DELETE FROM "${name}"`);
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built characters provision fixtures: main=${mainOut} mount=${mountOut} (character ${created.id}, vault wiped)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`characters provision fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
