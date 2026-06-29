/**
 * Tier-2 fixture builder — the mount-index sibling DB for `doc_mount_blobs` (the
 * binary byte-store, build step 8 of the document-store overlay slice).
 *
 * The blob table declares `FOREIGN KEY (fileId) REFERENCES doc_mount_files(id)`,
 * and the writable open enables `foreign_keys = ON`, so each blob's `fileId` must
 * reference a real `doc_mount_files` row. This builder therefore:
 *   1. seeds the parent `doc_mount_files` rows (pinned ids) through v4's REAL
 *      `DocMountFilesRepository` (its getCollection creates the table in the
 *      mount-index DB);
 *   2. materializes the `doc_mount_blobs` table by triggering v4's REAL
 *      `DocMountBlobsRepository` table-init (its `db()` runs the hand-written DDL
 *      with the `data BLOB` column — which `generateDDL` could NOT produce, since
 *      `DocMountBlobMetadataSchema` deliberately omits `data`).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-blobs-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-doc-mount-blobs-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Parent {
  id: string;
  sha256: string;
  fileType: string;
}
interface Spec {
  testPepperBase64: string;
  parents: Parent[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'doc-mount-blobs-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) throw new Error('QT_FIXTURE_OUT must point at the mount-index fixture .db to write');
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-blobs-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { DocMountFilesRepository } = await import(
    '@/lib/database/repositories/doc-mount-files.repository'
  );
  const { DocMountBlobsRepository } = await import(
    '@/lib/database/repositories/doc-mount-blobs.repository'
  );

  await initializeDatabase();

  // 1. Seed the parent file rows (pinned ids) so the blob FK is satisfiable.
  const files = new DocMountFilesRepository();
  for (const parent of spec.parents) {
    await files.create(
      {
        sha256: parent.sha256,
        fileSizeBytes: 0,
        fileType: parent.fileType as never,
        source: 'database',
      } as never,
      {
        id: parent.id,
        createdAt: '2026-01-01T00:00:00.000Z',
        updatedAt: '2026-01-01T00:00:00.000Z',
      }
    );
  }

  // 2. Materialize the doc_mount_blobs table via the real repo's hand-written DDL
  //    (a harmless read triggers its lazy table-init).
  const blobs = new DocMountBlobsRepository();
  await blobs.findByFileId('00000000-0000-4000-8000-000000000000');

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built doc_mount_blobs fixture: ${out} (${spec.parents.length} parent file rows)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`doc-mount-blobs fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
