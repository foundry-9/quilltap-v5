/**
 * Tier-2 oracle case — v4's `CharactersRepository.create` (Phase-2, the
 * store-backed capstone sub-unit 3b — the keystone create integration).
 *
 * Drives v4's REAL `repos.characters.create(character)` end-to-end: the slim-row
 * `_create` (FK nulled), `ensureCharacterVault` (mint a '<name> Character Vault'
 * mount point, `scaffoldCharacterMount`, `writeCharacterVaultManagedFields`, then
 * `linkCharacterToVault` set + confirm the FK), and the closing overlay re-read
 * (read-only). We do NOT mock the storage boundary and do NOT set
 * QUILLTAP_JOB_CHILD, so reindexSingleFile runs — but for a database-backed store
 * it calls no model, and its only persisted divergence from the Rust storage
 * primitive is the link chunkCount + the doc_mount_chunks rows. The differential
 * pins chunkCount and excludes doc_mount_chunks.
 *
 * A character spans two DBs, so we dump BOTH:
 *   - the MAIN slim `characters` row (via `rawQuery`);
 *   - the MOUNT-INDEX store tables (doc_mount_points / _folders / _files /
 *     _documents / _file_links) via the raw handle.
 *
 * NORMALIZATION (done identically on both dumps by the Rust harness): nothing is
 * pinned — the character id, the mount point id, and every file/document/link/
 * folder id are minted, so the harness remaps ids to first-seen tokens in
 * natural-key order ACROSS all tables (FKs verify by relationship) and placeholders
 * timestamps. chunkCount is pinned (reindex artifact).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixtures:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARCREATE_MAIN=/tmp/qt-charcreate-main.db \
 *   QT_FIXTURE_CHARCREATE_MOUNT=/tmp/qt-charcreate-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/characters-create.ts \
 *     > /tmp/oracle-charcreate.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Spec {
  testPepperBase64: string;
  character: Record<string, unknown>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'characters-create-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainFixture = process.env.QT_FIXTURE_CHARCREATE_MAIN;
  const mountFixture = process.env.QT_FIXTURE_CHARCREATE_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_CHARCREATE_MAIN and QT_FIXTURE_CHARCREATE_MOUNT must point at the fixtures from build-characters-create-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-charcreate-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'charcreate-main-work.db');
  const mountWork = join(scratch, 'charcreate-mount-work.db');
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

  // The op under test: create the character end-to-end.
  await repos.characters.create(spec.character as never);

  // MAIN db: the slim characters row, read RAW through v4's own backend.
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

  // MOUNT-INDEX db: the store tables, read directly through the raw handle.
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
      case: 'characters-create-tier2',
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
  process.stderr.write(`characters-create-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
