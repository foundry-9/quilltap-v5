/**
 * Differential oracle — the character fact-sheet (`metadata.json`) round-trip
 * through v4's REAL `applyDocumentStoreWriteOverlay`.
 *
 * `metadata` is a writable managed field with NO DB column — the vault file is
 * its sole source of truth — and its patch semantics differ from every other
 * JSON file in the vault: it is a whole-object REPLACE, not a read-modify-write
 * merge (properties.json merges only because five Character fields share that one
 * file; metadata is one field owning one file, so a merge would make deleting a
 * key impossible). This drives v4's real write-overlay over a REAL two-DB fixture
 * (the character + linked vault the charupd builder bakes) and, per op, emits
 * whether `metadata` survived into the DB-bound patch and the resulting
 * metadata.json bytes. The Rust port
 * (`db::vault_character_update::apply_document_store_write_overlay`) must match.
 *
 * Templated on v4's __tests__/.../metadata-vault-roundtrip.test.ts, but a real
 * write→store→read cycle (not the in-memory Map mock), so the two ports agree on
 * the actual stored bytes.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the charupd fixtures:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server   # or /tmp/qt-v4-baseline (pinned)
 *   QT_FIXTURE_CHARUPD_MAIN=/tmp/qt-charupd-main.db \
 *   QT_FIXTURE_CHARUPD_MOUNT=/tmp/qt-charupd-mount.db \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/cases/metadata-vault-roundtrip.ts \
 *     > /tmp/oracle-metadata-roundtrip.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

const METADATA_JSON_PATH = 'metadata.json';

interface Op {
  label: string;
  patch: Record<string, unknown>;
  presetMetadataFile?: string;
}
interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'metadata-vault-roundtrip.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainFixture = process.env.QT_FIXTURE_CHARUPD_MAIN;
  const mountFixture = process.env.QT_FIXTURE_CHARUPD_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_CHARUPD_MAIN and QT_FIXTURE_CHARUPD_MOUNT must point at the fixtures from build-characters-update-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-metadata-roundtrip-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'metadata-roundtrip-main-work.db');
  const mountWork = join(scratch, 'metadata-roundtrip-mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { applyDocumentStoreWriteOverlay } = await import(
    '@/lib/database/repositories/character-properties-overlay'
  );
  const { writeDatabaseDocument, readDatabaseDocument, DatabaseStoreError } = await import(
    '@/lib/mount-index/database-store'
  );

  await initializeDatabase();
  const repos = getRepositories();

  // Read the baked character id + its vault mount FK (both sides target the same).
  const idRow = (await rawQuery('SELECT id FROM characters LIMIT 1')) as Array<{ id: string }>;
  if (idRow.length === 0) throw new Error('fixture has no character row');
  const characterId = idRow[0].id;
  const raw = await repos.characters.findByIdRaw(characterId);
  const mountPointId = raw?.characterDocumentMountPointId as string;
  if (!mountPointId) throw new Error('baked character has no vault mount point');

  async function readMetadataFile(): Promise<string | null> {
    try {
      const doc = await readDatabaseDocument(mountPointId, METADATA_JSON_PATH);
      return doc.content;
    } catch (err) {
      if (err instanceof DatabaseStoreError && err.code === 'NOT_FOUND') return null;
      throw err;
    }
  }

  const rows: Array<{
    label: string;
    dbPatchHasMetadata: boolean;
    fileContent: string | null;
  }> = [];

  for (const op of spec.ops) {
    if (op.presetMetadataFile !== undefined) {
      // Seed a (possibly unparseable) file BEFORE the overlay runs, to prove the
      // whole-object replace overwrites it without reading it first.
      await writeDatabaseDocument(mountPointId, METADATA_JSON_PATH, op.presetMetadataFile);
    }
    const dbPatch = await applyDocumentStoreWriteOverlay(characterId, op.patch as never);
    rows.push({
      label: op.label,
      dbPatchHasMetadata: 'metadata' in dbPatch,
      fileContent: await readMetadataFile(),
    });
  }

  await closeDatabase();

  process.stdout.write(JSON.stringify({ case: 'metadata-vault-roundtrip', rows }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`metadata-vault-roundtrip oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
