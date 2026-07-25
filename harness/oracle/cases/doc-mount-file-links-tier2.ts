/**
 * Tier-2 oracle case — the document-store STORAGE PRIMITIVE, chunk pass included.
 *
 * Drives v4's REAL `writeDatabaseDocument` (lib/mount-index/database-store.ts) —
 * the content/link/folder split (`linkDocumentContent` + `ensureLinkFolderId`)
 * PLUS the post-write `reindexSingleFile` chunk pass (P4.6BK: v5 chunks on write
 * too, so the pass is part of the contract under test). QUILLTAP_JOB_CHILD stays
 * UNSET — for a database-backed store the chunk pass calls no model and is fully
 * deterministic; `emitDocumentWritten` fires into a listener-less emitter here.
 * Proves what state v4 leaves the FIVE mount-index tables in — doc_mount_files /
 * doc_mount_documents / doc_mount_file_links / doc_mount_folders /
 * doc_mount_chunks — after a fixed write sequence.
 *
 * NORMALIZATION (done identically on both dumps by the Rust harness): every id is
 * minted internally (`randomUUID`) and every timestamp is internal, so NOTHING is
 * pinnable. The harness remaps ids to first-seen tokens in natural-key order
 * ACROSS all five tables (so the cross-table FKs — document.fileId,
 * link.fileId/folderId, folder.parentId, chunk.linkId — are verified) and
 * placeholders timestamps. The store's mountPointId is the one id that IS pinned
 * (seeded), so it is left literal and matches outright. Chunk rows carry no
 * natural key of their own, so the dump appends a derived `sortKey` column
 * (`<mount name>#<link relativePath>#<zero-padded chunkIndex>`) and orders by it.
 *
 * Sibling-DB wiring mirrors doc-mount-points-tier2.ts (SQLITE_MOUNT_INDEX_PATH at
 * the working copy; read back through getRawMountIndexDatabase() directly).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_DOC_MOUNT_FILE_LINKS=/tmp/qt-dmfl-fixture.db \
 *     $N/npx tsx <worktree>/harness/oracle/cases/doc-mount-file-links-tier2.ts \
 *     > /tmp/oracle-dmfl.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind: 'write';
  relativePath: string;
  content: string;
}

interface Spec {
  testPepperBase64: string;
  store: { id: string };
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'doc-mount-file-links-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_DOC_MOUNT_FILE_LINKS;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_DOC_MOUNT_FILE_LINKS must point at the fixture from build-doc-mount-file-links-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-dmfl-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'dmfl-mount-index-work.db');
  copyFileSync(fixture, work);

  // Env MUST be set before importing v4 config/manager modules.
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );

  await initializeDatabase();

  // Drive v4's REAL `writeDatabaseDocument` — the storage transaction under test
  // PLUS its post-write `reindexSingleFile` chunk pass (P4.6BK: the chunk pass is
  // ported, so it is part of the contract; for a database-backed store it calls
  // no model and is deterministic). QUILLTAP_JOB_CHILD stays UNSET so the pass
  // runs, exactly as it does for every parent-side v4 write.
  const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
  for (const op of spec.ops) {
    await writeDatabaseDocument(spec.store.id, op.relativePath, op.content);
  }

  const midb = getRawMountIndexDatabase();
  if (!midb) {
    throw new Error('mount-index DB handle unavailable (degraded open?)');
  }
  const dumpTable = (table: string, orderBy: string) => {
    const columns = (
      midb.pragma(`table_info(${table})`) as Array<{ name: string }>
    ).map((c) => c.name);
    const rawRows = midb
      .prepare(`SELECT * FROM ${table}`)
      .all() as Array<Record<string, unknown>>;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };

  const files = dumpTable('doc_mount_files', 'sha256');
  const documents = dumpTable('doc_mount_documents', 'contentSha256');
  const links = dumpTable('doc_mount_file_links', 'relativePath');
  const folders = dumpTable('doc_mount_folders', 'path');

  // Chunk rows have no natural key of their own; append a derived `sortKey`
  // (`<mount name>#<link relativePath>#<zero-padded chunkIndex>`) and order by it — the Rust
  // harness dumps the same shape (P4.6BK chunk-dump convention).
  const chunkColumns = (
    midb.pragma('table_info(doc_mount_chunks)') as Array<{ name: string }>
  ).map((c) => c.name);
  const chunkRows = midb
    .prepare(
      "SELECT c.*, COALESCE(p.name, '') || '#' || COALESCE(l.relativePath, '') || '#' || printf('%05d', CAST(c.chunkIndex AS INTEGER)) AS sortKey \
       FROM doc_mount_chunks c \
       LEFT JOIN doc_mount_file_links l ON l.id = c.linkId \
       LEFT JOIN doc_mount_points p ON p.id = c.mountPointId"
    )
    .all() as Array<Record<string, unknown>>;
  const chunks = canonicalizeRows({
    table: 'doc_mount_chunks',
    columns: [...chunkColumns, 'sortKey'],
    rawRows: chunkRows,
    orderBy: 'sortKey',
  });

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stdout.write(
    JSON.stringify({
      case: 'doc-mount-file-links-tier2',
      files,
      documents,
      links,
      folders,
      chunks,
    }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`doc-mount-file-links-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
