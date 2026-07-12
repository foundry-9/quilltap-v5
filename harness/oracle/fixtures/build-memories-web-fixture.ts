/**
 * The committed memories server-surface fixture builder (P4.6s deliverable #3) —
 * the shared test-pepper substrate for `memories_routes_equivalence` AND the
 * sibling P4.6t Commonplace-Book-SPA Playwright e2e (fixture-guarded on
 * `memories-main.db`).
 *
 * Bakes, via v4's REAL repositories + the REAL builtin TF-IDF vectorizer (all
 * ids/timestamps pinned so both differential sides read identical rows — reads
 * diff with no remap; only the CREATE/REINFORCE/queue/regenerate mutations mint
 * fresh ids/jobs, remapped or blanked by the harness):
 *   - one user (the FIXTURE_USER; the web e2e rewrites it to SINGLE_USER_ID),
 *     one connection profile + api key, one chat_settings row (cheapLLMSettings
 *     → the connection, autoHousekeepingSettings default), and one BUILTIN
 *     default embedding profile with a fitted tfidf_vocabularies row,
 *   - THREE characters:
 *       MNEMO — the rich character: ≥40 memories (pagination + housekeeping caps),
 *               varied importance/source/keywords, a tagged subset (tagDetails),
 *               a near-duplicate anchor (gate REINFORCE), a related pair
 *               (relatedMemoryIds unlink scrub + related expansion), most with
 *               builtin embeddings + a handful WITHOUT (embeddingStatus / backfill
 *               / text-fallback), plus AUTO memories carrying sourceMessageId into
 *               a salon chat's swipe group (byMessage),
 *       ORLA  — a few memories (character-counts sort middle),
 *       PIP   — ZERO memories (counts-sort tail + embeddingStatus 100%/empty),
 *   - TWO chats owned by the user:
 *       CHAT_SALON — salon type, a USER + two swipe-sibling ASSISTANT messages
 *                    (byMessage swipe-group expansion); MNEMO memories link to them,
 *       CHAT_AUTO  — autonomous type, two non-silent ASSISTANT messages
 *                    (queue-memories autonomous branch), no USER messages.
 *
 * Regenerate (Node 24, from the v4 checkout) + re-copy the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MEM_MAIN=$W/crates/quilltap-web/tests/fixtures/memories-main.db \
 *   QT_FIXTURE_MEM_MOUNT=$W/crates/quilltap-web/tests/fixtures/memories-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-memories-web-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
}

// ── Pinned entity ids (shared verbatim with the Rust differential) ──────────
const MNEMO = 'b1000000-0000-4000-8000-000000000001';
const ORLA = 'b1000000-0000-4000-8000-000000000002';
const PIP = 'b1000000-0000-4000-8000-000000000003';

const CONN = 'c0000001-0000-4000-8000-000000000001';
const APIKEY = 'a0000001-0000-4000-8000-000000000001';
const EP_BUILTIN = 'b8000000-0000-4000-8000-000000000001';

const TAG_A = 'b5000000-0000-4000-8000-000000000001'; // Lore (plain)
const TAG_B = 'b5000000-0000-4000-8000-000000000002'; // People (visualStyle)

// Special MNEMO memories (rest are generated in a loop).
const MEM_NEARDUP = 'b2000000-0000-4000-8000-0000000000d0'; // gate REINFORCE anchor
const MEM_REL_A = 'b2000000-0000-4000-8000-0000000000e0'; // related pair A
const MEM_REL_B = 'b2000000-0000-4000-8000-0000000000e1'; // related pair B
const MEM_TAGGED_1 = 'b2000000-0000-4000-8000-0000000000a0'; // tags [TAG_A]
const MEM_TAGGED_2 = 'b2000000-0000-4000-8000-0000000000a1'; // tags [TAG_A, TAG_B]
const MEM_SW1 = 'b2000000-0000-4000-8000-0000000000f0'; // sourceMessageId = MSG_S2A
const MEM_SW2 = 'b2000000-0000-4000-8000-0000000000f1'; // sourceMessageId = MSG_S2B
// Memories WITHOUT embeddings (seeded after the reindex).
const MEM_NOEMB = (i: number) => `b2000000-0000-4000-8000-0000000000b${i}`;

// Chats + messages.
const CHAT_SALON = 'c1000000-0000-4000-8000-000000000001';
const CHAT_AUTO = 'c1000000-0000-4000-8000-000000000002';
const MSG_S1 = 'd1000000-0000-4000-8000-000000000001'; // USER
const MSG_S2A = 'd1000000-0000-4000-8000-000000000002'; // ASSISTANT swipe a
const MSG_S2B = 'd1000000-0000-4000-8000-000000000003'; // ASSISTANT swipe b
const SWIPE_GROUP = 'd9000000-0000-4000-8000-000000000001';
const MSG_A1 = 'd1000000-0000-4000-8000-0000000000a1'; // autonomous ASSISTANT
const MSG_A2 = 'd1000000-0000-4000-8000-0000000000a2'; // autonomous ASSISTANT

const PART_ARIA = 'e1000000-0000-4000-8000-000000000001';
const PART_USER = 'e1000000-0000-4000-8000-000000000002';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'memories-web.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_MEM_MAIN;
  const mountOut = process.env.QT_FIXTURE_MEM_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_MEM_MAIN and QT_FIXTURE_MEM_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-mem-web-fixture-'));
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
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { EmbeddingProfileSchema } = await import('@/lib/schemas/profile.types');
  const { TagSchema } = await import('@/lib/schemas/tag.types');
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
  // Register the provider plugins (incl. the BUILTIN embeddings provider) so the
  // real builtin embedding path resolves at generate time.
  const { initializePlugins } = await import('@/lib/startup/plugin-initialization');
  await initializePlugins();
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('embedding_profiles', EmbeddingProfileSchema);
  await ensureCollection('tags', TagSchema);

  // A minimal mount-index (the memories surface never touches it, but the Db
  // opens with the standard 3-partition shape, so materialize the tables empty).
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

  // The instance_settings key/value table (the recall / extraction-limits /
  // extraction-concurrency config writes target it; reads default gracefully but
  // an INSERT into a missing table throws).
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const mainDb = getRawDatabase();
  if (!mainDb) throw new Error('main DB handle unavailable');
  mainDb.exec(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );

  const repos = getRepositories();
  const TS = spec.seedTimestamp;

  // 1. User + api key + connection profile.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );
  const { ApiKeySchema } = await import('@/lib/schemas/profile.types');
  await ensureCollection('api_keys', ApiKeySchema);
  const { getCollection } = await import('@/lib/database/manager');
  const apiKeyCol = await getCollection('api_keys');
  await apiKeyCol.insertOne(
    ApiKeySchema.parse({
      id: APIKEY,
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
      apiKeyId: APIKEY,
      isCheap: true,
    } as never,
    { id: CONN, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. chat_settings — cheapLLMSettings points the default cheap profile at the
  // connection (queue-memories + regenerate-all resolve it); autoHousekeeping
  // omitted so the config GET default-injects.
  await repos.chatSettings.create(
    {
      userId: spec.userId,
      cheapLLMSettings: {
        strategy: 'USER_DEFINED',
        defaultCheapProfileId: CONN,
        userDefinedProfileId: CONN,
        fallbackToLocal: false,
      },
    } as never,
    { id: 'c8000000-0000-4000-8000-000000000001', createdAt: TS, updatedAt: TS } as never,
  );

  // 3. BUILTIN default embedding profile.
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
    { id: EP_BUILTIN, createdAt: TS, updatedAt: TS } as never,
  );

  // 4. Tags — TAG_A plain, TAG_B with a visualStyle (tagDetails enrichment).
  await repos.tags.create(
    { userId: spec.userId, name: 'Lore', nameLower: 'lore', quickHide: false } as never,
    { id: TAG_A, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.tags.create(
    {
      userId: spec.userId,
      name: 'People',
      nameLower: 'people',
      quickHide: false,
      visualStyle: {
        foregroundColor: '#1f2937',
        backgroundColor: '#fde68a',
        emojiOnly: false,
        bold: true,
        italic: false,
        strikethrough: false,
      },
    } as never,
    { id: TAG_B, createdAt: TS, updatedAt: TS } as never,
  );

  // 5. Characters.
  const mkChar = async (id: string, name: string, controlledBy: string) =>
    repos.characters.create(
      {
        name,
        userId: spec.userId,
        title: null,
        description: `${name} the test character.`,
        personality: null,
        aliases: [],
        controlledBy,
        talkativeness: 0.5,
        defaultConnectionProfileId: CONN,
        isFavorite: false,
        npc: false,
        tags: [],
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  await mkChar(MNEMO, 'Mnemo', 'llm');
  await mkChar(ORLA, 'Orla', 'llm');
  await mkChar(PIP, 'Pip', 'user');

  // 6. Chats — a salon chat (USER + two swipe-sibling ASSISTANT messages) and an
  // autonomous chat (two non-silent ASSISTANT messages, no USER).
  const mkParticipant = (id: string, characterId: string, controlledBy: string) => ({
    id,
    type: 'CHARACTER',
    characterId,
    controlledBy,
    status: 'active',
    connectionProfileId: CONN,
    createdAt: TS,
    updatedAt: TS,
  });
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Salon Chat',
      participants: [
        mkParticipant(PART_ARIA, MNEMO, 'llm'),
        mkParticipant(PART_USER, PIP, 'user'),
      ],
      chatType: 'salon',
      contextSummary: null,
      tags: [],
    } as never,
    { id: CHAT_SALON, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.chats.addMessages(CHAT_SALON, [
    {
      id: MSG_S1,
      type: 'message',
      role: 'USER',
      content: 'Tell me about the lighthouse keeper and the storm.',
      participantId: PART_USER,
      createdAt: '2026-03-02T00:00:00.000Z',
      attachments: [],
    },
    {
      id: MSG_S2A,
      type: 'message',
      role: 'ASSISTANT',
      content: 'The keeper Alden kept the beacon lit through the tempest.',
      participantId: PART_ARIA,
      swipeGroupId: SWIPE_GROUP,
      createdAt: '2026-03-02T00:01:00.000Z',
      attachments: [],
      provider: 'OPENAI_COMPATIBLE',
      modelName: 'mock-model',
    },
    {
      id: MSG_S2B,
      type: 'message',
      role: 'ASSISTANT',
      content: 'Alden the keeper braved the gale to keep the lantern burning.',
      participantId: PART_ARIA,
      swipeGroupId: SWIPE_GROUP,
      createdAt: '2026-03-02T00:02:00.000Z',
      attachments: [],
      provider: 'OPENAI_COMPATIBLE',
      modelName: 'mock-model',
    },
  ] as never);
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Autonomous Room',
      participants: [mkParticipant('e2000000-0000-4000-8000-000000000001', MNEMO, 'llm')],
      chatType: 'autonomous',
      contextSummary: null,
      tags: [],
    } as never,
    { id: CHAT_AUTO, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.chats.addMessages(CHAT_AUTO, [
    {
      id: MSG_A1,
      type: 'message',
      role: 'ASSISTANT',
      content: 'Mnemo muses alone about the tide charts.',
      participantId: 'e2000000-0000-4000-8000-000000000001',
      createdAt: '2026-03-03T00:00:00.000Z',
      attachments: [],
      provider: 'OPENAI_COMPATIBLE',
      modelName: 'mock-model',
    },
    {
      id: MSG_A2,
      type: 'message',
      role: 'ASSISTANT',
      content: 'Mnemo records the northern currents at dusk.',
      participantId: 'e2000000-0000-4000-8000-000000000001',
      createdAt: '2026-03-03T00:01:00.000Z',
      attachments: [],
      provider: 'OPENAI_COMPATIBLE',
      modelName: 'mock-model',
    },
  ] as never);

  // 7. Memories. A helper that pins id/timestamps and defaults the server-only
  // fields the repo requires.
  type MemSeed = {
    id: string;
    characterId: string;
    content: string;
    summary: string;
    keywords?: string[];
    tags?: string[];
    importance: number;
    source: 'AUTO' | 'MANUAL';
    chatId?: string | null;
    sourceMessageId?: string | null;
    relatedMemoryIds?: string[];
    reinforcementCount?: number;
    reinforcedImportance?: number;
    witnessedContext?: string | null;
    createdAt?: string;
  };
  const seedMemory = async (m: MemSeed) => {
    const importance = m.importance;
    await repos.memories.create(
      {
        characterId: m.characterId,
        aboutCharacterId: null,
        chatId: m.chatId ?? null,
        projectId: null,
        content: m.content,
        summary: m.summary,
        keywords: m.keywords ?? [],
        tags: m.tags ?? [],
        importance,
        embedding: null,
        source: m.source,
        witnessedContext: m.witnessedContext ?? null,
        sourceMessageId: m.sourceMessageId ?? null,
        lastAccessedAt: null,
        reinforcementCount: m.reinforcementCount ?? 1,
        lastReinforcedAt: null,
        relatedMemoryIds: m.relatedMemoryIds ?? [],
        reinforcedImportance: m.reinforcedImportance ?? importance,
      } as never,
      { id: m.id, createdAt: m.createdAt ?? TS, updatedAt: TS },
    );
  };

  // 40 generated MNEMO filler memories (pagination + housekeeping caps + counts).
  // Content pulls from a small nautical vocabulary so the fitted TF-IDF has real
  // overlap with the search query.
  const NOUNS = ['harbor', 'beacon', 'compass', 'anchor', 'cabin', 'sail', 'tide', 'gull'];
  const VERBS = ['charts', 'mends', 'guards', 'hoists', 'watches', 'signals'];
  for (let i = 0; i < 40; i++) {
    const noun = NOUNS[i % NOUNS.length];
    const verb = VERBS[i % VERBS.length];
    await seedMemory({
      id: `b2000000-0000-4000-8000-1000000000${(i + 10).toString().padStart(2, '0')}`,
      characterId: MNEMO,
      content: `Mnemo ${verb} the ${noun} beside the quiet harbor at dawn number ${i}.`,
      summary: `Note ${i}: the ${noun}.`,
      keywords: [noun, verb],
      importance: Math.round((0.1 + (i % 9) * 0.1) * 100) / 100,
      source: i % 2 === 0 ? 'AUTO' : 'MANUAL',
      // Stagger createdAt so the default createdAt-desc sort is observable.
      createdAt: `2026-02-${(i % 27 + 1).toString().padStart(2, '0')}T00:00:00.000Z`,
    });
  }

  // Tagged memories (tagDetails enrichment on list/get/search).
  await seedMemory({
    id: MEM_TAGGED_1,
    characterId: MNEMO,
    content: 'The old captain Alden mentored every apprentice at the lighthouse.',
    summary: 'Captain Alden mentors apprentices.',
    keywords: ['captain', 'alden', 'lighthouse'],
    tags: [TAG_A],
    importance: 0.8,
    source: 'MANUAL',
  });
  await seedMemory({
    id: MEM_TAGGED_2,
    characterId: MNEMO,
    content: 'Alden the lighthouse keeper trusts only the harbor master.',
    summary: 'Alden trusts the harbor master.',
    keywords: ['alden', 'harbor', 'trust'],
    tags: [TAG_A, TAG_B],
    importance: 0.75,
    source: 'MANUAL',
  });

  // Near-duplicate anchor — the create differential posts this exact content, so
  // the gate finds this at cosine ~1.0 and REINFORCEs it (no new row).
  await seedMemory({
    id: MEM_NEARDUP,
    characterId: MNEMO,
    content: 'Mnemo always double-checks the barometer before hoisting the mainsail.',
    summary: 'Mnemo checks the barometer before the mainsail.',
    keywords: ['barometer', 'mainsail'],
    importance: 0.6,
    source: 'AUTO',
  });

  // Related pair — REL_A ↔ REL_B. Deleting REL_B scrubs REL_A.relatedMemoryIds.
  await seedMemory({
    id: MEM_REL_A,
    characterId: MNEMO,
    content: 'The smuggler cove lies east of the reef, hidden by fog.',
    summary: 'Smuggler cove east of the reef.',
    keywords: ['smuggler', 'cove', 'reef'],
    importance: 0.7,
    source: 'AUTO',
    relatedMemoryIds: [MEM_REL_B],
  });
  await seedMemory({
    id: MEM_REL_B,
    characterId: MNEMO,
    content: 'The reef hides a smuggler cove reachable only at low tide.',
    summary: 'The reef hides a cove reachable at low tide.',
    keywords: ['reef', 'smuggler', 'tide'],
    importance: 0.7,
    source: 'AUTO',
    relatedMemoryIds: [MEM_REL_A],
  });

  // Swipe-group source memories (byMessage): AUTO memories anchored to the two
  // swipe-sibling assistant messages in CHAT_SALON.
  await seedMemory({
    id: MEM_SW1,
    characterId: MNEMO,
    content: 'Keeper Alden lit the beacon through the tempest.',
    summary: 'Alden lit the beacon in the storm.',
    keywords: ['alden', 'beacon', 'storm'],
    importance: 0.65,
    source: 'AUTO',
    chatId: CHAT_SALON,
    sourceMessageId: MSG_S2A,
    witnessedContext: 'user_present',
  });
  await seedMemory({
    id: MEM_SW2,
    characterId: MNEMO,
    content: 'Alden braved the gale to keep the lantern burning.',
    summary: 'Alden kept the lantern burning in the gale.',
    keywords: ['alden', 'lantern', 'gale'],
    importance: 0.65,
    source: 'AUTO',
    chatId: CHAT_SALON,
    sourceMessageId: MSG_S2B,
    witnessedContext: 'user_present',
  });

  // ORLA — four memories (character-counts sort middle; not embedded).
  for (let i = 0; i < 4; i++) {
    await seedMemory({
      id: `b3000000-0000-4000-8000-${(i + 1).toString().padStart(12, '0')}`,
      characterId: ORLA,
      content: `Orla recalls the festival of lanterns in the ${['spring', 'summer', 'autumn', 'winter'][i]}.`,
      summary: `Orla and the ${['spring', 'summer', 'autumn', 'winter'][i]} festival.`,
      keywords: ['orla', 'festival'],
      importance: 0.5,
      source: 'AUTO',
    });
  }

  // 8. Fit the BUILTIN vocabulary over the CURRENT (embedded) MNEMO corpus, then
  // embed every current MNEMO memory through v4's REAL builtin path (both column +
  // vector store). The `no-embedding` rows are seeded AFTER, so they stay bare.
  const embedTargetMemories = await repos.memories.findByCharacterId(MNEMO);
  const corpus = embedTargetMemories.map((m) => `${m.summary}\n\n${m.content}`);
  const plugin = await import('@/plugins/dist/qtap-plugin-builtin-embeddings');
  const TfIdfVectorizer = (plugin as { TfIdfVectorizer: new (b: boolean) => any }).TfIdfVectorizer;
  const vectorizer = new TfIdfVectorizer(true); // include bigrams (the refit default)
  vectorizer.fitCorpus(corpus);
  const state = vectorizer.getState();
  if (!state) throw new Error('TF-IDF fit produced no state');
  await repos.tfidfVocabularies.upsertByProfileId(EP_BUILTIN, {
    profileId: EP_BUILTIN,
    userId: spec.userId,
    vocabulary: JSON.stringify(state.vocabulary),
    idf: JSON.stringify(state.idf),
    avgDocLength: state.avgDocLength,
    vocabularySize: state.vocabularySize,
    includeBigrams: state.includeBigrams,
    fittedAt: state.fittedAt,
  } as never);

  const { generateMissingEmbeddings } = await import('@/lib/memory/memory-service');
  const embResult = await generateMissingEmbeddings(MNEMO, {
    userId: spec.userId,
    embeddingProfileId: EP_BUILTIN,
    batchSize: 100,
  });

  // 9. Memories WITHOUT embeddings (seeded post-reindex) — backfill / text
  // fallback / embeddingStatus percent.
  for (let i = 0; i < 4; i++) {
    await seedMemory({
      id: MEM_NOEMB(i),
      characterId: MNEMO,
      content: `Mnemo jotted an unindexed marginal note number ${i} about the estuary.`,
      summary: `Unindexed note ${i}.`,
      keywords: ['estuary'],
      importance: 0.4,
      source: 'AUTO',
    });
  }

  // Materialize the empty background_jobs table (the backfill/regenerate reads
  // sweep it) so both sides read identical empty state.
  await repos.backgroundJobs.findByUserId(spec.userId, 'PENDING');

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built memories-web fixture: main=${mainOut} mount=${mountOut} ` +
      `(3 chars, ${embedTargetMemories.length + 4} MNEMO memories [${embResult.processed} embedded, ` +
      `${embResult.failed} failed], vocab size ${state.vocabularySize})\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memories-web fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
