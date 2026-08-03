/**
 * Tier-3 SEED fixture builder for the EMBEDDING_GENERATE job handler family
 * (P4.6BL — v4 `handleEmbeddingGenerate`,
 * lib/background-jobs/handlers/embedding-generate.ts).
 *
 * Bakes the SEED both differential sides start from, across BOTH databases,
 * via v4's REAL repo `create` calls (pinned ids + timestamps):
 *   - one character (full vault provisioning — the MEMORY branch's
 *     vector-index writes need a characterId with a real home);
 *   - three memories without embeddings (happy / existing-vector-entry /
 *     permanent-failure), plus a seed `vector_indices` meta row and one
 *     ORTHOGONAL `vector_entries` row for the existing-entry case;
 *   - three conversation chunks (happy / whitespace-only / transient-failure);
 *   - one help doc without an embedding;
 *   - two doc mount chunks in the mount-index DB (happy / oversize — the
 *     oversize content is materialized here as `'x'.repeat(oversizeChars)`,
 *     one char past `EMBEDDING_MAX_CHARS`);
 *   - the `embedding_status` PENDING rows (the mc-happy chunk deliberately has
 *     NONE — pinning that `markAsEmbedded` never creates one);
 *   - the fourteen pinned PENDING `EMBEDDING_GENERATE` background jobs the
 *     oracle's claim loop drains (incl. the missing-entity ids and the BOGUS
 *     entity type).
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<the v5 worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-eg-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-eg-mount.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-embedding-generate-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface MemorySeed {
  id: string;
  summary: string;
  content: string;
  occurredAt?: string;
}
interface ChunkSeed {
  id: string;
  interchangeIndex: number;
  content: string;
  participantNames: string[];
}
interface HelpDocSeed {
  id: string;
  title: string;
  path: string;
  url: string;
  content: string;
  contentHash: string;
}
interface MountChunkSeed {
  id: string;
  linkId: string;
  mountPointId: string;
  chunkIndex: number;
  content: string;
  tokenCount: number;
  headingContext: string | null;
}
interface StatusSeed {
  id: string;
  entityType: string;
  entityId: string;
}
interface JobSeed {
  id: string;
  name: string;
  entityType: string;
  entityId: string;
  characterId?: string;
  chatId?: string;
  priority: number;
  createdAt: string;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  profileId: string;
  seedTimestamp: string;
  embedVector: number[];
  oversizeChars: number;
  character: { id: string; name: string; controlledBy: string };
  secondCharacter: { id: string; name: string; controlledBy: string };
  embeddingProfile: { id: string; name: string; provider: string; modelName: string };
  chatId: string;
  memories: MemorySeed[];
  seedVectorEntryForMemoryId: string;
  orphanVectorEntryId: string;
  conversationChunks: ChunkSeed[];
  helpDocs: HelpDocSeed[];
  mountChunks: MountChunkSeed[];
  oversizeMountChunkId: string;
  statusRows: StatusSeed[];
  jobs: JobSeed[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'embedding-generate-jobs.json'), 'utf8')
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-eg-build-'));
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
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  const repos = getRepositories();

  // Materialize the mount-index content tables (the vault scaffold + the
  // chunk-on-write path + this builder's own doc_mount_chunks creates need
  // them).
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
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

  // The single user — the route phase's `createAuthenticatedHandler` resolves
  // the session against this row.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp } as never
  );

  await repos.characters.create(
    {
      name: spec.character.name,
      userId: spec.userId,
      controlledBy: spec.character.controlledBy,
    } as never,
    { id: spec.character.id }
  );
  // The zero-memory second character — the generate-embeddings route arm's
  // "All memories already have embeddings" early return needs one.
  await repos.characters.create(
    {
      name: spec.secondCharacter.name,
      userId: spec.userId,
      controlledBy: spec.secondCharacter.controlledBy,
    } as never,
    { id: spec.secondCharacter.id }
  );
  // The default embedding profile the generate-embeddings route arm gates on
  // (the profile is never RESOLVED in this family — both sides embed through
  // the canned seam — it only has to exist and be the default).
  await repos.embeddingProfiles.create(
    {
      userId: spec.userId,
      name: spec.embeddingProfile.name,
      provider: spec.embeddingProfile.provider,
      modelName: spec.embeddingProfile.modelName,
      isDefault: true,
    } as never,
    {
      id: spec.embeddingProfile.id,
      createdAt: spec.seedTimestamp,
      updatedAt: spec.seedTimestamp,
    }
  );

  for (const m of spec.memories) {
    await repos.memories.create(
      {
        characterId: spec.character.id,
        aboutCharacterId: null,
        chatId: null,
        projectId: null,
        content: m.content,
        summary: m.summary,
        occurredAt: m.occurredAt ?? null,
        keywords: [],
        tags: [],
        importance: 0.4,
        embedding: null,
        source: 'AUTO',
        witnessedContext: null,
        sourceMessageId: null,
        lastAccessedAt: null,
        reinforcementCount: 1,
        lastReinforcedAt: null,
        relatedMemoryIds: [],
        reinforcedImportance: 0.4,
      } as never,
      { id: m.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  // The vector seed: a meta row for the character plus one ORTHOGONAL entry
  // (e1) for the existing-entry case — its job's embed (e0) must land on the
  // updateEntryEmbedding path, not addEntry. The seed meta DELIBERATELY
  // carries the WRONG dimensions (4, not the embed vector's 8) so the
  // handler's `saveMeta(characterId, result.dimensions)` is a VISIBLE value
  // change in the diff — with the right value seeded, a port that skipped
  // saveMeta entirely would pass invisibly (updatedAt is normalized).
  const vectors = new VectorIndicesRepository();
  await vectors.saveMeta(spec.character.id, 4);
  await vectors.addEntry({
    id: spec.seedVectorEntryForMemoryId,
    characterId: spec.character.id,
    embedding: new Float32Array([0, 1, 0, 0, 0, 0, 0, 0]),
  });
  // An ORPHAN vector entry (no memory row) — only rebuild-index's deleteStore
  // removes it, so a port that skips the delete keeps it and diverges loudly.
  await vectors.addEntry({
    id: spec.orphanVectorEntryId,
    characterId: spec.character.id,
    embedding: new Float32Array([0, 0, 1, 0, 0, 0, 0, 0]),
  });

  for (const c of spec.conversationChunks) {
    await repos.conversationChunks.create(
      {
        chatId: spec.chatId,
        interchangeIndex: c.interchangeIndex,
        content: c.content,
        participantNames: c.participantNames,
        messageIds: [],
        embedding: null,
      } as never,
      { id: c.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  for (const d of spec.helpDocs) {
    await repos.helpDocs.create(
      {
        title: d.title,
        path: d.path,
        url: d.url,
        content: d.content,
        contentHash: d.contentHash,
        embedding: null,
      } as never,
      { id: d.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  for (const mc of spec.mountChunks) {
    const content =
      mc.id === spec.oversizeMountChunkId ? 'x'.repeat(spec.oversizeChars) : mc.content;
    await repos.docMountChunks.create(
      {
        linkId: mc.linkId,
        mountPointId: mc.mountPointId,
        chunkIndex: mc.chunkIndex,
        content,
        tokenCount: mc.tokenCount,
        headingContext: mc.headingContext,
        embedding: null,
      } as never,
      { id: mc.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  for (const s of spec.statusRows) {
    await repos.embeddingStatus.create(
      {
        userId: spec.userId,
        entityType: s.entityType,
        entityId: s.entityId,
        profileId: spec.profileId,
        status: 'PENDING',
        embeddedAt: null,
        error: null,
      } as never,
      { id: s.id, createdAt: spec.seedTimestamp }
    );
  }

  for (const j of spec.jobs) {
    const payload: Record<string, unknown> = {
      entityType: j.entityType,
      entityId: j.entityId,
      profileId: spec.profileId,
    };
    if (j.characterId) payload.characterId = j.characterId;
    if (j.chatId) payload.chatId = j.chatId;
    await repos.backgroundJobs.create(
      {
        userId: spec.userId,
        type: 'EMBEDDING_GENERATE',
        status: 'PENDING',
        payload,
        priority: j.priority,
        attempts: 0,
        maxAttempts: 3,
        lastError: null,
        scheduledAt: spec.seedTimestamp,
        startedAt: null,
        completedAt: null,
      } as never,
      { id: j.id, createdAt: j.createdAt, updatedAt: j.createdAt }
    );
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built embedding-generate fixture: ${outMain} + ${outMount} ` +
      `(${spec.memories.length} memories, ${spec.conversationChunks.length} chunks, ` +
      `${spec.helpDocs.length} help docs, ${spec.mountChunks.length} mount chunks, ` +
      `${spec.statusRows.length} status rows, ${spec.jobs.length} jobs)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`embedding-generate fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
