/**
 * Read-differential fixture builder — the two databases (main + mount-index) for
 * the first-message-context builder (`buildFirstMessageContext` /
 * `loadParticipantMemories` / `loadProjectContext`).
 *
 * Seeds, through v4's REAL repos: full-vault characters (`repos.characters.create`,
 * pinned ids — `description` is a vault-managed field, so the overlay hydrates it
 * back on read), a real project (`repos.projects.create` — provisions its store),
 * and the speaker's memories (`repos.memories.create`, pinned ids + seed timestamp)
 * with per-character vector entries (`saveMeta` + `addEntry`) for the semantic-search
 * candidates. Both the oracle and the Rust port then READ this fixture and run the
 * same case corpus.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-fmc-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-fmc-mount.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-first-message-context-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Character {
  id: string;
  name: string;
  controlledBy: string;
  description: string | null;
}
interface Project {
  id: string;
  name: string;
  description: string;
  instructions: string;
}
interface SeedMemory {
  id: string;
  characterId: string;
  aboutCharacterId: string | null;
  content: string;
  summary: string;
  importance: number;
  vector: number[] | null;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  characters: Character[];
  project: Project;
  seedMemories: SeedMemory[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'first-message-context.json'), 'utf8')) as Spec;

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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-fmc-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = outMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = outMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { VectorIndicesRepository } = await import(
    '@/lib/database/repositories/vector-indices.repository'
  );
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

  // Materialize the mount-index store tables (the character-vault scaffold +
  // project store provisioning need them; the ports never issue CREATE TABLE).
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
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }

  // [40319484] `gcOrphanedFileRow` runs inside every content-addressed rewrite
  // and deletes from `doc_mount_blobs` unconditionally, so a mount index that
  // lacks the table now throws `no such table` on the SECOND write to any path.
  // v4's blobs repository creates it lazily from hand-written DDL
  // (`doc-mount-blobs.repository.ts:113`); copied verbatim so the fixture carries
  // exactly the table a real instance has (it is in `fresh_schema.json` too).
  // (Added at the 2026-08-06 unification: this builder predated the GC and its
  // vault writes died v4-side at the `7df7de8e` regen.)
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

  const repos = getRepositories();

  // Full-vault characters (pinned ids). description is a vault-managed field, so
  // the create projects it into the store and the read overlay restores it.
  for (const c of spec.characters) {
    await repos.characters.create(
      {
        name: c.name,
        userId: spec.userId,
        description: c.description,
        controlledBy: c.controlledBy,
      } as never,
      { id: c.id }
    );
  }

  // A real project (provisions a store) carrying description + instructions
  // (both vault-managed markdown fields on the project store).
  await repos.projects.create(
    {
      userId: spec.userId,
      name: spec.project.name,
      description: spec.project.description,
      instructions: spec.project.instructions,
    } as never,
    { id: spec.project.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
  );

  // Memories + per-character vector entries.
  const vectors = new VectorIndicesRepository();
  let seededMemories = 0;
  let seededVectors = 0;
  for (const m of spec.seedMemories) {
    await repos.memories.create(
      {
        characterId: m.characterId,
        aboutCharacterId: m.aboutCharacterId,
        chatId: null,
        projectId: null,
        content: m.content,
        summary: m.summary,
        keywords: [],
        tags: [],
        importance: m.importance,
        embedding: null,
        source: 'AUTO',
        witnessedContext: null,
        sourceMessageId: null,
        lastAccessedAt: null,
        reinforcementCount: 1,
        lastReinforcedAt: null,
        relatedMemoryIds: [],
        reinforcedImportance: m.importance,
      } as never,
      { id: m.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
    seededMemories++;
    if (m.vector) {
      await vectors.saveMeta(m.characterId, m.vector.length);
      await vectors.addEntry({
        id: m.id,
        characterId: m.characterId,
        embedding: new Float32Array(m.vector),
      });
      seededVectors++;
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

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built first-message-context fixture: main=${outMain} mount=${outMount} (${spec.characters.length} chars, ${seededMemories} memories, ${seededVectors} vectors)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
