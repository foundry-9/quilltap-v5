/**
 * Tier-2 oracle case — v4's `scaffoldCharacterMount` (Phase-2, mount-index DB,
 * the store-backed capstone sub-unit 3a).
 *
 * Drives v4's REAL `scaffoldCharacterMount(mountPointId)` against the seeded
 * mount point — the whole preset-structure machine: ensureFolderPath for the
 * seven top-level folders, writeDatabaseDocument for the six blank markdown files
 * + two seeded JSON files (each through linkDocumentContent, deduping the six
 * blanks by sha). We do NOT set QUILLTAP_JOB_CHILD, so v4's post-write
 * reindexSingleFile chunk pass runs — but for a database-backed store it calls no
 * model (deterministic), and its only persisted divergence from the Rust storage
 * primitive is the link chunkCount + the doc_mount_chunks rows. The differential
 * pins chunkCount and excludes doc_mount_chunks.
 *
 * Dumps the MOUNT-INDEX store tables (via the raw handle): doc_mount_points (the
 * unchanged seed), doc_mount_folders (7), doc_mount_files (3, deduped),
 * doc_mount_documents (3), doc_mount_file_links (8).
 *
 * NORMALIZATION (done identically on both dumps by the Rust harness): the seeded
 * mountPointId is pinned; folder/file/document/link ids are minted, so the harness
 * remaps ids to first-seen tokens in natural-key order across all tables (so the
 * cross-table FKs verify by relationship) and placeholders timestamps; chunkCount
 * is pinned (reindex artifact).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixtures:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SCAFFOLD_MAIN=/tmp/qt-scaffold-main.db \
 *   QT_FIXTURE_SCAFFOLD_MOUNT=/tmp/qt-scaffold-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/characters-scaffold.ts \
 *     > /tmp/oracle-scaffold.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Spec {
  testPepperBase64: string;
  mountPointId: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'characters-scaffold-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainFixture = process.env.QT_FIXTURE_SCAFFOLD_MAIN;
  const mountFixture = process.env.QT_FIXTURE_SCAFFOLD_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_SCAFFOLD_MAIN and QT_FIXTURE_SCAFFOLD_MOUNT must point at the fixtures from build-characters-scaffold-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-scaffold-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'scaffold-main-work.db');
  const mountWork = join(scratch, 'scaffold-mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { scaffoldCharacterMount } = await import('@/lib/mount-index/character-scaffold');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );

  await initializeDatabase();

  // The op under test: scaffold the seeded mount point.
  await scaffoldCharacterMount(spec.mountPointId);

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
      case: 'characters-scaffold-tier2',
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
  process.stderr.write(`characters-scaffold-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
