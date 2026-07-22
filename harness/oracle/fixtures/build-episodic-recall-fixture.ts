/**
 * The committed episodic-recall fixture builder (P4.d13 unit 7) — the shared
 * test-pepper substrate for the NEW `recall_replay_equivalence` (tier-3
 * mocked-distill) and `vault_conv_search_equivalence` (the unit-4 timeRange
 * staging) differentials.
 *
 * Bakes, via v4's REAL repositories + the REAL builtin TF-IDF vectorizer (all
 * ids/timestamps pinned so both differential sides read identical rows):
 *   - one user + a cheap connection profile + api key + chat_settings
 *     (cheapLLMSettings → the connection: the recall-replay route's anchor
 *     resolution), and one BUILTIN default embedding profile with a fitted
 *     tfidf_vocabularies row (deterministic embeddings BOTH sides),
 *   - ELOWEN (llm) — the searched character, with a memory corpus mixing
 *     `kind: 'episodic'` rows (occurredAt in/out of the test window,
 *     narrativeTime, entities, targeting-tag keywords) and plain semantic
 *     rows, including:
 *       · REMEMBERED — in-window episodic, `past` tag, about the quay,
 *       · WHISPERED  — in-window episodic in the chat's recall-history ring
 *         (the suspension arm: old path repeat↓, new path suspended),
 *       · OUTSIDE    — episodic, out of window (`past` tag),
 *       · ABOUTPIP   — aboutCharacterId = Pip's character (participant boost),
 *       · PLAIN      — semantic, no occurredAt (createdAt window fallback),
 *       · RESCUE     — contains the verbatim place name, embedding COLUMN set
 *         but its vector-index entry DELETED post-embed (the entity-anchor
 *         searchByContent union is the only way it enters the pool),
 *   - PIPC (user-controlled) — the "user" participant's character,
 *   - one chat with 5 partitionable turns (USER + ASSISTANT, dated createdAt —
 *     the replay clock reads the turn's own date), participants
 *     [Pip (controlledBy user), Elowen (llm, connectionProfileId)], and
 *     `commonplaceRecallHistory = { turns: [[WHISPERED]] }`,
 *   - THREE dated vault conversation summaries in ELOWEN's vault (via v4's
 *     REAL `writeConversationSummaryToVaults`) + one-chunk-per-summary
 *     `doc_mount_chunks` rows embedded with the SAME fitted vectorizer (the
 *     unit-4 vault search substrate).
 *
 * Regenerate (Node 24, from the v4 checkout) + re-copy the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_ER_MAIN=$W/crates/quilltap-web/tests/fixtures/episodic-recall-main.db \
 *   QT_FIXTURE_ER_MOUNT=$W/crates/quilltap-web/tests/fixtures/episodic-recall-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-episodic-recall-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedMemory {
  id: string;
  content: string;
  summary: string;
  importance: number;
  keywords?: string[];
  occurredAt?: string | null;
  narrativeTime?: string | null;
  kind?: string;
  entities?: string[];
  aboutCharacterId?: string | null;
  chatId?: string | null;
  /** True → delete the vector-index entry after embedding (the rescue row). */
  unindexed?: boolean;
}
interface SeedSummary {
  chatId: string;
  chatTitle: string;
  summary: string;
  firstMessageAt: string | null;
  lastMessageAt: string | null;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  elowenId: string;
  pipCharacterId: string;
  connId: string;
  apiKeyId: string;
  embeddingProfileId: string;
  chatSettingsId: string;
  chatId: string;
  partElowen: string;
  partPip: string;
  memories: SeedMemory[];
  summaries: SeedSummary[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'episodic-recall.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_ER_MAIN;
  const mountOut = process.env.QT_FIXTURE_ER_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_ER_MAIN and QT_FIXTURE_ER_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-episodic-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { EmbeddingProfileSchema, ApiKeySchema } = await import('@/lib/schemas/profile.types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  const { initializePlugins } = await import('@/lib/startup/plugin-initialization');
  await initializePlugins();
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('embedding_profiles', EmbeddingProfileSchema);
  await ensureCollection('api_keys', ApiKeySchema);

  // Materialize the mount-index content tables (the vault scaffold + the
  // summary writes need them).
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  const mainDb = getRawDatabase();
  if (!mainDb) throw new Error('main DB handle unavailable');
  mainDb.exec(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );

  const repos = getRepositories();
  const TS = spec.seedTimestamp;

  // 1. User + api key + cheap connection profile + chat_settings.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );
  const { getCollection } = await import('@/lib/database/manager');
  const apiKeyCol = await getCollection('api_keys');
  await apiKeyCol.insertOne(
    ApiKeySchema.parse({
      id: spec.apiKeyId,
      userId: spec.userId,
      label: 'mock-key',
      provider: 'OPENAI_COMPATIBLE',
      key_value: 'sk-synthetic-mock-key',
      isActive: true,
      lastUsed: null,
      createdAt: TS,
      updatedAt: TS,
    }) as never,
  );
  await repos.connections.create(
    {
      userId: spec.userId,
      name: 'Local Mock',
      provider: 'OPENAI_COMPATIBLE',
      modelName: 'mock-model',
      baseUrl: 'http://127.0.0.1:1/v1',
      apiKeyId: spec.apiKeyId,
      isCheap: true,
    } as never,
    { id: spec.connId, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.chatSettings.create(
    {
      userId: spec.userId,
      cheapLLMSettings: {
        strategy: 'USER_DEFINED',
        defaultCheapProfileId: spec.connId,
        userDefinedProfileId: spec.connId,
        fallbackToLocal: false,
      },
    } as never,
    { id: spec.chatSettingsId, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. BUILTIN default embedding profile.
  await repos.embeddingProfiles.create(
    {
      userId: spec.userId,
      name: 'Built-in TF-IDF',
      provider: 'BUILTIN',
      apiKeyId: null,
      baseUrl: null,
      modelName: 'tfidf-bm25-v1',
      dimensions: null,
      truncateToDimensions: null,
      normalizeL2: true,
      isDefault: true,
      tags: [],
    } as never,
    { id: spec.embeddingProfileId, createdAt: TS, updatedAt: TS } as never,
  );

  // 3. Characters (Elowen gets the vault; Pip is the user's own character).
  const mkChar = async (id: string, name: string, controlledBy: string) =>
    repos.characters.create(
      {
        name,
        userId: spec.userId,
        title: null,
        description: `${name} of the quayside.`,
        personality: null,
        aliases: [],
        controlledBy,
        talkativeness: 0.5,
        defaultConnectionProfileId: spec.connId,
        isFavorite: false,
        npc: false,
        tags: [],
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  await mkChar(spec.elowenId, 'Elowen', 'llm');
  await mkChar(spec.pipCharacterId, 'Pip', 'user');

  // 4. The chat: 5 dated turns, the recall-history ring, participants.
  const WHISPERED = spec.memories.find((m) => m.id.endsWith('00a2'));
  if (!WHISPERED) throw new Error('whispered memory missing from spec');
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'The Quayside Voyage',
      participants: [
        {
          id: spec.partPip,
          type: 'CHARACTER',
          characterId: spec.pipCharacterId,
          controlledBy: 'user',
          status: 'active',
          createdAt: TS,
          updatedAt: TS,
        },
        {
          id: spec.partElowen,
          type: 'CHARACTER',
          characterId: spec.elowenId,
          controlledBy: 'llm',
          status: 'active',
          connectionProfileId: spec.connId,
          createdAt: TS,
          updatedAt: TS,
        },
      ],
      chatType: 'salon',
      contextSummary: null,
      tags: [],
      commonplaceRecallHistory: { turns: [[WHISPERED.id]] },
    } as never,
    { id: spec.chatId, createdAt: TS, updatedAt: TS } as never,
  );
  const msg = (
    n: number,
    role: 'USER' | 'ASSISTANT',
    content: string,
    createdAt: string,
  ) => ({
    id: `d1000000-0000-4000-8000-0000000000${n.toString(16).padStart(2, '0')}`,
    type: 'message',
    role,
    content,
    participantId: role === 'USER' ? spec.partPip : spec.partElowen,
    createdAt,
    attachments: [],
    ...(role === 'ASSISTANT' ? { provider: 'OPENAI_COMPATIBLE', modelName: 'mock-model' } : {}),
  });
  await repos.chats.addMessages(spec.chatId, [
    msg(1, 'USER', 'Shall we walk the quay this morning?', '2026-05-20T09:00:00.000Z'),
    msg(2, 'ASSISTANT', 'The gulls are loud over Gullwing Quay today.', '2026-05-20T09:01:00.000Z'),
    msg(3, 'USER', 'Tell me about the sextant you bought.', '2026-06-02T10:00:00.000Z'),
    msg(4, 'ASSISTANT', 'A brass sextant, from the quay market.', '2026-06-02T10:01:00.000Z'),
    msg(5, 'USER', 'What did the harbormaster say about the tide ledger?', '2026-06-05T11:00:00.000Z'),
    msg(6, 'ASSISTANT', 'That the ledger page went missing in the storm.', '2026-06-05T11:01:00.000Z'),
    msg(7, 'USER', 'Remember when we visited Gullwing Quay last week?', '2026-06-10T12:00:00.000Z'),
    msg(8, 'ASSISTANT', 'I remember the dawn light on the water.', '2026-06-10T12:01:00.000Z'),
    msg(9, 'USER', 'And what should we do tomorrow?', '2026-06-11T09:00:00.000Z'),
  ] as never);

  // 5. Memories.
  const seedMemory = async (m: SeedMemory) =>
    repos.memories.create(
      {
        characterId: spec.elowenId,
        aboutCharacterId: m.aboutCharacterId ?? null,
        chatId: m.chatId ?? null,
        projectId: null,
        content: m.content,
        summary: m.summary,
        keywords: m.keywords ?? [],
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
        occurredAt: m.occurredAt ?? null,
        narrativeTime: m.narrativeTime ?? null,
        kind: m.kind ?? 'semantic',
        entities: m.entities ?? [],
      } as never,
      { id: m.id, createdAt: TS, updatedAt: TS },
    );
  for (const m of spec.memories) await seedMemory(m);

  // 6. Fit the BUILTIN vocabulary over the corpus, embed every memory (column +
  // vector store) through v4's REAL path, then DELETE the rescue rows' index
  // entries (column embedding kept — the searchByContent union re-scores it).
  const all = await repos.memories.findByCharacterId(spec.elowenId);
  const corpus = all.map((m) => `${m.summary}\n\n${m.content}`);
  const plugin = await import('@/plugins/dist/qtap-plugin-builtin-embeddings');
  const TfIdfVectorizer = (plugin as { TfIdfVectorizer: new (b: boolean) => any }).TfIdfVectorizer;
  const vectorizer = new TfIdfVectorizer(true);
  vectorizer.fitCorpus(corpus);
  const state = vectorizer.getState();
  if (!state) throw new Error('TF-IDF fit produced no state');
  await repos.tfidfVocabularies.upsertByProfileId(spec.embeddingProfileId, {
    profileId: spec.embeddingProfileId,
    userId: spec.userId,
    vocabulary: JSON.stringify(state.vocabulary),
    idf: JSON.stringify(state.idf),
    avgDocLength: state.avgDocLength,
    vocabularySize: state.vocabularySize,
    includeBigrams: state.includeBigrams,
    fittedAt: state.fittedAt,
  } as never);

  const { generateMissingEmbeddings } = await import('@/lib/memory/memory-service');
  const embResult = await generateMissingEmbeddings(spec.elowenId, {
    userId: spec.userId,
    embeddingProfileId: spec.embeddingProfileId,
    batchSize: 100,
  });

  let unindexed = 0;
  for (const m of spec.memories) {
    if (!m.unindexed) continue;
    // The per-embedding rows live in `vector_entries` (keyed by the memory id);
    // `vector_indices` is the per-character metadata row and stays.
    mainDb.prepare('DELETE FROM "vector_entries" WHERE "id" = ?').run(m.id);
    unindexed++;
  }

  // 7. Dated vault conversation summaries (unit 4's substrate) + one chunk per
  // summary embedded with the SAME fitted vectorizer.
  const { writeConversationSummaryToVaults } = await import(
    '@/lib/file-storage/conversation-summary-vault-bridge'
  );
  for (const s of spec.summaries) {
    await writeConversationSummaryToVaults({
      chatId: s.chatId,
      chatTitle: s.chatTitle,
      summary: s.summary,
      summaryGeneration: 1,
      participantCharacterIds: [spec.elowenId],
      messageCount: 4,
      firstMessageAt: s.firstMessageAt,
      lastMessageAt: s.lastMessageAt,
      updatedAt: TS,
    } as never);
  }
  // Chunk each summary doc (the write reindexes links but chunk EMBEDDING is a
  // background job — bake deterministic chunks directly).
  const elowenRaw = await repos.characters.findByIdRaw(spec.elowenId);
  const vaultId = elowenRaw?.characterDocumentMountPointId as string | null;
  if (!vaultId) throw new Error('Elowen vault not minted');
  const links = midb
    .prepare(
      'SELECT "id", "fileId", "relativePath" FROM "doc_mount_file_links" WHERE "mountPointId" = ? AND "relativePath" LIKE ?',
    )
    .all(vaultId, 'Conversation Summaries/%') as Array<{
    id: string;
    fileId: string;
    relativePath: string;
  }>;
  const { DocMountChunksRepository } = await import(
    '@/lib/database/repositories/doc-mount-chunks.repository'
  );
  const chunksRepo = new DocMountChunksRepository();
  let chunkN = 0;
  for (const link of links) {
    const doc = midb
      .prepare('SELECT "content" FROM "doc_mount_documents" WHERE "fileId" = ?')
      .get(link.fileId) as { content: string } | undefined;
    if (!doc) throw new Error(`summary document missing for ${link.relativePath}`);
    const vec = vectorizer.transform(doc.content) as number[];
    await chunksRepo.create(
      {
        linkId: link.id,
        mountPointId: vaultId,
        chunkIndex: 0,
        content: doc.content,
        tokenCount: doc.content.length,
        headingContext: null,
        embedding: new Float32Array(vec),
      } as never,
      {
        id: `bc000000-0000-4000-8000-0000000000${(++chunkN).toString().padStart(2, '0')}`,
        createdAt: TS,
        updatedAt: TS,
      },
    );
  }

  // Materialize tables both differential sides read/write.
  await repos.backgroundJobs.findByUserId(spec.userId, 'PENDING');

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built episodic-recall fixture: main=${mainOut} mount=${mountOut} ` +
      `(${all.length} memories [${embResult.processed} embedded, ${unindexed} de-indexed], ` +
      `${links.length} dated summaries chunked, vocab ${state.vocabularySize})\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`episodic-recall fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
