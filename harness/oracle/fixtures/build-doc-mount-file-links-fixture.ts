/**
 * Tier-2 fixture builder — the shared starting **mount-index** sibling DB for the
 * document-store storage primitive (`doc_mount_file_links` /
 * `doc_mount_documents` / `doc_mount_files` / `doc_mount_folders`).
 *
 * Mirrors build-doc-mount-points-fixture.ts (the sibling-DB recipe): point
 * SQLITE_MOUNT_INDEX_PATH at the fixture we keep, SQLITE_PATH at a throwaway main
 * DB, seed through v4's real repos (whose overridden getCollection creates each
 * table in the mount-index DB), then flush the mount-index handle.
 *
 * Unlike the single-table builders, the storage primitive raw-INSERTs into FOUR
 * tables, so all four must exist before the oracle (and the Rust port) run. We
 * materialize them by triggering each repo's lazy getCollection via a harmless
 * read; the join-y reads are wrapped (getCollection — which creates the repo's own
 * table — runs before the join query that might reference a not-yet-created
 * sibling table). One pinned doc_mount_points store row is also seeded so every
 * write's mountPointId is a real, pinned store id.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-dmfl-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-doc-mount-file-links-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  store: Record<string, unknown> & { id: string; createdAt: string; updatedAt: string };
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'doc-mount-file-links-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the mount-index fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-dmfl-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { DocMountPointsRepository } = await import(
    '@/lib/database/repositories/doc-mount-points.repository'
  );
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();

  // Seed the pinned store row (creates the doc_mount_points table too).
  const points = new DocMountPointsRepository();
  const { id, createdAt, updatedAt, ...storeData } = spec.store;
  await points.create(storeData as never, { id, createdAt, updatedAt });

  // Materialize the four content tables by running v4's own generated DDL against
  // the raw mount-index handle — the same CREATE TABLE the repos' getCollection
  // would emit, but deterministic and order-independent (no join-read fragility).
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) {
      midb.exec(sql);
    }
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built doc_mount_file_links mount-index fixture: ${out} (store ${spec.store.id})\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
