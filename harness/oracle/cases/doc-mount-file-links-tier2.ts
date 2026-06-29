/**
 * Tier-2 oracle case — the document-store STORAGE PRIMITIVE.
 *
 * Drives v4's REAL `writeDatabaseDocument` (lib/mount-index/database-store.ts),
 * which calls `DocMountFileLinksRepository.linkDocumentContent` + `ensureLinkFolderId`
 * — the content/link/folder split that lands every store-backed write. Proves what
 * state v4 leaves the four mount-index tables in after a fixed write sequence, so
 * the Rust port can be diffed against it.
 *
 * QUILLTAP_JOB_CHILD=1 is set so `writeDatabaseDocument` SKIPS the post-write
 * `reindexSingleFile` chunk/embed pass (model-dependent, side-effecting) and the
 * `emitDocumentWritten` event — isolating a clean content-only diff over
 * doc_mount_files / doc_mount_documents / doc_mount_file_links / doc_mount_folders.
 *
 * NORMALIZATION (done identically on both dumps by the Rust harness): every id is
 * minted internally by linkDocumentContent (`randomUUID`) and every timestamp is a
 * single internal `now`, so NOTHING is pinnable. The harness remaps ids to
 * first-seen tokens in natural-key order ACROSS all four tables (so the
 * cross-table FKs — document.fileId, link.fileId/folderId, folder.parentId — are
 * verified) and placeholders timestamps. The store's mountPointId is the one id
 * that IS pinned (seeded), so it is left literal and matches outright.
 *
 * Sibling-DB wiring mirrors doc-mount-points-tier2.ts (SQLITE_MOUNT_INDEX_PATH at
 * the working copy; read back through getRawMountIndexDatabase() directly).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_DOC_MOUNT_FILE_LINKS=/tmp/qt-dmfl-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/doc-mount-file-links-tier2.ts \
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

/** Mirror of v4 `detectDatabaseFileType` (database-store.ts:33), extension-keyed. */
function detectFileType(rel: string): 'markdown' | 'txt' | 'json' | 'jsonl' {
  const dot = rel.lastIndexOf('.');
  const ext = dot >= 0 ? rel.slice(dot).toLowerCase() : '';
  switch (ext) {
    case '.md':
    case '.markdown':
      return 'markdown';
    case '.txt':
      return 'txt';
    case '.json':
      return 'json';
    case '.jsonl':
    case '.ndjson':
      return 'jsonl';
    default:
      throw new Error(`unsupported extension for database store: ${rel}`);
  }
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
  const { DocMountFileLinksRepository } = await import(
    '@/lib/database/repositories/doc-mount-file-links.repository'
  );
  const { sha256OfString } = await import('@/lib/utils/sha256');

  await initializeDatabase();

  // Drive v4's REAL `linkDocumentContent` directly — the storage transaction under
  // test. We do NOT call `writeDatabaseDocument`: its post-write `reindexSingleFile`
  // pass would chunk/embed and MUTATE the link rows (chunkCount/conversionStatus),
  // and its only skip-switch (`QUILLTAP_JOB_CHILD=1`) reroutes `getRepositories()`
  // through the forked-child write proxy (needs a job scope) — neither is usable
  // here. Instead we replicate `writeDatabaseDocument`'s trivial, deterministic
  // input derivation (extension→fileType, real `sha256OfString`, UTF-16 length,
  // UTF-8 byte length, basename) and feed `linkDocumentContent` exactly what the
  // Rust port's `write_database_document` derives. `folderId` is left null so
  // `ensureLinkFolderId` derives + creates it, matching the port.
  const repo = new DocMountFileLinksRepository();
  for (const op of spec.ops) {
    const rel = op.relativePath; // corpus paths are already clean POSIX
    await repo.linkDocumentContent({
      mountPointId: spec.store.id,
      relativePath: rel,
      fileName: rel.includes('/') ? rel.slice(rel.lastIndexOf('/') + 1) : rel,
      folderId: null,
      fileType: detectFileType(rel),
      content: op.content,
      contentSha256: sha256OfString(op.content),
      plainTextLength: op.content.length, // JS .length = UTF-16 code units
      fileSizeBytes: Buffer.byteLength(op.content, 'utf-8'),
    });
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

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stdout.write(
    JSON.stringify({ case: 'doc-mount-file-links-tier2', files, documents, links, folders }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`doc-mount-file-links-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
