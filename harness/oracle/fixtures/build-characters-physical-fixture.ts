/**
 * Tier-2 fixture builder — the shared starting state for the
 * physicalDescription-via-update seam of v4's `CharactersRepository.update`
 * (Phase-2, the store-backed capstone).
 *
 * Runs v4's REAL `repos.characters.create(character)` once with NO
 * physicalDescription, baking the slim row + a provisioned vault whose physical-*
 * files hold the scaffold defaults (a blank physical-description.md and the
 * four-key physical-prompts.json). The single update op under test then sets a
 * non-null physicalDescription, overwriting both files. Both the oracle and the
 * Rust port operate on a COPY of this SAME baked fixture (minted ids/timestamps
 * match; the character id is read back from the fixture).
 *
 *   - MAIN db (QT_FIXTURE_CHARPHYS_MAIN): the slim `characters` table + the row.
 *   - MOUNT-INDEX db (QT_FIXTURE_CHARPHYS_MOUNT): the store tables + the vault.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARPHYS_MAIN=/tmp/qt-charphys-main.db \
 *   QT_FIXTURE_CHARPHYS_MOUNT=/tmp/qt-charphys-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-characters-physical-fixture.ts
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
  const specPath = join(here, 'characters-physical-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_CHARPHYS_MAIN;
  const mountOut = process.env.QT_FIXTURE_CHARPHYS_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error(
      'QT_FIXTURE_CHARPHYS_MAIN and QT_FIXTURE_CHARPHYS_MOUNT must both point at the .db files to write'
    );
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-charphys-fixture-build-'));
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

  // Bake the character + vault by running v4's REAL create (no physicalDescription).
  const repos = getRepositories();
  const created = await repos.characters.create(spec.character as never);

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built characters physical fixtures: main=${mainOut} mount=${mountOut} (character ${created.id})\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`characters physical fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
