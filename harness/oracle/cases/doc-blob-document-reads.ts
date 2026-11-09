/**
 * Oracle case (P4.D380 item 3): v4 `a3d53eae1`'s blob-as-document readers —
 * `readDatabaseBlobAsDocument` / `readDatabaseBlobForDocument`
 * (`lib/mount-index/database-store.ts:112-171` at `624d0d25c`), driven REAL.
 *
 * Builds its own planted pair (MINTED per run — not committed): copies the
 * committed mounts pair (`crates/quilltap-web/tests/fixtures/mounts-{main,
 * mount}.db`, located from this file's path, read-only) to
 * `QT_FIXTURE_DBDR_MAIN` / `QT_FIXTURE_DBDR_MOUNT`, plants uploads through v4's
 * REAL `storeMountFile`, then pins every planted link's `lastModified` and
 * blob's `updatedAt` to fixed instants (raw UPDATEs) so the two `mtime`s are
 * deterministic, and plants a converted PDF text by raw UPDATE (v4's converter
 * cannot read the garbage bytes). The Rust side
 * (`doc_blob_document_reads_equivalence.rs`) opens the SAME planted pair and
 * reads the same paths — so both sides read byte-identical state.
 *
 * Emitted: one row per read path `{ path, asDocument, forDocument, lines }`
 * (`lines` = v4's captured logger lines, DEBUG included).
 *
 * Run (Node 24, from the v4 pin worktree):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_DBDR_MAIN=/tmp/qt-dbdr-main.db QT_FIXTURE_DBDR_MOUNT=/tmp/qt-dbdr-mount.db \
 *   LOG_LEVEL=debug \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/cases/doc-blob-document-reads.ts \
 *     > /tmp/oracle-doc-blob-document-reads.ndjson
 */

