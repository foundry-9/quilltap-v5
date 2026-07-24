/**
 * P4.9G1 fixture builder — the committed test-pepper substrate shared by the
 * Data & System server-half differentials:
 *
 *   - `system_jobs_routes_equivalence`  (tasks-queue / jobs / concurrency /
 *                                        delete-data-preview response diff)
 *   - `system_delete_data_equivalence`  (tier-2 DB-state diff after delete-all)
 *   - (extendable) the export/import/backup families as they land.
 *
 * Everything is baked via v4's REAL repos so the DDL + row shapes are
 * production-identical (the established fixture-builder pattern —
 * build-groups-projects-fixture.ts is the exemplar).
 *
 * ── THREE FILES, ONE FAMILY ──────────────────────────────────────────────────
 *   - `system-data-main.db`     — the user graph (users / characters / chats+
 *                                 messages / memories / tags / project / group /
 *                                 the three profile kinds / a user roleplay
 *                                 template / folder / chat_settings / files
 *                                 [one IMAGE + one backup .zip under /backups] /
 *                                 two text_replacement_rules) + the
 *                                 `background_jobs` corpus (one row in every
 *                                 status, incl. a PROCESSING one for the
 *                                 delete-block arm and a retry-exhausted FAILED
 *                                 for the active-set filter).
 *   - `system-data-mount.db`    — the mount-index sibling: the character vaults
 *                                 (minted by the REAL create) + the project's
 *                                 and group's official stores + one embedded
 *                                 chunk (the delete sweep of doc_mount_chunks).
 *   - `system-data-llmlogs.db`  — the llm-logs PARTITION carrying a small corpus.
 *
 * The `background_jobs` rows pin `scheduledAt` in the past (2020) and give every
 * row a DISTINCT `scheduledAt`/`createdAt` so the tasks-queue sort (priority
 * DESC, scheduledAt ASC) and the newest-first `findByUserId` are deterministic
 * and byte-identical on both sides.
 *
 * Regenerate (Node 24, from the v4 checkout) + re-copy the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SD_MAIN=$W/crates/quilltap-web/tests/fixtures/system-data-main.db \
 *   QT_FIXTURE_SD_MOUNT=$W/crates/quilltap-web/tests/fixtures/system-data-mount.db \
 *   QT_FIXTURE_SD_LLM=$W/crates/quilltap-web/tests/fixtures/system-data-llmlogs.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-system-data-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedJob {
  id: string;
  type: string;
  status: string;
  payload: Record<string, unknown>;
  priority: number;
  attempts: number;
  maxAttempts: number;
  lastError?: string;
  scheduledAt: string;
  startedAt?: string;
  completedAt?: string;
  createdAt: string;
  updatedAt: string;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  jobs: SeedJob[];
}

// ── Pinned entity ids (shared verbatim with the Rust differentials) ──────────
const LORIAN = 'a1000000-0000-4000-8000-000000000001';
const RIYA = 'a1000000-0000-4000-8000-000000000002';
const CONN = 'c0000001-0000-4000-8000-000000000001';
const APIKEY = 'a0000001-0000-4000-8000-000000000001';
const GROUP_1 = 'a2000000-0000-4000-8000-000000000001';
const PROJECT_1 = 'a3000000-0000-4000-8000-000000000001';
const RT_USER = 'a4000000-0000-4000-8000-000000000001';
const TAG_1 = 'a5000000-0000-4000-8000-000000000001';
const TAG_2 = 'a5000000-0000-4000-8000-000000000002';
const IMG_PROFILE = 'a6000000-0000-4000-8000-000000000001';
const EMB_PROFILE = 'a8000000-0000-4000-8000-000000000001';
const FOLDER_1 = 'a9000000-0000-4000-8000-000000000001';
const WARDROBE_1 = 'ac000000-0000-4000-8000-000000000001';
const MEM_1 = 'ad000000-0000-4000-8000-000000000001';
const MEM_2 = 'ad000000-0000-4000-8000-000000000002';
const CHAT_1 = 'c1000000-0000-4000-8000-000000000001';
const CHAT_2 = 'c1000000-0000-4000-8000-000000000002';
const MSG_11 = 'd1000000-0000-4000-8000-000000000001';
const MSG_12 = 'd1000000-0000-4000-8000-000000000002';
const FILE_IMG = 'f0000001-0000-4000-8000-000000000001';
const FILE_BACKUP = 'f0000002-0000-4000-8000-000000000002';
const LOG_1 = 'e5000000-0000-4000-8000-000000000001';
const LOG_2 = 'e5000000-0000-4000-8000-000000000002';
const CHAT_SETTINGS = 'ab000000-0000-4000-8000-000000000001';
const TRR_1 = 'ae000000-0000-4000-8000-000000000001';
const TRR_2 = 'ae000000-0000-4000-8000-000000000002';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'system-data.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_SD_MAIN;
  const mountOut = process.env.QT_FIXTURE_SD_MOUNT;
  const llmOut = process.env.QT_FIXTURE_SD_LLM;
  if (!mainOut || !mountOut || !llmOut) {
    throw new Error('QT_FIXTURE_SD_{MAIN,MOUNT,LLM} must point at the .db files to write');
  }
  for (const out of [mainOut, mountOut, llmOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-sd-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.SQLITE_LLM_LOGS_PATH = llmOut;
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
  const { CharacterSchema, ChatMetadataSchema, RoleplayTemplateSchema } = await import(
    '@/lib/schemas/types'
  );
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { GroupSchema } = await import('@/lib/schemas/group.types');
  const { ProjectSchema } = await import('@/lib/schemas/project.types');
  const { ImageProfileSchema, EmbeddingProfileSchema, ApiKeySchema } = await import(
    '@/lib/schemas/profile.types'
  );
  const { TagSchema } = await import('@/lib/schemas/tag.types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
    GroupCharacterMemberSchema,
    GroupDocMountLinkSchema,
    ProjectDocMountLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('chats', ChatMetadataSchema);
  await ensureCollection('groups', GroupSchema);
  await ensureCollection('projects', ProjectSchema);
  await ensureCollection('roleplay_templates', RoleplayTemplateSchema);
  await ensureCollection('image_profiles', ImageProfileSchema);
  await ensureCollection('embedding_profiles', EmbeddingProfileSchema);
  await ensureCollection('tags', TagSchema);
  await ensureCollection('api_keys', ApiKeySchema);

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
    ['group_character_members', GroupCharacterMemberSchema],
    ['group_doc_mount_links', GroupDocMountLinkSchema],
    ['project_doc_mount_links', ProjectDocMountLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  const { DocMountBlobsRepository } = await import(
    '@/lib/database/repositories/doc-mount-blobs.repository'
  );
  await new DocMountBlobsRepository().findByFileId('00000000-0000-4000-8000-000000000000');

  // The instance_settings key/value table (the concurrency get/set surface reads
  // + upserts it; the maxConcurrentJobs row is deliberately absent → default 4).
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const mainDb = getRawDatabase();
  if (!mainDb) throw new Error('main DB handle unavailable');
  mainDb.exec(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );

  const repos = getRepositories();

  // 1. User + api key + connection profile.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );
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
    } as never,
    { id: CONN, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Two characters.
  for (const [id, name] of [
    [LORIAN, 'Lorian'],
    [RIYA, 'Riya'],
  ] as Array<[string, string]>) {
    await repos.characters.create(
      {
        name,
        userId: spec.userId,
        title: null,
        description: `${name} of the fixture.`,
        personality: null,
        aliases: [],
        controlledBy: 'llm',
        talkativeness: 0.5,
        defaultConnectionProfileId: CONN,
        isFavorite: false,
        npc: false,
        tags: [TAG_1],
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  }

  // 3. Two tags.
  for (const [id, name] of [
    [TAG_1, 'Companion'],
    [TAG_2, 'Rival'],
  ] as Array<[string, string]>) {
    await repos.tags.create(
      { userId: spec.userId, name, nameLower: name.toLowerCase(), quickHide: false } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  }

  // 4. Image + embedding profiles.
  await repos.imageProfiles.create(
    {
      userId: spec.userId,
      name: 'Primary Imagery',
      provider: 'OPENAI_COMPATIBLE',
      apiKeyId: APIKEY,
      baseUrl: 'http://127.0.0.1:1/v1',
      modelName: 'mock-image-model',
      parameters: { steps: 30 },
      isDefault: true,
      isDangerousCompatible: false,
      tags: [TAG_1],
    } as never,
    { id: IMG_PROFILE, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.embeddingProfiles.create(
    {
      userId: spec.userId,
      name: 'Local Embeddings',
      provider: 'OPENAI',
      apiKeyId: null,
      baseUrl: 'http://127.0.0.1:1/v1',
      modelName: 'mock-embed',
      dimensions: 8,
      truncateToDimensions: null,
      normalizeL2: false,
      isDefault: true,
      tags: [],
    } as never,
    { id: EMB_PROFILE, createdAt: TS, updatedAt: TS } as never,
  );

  // 5. A user roleplay template.
  await repos.roleplayTemplates.create(
    {
      userId: spec.userId,
      name: 'House Style',
      description: 'The fixture template.',
      systemPrompt: 'Write plainly.',
      isBuiltIn: false,
      tags: [],
      delimiters: [],
      renderingPatterns: [],
      dialogueDetection: null,
      narrationDelimiters: '*',
    } as never,
    { id: RT_USER, createdAt: TS, updatedAt: TS } as never,
  );

  // 6. A folder.
  await repos.folders.create(
    {
      userId: spec.userId,
      path: '/notes',
      name: 'Notes',
      parentFolderId: null,
      projectId: null,
    } as never,
    { id: FOLDER_1, createdAt: TS, updatedAt: TS } as never,
  );

  // 7. chat_settings for the user.
  await repos.chatSettings.create(
    {
      userId: spec.userId,
      cheapLLMSettings: { strategy: 'PROVIDER_CHEAPEST', fallbackToLocal: false },
      autoDetectRng: false,
    } as never,
    { id: CHAT_SETTINGS, createdAt: TS, updatedAt: TS } as never,
  );

  // 8. A project (provisions an official store) + a group (ditto), one member.
  await repos.projects.create(
    {
      name: 'The Voyage',
      description: 'A fixture project.',
      instructions: null,
      allowAnyCharacter: false,
      characterRoster: [LORIAN, RIYA],
      color: '#334455',
      defaultDisabledTools: [],
      defaultDisabledToolGroups: [],
      state: {},
      backgroundDisplayMode: 'project',
    } as never,
    { id: PROJECT_1, createdAt: TS, updatedAt: TS } as never,
  );
  const group = await repos.groups.create(
    { name: 'The Crew', description: 'A fixture group.', state: {} } as never,
    { id: GROUP_1, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.groupCharacterMembers.addMember(GROUP_1, LORIAN);

  // 9. Two chats; chat 1 has two messages, chat 2 is empty.
  const mkParticipant = (id: string, characterId: string) => ({
    id,
    type: 'CHARACTER',
    characterId,
    controlledBy: 'llm',
    status: 'active',
    connectionProfileId: CONN,
    createdAt: TS,
    updatedAt: TS,
  });
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'A Lesson in Lift',
      participants: [mkParticipant('e1000000-0000-4000-8000-000000000001', LORIAN)],
      chatType: 'salon',
      contextSummary: null,
      projectId: PROJECT_1,
      tags: [TAG_1],
    } as never,
    { id: CHAT_1, createdAt: TS, updatedAt: '2026-03-02T00:00:00.000Z' } as never,
  );
  await repos.chats.addMessages(CHAT_1, [
    {
      id: MSG_11,
      type: 'message',
      role: 'USER',
      content: 'Hello there.',
      participantId: 'e1000000-0000-4000-8000-000000000001',
      createdAt: '2026-03-03T00:00:00.000Z',
      attachments: [],
    },
    {
      id: MSG_12,
      type: 'message',
      role: 'ASSISTANT',
      content: 'A fine day for a voyage.',
      participantId: 'e1000000-0000-4000-8000-000000000001',
      createdAt: '2026-03-04T00:00:00.000Z',
      attachments: [],
      provider: 'OPENAI_COMPATIBLE',
      modelName: 'mock-model',
    },
  ] as never);
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Quiet Interlude',
      participants: [mkParticipant('e1000000-0000-4000-8000-000000000002', RIYA)],
      chatType: 'salon',
      contextSummary: null,
      tags: [],
    } as never,
    { id: CHAT_2, createdAt: TS, updatedAt: '2026-03-05T00:00:00.000Z' } as never,
  );

  // 10. Two memories (one per character).
  await repos.memories.create(
    {
      characterId: LORIAN,
      aboutCharacterId: null,
      chatId: CHAT_1,
      projectId: null,
      content: 'Lorian remembers the road.',
      summary: 'the road',
      keywords: [],
      tags: [],
      importance: 0.6,
      embedding: null,
      source: 'AUTO',
      witnessedContext: null,
      sourceMessageId: null,
      lastAccessedAt: null,
      reinforcementCount: 1,
      lastReinforcedAt: null,
      relatedMemoryIds: [],
      reinforcedImportance: 0.6,
    } as never,
    { id: MEM_1, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.memories.create(
    {
      characterId: RIYA,
      aboutCharacterId: null,
      chatId: null,
      projectId: null,
      content: 'Riya recalls the harbour.',
      summary: 'the harbour',
      keywords: [],
      tags: [],
      importance: 0.4,
      embedding: null,
      source: 'MANUAL',
      witnessedContext: null,
      sourceMessageId: null,
      lastAccessedAt: null,
      reinforcementCount: 1,
      lastReinforcedAt: null,
      relatedMemoryIds: [],
      reinforcedImportance: 0.4,
    } as never,
    { id: MEM_2, createdAt: TS, updatedAt: TS } as never,
  );

  // 11. One global wardrobe item on Lorian.
  await repos.wardrobe.create(
    {
      characterId: LORIAN,
      title: 'Travelling Coat',
      description: 'A weathered coat.',
      imagePrompt: 'brown coat',
      types: ['top'],
      componentItemIds: [],
      appropriateness: null,
      isDefault: false,
      replace: false,
      migratedFromClothingRecordId: null,
      archivedAt: null,
    } as never,
    { id: WARDROBE_1, createdAt: TS, updatedAt: TS } as never,
  );

  // 12. Files — one IMAGE + one backup .zip in /backups (the delete-all backups count).
  await repos.files.create(
    {
      userId: spec.userId,
      linkedTo: [],
      tags: [],
      sha256: '1111111111111111111111111111111111111111111111111111111111111111',
      originalFilename: 'portrait.png',
      mimeType: 'image/png',
      size: 128,
      source: 'UPLOADED',
      category: 'IMAGE',
      storageKey: `${spec.userId}/portrait.png`,
      width: 16,
      height: 16,
    } as never,
    { id: FILE_IMG, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.files.create(
    {
      userId: spec.userId,
      linkedTo: [],
      tags: [],
      sha256: '2222222222222222222222222222222222222222222222222222222222222222',
      originalFilename: 'quilltap-backup-2026.zip',
      mimeType: 'application/zip',
      size: 4096,
      source: 'GENERATED',
      category: 'BACKUP',
      storageKey: `${spec.userId}/backups/quilltap-backup-2026.zip`,
      folderPath: '/backups',
    } as never,
    { id: FILE_BACKUP, createdAt: TS, updatedAt: TS } as never,
  );

  // 13. Two text-replacement rules (the clearFormat3Entities main-DB sweep).
  const { TextReplacementRulesRepository } = await import(
    '@/lib/database/repositories/text-replacement-rules.repository'
  );
  const trrRepo = new TextReplacementRulesRepository();
  await trrRepo.create(
    { fromText: 'teh', toText: 'the', caseSensitive: false, enabled: true, sortOrder: 0 } as never,
    { id: TRR_1, createdAt: TS, updatedAt: TS } as never,
  );
  await trrRepo.create(
    { fromText: 'adn', toText: 'and', caseSensitive: true, enabled: true, sortOrder: 1 } as never,
    { id: TRR_2, createdAt: TS, updatedAt: TS } as never,
  );

  // 14. One embedded chunk in the project's official store (the doc_mount_chunks sweep).
  const projectMp = (
    midb.prepare('SELECT id FROM doc_mount_points ORDER BY createdAt LIMIT 1').get() as
      | { id: string }
      | undefined
  )?.id;
  if (projectMp) {
    const { storeMountFile } = await import('@/lib/mount-index/store-file');
    await storeMountFile({
      mountPointId: projectMp,
      relativePath: 'notes/intro.md',
      data: Buffer.from('# Intro\n\nStore content.', 'utf8'),
      originalMimeType: 'text/markdown',
      transcodeImages: false,
      extractText: false,
      enqueueEmbedding: false,
    } as never);
    const linkId = (
      midb
        .prepare('SELECT id FROM doc_mount_file_links WHERE mountPointId = ? LIMIT 1')
        .get(projectMp) as { id: string } | undefined
    )?.id;
    if (linkId) {
      const embedding = Buffer.from(new Uint8Array(new Float32Array([1]).buffer));
      midb
        .prepare(
          'INSERT INTO doc_mount_chunks (id, linkId, mountPointId, chunkIndex, content, tokenCount, headingContext, embedding, createdAt, updatedAt) ' +
            'VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)',
        )
        .run(
          'a7000000-0000-4000-8000-000000000001',
          linkId,
          projectMp,
          0,
          'Store content.',
          3,
          null,
          embedding,
          TS,
          TS,
        );
    }
  }

  // 15. background_jobs — one row in every status (the tasks-queue + jobs corpus).
  for (const job of spec.jobs) {
    await repos.backgroundJobs.create(
      {
        userId: spec.userId,
        type: job.type,
        status: job.status,
        payload: job.payload,
        priority: job.priority,
        attempts: job.attempts,
        maxAttempts: job.maxAttempts,
        lastError: job.lastError ?? null,
        scheduledAt: job.scheduledAt,
        startedAt: job.startedAt ?? null,
        completedAt: job.completedAt ?? null,
      } as never,
      { id: job.id, createdAt: job.createdAt, updatedAt: job.updatedAt } as never,
    );
  }

  // 16. llm_logs corpus (the delete-all does NOT touch this partition, but the
  //     family carries it for the export/backup surfaces).
  for (const [id, type] of [
    [LOG_1, 'CHAT_MESSAGE'],
    [LOG_2, 'MEMORY_EXTRACTION'],
  ] as Array<[string, string]>) {
    await repos.llmLogs.create(
      {
        userId: spec.userId,
        type,
        messageId: null,
        chatId: CHAT_1,
        characterId: LORIAN,
        autonomousRunId: null,
        provider: 'OPENAI_COMPATIBLE',
        modelName: 'mock-model',
        request: { messages: [], messageCount: 0, temperature: null, maxTokens: null },
        response: { content: 'hello', contentLength: 5, error: null, finishReason: 'stop' },
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  }
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  closeLLMLogsSQLiteClient();

  void group;
  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built system-data fixture: main=${mainOut} mount=${mountOut} llm=${llmOut} ` +
      `(2 characters, 2 chats, 2 memories, ${spec.jobs.length} background jobs, 2 llm-logs)\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`system-data fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
