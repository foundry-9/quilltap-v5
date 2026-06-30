/**
 * Tier-2 fixture builder — the shared starting state for v4's
 * `scaffoldCharacterMount` (Phase-2, mount-index DB, the store-backed capstone
 * sub-unit 3a).
 *
 * scaffoldCharacterMount operates entirely on the MOUNT-INDEX DB (its repos —
 * docMountPoints / docMountFolders / docMountDocuments / docMountFileLinks — all
 * route there). So this builds the MOUNT-INDEX fixture (QT_FIXTURE_SCAFFOLD_MOUNT)
 * with:
 *   - every store table the scaffold/write path touches, materialized via v4's
 *     generated DDL (idempotent CREATE TABLE; they must pre-exist because the Rust
 *     port never issues DDL) — same recipe as build-groups-tier2-fixture.ts;
 *   - one seeded database-backed CHARACTER mount point (the scaffold target), via
 *     v4's real `repos.docMountPoints.create` with a pinned id + timestamps.
 *
 * `doc_mount_chunks` IS materialized so v4's post-write reindexSingleFile runs
 * cleanly; the differential pins the resulting chunkCount and excludes that table.
 *
 * The MAIN db (QT_FIXTURE_SCAFFOLD_MAIN) is a throwaway — v4's initializeDatabase
 * needs it, but the scaffold never writes there (the Rust port doesn't open it).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SCAFFOLD_MAIN=/tmp/qt-scaffold-main.db \
 *   QT_FIXTURE_SCAFFOLD_MOUNT=/tmp/qt-scaffold-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-characters-scaffold-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  mountPointId: string;
  mountPoint: Record<string, unknown>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'characters-scaffold-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_SCAFFOLD_MAIN;
  const mountOut = process.env.QT_FIXTURE_SCAFFOLD_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error(
      'QT_FIXTURE_SCAFFOLD_MAIN and QT_FIXTURE_SCAFFOLD_MOUNT must both point at the .db files to write'
    );
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-scaffold-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
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

  // MOUNT-INDEX db: materialize every store table the scaffold/write path touches.
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

  // Seed the scaffold target: a database-backed character mount point with a
  // pinned id + timestamps, through v4's real (mount-index-routed) repo.
  const repos = getRepositories();
  await repos.docMountPoints.create(spec.mountPoint as never, {
    id: spec.mountPointId,
    createdAt: spec.mountPoint.createdAt as string,
    updatedAt: spec.mountPoint.updatedAt as string,
  });

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built characters scaffold fixtures: main=${mainOut} mount=${mountOut} (1 mount point)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`scaffold fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
