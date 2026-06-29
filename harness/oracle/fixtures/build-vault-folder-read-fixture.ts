/**
 * Read-differential fixture builder — the shared mount-index sibling DB for the
 * vault read overlay's directory-listing load
 * (`DocMountDocumentsRepository.findManyByMountPointsInFolder`).
 *
 * Mirrors build-doc-mount-file-links-fixture.ts: point SQLITE_MOUNT_INDEX_PATH at
 * the fixture, SQLITE_PATH at a throwaway main DB, seed the pinned stores +
 * materialize the four content tables, then write the corpus files via v4's real
 * `writeDatabaseDocument` (QUILLTAP_JOB_CHILD=1 so the post-write reindex is
 * skipped — no model, no chunks). Unlike the storage-primitive fixture (which is
 * written by the test), THIS fixture is pre-seeded with file content so both the
 * oracle and the Rust port can READ the same rows.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-vault-folder-read-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-vault-folder-read-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Store {
  id: string;
  createdAt: string;
  updatedAt: string;
  [k: string]: unknown;
}
interface SeedFile {
  mountPointId: string;
  relativePath: string;
  content: string;
}
interface Spec {
  testPepperBase64: string;
  stores: Store[];
  files: SeedFile[];
}

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
      return 'jsonl';
    default:
      return 'markdown';
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'vault-folder-read-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the mount-index fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-vault-folder-read-build-'));
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
  const { DocMountFileLinksRepository } = await import(
    '@/lib/database/repositories/doc-mount-file-links.repository'
  );
  const { sha256OfString } = await import('@/lib/utils/sha256');

  await initializeDatabase();

  // Seed the pinned store rows (also creates the doc_mount_points table).
  const points = new DocMountPointsRepository();
  for (const store of spec.stores) {
    const { id, createdAt, updatedAt, ...storeData } = store;
    await points.create(storeData as never, { id, createdAt, updatedAt });
  }

  // Materialize the four content tables (same DDL the repos' getCollection emits).
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

  // Seed the corpus files by driving v4's real linkDocumentContent directly —
  // the same approach the storage-primitive oracle uses (NOT writeDatabaseDocument:
  // its post-write reindex would chunk/embed, and its only skip-switch
  // QUILLTAP_JOB_CHILD=1 reroutes repos through the forked-child write proxy, which
  // breaks initializeDatabase here). We replicate writeDatabaseDocument's trivial,
  // deterministic input derivation; folderId:null lets ensureLinkFolderId create
  // the parent folder rows.
  const links = new DocMountFileLinksRepository();
  for (const f of spec.files) {
    const rel = f.relativePath;
    await links.linkDocumentContent({
      mountPointId: f.mountPointId,
      relativePath: rel,
      fileName: rel.includes('/') ? rel.slice(rel.lastIndexOf('/') + 1) : rel,
      folderId: null,
      fileType: detectFileType(rel),
      content: f.content,
      contentSha256: sha256OfString(f.content),
      plainTextLength: f.content.length,
      fileSizeBytes: Buffer.byteLength(f.content, 'utf-8'),
    });
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built vault-folder-read mount-index fixture: ${out} (${spec.files.length} files, ${spec.stores.length} stores)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