import { copyFileSync, existsSync, mkdtempSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const MP_DB = 'b6000000-0000-4000-8000-000000000001';

const PLANTS: Array<{ path: string; mime: string; bytes: Buffer; extractText: boolean }> = [
  { path: 'uploads/a.md', mime: 'text/markdown', extractText: true, bytes: Buffer.from('# A\n\nalpha upload\n', 'utf8') },
  { path: 'uploads/bad.md', mime: 'text/markdown', extractText: false,
    bytes: Buffer.concat([Buffer.from('broken ', 'utf8'), Buffer.from([0xff, 0xfe, 0x80]), Buffer.from(' bytes\n', 'utf8')]) },
  { path: 'uploads/bom.md', mime: 'text/markdown', extractText: false,
    bytes: Buffer.concat([Buffer.from([0xef, 0xbb, 0xbf]), Buffer.from('# BOM kept\n', 'utf8')]) },
  { path: 'uploads/data.json', mime: 'application/json', extractText: true, bytes: Buffer.from('{"n":1,"é":"ü"}\n', 'utf8') },
  { path: 'uploads/Shout.MD', mime: 'text/markdown', extractText: false, bytes: Buffer.from('upper-case extension\n', 'utf8') },
  { path: 'uploads/notes.yaml', mime: 'application/yaml', extractText: true, bytes: Buffer.from('a: 1\n', 'utf8') },
  { path: 'uploads/report.pdf', mime: 'application/pdf', extractText: false, bytes: Buffer.from('not really a pdf', 'utf8') },
  { path: 'uploads/raw.pdf', mime: 'application/pdf', extractText: false, bytes: Buffer.from('also not a pdf', 'utf8') },
  { path: 'uploads/bad-date.md', mime: 'text/markdown', extractText: false, bytes: Buffer.from('unparseable lastModified\n', 'utf8') },
];

const READS = [
  'uploads/a.md',
  'UPLOADS/A.md',
  'uploads/bad.md',
  'uploads/bom.md',
  'uploads/data.json',
  'uploads/Shout.MD',
  'uploads/notes.yaml',
  'uploads/report.pdf',
  'uploads/raw.pdf',
  'uploads/none.md',
  'uploads/bad-date.md',
  '/uploads//a.md',
];

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, '..', 'fixtures', 'mounts.json'), 'utf8')) as {
    testPepperBase64: string;
  };
  const committed = join(here, '..', '..', '..', 'crates', 'quilltap-web', 'tests', 'fixtures');
  const outMain = process.env.QT_FIXTURE_DBDR_MAIN;
  const outMount = process.env.QT_FIXTURE_DBDR_MOUNT;
  if (!outMain || !outMount) throw new Error('QT_FIXTURE_DBDR_MAIN and QT_FIXTURE_DBDR_MOUNT are required');
  for (const out of [outMain, outMount]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) if (existsSync(out + suffix)) rmSync(out + suffix);
  }
  copyFileSync(join(committed, 'mounts-main.db'), outMain);
  copyFileSync(join(committed, 'mounts-mount.db'), outMount);

  const scratch = mkdtempSync(join(tmpdir(), 'qt-dbdr-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = outMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = outMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  (globalThis as Record<string, unknown>).__quilltapJobHost = {
    child: null, spawning: false, shuttingDown: true, childCrashed: false,
    restartTimestamps: [], signalHandlersInstalled: false, invalidationListeners: new Set(),
  };

  const { logger } = await import('@/lib/logger');
  const captured: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown> };
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      captured.push({ level: data.level, message: data.message, fields });
    },
  });

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();
  const midb = getRawMountIndexDatabase() as unknown as {
    prepare: (s: string) => { run: (...a: unknown[]) => unknown };
  };

  const { storeMountFile } = await import('@/lib/mount-index/store-file');
  for (const p of PLANTS) {
    await storeMountFile({
      mountPointId: MP_DB,
      relativePath: p.path,
      data: p.bytes,
      originalMimeType: p.mime,
      originalFileName: p.path.split('/').pop(),
      treatNativeTextAsDocument: false,
      transcodeImages: true,
      extractText: p.extractText,
      enqueueEmbedding: false,
      collisionStrategy: 'unique-suffix',
      assetStorage: 'database',
    });
  }
  // Deterministic instants for both `mtime` sources.
  midb.prepare(
    `UPDATE doc_mount_file_links SET lastModified = '2026-03-04T05:06:07.890Z' WHERE mountPointId = ? AND relativePath LIKE 'uploads/%'`,
  ).run(MP_DB);
  midb.prepare(
    `UPDATE doc_mount_blobs SET updatedAt = '2026-01-02T03:04:05.678Z' WHERE fileId IN
       (SELECT fileId FROM doc_mount_file_links WHERE mountPointId = ? AND relativePath LIKE 'uploads/%')`,
  ).run(MP_DB);
  midb.prepare(
    `UPDATE doc_mount_file_links SET lastModified = 'not a date' WHERE mountPointId = ? AND relativePath = 'uploads/bad-date.md'`,
  ).run(MP_DB);
  // A converted PDF (v4's converter cannot read the garbage bytes).
  midb.prepare(
    `UPDATE doc_mount_file_links SET extractedText = 'Derived PDF text.', extractionStatus = 'converted'
       WHERE mountPointId = ? AND relativePath = 'uploads/report.pdf'`,
  ).run(MP_DB);

  const { readDatabaseBlobAsDocument, readDatabaseBlobForDocument } = await import(
    '@/lib/mount-index/database-store'
  );
  for (const path of READS) {
    captured.length = 0;
    const asDocument = await readDatabaseBlobAsDocument(MP_DB, path);
    const asLines = captured.splice(0);
    const forDocument = await readDatabaseBlobForDocument(MP_DB, path);
    const forLines = captured.splice(0);
    process.stdout.write(
      JSON.stringify({ path, asDocument, forDocument, asLines, forLines }) + '\n',
    );
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`doc-blob-document-reads failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
