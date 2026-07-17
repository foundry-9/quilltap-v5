/**
 * Differential oracle — the case-preserving vs case-adopting link upserts of
 * v4 `0a0419f5`, for the two link sites the document-store primitive
 * (doc-mount-file-links-tier2) does NOT cover:
 *
 *   - `linkBlobContent` (binary) — case-PRESERVING, like `linkDocumentContent`:
 *     writing `ART/logo.png` onto the existing `Art/Logo.png` updates it in
 *     place and keeps the stored casing; the `Art` folder is reused for the
 *     `ART` segment.
 *   - `linkFilesystemFile` (scanner) — case-ADOPTING: a case-only rename on
 *     disk (`Notes/Draft.md` → `Notes/draft.md`) updates the existing row and
 *     ADOPTS the scanned casing (the filesystem is the source of truth).
 *
 * Drives v4's REAL repo methods over a fresh copy of the doc-mount-file-links
 * tier-2 fixture (the seeded database store), then dumps only the deterministic
 * casing-bearing columns (link relativePath/fileName, folder path, and the file
 * / blob row counts) — minted ids + timestamps are irrelevant to what this
 * proves, so they are dropped rather than remapped.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the shared fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-dmfl-fixture.db \
 *     $N/node --import tsx <W>/harness/oracle/fixtures/build-doc-mount-file-links-fixture.ts
 *   QT_FIXTURE_MOUNT_CASE_RES=/tmp/qt-dmfl-fixture.db \
 *     $N/node --import tsx <W>/harness/oracle/cases/mount-case-resolution.ts \
 *     > /tmp/oracle-mount-case-resolution.ndjson
 */

import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const STORE_ID = '5700000e-0000-4000-8000-0000000000d5';
const TEST_PEPPER = 'ZpjI5jcj5CYsyBA6zPH90G4frQEbv2WsAhERvEKrjJk=';

async function main(): Promise<void> {
  const fixture = process.env.QT_FIXTURE_MOUNT_CASE_RES;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_MOUNT_CASE_RES must point at the doc-mount-file-links fixture db');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-mount-case-res-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'mount-index-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { DocMountFileLinksRepository } = await import(
    '@/lib/database/repositories/doc-mount-file-links.repository'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();
  // v4's linkBlobContent assumes doc_mount_blobs exists (its lazy table-init
  // lives in the blobs repo's getCollection). v5's link_blob_content ensures it
  // itself — touch the blobs repo here so both sides start from the same shape.
  await (getRepositories() as { docMountBlobs: { findByFileId(id: string): Promise<unknown> } })
    .docMountBlobs.findByFileId('00000000-0000-4000-8000-000000000000');
  const repo = new DocMountFileLinksRepository();

  // --- blob case-PRESERVING upsert (Art/Logo.png then ART/logo.png) ---
  await repo.linkBlobContent({
    mountPointId: STORE_ID,
    relativePath: 'Art/Logo.png',
    fileName: 'Logo.png',
    folderId: null,
    originalFileName: 'Logo.png',
    originalMimeType: 'image/png',
    storedMimeType: 'image/png',
    sha256: '0'.repeat(64),
    data: Buffer.from('first-logo-bytes', 'utf-8'),
  });
  await repo.linkBlobContent({
    mountPointId: STORE_ID,
    relativePath: 'ART/logo.png',
    fileName: 'logo.png',
    folderId: null,
    originalFileName: 'logo.png',
    originalMimeType: 'image/png',
    storedMimeType: 'image/png',
    sha256: '0'.repeat(64),
    data: Buffer.from('second-logo-bytes-differ', 'utf-8'),
  });

  // --- filesystem case-ADOPTING upsert (Notes/Draft.md then Notes/draft.md) ---
  await repo.linkFilesystemFile({
    mountPointId: STORE_ID,
    relativePath: 'Notes/Draft.md',
    fileName: 'Draft.md',
    fileType: 'markdown',
    sha256: 'a'.repeat(64),
    fileSizeBytes: 12,
    lastModified: '2024-05-01T00:00:00.000Z',
  });
  await repo.linkFilesystemFile({
    mountPointId: STORE_ID,
    relativePath: 'Notes/draft.md',
    fileName: 'draft.md',
    fileType: 'markdown',
    sha256: 'a'.repeat(64),
    fileSizeBytes: 12,
    lastModified: '2024-05-02T00:00:00.000Z',
  });

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');

  const links = (
    midb
      .prepare(
        `SELECT relativePath, fileName FROM doc_mount_file_links
           WHERE mountPointId = ? ORDER BY relativePath`,
      )
      .all(STORE_ID) as Array<{ relativePath: string; fileName: string }>
  ).map((r) => ({ relativePath: r.relativePath, fileName: r.fileName }));
  const folders = (
    midb
      .prepare(`SELECT path FROM doc_mount_folders WHERE mountPointId = ? ORDER BY path`)
      .all(STORE_ID) as Array<{ path: string }>
  ).map((r) => ({ path: r.path }));
  const fileCount = (
    midb.prepare(`SELECT COUNT(*) AS n FROM doc_mount_files`).get() as { n: number }
  ).n;
  const blobCount = (
    midb.prepare(`SELECT COUNT(*) AS n FROM doc_mount_blobs`).get() as { n: number }
  ).n;

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stdout.write(JSON.stringify({ links, folders, fileCount, blobCount }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`mount-case-resolution oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
