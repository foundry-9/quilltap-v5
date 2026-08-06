/**
 * P4.9H2A embedding-profiles MANAGEMENT fixture builder — materializes the
 * committed `embedding-profiles-{main,mount}.db` family from the plaintext spec
 * (`ep-mgmt.json`), using v4's OWN `ensureCollection` DDL + repositories so the
 * schema is production-identical by construction. Serves the routes, reapply,
 * and dedup differentials.
 *
 * Contents:
 *   - five embedding_profiles (OPENAI default + apiKey + tags; BUILTIN + a fitted
 *     tfidf_vocabulary; an OPENAI truncation-bearing; an OLLAMA truncation-free;
 *     a small-dim Reapply Target with trunc=4), one api_key, two tags;
 *   - embedding_status rows for the default profile (embeddingStats + the
 *     matrix invalidate);
 *   - 8-dim vectors in every main embedding table (memories / vector_entries /
 *     conversation_chunks / help_docs) and mount `doc_mount_chunks` (reapply
 *     slices 8->4); 4-dim near-duplicate + distinct memories for dedup grouping.
 *
 * Run from the v4 checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server   (or a pinned worktree)
 *   QT_EP_MGMT_MAIN=<v5>/crates/quilltap-web/tests/fixtures/embedding-profiles-main.db \
 *   QT_EP_MGMT_MOUNT=<v5>/crates/quilltap-web/tests/fixtures/embedding-profiles-mount.db \
 *     $N/npx tsx <v5>/harness/oracle/fixtures/build-ep-mgmt-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  secondUserId: string;
  apiKey: { id: string; label: string; provider: string; keyValue: string };
  tags: Array<{ id: string; name: string; nameLower: string }>;
  characters: Array<{ id: string; name: string }>;
  embeddingProfiles: Array<{
    id: string;
    name: string;
    provider: string;
    apiKeyId: string | null;
    baseUrl: string | null;
    modelName: string;
    dimensions: number | null;
    truncateToDimensions: number | null;
    normalizeL2: boolean;
    isDefault: boolean;
    tags: string[];
  }>;
  vocabulary: {
    id: string;
    profileId: string;
    vocabulary: string;
    idf: string;
    avgDocLength: number;
    vocabularySize: number;
    includeBigrams: boolean;
    fittedAt: string;
  };
  statusRows: Array<{
    id: string;
    entityType: string;
    entityId: string;
    profileId: string;
    status: string;
  }>;
  reapplyMemories: Array<{ id: string; characterId: string; content: string; summary: string; embedding: number[] }>;
  dedupMemories: Array<{ id: string; characterId: string; content: string; summary: string; importance: number; embedding: number[] }>;
  mergeMemories: Array<{
    id: string;
    characterId: string;
    content: string;
    summary: string;
    importance: number;
    embedding: number[];
    occurredAt: string | null;
    narrativeTime: string | null;
    entities: string[];
    kind: string;
  }>;
  conversationChunks: Array<{ id: string; content: string; embedding: number[] }>;
  helpDocs: Array<{ id: string; title: string; path: string; url: string; content: string; embedding: number[] }>;
  vectorEntries: Array<{ id: string; characterId: string; embedding: number[] }>;
  docMountPoint: { id: string; name: string };
  docMountChunks: Array<{ id: string; content: string; embedding: number[] }>;
}

const CHUNK_LINK_ID = 'b0000000-0000-4000-8000-0000000000aa';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'ep-mgmt.json'), 'utf8')) as Spec;

  const outMain = process.env.QT_EP_MGMT_MAIN;
  const outMount = process.env.QT_EP_MGMT_MOUNT;
  if (!outMain || !outMount) {
    throw new Error('QT_EP_MGMT_MAIN and QT_EP_MGMT_MOUNT must point at the fixture .db files');
  }
  for (const base of [outMain, outMount]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = base + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ep-mgmt-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = outMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = outMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRawMountIndexDatabase } = await import(
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
  const { ApiKeySchema, EmbeddingProfileSchema } = await import('@/lib/schemas/types');
  const { TagSchema } = await import('@/lib/schemas/tag.types');
  const { TfidfVocabularySchema, EmbeddingStatusSchema } = await import(
    '@/lib/schemas/embedding-job.types'
  );
  const { BackgroundJobSchema } = await import('@/lib/schemas/job.types');

  await initializeDatabase();
  const repos = getRepositories();
  const TS = spec.seedTimestamp;

  // Ensure the raw-insert target tables exist (main DB).
  await ensureCollection('api_keys', ApiKeySchema as never);
  await ensureCollection('tags', TagSchema as never);
  await ensureCollection('embedding_profiles', EmbeddingProfileSchema as never);
  await ensureCollection('tfidf_vocabularies', TfidfVocabularySchema as never);
  await ensureCollection('embedding_status', EmbeddingStatusSchema as never);
  // The management routes enqueue jobs (matrix / refit / reindex / reapply); the
  // table must exist so both v4 and v5 write it and the differential can dump it.
  await ensureCollection('background_jobs', BackgroundJobSchema as never);

  const main = getRawDatabase();
  if (!main) throw new Error('main DB handle unavailable');

  // Materialize the mount-index content tables (the character vault scaffold
  // writes into them during character creation).
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
  // `gcOrphanedFileRow` deletes from `doc_mount_blobs` on every content-addressed
  // rewrite; a mount index lacking it throws on the second write. v4 creates it
  // lazily from hand-written DDL — copied verbatim (it is in fresh_schema.json).
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

  // Users.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never
  );
  await repos.users.create(
    { username: 'stranger', email: null, name: 'A Stranger' } as never,
    { id: spec.secondUserId, createdAt: TS, updatedAt: TS } as never
  );

  // API key (raw insert to pin the id — createApiKey generates its own).
  main
    .prepare(
      `INSERT INTO api_keys (id, userId, label, provider, key_value, isActive, lastUsed, createdAt, updatedAt)
       VALUES (?, ?, ?, ?, ?, 1, NULL, ?, ?)`
    )
    .run(spec.apiKey.id, spec.userId, spec.apiKey.label, spec.apiKey.provider, spec.apiKey.keyValue, TS, TS);

  // Tags.
  for (const t of spec.tags) {
    main
      .prepare(
        `INSERT INTO tags (id, userId, name, nameLower, quickHide, visualStyle, createdAt, updatedAt)
         VALUES (?, ?, ?, ?, 0, NULL, ?, ?)`
      )
      .run(t.id, spec.userId, t.name, t.nameLower, TS, TS);
  }

  // Characters.
  for (const c of spec.characters) {
    await repos.characters.create(
      { name: c.name, userId: spec.userId } as never,
      { id: c.id }
    );
  }

  // Embedding profiles.
  for (const p of spec.embeddingProfiles) {
    await repos.embeddingProfiles.create(
      {
        userId: spec.userId,
        name: p.name,
        provider: p.provider,
        apiKeyId: p.apiKeyId,
        baseUrl: p.baseUrl,
        modelName: p.modelName,
        dimensions: p.dimensions,
        truncateToDimensions: p.truncateToDimensions,
        normalizeL2: p.normalizeL2,
        isDefault: p.isDefault,
        tags: p.tags,
      } as never,
      { id: p.id, createdAt: TS, updatedAt: TS }
    );
  }

  // TF-IDF vocabulary for the BUILTIN profile.
  const v = spec.vocabulary;
  main
    .prepare(
      `INSERT INTO tfidf_vocabularies (id, profileId, userId, vocabulary, idf, avgDocLength, vocabularySize, includeBigrams, fittedAt, createdAt, updatedAt)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`
    )
    .run(
      v.id,
      v.profileId,
      spec.userId,
      v.vocabulary,
      v.idf,
      v.avgDocLength,
      v.vocabularySize,
      v.includeBigrams ? 1 : 0,
      v.fittedAt,
      TS,
      TS
    );

  // Embedding status rows for the default profile.
  for (const s of spec.statusRows) {
    main
      .prepare(
        `INSERT INTO embedding_status (id, userId, entityType, entityId, profileId, status, embeddedAt, error, createdAt, updatedAt)
         VALUES (?, ?, ?, ?, ?, ?, NULL, NULL, ?, ?)`
      )
      .run(s.id, spec.userId, s.entityType, s.entityId, s.profileId, s.status, TS, TS);
  }

  const vec = (arr: number[]) => new Float32Array(arr);

  // Memories: reapply (8-dim) + dedup (4-dim).
  for (const m of spec.reapplyMemories) {
    await repos.memories.create(
      {
        characterId: m.characterId,
        aboutCharacterId: null,
        chatId: null,
        projectId: null,
        content: m.content,
        summary: m.summary,
        occurredAt: null,
        keywords: [],
        tags: [],
        importance: 0.5,
        embedding: vec(m.embedding),
        source: 'AUTO',
        witnessedContext: null,
        sourceMessageId: null,
        lastAccessedAt: null,
        reinforcementCount: 1,
        lastReinforcedAt: null,
        relatedMemoryIds: [],
        reinforcedImportance: 0.5,
      } as never,
      { id: m.id, createdAt: TS, updatedAt: TS }
    );
  }
  for (const m of spec.dedupMemories) {
    await repos.memories.create(
      {
        characterId: m.characterId,
        aboutCharacterId: null,
        chatId: null,
        projectId: null,
        content: m.content,
        summary: m.summary,
        occurredAt: null,
        keywords: [],
        tags: [],
        importance: m.importance,
        embedding: vec(m.embedding),
        source: 'AUTO',
        witnessedContext: null,
        sourceMessageId: null,
        lastAccessedAt: null,
        reinforcementCount: 1,
        lastReinforcedAt: null,
        relatedMemoryIds: [],
        reinforcedImportance: m.importance,
      } as never,
      { id: m.id, createdAt: TS, updatedAt: TS }
    );
  }
  // Merge-path memories (4-dim) — episodic-column-bearing so a survivor whose
  // content is REWRITTEN with novel-detail footnotes exercises occurredAt /
  // entities / kind in the memories dump.
  for (const m of spec.mergeMemories) {
    await repos.memories.create(
      {
        characterId: m.characterId,
        aboutCharacterId: null,
        chatId: null,
        projectId: null,
        content: m.content,
        summary: m.summary,
        occurredAt: m.occurredAt,
        narrativeTime: m.narrativeTime,
        entities: m.entities,
        kind: m.kind,
        keywords: [],
        tags: [],
        importance: m.importance,
        embedding: vec(m.embedding),
        source: 'AUTO',
        witnessedContext: null,
        sourceMessageId: null,
        lastAccessedAt: null,
        reinforcementCount: 1,
        lastReinforcedAt: null,
        relatedMemoryIds: [],
        reinforcedImportance: m.importance,
      } as never,
      { id: m.id, createdAt: TS, updatedAt: TS }
    );
  }

  // Conversation chunks (8-dim). `chatId` is non-null in the schema; a
  // placeholder id suffices (the reapply walk selects by `embedding IS NOT NULL`
  // and never joins chats; no FK is enforced on this schema-generated table).
  const FAKE_CHAT_ID = 'd0000000-0000-4000-8000-0000000000ff';
  for (const c of spec.conversationChunks) {
    await repos.conversationChunks.create(
      {
        chatId: FAKE_CHAT_ID,
        interchangeIndex: 0,
        content: c.content,
        participantNames: [],
        messageIds: [],
        embedding: vec(c.embedding),
      } as never,
      { id: c.id, createdAt: TS, updatedAt: TS }
    );
  }

  // Help docs (8-dim).
  for (const d of spec.helpDocs) {
    await repos.helpDocs.create(
      {
        title: d.title,
        path: d.path,
        url: d.url,
        content: d.content,
        contentHash: d.id,
        embedding: vec(d.embedding),
      } as never,
      { id: d.id, createdAt: TS, updatedAt: TS }
    );
  }

  // Vector entries (8-dim) via the VectorIndicesRepository (it owns the table).
  const vectors = new VectorIndicesRepository();
  for (const e of spec.vectorEntries) {
    await vectors.saveMeta(e.characterId, e.embedding.length);
    await vectors.addEntry({ id: e.id, characterId: e.characterId, embedding: vec(e.embedding) });
  }

  // Mount point + chunks (8-dim).
  await repos.docMountPoints.create(
    {
      name: spec.docMountPoint.name,
      basePath: '',
      mountType: 'database',
      storeType: 'documents',
      includePatterns: ['*.md'],
      excludePatterns: [],
      enabled: true,
      scanStatus: 'idle',
      conversionStatus: 'idle',
      fileCount: 0,
      chunkCount: 0,
      totalSizeBytes: 0,
    } as never,
    { id: spec.docMountPoint.id, createdAt: TS, updatedAt: TS }
  );
  for (const c of spec.docMountChunks) {
    await repos.docMountChunks.create(
      {
        linkId: CHUNK_LINK_ID,
        mountPointId: spec.docMountPoint.id,
        chunkIndex: 0,
        content: c.content,
        tokenCount: c.content.split(/\s+/).length,
        headingContext: null,
        embedding: vec(c.embedding),
      } as never,
      { id: c.id, createdAt: TS, updatedAt: TS }
    );
  }

  await closeDatabase();

  process.stderr.write(
    `built embedding-profiles mgmt fixture: ${outMain} / ${outMount} ` +
      `(${spec.embeddingProfiles.length} profiles, ${spec.reapplyMemories.length + spec.dedupMemories.length + spec.mergeMemories.length} memories)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`ep-mgmt fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
