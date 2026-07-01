/**
 * Tier-2 oracle case — the PROVISION-ON-THE-FLY seam of v4's
 * `CharactersRepository.update` (Phase-2, the store-backed capstone).
 *
 * Drives v4's REAL `repos.characters.update(id, patch)` over the baked, vault-less
 * fixture character with a managed-field patch: applyDocumentStoreWriteOverlay sees
 * no linked vault + managed fields present, provisions a FRESH vault via
 * ensureCharacterVault (mint mount point, scaffold, project, link), re-reads the
 * FK, then routes the patch's managed fields onto the new vault. We do NOT set
 * QUILLTAP_JOB_CHILD, so reindexSingleFile runs; the differential pins chunkCount
 * and excludes doc_mount_chunks.
 *
 * The character id is read back from the fixture so both sides target the same
 * minted id. A character spans two DBs, so we dump BOTH the main slim `characters`
 * row and the mount-index store tables.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixtures:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARPROV_MAIN=/tmp/qt-charprov-main.db \
 *   QT_FIXTURE_CHARPROV_MOUNT=/tmp/qt-charprov-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/characters-provision.ts \
 *     > /tmp/oracle-charprov.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  patch: Record<string, unknown>;
}
interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'characters-provision-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainFixture = process.env.QT_FIXTURE_CHARPROV_MAIN;
  const mountFixture = process.env.QT_FIXTURE_CHARPROV_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_CHARPROV_MAIN and QT_FIXTURE_CHARPROV_MOUNT must point at the fixtures from build-characters-provision-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-charprov-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'charprov-main-work.db');
  const mountWork = join(scratch, 'charprov-mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );

  await initializeDatabase();
  const repos = getRepositories();

  const idRow = (await rawQuery('SELECT id FROM characters LIMIT 1')) as Array<{ id: string }>;
  if (idRow.length === 0) throw new Error('fixture has no character row');
  const characterId = idRow[0].id;

  for (const op of spec.ops) {
    await repos.characters.update(characterId, op.patch as never);
  }

  const charColumns = (
    (await rawQuery('PRAGMA table_info(characters)')) as Array<{ name: string }>
  ).map((c) => c.name);
  const charRows = (await rawQuery('SELECT * FROM characters')) as Array<
    Record<string, unknown>
  >;
  const characters = canonicalizeRows({
    table: 'characters',
    columns: charColumns,
    rawRows: charRows,
    orderBy: 'name',
  });

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable (degraded open?)');
  const dumpTable = (table: string, orderBy: string) => {
    const columns = (
      midb.pragma(`table_info(${table})`) as Array<{ name: string }>
    ).map((c) => c.name);
    const rawRows = midb
      .prepare(`SELECT * FROM ${table}`)
      .all() as Array<Record<string, unknown>>;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };

  const points = dumpTable('doc_mount_points', 'name');
  const folders = dumpTable('doc_mount_folders', 'path');
  const files = dumpTable('doc_mount_files', 'sha256');
  const documents = dumpTable('doc_mount_documents', 'contentSha256');
  const links = dumpTable('doc_mount_file_links', 'relativePath');

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stdout.write(
    JSON.stringify({
      case: 'characters-provision-tier2',
      characters,
      points,
      folders,
      files,
      documents,
      links,
    }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`characters-provision-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
