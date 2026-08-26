/**
 * P4.37 fixture builder — the committed test-pepper substrate for the Almanack
 * tier-2 differential (`almanack_tier2_equivalence`).
 *
 * Forked from `build-system-data-fixture.ts` (the P4.9G1 exemplar) and widened
 * so EVERY Almanack report section is populated NON-TRIVIALLY — a section whose
 * fixture data equals its `collect()` fallback shape is structurally blind (the
 * P4.D31 lesson), and the differential's fallback-coverage assertion enforces
 * this. The fork deliberately does NOT reuse the `system-data-*` output paths:
 * P4.D49's regen sweep and this lane must never contend.
 *
 * ── FOUR FILES, ONE FAMILY ───────────────────────────────────────────────────
 *   - `almanack-main.db`            — the user graph: user, 2 api keys, 2
 *     connection profiles (one cheap, one tool/web/dangerous with lifetime
 *     counters), 3 characters (one favourite Carina-answerer, one dresser with a
 *     core-whisper override, one vaultless NPC), tags, image + embedding
 *     profiles, roleplay + prompt templates (built-ins PRE-SEEDED so v4's
 *     report-time `findAllForUser` seeding is a no-op — the recorded read-only
 *     divergence), project + group, four chats (fresh salon w/ messages +
 *     removed participant, stale flag-bearing salon, an autonomous room with
 *     schedule/budgets/visibility, a 2020 relic), chat_documents ×2 (multi-doc),
 *     memories with the episodic columns + embeddings, files (image, backup,
 *     uploads, a generated image, a not-ok row), text-replacement rules (one
 *     disabled), plugin configs (incl. `qtap-plugin-mcp` with two servers),
 *     conversation chunks (one unembedded), tfidf, embedding_status (incl.
 *     PERMANENTLY_FAILED), vector index, terminal_sessions ×3, help_docs,
 *     migrations_state ×2, background_jobs in every status, instance settings
 *     (well-known mount pointers — lantern DANGLING for the unresolved arm).
 *   - `almanack-mount.db`           — the mount-index sibling: 3 character
 *     vaults (2 linked; the REAL create writes `properties.json` +
 *     `metadata.json`), project + group official stores, the Field Library, the
 *     Quilltap Uploads + General mounts, `Wardrobe/` (one archived, plus a
 *     dressing-`instructions.md` whose body says `archived: true`) /
 *     `Scenarios/` (one archived) / `Mail/` (one `alerted: false`) / `photos/` /
 *     `Tools/*.tool.json` (one llm-consult+effects+gated, one plain, one BROKEN
 *     for the parse-failure arm, one `.settings.json` preset), `state.json` in
 *     project/group/general stores, link-policy denials, a hard-link group,
 *     extraction/conversion error rows, cached mount stats.
 *   - `almanack-llmlogs.db`         — the llm-logs partition at the 4.9 schema
 *     (the two profile-attribution columns), rows across four `type`s with
 *     distinct `durationMs` (fractional averages), `cacheUsage` (read-hit and
 *     creation-only), a failure row, an unmeasured row, a usage-less row, and a
 *     row attributed to a DELETED profile id (the `(deleted profile …)` arm).
 *   - `almanack-llmlogs-legacy.db`  — the SAME partition with the two profile
 *     columns DROPPED (`ALTER TABLE … DROP COLUMN`; SQLite 3.53) — the
 *     approximate-attribution arm.
 *
 * The differential runs every case over /tmp COPIES — the generate case WRITES.
 *
 * Regenerate (Node 24, from the PINNED v4 worktree — see
 * `oracle-regen-pinned-v4-worktree`) + re-copy the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server   # behind-baseline? pass --v4 <pin> to recipe_sweep.py
 *   QT_FIXTURE_ALM_MAIN=$W/crates/quilltap-web/tests/fixtures/almanack-main.db \
 *   QT_FIXTURE_ALM_MOUNT=$W/crates/quilltap-web/tests/fixtures/almanack-mount.db \
 *   QT_FIXTURE_ALM_LLM=$W/crates/quilltap-web/tests/fixtures/almanack-llmlogs.db \
 *   QT_FIXTURE_ALM_LLM_LEGACY=$W/crates/quilltap-web/tests/fixtures/almanack-llmlogs-legacy.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-almanack-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync, copyFileSync } from 'node:fs';
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

// ── Pinned entity ids (shared verbatim with the Rust differential) ───────────
const LORIAN = 'a1000000-0000-4000-8000-000000000001';
const RIYA = 'a1000000-0000-4000-8000-000000000002';
const PIOTR = 'a1000000-0000-4000-8000-000000000003';
const DANGLING_MEMBER = 'a1000000-0000-4000-8000-0000000000ff';
const CONN = 'c0000001-0000-4000-8000-000000000001';
const CONN_CHEAP = 'c0000001-0000-4000-8000-000000000002';
const DELETED_PROFILE = 'cdead000-0000-4000-8000-000000000001';
const APIKEY = 'a0000001-0000-4000-8000-000000000001';
const APIKEY_SPARE = 'a0000001-0000-4000-8000-000000000002';
const GROUP_1 = 'a2000000-0000-4000-8000-000000000001';
const PROJECT_1 = 'a3000000-0000-4000-8000-000000000001';
const RT_USER = 'a4000000-0000-4000-8000-000000000001';
const TAG_1 = 'a5000000-0000-4000-8000-000000000001';
const TAG_2 = 'a5000000-0000-4000-8000-000000000002';
const IMG_PROFILE = 'a6000000-0000-4000-8000-000000000001';
const EMB_PROFILE = 'a8000000-0000-4000-8000-000000000001';
const FOLDER_1 = 'a9000000-0000-4000-8000-000000000001';
const MEM_1 = 'ad000000-0000-4000-8000-000000000001';
const MEM_2 = 'ad000000-0000-4000-8000-000000000002';
const MEM_3 = 'ad000000-0000-4000-8000-000000000003';
const MEM_4 = 'ad000000-0000-4000-8000-000000000004';
const CHAT_1 = 'c1000000-0000-4000-8000-000000000001';
const CHAT_2 = 'c1000000-0000-4000-8000-000000000002';
const CHAT_AUTON = 'c1000000-0000-4000-8000-000000000003';
const CHAT_RELIC = 'c1000000-0000-4000-8000-000000000004';
const MSG_11 = 'd1000000-0000-4000-8000-000000000001';
const MSG_12 = 'd1000000-0000-4000-8000-000000000002';
const FILE_IMG = 'f0000001-0000-4000-8000-000000000001';
const FILE_BACKUP = 'f0000002-0000-4000-8000-000000000002';
const FILE_TXT = 'f0000003-0000-4000-8000-000000000003';
const FILE_GEN = 'f0000005-0000-4000-8000-000000000005';
const FILE_MISSING = 'f0000006-0000-4000-8000-000000000006';
const CHAT_SETTINGS = 'ab000000-0000-4000-8000-000000000001';
const TRR_1 = 'ae000000-0000-4000-8000-000000000001';
const TRR_2 = 'ae000000-0000-4000-8000-000000000002';
const PROVIDER_MODEL_1 = 'b1000000-0000-4000-8000-000000000001';
const PROVIDER_MODEL_IMG = 'b1000000-0000-4000-8000-000000000002';
const PROVIDER_MODEL_EMB = 'b1000000-0000-4000-8000-000000000003';
const PROMPT_TEMPLATE_1 = 'b2000000-0000-4000-8000-000000000001';
const PLUGIN_CONFIG_1 = 'b3000000-0000-4000-8000-000000000001';
const PLUGIN_CONFIG_MCP = 'b3000000-0000-4000-8000-000000000002';
const CHAR_PLUGIN_DATA_1 = 'b4000000-0000-4000-8000-000000000001';
const CHAT_DOCUMENT_1 = 'b7000000-0000-4000-8000-000000000001';
const CHAT_DOCUMENT_2 = 'b7000000-0000-4000-8000-000000000002';
const CONV_CHUNK_1 = 'b8000000-0000-4000-8000-000000000001';
const CONV_CHUNK_2 = 'b8000000-0000-4000-8000-000000000002';
const TFIDF_VOCAB_1 = 'b9000000-0000-4000-8000-000000000001';
const EMB_STATUS_1 = 'ba000000-0000-4000-8000-000000000001';
const EMB_STATUS_2 = 'ba000000-0000-4000-8000-000000000002';
const EMB_STATUS_3 = 'ba000000-0000-4000-8000-000000000003';
const VECTOR_ENTRY_1 = 'bb000000-0000-4000-8000-000000000001';
const STORE_FIELD = 'bd000000-0000-4000-8000-000000000001';
const UPLOADS_MOUNT = 'be000000-0000-4000-8000-000000000001';
const GENERAL_MOUNT = 'be000000-0000-4000-8000-000000000002';
const LANTERN_DANGLING = 'be000000-0000-4000-8000-0000000000ff';
const TERM_1 = 'bf000000-0000-4000-8000-000000000001';
const TERM_2 = 'bf000000-0000-4000-8000-000000000002';
const TERM_3 = 'bf000000-0000-4000-8000-000000000003';
const LOG_IDS = Array.from(
  { length: 12 },
  (_, i) => `e5000000-0000-4000-8000-0000000000${(i + 1).toString(16).padStart(2, '0')}`,
);

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'almanack.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_ALM_MAIN;
  const mountOut = process.env.QT_FIXTURE_ALM_MOUNT;
  const llmOut = process.env.QT_FIXTURE_ALM_LLM;
  const llmLegacyOut = process.env.QT_FIXTURE_ALM_LLM_LEGACY;
  if (!mainOut || !mountOut || !llmOut || !llmLegacyOut) {
    throw new Error(
      'QT_FIXTURE_ALM_{MAIN,MOUNT,LLM,LLM_LEGACY} must point at the .db files to write',
    );
  }
  for (const out of [mainOut, mountOut, llmOut, llmLegacyOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-alm-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.SQLITE_LLM_LOGS_PATH = llmOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase, getCollection } = await import(
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

  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const mainDb = getRawDatabase();
  if (!mainDb) throw new Error('main DB handle unavailable');
  mainDb.exec(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );

  const repos = getRepositories();

  // 1. User + two api keys (one active/used, one inactive/never-used) + two
  //    connection profiles: the workhorse and the designated cheap one (which
  //    also carries the tool/web/dangerous census flags + lifetime counters).
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );
  const apiKeyCol = await getCollection('api_keys');
  await apiKeyCol.insertOne(
    ApiKeySchema.parse({
      id: APIKEY,
      userId: spec.userId,
      label: 'mock-key',
      provider: 'OPENAI_COMPATIBLE',
      key_value: 'sk-synthetic-mock-key',
      isActive: true,
      lastUsed: '2026-07-20T09:00:00.000Z',
      createdAt: TS,
      updatedAt: TS,
    }) as never,
  );
  await apiKeyCol.insertOne(
    ApiKeySchema.parse({
      id: APIKEY_SPARE,
      userId: spec.userId,
      label: 'spare-key',
      provider: 'ANTHROPIC',
      key_value: 'sk-synthetic-spare-key',
      isActive: false,
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
  await repos.connections.create(
    {
      userId: spec.userId,
      name: 'Cheap Courier',
      provider: 'OPENAI_COMPATIBLE',
      modelName: 'mock-cheap',
      baseUrl: 'http://127.0.0.1:1/v1',
      apiKeyId: APIKEY,
      isCheap: true,
      allowWebSearch: true,
      allowToolUse: true,
      isDangerousCompatible: true,
    } as never,
    { id: CONN_CHEAP, createdAt: TS, updatedAt: TS } as never,
  );
  // Lifetime counters live on the profile rows (phase 6's lifetime table).
  mainDb
    .prepare(
      'UPDATE "connection_profiles" SET "totalTokens" = ?, "totalPromptTokens" = ?, ' +
        '"totalCompletionTokens" = ?, "messageCount" = ? WHERE "id" = ?',
    )
    .run(52340, 41000, 11340, 87, CONN);
  mainDb
    .prepare(
      'UPDATE "connection_profiles" SET "totalTokens" = ?, "totalPromptTokens" = ?, ' +
        '"totalCompletionTokens" = ?, "messageCount" = ? WHERE "id" = ?',
    )
    .run(9130, 7000, 2130, 41, CONN_CHEAP);

  // 2. Three characters. The REAL create provisions each vault (keystone +
  //    metadata documents); Piotr is then unlinked raw — the vaultless arm.
  await repos.characters.create(
    {
      name: 'Lorian',
      userId: spec.userId,
      title: null,
      description: 'Lorian of the fixture.',
      personality: null,
      aliases: [],
      controlledBy: 'llm',
      talkativeness: 0.5,
      defaultConnectionProfileId: CONN,
      isFavorite: true,
      npc: false,
      canBeCarina: true,
      tags: [TAG_1],
    } as never,
    { id: LORIAN, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.characters.create(
    {
      name: 'Riya',
      userId: spec.userId,
      title: null,
      description: 'Riya of the fixture.',
      personality: null,
      aliases: [],
      controlledBy: 'llm',
      talkativeness: 0.5,
      defaultConnectionProfileId: CONN,
      isFavorite: false,
      npc: false,
      canDressThemselves: true,
      canCreateOutfits: true,
      coreWhisperEnabled: true,
      tags: [TAG_1],
    } as never,
    { id: RIYA, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.characters.create(
    {
      name: 'Piotr',
      userId: spec.userId,
      title: null,
      description: 'Piotr, kept off the books.',
      personality: null,
      aliases: [],
      controlledBy: 'user',
      talkativeness: 0.5,
      isFavorite: false,
      npc: true,
      systemTransparency: true,
      tags: [],
    } as never,
    { id: PIOTR, createdAt: TS, updatedAt: TS } as never,
  );
  // The vaultless arm: unlink Piotr's vault (the legacy pre-vault state). The
  // orphaned mount stays in the index — deterministic, and exactly what a real
  // instance carrying that history looks like.
  mainDb
    .prepare('UPDATE "characters" SET "characterDocumentMountPointId" = NULL WHERE "id" = ?')
    .run(PIOTR);

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

  // 4. Image + embedding profiles (both default; the embedding one is the
  //    designated profile AND the dimension-mismatch reference: profile says 8,
  //    stored vectors are 4-wide).
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

  // 7. chat_settings — the dial-bearing bag phases 2, 3 and 6 all read: the
  //    designated image-prompt profile, the theme preference, the logging
  //    dials (retention 45 — non-default), and a non-default value in most
  //    feature-config sections so none coincides with its fallback.
  await repos.chatSettings.create(
    {
      userId: spec.userId,
      cheapLLMSettings: {
        strategy: 'PROVIDER_CHEAPEST',
        fallbackToLocal: false,
        imagePromptProfileId: CONN_CHEAP,
      },
      llmLoggingSettings: { enabled: true, verboseMode: true, retentionDays: 45 },
      themePreference: { activeThemeId: 'madmans-box', colorMode: 'dark' },
      dangerousContentSettings: { mode: 'DETECT_ONLY', threshold: 0.55 },
      contextCompressionSettings: { enabled: true, windowSize: 7 },
      agentModeSettings: { maxTurns: 6, defaultEnabled: false },
      storyBackgroundsSettings: { enabled: true, defaultImageProfileId: IMG_PROFILE },
      defaultTimestampConfig: { mode: 'EVERY_MESSAGE', format: 'ISO8601' },
      autoLockSettings: { enabled: true, idleMinutes: 20 },
      memoryCascadePreferences: {
        onMessageDelete: 'KEEP_MEMORIES',
        onSwipeRegenerate: 'DELETE_MEMORIES',
      },
      autoDetectRng: false,
      avatarDisplayMode: 'GROUP_ONLY',
      avatarDisplayStyle: 'SQUARE',
      coreWhisper: { enabled: true, interval: 9, silenceThreshold: 2 },
      thinkingDisplay: { defaultVisible: false, defaultCollapsed: true },
      answerConfirmationSettings: { enabled: true },
      autonomousRoomSettings: { dailyTokenBudget: 50000, visibilityDefault: 'household' },
      autoHousekeepingSettings: { enabled: true, perCharacterCap: 1500 },
      composerSpellcheck: false,
      autoScrollOnResponseComplete: true,
      imageDescriptionProfileId: CONN,
    } as never,
    { id: CHAT_SETTINGS, createdAt: TS, updatedAt: TS } as never,
  );

  // 8. A project + a group (official stores provisioned by the real creates);
  //    the group has one real member and one DANGLING member (the
  //    "(missing character)" cross-database arm).
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
  await repos.groups.create(
    { name: 'The Crew', description: 'A fixture group.', state: {} } as never,
    { id: GROUP_1, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.groupCharacterMembers.addMember(GROUP_1, LORIAN);
  await repos.groupCharacterMembers.addMember(GROUP_1, DANGLING_MEMBER);

  // 8b. The Field Library (a second store, group-linked), the Quilltap Uploads
  //     and Quilltap General mounts + their instance-settings pointers, and a
  //     DANGLING lantern pointer (the unresolved well-known arm).
  await repos.docMountPoints.create(
    {
      name: 'Field Library',
      basePath: '',
      mountType: 'database',
      storeType: 'documents',
      includePatterns: [],
      excludePatterns: [],
      enabled: true,
    } as never,
    { id: STORE_FIELD, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.groupDocMountLinks.link(GROUP_1, STORE_FIELD);
  await repos.docMountPoints.create(
    {
      name: 'Quilltap Uploads',
      basePath: '',
      mountType: 'database',
      storeType: 'documents',
      includePatterns: [],
      excludePatterns: [],
      enabled: true,
    } as never,
    { id: UPLOADS_MOUNT, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.docMountPoints.create(
    {
      name: 'Quilltap General',
      basePath: '',
      mountType: 'database',
      storeType: 'documents',
      includePatterns: [],
      excludePatterns: [],
      enabled: true,
    } as never,
    { id: GENERAL_MOUNT, createdAt: TS, updatedAt: TS } as never,
  );
  const setSetting = mainDb.prepare(
    'INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)',
  );
  setSetting.run('userUploadsMountPointId', UPLOADS_MOUNT);
  setSetting.run('generalMountPointId', GENERAL_MOUNT);
  setSetting.run('lanternBackgroundsMountPointId', LANTERN_DANGLING);

  // 9. Four chats.
  const mkParticipant = (
    id: string,
    characterId: string,
    status: 'active' | 'removed' = 'active',
  ) => ({
    id,
    type: 'CHARACTER',
    characterId,
    controlledBy: 'llm',
    status,
    connectionProfileId: CONN,
    createdAt: TS,
    updatedAt: TS,
  });
  // CHAT_1 — fresh, project-bound, two active participants + one REMOVED
  // (Piotr; counted per COUNT_REMOVED_PARTICIPANTS), two messages, cost/token
  // counters set raw below.
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'A Lesson in Lift',
      participants: [
        mkParticipant('e1000000-0000-4000-8000-000000000001', LORIAN),
        mkParticipant('e1000000-0000-4000-8000-000000000002', RIYA),
        mkParticipant('e1000000-0000-4000-8000-000000000003', PIOTR, 'removed'),
      ],
      chatType: 'salon',
      contextSummary: null,
      projectId: PROJECT_1,
      tags: [TAG_1],
    } as never,
    { id: CHAT_1, createdAt: TS, updatedAt: '2026-07-30T00:00:00.000Z' } as never,
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
  // CHAT_2 — stale, flag-bearing: paused, agent-mode, dangerous, document
  // mode, equipped outfit, pending outfit notifications, narrative timeline,
  // non-empty state.
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Quiet Interlude',
      participants: [mkParticipant('e1000000-0000-4000-8000-000000000011', RIYA)],
      chatType: 'salon',
      contextSummary: null,
      tags: [],
      isPaused: true,
      agentModeEnabled: true,
      isDangerousChat: true,
      documentEditingMode: true,
      timelineMode: 'narrative',
      state: { scene: 'harbour', weather: 'fog' },
    } as never,
    { id: CHAT_2, createdAt: TS, updatedAt: '2026-03-05T00:00:00.000Z' } as never,
  );
  mainDb
    .prepare(
      'UPDATE "chats" SET "equippedOutfit" = ?, "pendingOutfitNotifications" = ? WHERE "id" = ?',
    )
    .run(
      JSON.stringify({ [RIYA]: 'ac000000-0000-4000-8000-000000000001' }),
      JSON.stringify([{ characterId: RIYA, outfitId: 'ac000000-0000-4000-8000-000000000001' }]),
      CHAT_2,
    );
  // CHAT_AUTON — the autonomous room: paused mid-run, cron-scheduled and
  // OVERDUE (nextRunAt < pinned now), all four budgets, destructive tools
  // pre-authorized, per-room visibility.
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'The Night Shift',
      participants: [mkParticipant('e1000000-0000-4000-8000-000000000021', LORIAN)],
      chatType: 'autonomous',
      contextSummary: null,
      tags: [],
      runState: 'paused',
      runVisibility: 'household',
      scheduleCron: '0 3 * * *',
      scheduleNextRunAt: '2026-07-31T03:00:00.000Z',
      scheduleLastRunAt: '2026-07-30T03:00:00.000Z',
      budgetMaxTurns: 40,
      budgetMaxTokens: 200000,
      budgetMaxWallClockMs: 7200000,
      budgetEstimatedSpendCapUSD: 1.5,
      runDestructiveToolsAllowed: 1,
    } as never,
    { id: CHAT_AUTON, createdAt: TS, updatedAt: '2026-07-01T00:00:00.000Z' } as never,
  );
  // CHAT_RELIC — a 2020 relic (stale-sweep eligibility). `help` also exercises
  // the personae EXCLUDED_CHAT_TYPES arm: Lorian's chat count must not count it.
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'The Relic',
      participants: [mkParticipant('e1000000-0000-4000-8000-000000000031', LORIAN)],
      chatType: 'help',
      contextSummary: null,
      tags: [],
    } as never,
    { id: CHAT_RELIC, createdAt: '2020-01-01T00:00:00.000Z', updatedAt: '2020-06-01T00:00:00.000Z' } as never,
  );
  // Chat-level cost/token counters (phase 3's chatStats).
  mainDb
    .prepare(
      'UPDATE "chats" SET "estimatedCostUSD" = ?, "totalPromptTokens" = ?, ' +
        '"totalCompletionTokens" = ?, "messageCount" = ? WHERE "id" = ?',
    )
    .run(0.1234, 15000, 3500, 24, CHAT_1);
  mainDb
    .prepare(
      'UPDATE "chats" SET "estimatedCostUSD" = ?, "totalPromptTokens" = ?, ' +
        '"totalCompletionTokens" = ?, "messageCount" = ? WHERE "id" = ?',
    )
    .run(0.0501, 4000, 900, 7, CHAT_2);

  // 9b. Two ACTIVE chat_documents on CHAT_1 (the multi-document arm).
  await repos.chatDocuments.create(
    {
      chatId: CHAT_1,
      filePath: 'notes/intro.md',
      scope: 'project',
      mountPoint: null,
      displayTitle: 'Intro',
      isActive: true,
    } as never,
    { id: CHAT_DOCUMENT_1, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.chatDocuments.create(
    {
      chatId: CHAT_1,
      filePath: 'notes/appendix.md',
      scope: 'project',
      mountPoint: null,
      displayTitle: 'Appendix',
      isActive: true,
    } as never,
    { id: CHAT_DOCUMENT_2, createdAt: TS, updatedAt: TS } as never,
  );

  // 10. Four memories: kinds, sources, witnessed contexts, the episodic
  //     columns, embeddings and reinforcement.
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
      witnessedContext: 'user_present',
      sourceMessageId: null,
      lastAccessedAt: null,
      reinforcementCount: 3,
      lastReinforcedAt: '2026-07-01T00:00:00.000Z',
      relatedMemoryIds: [],
      reinforcedImportance: 0.75,
      kind: 'episodic',
      occurredAt: '2026-02-14T18:30:00.000Z',
      narrativeTime: 'the eve of the launch',
      entities: ['Lorian', 'the road'],
    } as never,
    { id: MEM_1, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.memories.create(
    {
      characterId: RIYA,
      aboutCharacterId: LORIAN,
      chatId: null,
      projectId: null,
      content: 'Riya recalls the harbour.',
      summary: 'the harbour',
      keywords: [],
      tags: [],
      importance: 0.4,
      embedding: null,
      source: 'MANUAL',
      witnessedContext: 'manual',
      sourceMessageId: null,
      lastAccessedAt: null,
      reinforcementCount: 1,
      lastReinforcedAt: null,
      relatedMemoryIds: [],
      reinforcedImportance: 0.4,
    } as never,
    { id: MEM_2, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.memories.create(
    {
      characterId: LORIAN,
      aboutCharacterId: null,
      chatId: CHAT_1,
      projectId: null,
      content: 'Lorian remembers the storm over the strait.',
      summary: 'the storm',
      keywords: ['storm'],
      tags: [],
      importance: 0.7,
      embedding: new Float32Array([0.25, -0.5, 0.125, 1]),
      source: 'AUTO',
      witnessedContext: null,
      sourceMessageId: null,
      lastAccessedAt: null,
      reinforcementCount: 1,
      lastReinforcedAt: null,
      relatedMemoryIds: [],
      reinforcedImportance: 0.7,
    } as never,
    { id: MEM_3, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.memories.create(
    {
      characterId: RIYA,
      aboutCharacterId: null,
      chatId: CHAT_2,
      projectId: null,
      content: 'Riya noted the fog rolling in.',
      summary: 'the fog',
      keywords: [],
      tags: [],
      importance: 0.5,
      embedding: new Float32Array([0.5, 0.5, -0.5, -0.5]),
      source: 'AUTO',
      witnessedContext: 'autonomous_room',
      sourceMessageId: null,
      lastAccessedAt: null,
      reinforcementCount: 2,
      lastReinforcedAt: '2026-06-01T00:00:00.000Z',
      relatedMemoryIds: [],
      reinforcedImportance: 0.55,
      kind: 'episodic',
      occurredAt: '2026-03-05T07:00:00.000Z',
      entities: ['Riya'],
    } as never,
    { id: MEM_4, createdAt: TS, updatedAt: TS } as never,
  );

  // 11. Files — an image, a backup zip, an uploaded doc, a GENERATED image
  //     (generated-images-by-model), and a not-ok row (fileStatus).
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
  const { writeUserUploadToMountStore } = await import('@/lib/file-storage/user-uploads-bridge');
  const smallBytes = Buffer.from(
    'Field notes, kept plainly: the lift is strongest before noon.\n',
    'utf8',
  );
  const smallStored = await writeUserUploadToMountStore({
    filename: 'field-notes.txt',
    content: smallBytes,
    contentType: 'text/plain',
    subfolder: 'uploads',
  });
  await repos.files.create(
    {
      userId: spec.userId,
      linkedTo: [CHAT_1],
      tags: [TAG_1],
      sha256: smallStored.sha256,
      originalFilename: 'field-notes.txt',
      mimeType: smallStored.storedMimeType,
      size: smallStored.sizeBytes,
      source: 'UPLOADED',
      category: 'DOCUMENT',
      storageKey: smallStored.storageKey,
      folderPath: '/notes',
      isPlainText: true,
    } as never,
    { id: FILE_TXT, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.files.create(
    {
      userId: spec.userId,
      linkedTo: [],
      tags: [],
      sha256: '3333333333333333333333333333333333333333333333333333333333333333',
      originalFilename: 'sunrise.webp',
      mimeType: 'image/webp',
      size: 20480,
      source: 'GENERATED',
      category: 'IMAGE',
      storageKey: `${spec.userId}/sunrise.webp`,
      generationModel: 'mock-image-model',
      generationPrompt: 'a sunrise over the strait',
      projectId: PROJECT_1,
    } as never,
    { id: FILE_GEN, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.files.create(
    {
      userId: spec.userId,
      linkedTo: [],
      tags: [],
      sha256: '4444444444444444444444444444444444444444444444444444444444444444',
      originalFilename: 'lost-plate.png',
      mimeType: 'image/png',
      size: 64,
      source: 'UPLOADED',
      category: 'IMAGE',
      storageKey: `${spec.userId}/lost-plate.png`,
    } as never,
    { id: FILE_MISSING, createdAt: TS, updatedAt: TS } as never,
  );
  mainDb.prepare('UPDATE "files" SET "fileStatus" = ? WHERE "id" = ?').run('missing', FILE_MISSING);

  // 12. Text-replacement rules — one enabled, one disabled.
  const { TextReplacementRulesRepository } = await import(
    '@/lib/database/repositories/text-replacement-rules.repository'
  );
  const trrRepo = new TextReplacementRulesRepository();
  await trrRepo.create(
    { fromText: 'teh', toText: 'the', caseSensitive: false, enabled: true, sortOrder: 0 } as never,
    { id: TRR_1, createdAt: TS, updatedAt: TS } as never,
  );
  await trrRepo.create(
    { fromText: 'adn', toText: 'and', caseSensitive: true, enabled: false, sortOrder: 1 } as never,
    { id: TRR_2, createdAt: TS, updatedAt: TS } as never,
  );

  // 13. Provider-model cache — chat + image + embedding rows for the active
  //     key's provider (v4's collectModels reads the cache, never the network,
  //     when the cache is warm — keep it warm for every active-key provider).
  await repos.providerModels.create(
    {
      provider: 'OPENAI_COMPATIBLE',
      modelId: 'mock-model',
      modelType: 'chat',
      displayName: 'Mock Model',
      baseUrl: 'http://127.0.0.1:1/v1',
      contextWindow: 8192,
      maxOutputTokens: 2048,
      deprecated: false,
      experimental: false,
    } as never,
    { id: PROVIDER_MODEL_1, createdAt: TS, updatedAt: '2026-06-01T00:00:00.000Z' } as never,
  );
  await repos.providerModels.create(
    {
      provider: 'OPENAI_COMPATIBLE',
      modelId: 'mock-image-model',
      modelType: 'image',
      displayName: 'Mock Image Model',
      baseUrl: 'http://127.0.0.1:1/v1',
      contextWindow: null,
      maxOutputTokens: null,
      deprecated: false,
      experimental: false,
    } as never,
    { id: PROVIDER_MODEL_IMG, createdAt: TS, updatedAt: '2026-06-02T00:00:00.000Z' } as never,
  );
  await repos.providerModels.create(
    {
      provider: 'OPENAI',
      modelId: 'mock-embed',
      modelType: 'embedding',
      displayName: 'Mock Embed',
      baseUrl: 'http://127.0.0.1:1/v1',
      contextWindow: null,
      maxOutputTokens: null,
      deprecated: false,
      experimental: false,
    } as never,
    { id: PROVIDER_MODEL_EMB, createdAt: TS, updatedAt: '2026-06-03T00:00:00.000Z' } as never,
  );

  // 14. Prompt templates: a user one now; built-ins are seeded via v4's real
  //     listing at the END of the build (see step 22).
  await repos.promptTemplates.create(
    {
      userId: spec.userId,
      name: 'Fixture Prompt',
      content: 'Speak plainly and carry a small lantern.',
      description: 'The fixture prompt template.',
      isBuiltIn: false,
      category: 'COMPANION',
      modelHint: null,
      tags: [TAG_2],
    } as never,
    { id: PROMPT_TEMPLATE_1, createdAt: TS, updatedAt: TS } as never,
  );

  // 15. Plugin configs (one bundled fixture plugin on; the MCP plugin with two
  //     servers — one enabled — and non-default reconnect dials) + one
  //     per-character plugin-data row.
  await repos.pluginConfigs.create(
    {
      userId: spec.userId,
      pluginName: 'qtap-plugin-fixture',
      config: { endpoint: 'http://127.0.0.1:1', retries: 2 },
      enabled: true,
    } as never,
    { id: PLUGIN_CONFIG_1, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.pluginConfigs.create(
    {
      userId: spec.userId,
      pluginName: 'qtap-plugin-mcp',
      config: {
        servers: JSON.stringify([
          { name: 'alpha', enabled: true },
          { displayName: 'Beta Bridge', enabled: false },
        ]),
        autoReconnect: false,
        maxReconnectAttempts: 7,
      },
      enabled: true,
    } as never,
    { id: PLUGIN_CONFIG_MCP, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.characterPluginData.create(
    {
      characterId: LORIAN,
      pluginName: 'qtap-plugin-fixture',
      data: { visits: 3, lastPort: 'Ravenna' },
    } as never,
    { id: CHAR_PLUGIN_DATA_1, createdAt: TS, updatedAt: TS } as never,
  );

  // 16. Conversation chunks (one embedded, one not), tfidf, embedding_status
  //     (EMBEDDED / PENDING / PERMANENTLY_FAILED), a vector index + entry.
  await repos.conversationChunks.create(
    {
      chatId: CHAT_1,
      interchangeIndex: 0,
      content: 'Hello there. / A fine day for a voyage.',
      participantNames: ['Lorian'],
      messageIds: [MSG_11, MSG_12],
      embedding: new Float32Array([0.1, -0.25, 0.5, 0.75]),
    } as never,
    { id: CONV_CHUNK_1, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.conversationChunks.create(
    {
      chatId: CHAT_2,
      interchangeIndex: 0,
      content: 'The fog held its tongue.',
      participantNames: ['Riya'],
      messageIds: [],
      embedding: null,
    } as never,
    { id: CONV_CHUNK_2, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.tfidfVocabularies.create(
    {
      profileId: EMB_PROFILE,
      userId: spec.userId,
      vocabulary: JSON.stringify([
        ['road', 0],
        ['harbour', 1],
      ]),
      idf: JSON.stringify([0.5, 1.5]),
      avgDocLength: 4.5,
      vocabularySize: 2,
      includeBigrams: true,
      fittedAt: TS,
    } as never,
    { id: TFIDF_VOCAB_1, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.embeddingStatus.create(
    {
      userId: spec.userId,
      entityType: 'MEMORY',
      entityId: MEM_3,
      profileId: EMB_PROFILE,
      status: 'EMBEDDED',
      embeddedAt: TS,
      error: null,
    } as never,
    { id: EMB_STATUS_1, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.embeddingStatus.create(
    {
      userId: spec.userId,
      entityType: 'MEMORY',
      entityId: MEM_1,
      profileId: EMB_PROFILE,
      status: 'PENDING',
      embeddedAt: null,
      error: null,
    } as never,
    { id: EMB_STATUS_2, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.embeddingStatus.create(
    {
      userId: spec.userId,
      entityType: 'CONVERSATION_CHUNK',
      entityId: CONV_CHUNK_2,
      profileId: EMB_PROFILE,
      // v4's schema can only store PENDING/EMBEDDED/FAILED. v4 `e6554b6e`
      // (Bug 19) fixed the almanack's census to count FAILED (was
      // 'PERMANENTLY_FAILED', which nothing can write, so the cell was always
      // 0). This row is now counted in the `failed` cell.
      status: 'FAILED',
      embeddedAt: null,
      error: 'input too large',
    } as never,
    { id: EMB_STATUS_3, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.vectorIndices.saveMeta(LORIAN, 4);
  await repos.vectorIndices.addEntry({
    id: VECTOR_ENTRY_1,
    characterId: LORIAN,
    embedding: new Float32Array([0.1, -0.25, 0.5, 0.75]),
  } as never);

  // 17. Terminal sessions — one live, one clean exit, one non-zero exit;
  //     two distinct shells.
  const { TerminalSessionsRepository } = await import(
    '@/lib/database/repositories/terminal-sessions.repository'
  );
  const termRepo = new TerminalSessionsRepository();
  await termRepo.create(
    {
      chatId: CHAT_1,
      label: 'watch',
      shell: '/bin/zsh',
      cwd: '/tmp',
      startedAt: '2026-07-01T10:00:00.000Z',
      exitedAt: null,
      exitCode: null,
      transcriptPath: null,
    } as never,
    { id: TERM_1, createdAt: TS, updatedAt: TS } as never,
  );
  await termRepo.create(
    {
      chatId: CHAT_1,
      label: null,
      shell: '/bin/bash',
      cwd: '/tmp',
      startedAt: '2026-07-01T09:00:00.000Z',
      exitedAt: '2026-07-01T09:05:00.000Z',
      exitCode: 0,
      transcriptPath: null,
    } as never,
    { id: TERM_2, createdAt: TS, updatedAt: TS } as never,
  );
  await termRepo.create(
    {
      chatId: CHAT_2,
      label: null,
      shell: '/bin/zsh',
      cwd: '/tmp',
      startedAt: '2026-07-02T09:00:00.000Z',
      exitedAt: '2026-07-02T09:01:00.000Z',
      exitCode: 1,
      transcriptPath: null,
    } as never,
    { id: TERM_3, createdAt: TS, updatedAt: TS } as never,
  );

  // 18. Instance settings: retention (45 stale days), sweep bookkeeping,
  //     concurrency, memory recall + extraction limits, the version tripwire.
  for (const [key, value] of [
    ['memoryRecall', '{"scopePolicy":"exclude","expandRelated":true}'],
    ['dataRetention', '{"staleChatDays":45}'],
    ['lastMaintenanceSweepAt', '2026-07-20T06:00:00.000Z'],
    ['highest_app_version', '"4.9.0"'],
    ['maxConcurrentJobs', '6'],
    [
      'memoryExtractionLimits',
      '{"enabled":true,"maxPerHour":12,"softStartFraction":0.5,"softFloor":0.6}',
    ],
  ] as Array<[string, string]>) {
    setSetting.run(key, value);
  }

  // 19. background_jobs — one row in every status.
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

  // 20. The Scriptorium documents: wardrobe/scenarios/mail/photos/tools/state
  //     across the tiers, then the policy/error/hard-link arms raw.
  const { storeMountFile } = await import('@/lib/mount-index/store-file');
  const store = async (
    mountPointId: string,
    relativePath: string,
    content: string | Buffer,
    mime = 'text/markdown',
  ) => {
    await storeMountFile({
      mountPointId,
      relativePath,
      data: typeof content === 'string' ? Buffer.from(content, 'utf8') : content,
      originalMimeType: mime,
      transcodeImages: false,
      extractText: false,
      enqueueEmbedding: false,
      // state.json is seeded empty by the store provisioners; overwrite it.
      force: true,
    } as never);
  };

  const vaultOf = (characterId: string): string => {
    const row = mainDb
      .prepare('SELECT "characterDocumentMountPointId" AS id FROM "characters" WHERE "id" = ?')
      .get(characterId) as { id: string | null } | undefined;
    if (!row?.id) throw new Error(`character ${characterId} has no vault`);
    return row.id;
  };
  const projectStore = (
    mainDb.prepare('SELECT "officialMountPointId" AS id FROM "projects" WHERE "id" = ?').get(
      PROJECT_1,
    ) as { id: string | null }
  ).id;
  const groupStore = (
    mainDb.prepare('SELECT "officialMountPointId" AS id FROM "groups" WHERE "id" = ?').get(
      GROUP_1,
    ) as { id: string | null }
  ).id;
  if (!projectStore || !groupStore) throw new Error('official stores were not provisioned');
  const lorianVault = vaultOf(LORIAN);
  const riyaVault = vaultOf(RIYA);

  // Wardrobe by tier (one archived item on Lorian).
  await store(
    lorianVault,
    'Wardrobe/travelling-coat.md',
    '---\ntitle: Travelling Coat\narchived: false\n---\nA weathered coat.\n',
  );
  await store(
    lorianVault,
    'Wardrobe/parade-uniform.md',
    '---\ntitle: Parade Uniform\narchived: true\n---\nMothballed.\n',
  );
  await store(
    projectStore,
    'Wardrobe/voyage-slicker.md',
    '---\ntitle: Voyage Slicker\narchived: false\n---\nShared oilskin.\n',
  );
  await store(
    groupStore,
    'Wardrobe/crew-jersey.md',
    '---\ntitle: Crew Jersey\narchived: false\n---\nMatching jerseys.\n',
  );
  await store(
    GENERAL_MOUNT,
    'Wardrobe/plain-apron.md',
    '---\ntitle: Plain Apron\narchived: false\n---\nAnyone may borrow it.\n',
  );
  // [P4.D119 / v4 `b86bb1a5`] The dressing-instructions file lives in the
  // Wardrobe folder but is NOT a garment: the `items` count excludes it by
  // exact path. Its body deliberately contains the literal `archived: true`,
  // because v4 gave the sibling `archived` LIKE-count NO exclusion — the file
  // still counts as an archived garment there, and that asymmetry is what this
  // seed pins.
  await store(
    lorianVault,
    'Wardrobe/instructions.md',
    'You prefer tweeds for fieldwork.\n\nNever anything archived: true.\n',
  );

  // Scenarios by tier.
  await store(lorianVault, 'Scenarios/the-crossing.md', '# The Crossing\n\nA night passage.\n');
  await store(projectStore, 'Scenarios/harbour-watch.md', '# Harbour Watch\n');
  await store(groupStore, 'Scenarios/mess-night.md', '# Mess Night\n');
  await store(GENERAL_MOUNT, 'Scenarios/open-house.md', '# Open House\n');
  // [P4.D120 / v4 `d25dacc1`] One ARCHIVED scenario, so the Scriptorium row's
  // new `Archived` column is measurable (the same `%archived: true%` LIKE the
  // wardrobe tally uses).
  await store(
    lorianVault,
    'Scenarios/mothballed.md',
    '---\nname: Mothballed\narchived: true\n---\n\nA scene put away.\n',
  );

  // The Post Office — letters in two vaults; one unannounced.
  await store(
    lorianVault,
    'Mail/inbox/letter-from-riya.md',
    '---\nfrom: Riya\nalerted: false\n---\nCome down to the quay.\n',
  );
  await store(
    riyaVault,
    'Mail/inbox/letter-from-lorian.md',
    '---\nfrom: Lorian\nalerted: true\n---\nThe quay it is.\n',
  );

  // Photos — one vault photo, one user-gallery photo (binary, so they land as
  // blobs with real byte sizes).
  const photoBytes = Buffer.from([0x52, 0x49, 0x46, 0x46, 1, 2, 3, 4, 5, 6, 7, 8]);
  await store(lorianVault, 'photos/portrait-sketch.webp', photoBytes, 'image/webp');
  await store(UPLOADS_MOUNT, 'photos/quay-at-dawn.webp', photoBytes, 'image/webp');

  // Custom tools in the Field Library: one full-dress (llm consult + effects +
  // availability gate), one plain, one BROKEN, one preset sibling. Shapes per
  // v4's real CustomToolSchema (roll/outcomes/record-parameters).
  await store(
    STORE_FIELD,
    'Tools/roll_fate.tool.json',
    JSON.stringify(
      {
        name: 'roll_fate',
        description: 'Consults the wheel of fate.',
        roll: { min: 1, max: 6 },
        parameters: { stakes: { type: 'string', default: 'everything' } },
        outcomes: [
          { when: { gte: 4 }, message: 'Triumph over {{params.stakes}}.', state: 'success' },
          { when: true, message: 'Calamity.', state: 'failure' },
        ],
        llm: {
          prompt: 'Weigh the stakes: {{params.stakes}}',
          errorMessage: 'The oracle is silent.',
        },
        effects: [{ target: 'state.lastFate', value: "'sealed'" }],
        availableWhen: { metadata: { mood: { eq: 'daring' } } },
      },
      null,
      2,
    ),
    'application/json',
  );
  await store(
    STORE_FIELD,
    'Tools/greet.tool.json',
    JSON.stringify(
      {
        name: 'greet',
        description: 'A plain greeting.',
        roll: { min: 1, max: 1 },
        outcomes: [{ when: true, message: 'Hello there.', state: 'info' }],
      },
      null,
      2,
    ),
    'application/json',
  );
  await store(STORE_FIELD, 'Tools/broken.tool.json', '{ this is not json', 'application/json');
  await store(
    STORE_FIELD,
    'Tools/roll_fate.default.settings.json',
    JSON.stringify({ stakes: 'the whole voyage' }, null, 2),
    'application/json',
  );

  // State cascade: non-empty root state.json in project/group/general stores.
  await store(projectStore, 'state.json', JSON.stringify({ act: 2 }), 'application/json');
  await store(groupStore, 'state.json', JSON.stringify({ morale: 'high' }), 'application/json');
  await store(GENERAL_MOUNT, 'state.json', JSON.stringify({ season: 'spring' }), 'application/json');

  // Policy denials + extraction/conversion errors + a hard-link group (raw —
  // these are census columns, not flows).
  midb
    .prepare(
      'UPDATE doc_mount_file_links SET allowEmbed = 0, allowCharacterRead = 0, allowCharacterWrite = 0 ' +
        "WHERE mountPointId = ? AND relativePath = 'Wardrobe/parade-uniform.md'",
    )
    .run(lorianVault);
  midb
    .prepare(
      "UPDATE doc_mount_file_links SET extractionStatus = 'error' " +
        "WHERE mountPointId = ? AND relativePath = 'Tools/broken.tool.json'",
    )
    .run(STORE_FIELD);
  midb
    .prepare(
      "UPDATE doc_mount_file_links SET conversionStatus = 'error' " +
        "WHERE mountPointId = ? AND relativePath = 'photos/portrait-sketch.webp'",
    )
    .run(lorianVault);
  midb
    .prepare(
      "UPDATE doc_mount_file_links SET linkGroupId = 'aaaa0000-0000-4000-8000-000000000001' " +
        "WHERE relativePath IN ('Scenarios/harbour-watch.md', 'Scenarios/mess-night.md')",
    )
    .run();

  // One mount wears a scan error + non-idle statuses; cached stats get
  // distinct values so vault storage figures are non-zero (phase 5's bytes).
  midb
    .prepare(
      "UPDATE doc_mount_points SET scanStatus = 'error', lastScanError = 'directory walked off' " +
        'WHERE id = ?',
    )
    .run(STORE_FIELD);
  midb
    .prepare("UPDATE doc_mount_points SET conversionStatus = 'converting' WHERE id = ?")
    .run(UPLOADS_MOUNT);
  midb
    .prepare('UPDATE doc_mount_points SET fileCount = 5, chunkCount = 2, totalSizeBytes = 8192 WHERE id = ?')
    .run(lorianVault);
  midb
    .prepare('UPDATE doc_mount_points SET fileCount = 3, chunkCount = 0, totalSizeBytes = 4096 WHERE id = ?')
    .run(riyaVault);

  // One embedded chunk in the project store (the chunk census).
  const introLink = midb
    .prepare('SELECT id FROM doc_mount_file_links WHERE mountPointId = ? LIMIT 1')
    .get(projectStore) as { id: string } | undefined;
  if (introLink) {
    const embedding = Buffer.from(new Uint8Array(new Float32Array([1]).buffer));
    midb
      .prepare(
        'INSERT INTO doc_mount_chunks (id, linkId, mountPointId, chunkIndex, content, tokenCount, headingContext, embedding, createdAt, updatedAt) ' +
          'VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)',
      )
      .run(
        'a7000000-0000-4000-8000-000000000001',
        introLink.id,
        projectStore,
        0,
        'Store content.',
        3,
        null,
        embedding,
        TS,
        TS,
      );
  }

  // 21. help_docs + migrations_state — inserted raw if v4's init did not
  //     already provide them (both tables are plain ledgers here).
  mainDb.exec(
    'CREATE TABLE IF NOT EXISTS "help_docs" ("id" TEXT PRIMARY KEY, "slug" TEXT, "title" TEXT, ' +
      '"content" TEXT, "embedding" BLOB, "createdAt" TEXT, "updatedAt" TEXT)',
  );
  const helpCount = (mainDb.prepare('SELECT COUNT(*) AS n FROM "help_docs"').get() as { n: number })
    .n;
  if (helpCount === 0) {
    const emb = Buffer.from(new Uint8Array(new Float32Array([0.5, 0.5, 0.5, 0.5]).buffer));
    mainDb
      .prepare(
        'INSERT INTO "help_docs" ("id", "slug", "title", "content", "embedding", "createdAt", "updatedAt") VALUES (?, ?, ?, ?, ?, ?, ?)',
      )
      .run('cc000000-0000-4000-8000-000000000001', 'getting-started', 'Getting Started', 'Words.', emb, TS, TS);
    mainDb
      .prepare(
        'INSERT INTO "help_docs" ("id", "slug", "title", "content", "embedding", "createdAt", "updatedAt") VALUES (?, ?, ?, ?, ?, ?, ?)',
      )
      .run('cc000000-0000-4000-8000-000000000002', 'custom-tools', 'Custom Tools', 'More words.', null, TS, TS);
  }
  mainDb.exec(
    'CREATE TABLE IF NOT EXISTS "migrations_state" ("id" TEXT PRIMARY KEY, "completedAt" TEXT, "quilltapVersion" TEXT)',
  );
  const migCount = (
    mainDb.prepare('SELECT COUNT(*) AS n FROM "migrations_state"').get() as { n: number }
  ).n;
  if (migCount === 0) {
    const insMig = mainDb.prepare(
      'INSERT INTO "migrations_state" ("id", "completedAt", "quilltapVersion") VALUES (?, ?, ?)',
    );
    insMig.run('add-llm-logs-profile-columns-v1', '2026-08-01T00:00:00.000Z', '4.8.0');
    insMig.run('provision-user-uploads-mount-v1', '2026-07-01T00:00:00.000Z', '4.7.2');
  }

  // 22. Seed the built-in templates through v4's REAL listings — with the
  //     plugin system initialized first, exactly as the oracle initializes it,
  //     so the system-prompt registry's built-ins are BAKED into the committed
  //     fixture and the report-time `findAllForUser` seeding is a no-op on
  //     both sides (v5's report is read-only — the recorded divergence).
  const { initializePlugins } = await import('@/lib/startup/plugin-initialization');
  await initializePlugins();
  await repos.promptTemplates.findAllForUser(spec.userId);
  await repos.roleplayTemplates.findAllForUser(spec.userId);

  // 23. llm_logs — the wire records. Distinct durations (fractional average),
  //     cacheUsage arms, a failure, an unmeasured row, a usage-less row, a
  //     deleted-profile attribution, and profile-less rows for the
  //     approximate arm.
  const mkUsage = (p: number, c: number) => ({
    promptTokens: p,
    completionTokens: c,
    totalTokens: p + c,
  });
  const logRows: Array<Record<string, unknown>> = [
    // CHAT_MESSAGE on the workhorse profile — three measured rows.
    {
      type: 'CHAT_MESSAGE',
      connectionProfileId: CONN,
      durationMs: 120,
      usage: mkUsage(1000, 200),
      cacheUsage: { cacheReadInputTokens: 800, cacheCreationInputTokens: 0 },
    },
    {
      type: 'CHAT_MESSAGE',
      connectionProfileId: CONN,
      durationMs: 480,
      usage: mkUsage(1100, 300),
      cacheUsage: { cacheReadInputTokens: 0, cacheCreationInputTokens: 1200 },
    },
    {
      type: 'CHAT_MESSAGE',
      connectionProfileId: CONN,
      durationMs: 301,
      usage: mkUsage(900, 150),
    },
    // A CHAT_MESSAGE on the cheap profile with NO duration (unmeasured arm).
    {
      type: 'CHAT_MESSAGE',
      connectionProfileId: CONN_CHEAP,
      durationMs: null,
      usage: mkUsage(500, 80),
    },
    // MEMORY_EXTRACTION on the cheap profile — one failure.
    {
      type: 'MEMORY_EXTRACTION',
      connectionProfileId: CONN_CHEAP,
      durationMs: 60,
      usage: mkUsage(400, 60),
    },
    {
      type: 'MEMORY_EXTRACTION',
      connectionProfileId: CONN_CHEAP,
      durationMs: 90,
      usage: mkUsage(420, 0),
      error: 'model refused the errand',
    },
    // IMAGE_GENERATION on the image profile — one usage-less.
    {
      type: 'IMAGE_GENERATION',
      imageProfileId: IMG_PROFILE,
      modelName: 'mock-image-model',
      durationMs: 1500,
      usage: mkUsage(50, 0),
    },
    {
      type: 'IMAGE_GENERATION',
      imageProfileId: IMG_PROFILE,
      modelName: 'mock-image-model',
      durationMs: 2500,
      usage: null,
    },
    // A row attributed to a profile that no longer exists (the
    // "(deleted profile …)" label arm).
    {
      type: 'CHAT_MESSAGE',
      connectionProfileId: DELETED_PROFILE,
      durationMs: 240,
      usage: mkUsage(700, 90),
    },
    // Profile-less rows (pre-4.9 vintage on a migrated DB; the approximate
    // grouping absorbs them either way).
    { type: 'TITLE_GENERATION', durationMs: 45, usage: mkUsage(120, 12) },
    { type: 'TITLE_GENERATION', durationMs: null, usage: null },
    { type: 'SUMMARIZATION', durationMs: 350, usage: mkUsage(2000, 400) },
  ];
  let logIdx = 0;
  for (const row of logRows) {
    const id = LOG_IDS[logIdx];
    const createdAt = `2026-07-${String(10 + logIdx).padStart(2, '0')}T00:00:00.000Z`;
    logIdx += 1;
    await repos.llmLogs.create(
      {
        userId: spec.userId,
        type: row.type,
        messageId: null,
        chatId: CHAT_1,
        characterId: LORIAN,
        autonomousRunId: null,
        provider: 'OPENAI_COMPATIBLE',
        modelName: (row.modelName as string) ?? 'mock-model',
        connectionProfileId: (row.connectionProfileId as string) ?? null,
        imageProfileId: (row.imageProfileId as string) ?? null,
        request: { messages: [], messageCount: 0, temperature: null, maxTokens: null },
        response: {
          content: row.error ? '' : 'hello',
          contentLength: row.error ? 0 : 5,
          error: (row.error as string) ?? null,
          finishReason: row.error ? null : 'stop',
        },
        usage: row.usage ?? null,
        cacheUsage: row.cacheUsage ?? null,
        durationMs: row.durationMs ?? null,
      } as never,
      { id, createdAt, updatedAt: createdAt } as never,
    );
  }
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  closeLLMLogsSQLiteClient();

  closeMountIndexSQLiteClient();
  await closeDatabase();

  // 24. The legacy llm-logs variant: the SAME partition minus the two 4.9
  //     profile-attribution columns (the approximate arm). Copy, then DROP
  //     COLUMN through the cipher driver with the raw-hex key form.
  copyFileSync(llmOut, llmLegacyOut);
  const cipherDriverPath = join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  const { createRequire } = await import('node:module');
  const requireCjs = createRequire(import.meta.url);
  const SqliteMC = requireCjs(cipherDriverPath) as new (path: string) => {
    pragma: (s: string) => unknown;
    exec: (s: string) => unknown;
    close: () => void;
  };
  const keyHex = Buffer.from(spec.testPepperBase64, 'base64').toString('hex');
  const legacy = new SqliteMC(llmLegacyOut);
  legacy.pragma(`key="x'${keyHex}'"`);
  legacy.exec('ALTER TABLE "llm_logs" DROP COLUMN "connectionProfileId"');
  legacy.exec('ALTER TABLE "llm_logs" DROP COLUMN "imageProfileId"');
  legacy.exec('VACUUM');
  legacy.close();

  // Journal files are build detritus — the committed family is the .db files.
  for (const out of [mainOut, mountOut, llmOut, llmLegacyOut]) {
    for (const suffix of ['-journal', '-wal', '-shm']) {
      const pth = out + suffix;
      if (existsSync(pth)) rmSync(pth);
    }
  }
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(
    `built almanack fixture: main=${mainOut} mount=${mountOut} llm=${llmOut} legacy=${llmLegacyOut} ` +
      `(3 characters, 4 chats, 4 memories, ${spec.jobs.length} background jobs, ${logRows.length} llm-logs)\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`almanack fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
