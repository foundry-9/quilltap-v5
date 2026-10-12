/**
 * Tier-2 fixture builder for the Commonplace canon reads (v4 `d58548051`
 * `lib/memory/cheap-llm-tasks/canon.ts` — `loadCommonplaceCanon`,
 * `loadCanonForSelfWithCommonplace`, `loadCanonForObserverAboutSubject`).
 *
 * Bakes the vault state both differential sides read: the spec's characters
 * via v4's REAL `repos.characters.create` (pinned ids, full vault
 * provisioning in the mount-index DB), then every spec file into the owning
 * character's vault via the real `linkDocumentContent` (the
 * memory-processor builder's idiom — NOT `writeDatabaseDocument`, whose
 * `QUILLTAP_JOB_CHILD=1` skip-switch breaks `initializeDatabase`). Both the
 * tsx oracle and the Rust test then copy BOTH files and only READ.
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-canon-commonplace-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-canon-commonplace-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-canon-commonplace-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CharacterSpec {
  id: string;
  name: string;
  identity?: string;
  description?: string;
  manifesto?: string;
  personality?: string;
  controlledBy: string;
  files: Array<{ path: string; content: string }>;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  characters: CharacterSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'canon-commonplace-tier2.json'), 'utf8')
  ) as Spec;

  const outMain = process.env.QT_FIXTURE_OUT;
  const outMount = process.env.QT_FIXTURE_MOUNT_OUT;
  if (!outMain || !outMount) {
    throw new Error('QT_FIXTURE_OUT and QT_FIXTURE_MOUNT_OUT must point at the fixture .db files');
  }
  for (const base of [outMain, outMount]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = base + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-canon-commonplace-build-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = outMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = outMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { DocMountFileLinksRepository } = await import(
    '@/lib/database/repositories/doc-mount-file-links.repository'
  );
  const { sha256OfString } = await import('@/lib/utils/sha256');

  await initializeDatabase();
  const repos = getRepositories();

  // Materialize the mount-index content tables (hand-written repos whose DDL
  // v4 emits at startup elsewhere; the vault scaffold needs them).
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');
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

  // Characters with full vault provisioning, pinned ids.
  for (const c of spec.characters) {
    await repos.characters.create(
      {
        name: c.name,
        userId: spec.userId,
        identity: c.identity ?? null,
        description: c.description ?? null,
        manifesto: c.manifesto ?? null,
        personality: c.personality ?? null,
        controlledBy: c.controlledBy,
      } as never,
      { id: c.id }
    );
  }

  // The vault files (Others/ hand canon + Commonplace/ digest mirrors).
  const links = new DocMountFileLinksRepository();
  let files = 0;
  for (const c of spec.characters) {
    if (c.files.length === 0) continue;
    const raw = await repos.characters.findByIdRaw(c.id);
    const mountId = raw?.characterDocumentMountPointId;
    if (!mountId) throw new Error(`character ${c.name} has no vault mount point`);
    for (const f of c.files) {
      const rel = f.path;
      await links.linkDocumentContent({
        mountPointId: mountId,
        relativePath: rel,
        fileName: rel.includes('/') ? rel.slice(rel.lastIndexOf('/') + 1) : rel,
        folderId: null,
        fileType: 'markdown',
        content: f.content,
        contentSha256: sha256OfString(f.content),
        plainTextLength: f.content.length,
        fileSizeBytes: Buffer.byteLength(f.content, 'utf-8'),
      });
      files++;
    }
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built canon-commonplace fixture: ${outMain} + ${outMount} ` +
      `(${spec.characters.length} characters, ${files} vault files)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`canon-commonplace fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
