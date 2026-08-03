/**
 * Read-differential fixture builder — the mount-index sibling DB for the per-turn
 * knowledge injector (`retrieveKnowledgeForTurn` / `searchDocumentChunks`).
 *
 * Seeds the four tier stores, then the Knowledge/ (and one out-of-prefix) files via
 * v4's REAL `linkDocumentContent` (driven directly — NOT `writeDatabaseDocument`,
 * whose QUILLTAP_JOB_CHILD=1 breaks initializeDatabase; the policy cascade fires from
 * the markdown frontmatter, so `blocked.md`'s `character_read: false` sets
 * `allowCharacterRead=0`), then the `doc_mount_chunks` rows with embeddings directly
 * (linkDocumentContent does NOT chunk/embed — the reindex path is skipped). Each
 * chunk's `linkId` is the id `linkDocumentContent` returns for its file, so the
 * search's linkId → policy / display joins resolve. Both the oracle and the Rust port
 * then READ this fixture.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-knowledge-injector-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-knowledge-injector-fixture.ts
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
  fileType: string;
  content: string;
}
interface SeedChunk {
  id: string;
  mountPointId: string;
  relativePath: string;
  chunkIndex: number;
  headingContext: string | null;
  content: string;
  vector: number[];
}
interface Spec {
  testPepperBase64: string;
  stores: Store[];
  files: SeedFile[];
  chunks: SeedChunk[];
}

// A body long enough (> 500 tokens at 3.5 chars/token ≈ > 1750 chars) that the
// injector demotes its file to a pointer.
const LONG_BODY = 'word '.repeat(600).trim();

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'knowledge-injector.json'), 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) throw new Error('QT_FIXTURE_OUT must point at the mount-index fixture .db to write');
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-knowledge-injector-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db'); // throwaway main DB
  process.env.SQLITE_MOUNT_INDEX_PATH = out; // the fixture we KEEP
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
  const { DocMountChunksRepository } = await import(
    '@/lib/database/repositories/doc-mount-chunks.repository'
  );
  const { sha256OfString } = await import('@/lib/utils/sha256');

  await initializeDatabase();

  // Seed the pinned store rows (also creates the doc_mount_points table).
  const points = new DocMountPointsRepository();
  for (const store of spec.stores) {
    const { id, createdAt, updatedAt, ...storeData } = store;
    await points.create(storeData as never, { id, createdAt, updatedAt });
  }

  // Materialize the four content tables.
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }

  // [40319484] `gcOrphanedFileRow` runs inside every content-addressed rewrite
  // and deletes from `doc_mount_blobs` unconditionally, so a mount index that
  // lacks the table now throws `no such table` on the SECOND write to any path.
  // v4's blobs repository creates it lazily from hand-written DDL
  // (`doc-mount-blobs.repository.ts:113`); copied verbatim so the fixture carries
  // exactly the table a real instance has (it is in `fresh_schema.json` too).
  midb.exec(`
    CREATE TABLE IF NOT EXISTS "doc_mount_blobs" (
      "id" TEXT PRIMARY KEY,
      "fileId" TEXT NOT NULL,
      "sha256" TEXT NOT NULL,
      "sizeBytes" INTEGER NOT NULL,
      "storedMimeType" TEXT NOT NULL,
      "data" BLOB NOT NULL,
      "createdAt" TEXT NOT NULL,
      "updatedAt" TEXT NOT NULL,
      FOREIGN KEY ("fileId") REFERENCES "doc_mount_files" ("id") ON DELETE CASCADE
    )
  `);
  midb.exec(
    'CREATE UNIQUE INDEX IF NOT EXISTS "idx_doc_mount_blobs_fileId" ON "doc_mount_blobs" ("fileId")'
  );

  // Link each file; capture (mountPointId, relativePath) -> linkId for chunk seeding.
  const links = new DocMountFileLinksRepository();
  const linkIdByPath = new Map<string, string>();
  for (const f of spec.files) {
    const content = f.content.replace('LONG_BODY_PLACEHOLDER', LONG_BODY);
    const rel = f.relativePath;
    const { link } = await links.linkDocumentContent({
      mountPointId: f.mountPointId,
      relativePath: rel,
      fileName: rel.includes('/') ? rel.slice(rel.lastIndexOf('/') + 1) : rel,
      folderId: null,
      fileType: f.fileType as never,
      content,
      contentSha256: sha256OfString(content),
      plainTextLength: content.length,
      fileSizeBytes: Buffer.byteLength(content, 'utf-8'),
    });
    linkIdByPath.set(`${f.mountPointId}::${rel}`, link.id);
  }

  // Seed chunks with embeddings directly (linkDocumentContent doesn't chunk/embed).
  const chunks = new DocMountChunksRepository();
  for (const c of spec.chunks) {
    const linkId = linkIdByPath.get(`${c.mountPointId}::${c.relativePath}`);
    if (!linkId) throw new Error(`no link for chunk ${c.id} at ${c.relativePath}`);
    await chunks.create(
      {
        linkId,
        mountPointId: c.mountPointId,
        chunkIndex: c.chunkIndex,
        content: c.content,
        tokenCount: c.content.length,
        headingContext: c.headingContext,
        embedding: new Float32Array(c.vector),
      } as never,
      {
        id: c.id,
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-01T00:00:00.000Z',
      }
    );
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built knowledge-injector fixture: ${out} (${spec.stores.length} stores, ${spec.files.length} files, ${spec.chunks.length} chunks)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
