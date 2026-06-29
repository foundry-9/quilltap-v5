/**
 * Tier-2 fixture builder — the shared starting state for the `projects`
 * store-backed entity (the document-store overlay slice, build step 4).
 *
 * Identical recipe to build-groups-tier2-fixture.ts, swapping the slim table
 * (`projects` via ProjectSchema) and the entity↔store link table
 * (`project_doc_mount_links`). Builds TWO fixtures (main + mount-index), both
 * starting EMPTY — `repos.projects.create()` provisions the store itself. The
 * mount-index content tables are materialized via v4's generated DDL (they must
 * pre-exist for the Rust port, which never issues CREATE TABLE);
 * `doc_mount_chunks` is materialized so v4's post-write reindex runs cleanly
 * (the differential pins `chunkCount` and excludes the chunks table).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_PROJECTS_MAIN=/tmp/qt-projects-main.db \
 *   QT_FIXTURE_PROJECTS_MOUNT=/tmp/qt-projects-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-projects-tier2-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'projects-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_PROJECTS_MAIN;
  const mountOut = process.env.QT_FIXTURE_PROJECTS_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error(
      'QT_FIXTURE_PROJECTS_MAIN and QT_FIXTURE_PROJECTS_MOUNT must both point at the .db files to write'
    );
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-projects-fixture-build-'));
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
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { ProjectSchema } = await import('@/lib/schemas/project.types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
    ProjectDocMountLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();

  // MAIN db: the slim `projects` table (store-resident columns stay NULL/default).
  await ensureCollection('projects', ProjectSchema);

  // MOUNT-INDEX db: materialize the store tables + the project link table.
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
    ['project_doc_mount_links', ProjectDocMountLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) {
      midb.exec(sql);
    }
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built projects tier-2 fixtures: main=${mainOut} mount=${mountOut}\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`projects fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
