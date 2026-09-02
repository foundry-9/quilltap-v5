/**
 * P4.6ao fixture builder (lane A) — the committed test-pepper substrate shared by
 * the token/cost + background-generation differentials:
 *
 *   - `cost_background_routes_equivalence` (units 1–2: `chatGetCost` +
 *     `chatRegenerateBackground`)
 *   - `title_update_tier3_equivalence`      (unit 3: the TITLE_UPDATE handler)
 *
 * Everything is baked via v4's REAL repos so the DDL + row shapes are
 * production-identical. Three users carry the three `storyBackgroundsSettings`
 * arms the regenerate edge distinguishes (v4 reads settings by USER, so one
 * settings row per arm is the only way to seed all three in one fixture):
 *
 *   - `userEnabledId`   — backgrounds ENABLED, `defaultImageProfileId` → a profile
 *                         WITH an apiKeyId (resolves) → the success arm.
 *   - `userDisabledId`  — backgrounds DISABLED                          → arm 1.
 *   - `userNoProfileId` — backgrounds ENABLED, no profile of any tier   → arm 2.
 *
 * (`chatNoCharsId` under `userEnabledId` gives arm 3.) The handler never
 * cross-checks chat ownership against the settings user — the route's own
 * `findById` 404 is the only ownership-ish gate — so a single chat can be driven
 * under different users to select the arm.
 *
 * [P4.D146 / v4 `70505745a`] `chatMixedStatusId` carries ONE PARTICIPANT PER
 * STATUS (active / silent / absent / removed) and `chatAllLeftId` carries only
 * the two who left. Both story-background enqueue sites filter on
 * `isParticipantPresent`, and a fixture whose participants are all `active`
 * — which this one was — cannot discriminate that filter at all.
 *
 * `chatDetailedId` carries the Unit-1 detailed-breakdown corpus, including the two
 * quirks the order calls out: a message whose `tokenCount` DIVERGES from
 * prompt+completion, and a system event with a NULL `estimatedCostUSD` (v4's
 * repo read OMITS a NULL nullable-optional column, so the JS object sees
 * `undefined` — and the per-event `source` ternary tests `!== null` ONLY, so that
 * row lands `cost: null` with `source: 'registry'`).
 *
 * A mount-index sibling is built alongside the main partition: `repos.characters
 * .create` mints a character vault there (the P4.6ab lesson — vault mounts ride
 * with characters), and `resolveImageProfileForChat`'s project tier reads the
 * store overlay. No fixture row lives in the llm-logs partition.
 *
 * Regenerate (Node 24, from the v4 checkout) + re-copy the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CB_MAIN=$W/crates/quilltap-web/tests/fixtures/cost-background-main.db \
 *   QT_FIXTURE_CB_MOUNT=$W/crates/quilltap-web/tests/fixtures/cost-background-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-cost-background-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface DetailedMessageSeed {
  id: string;
  kind: 'message' | 'system';
  role?: string;
  content?: string;
  systemEventType?: string;
  description?: string;
  promptTokens: number | null;
  completionTokens: number | null;
  tokenCount?: number | null;
  totalTokens?: number | null;
  provider?: string;
  modelName?: string;
  estimatedCostUSD?: number | null;
  createdAt: string;
}
interface PlainMessageSeed {
  id: string;
  role: string;
  content: string;
  createdAt: string;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userEnabledId: string;
  userDisabledId: string;
  userNoProfileId: string;
  apiKeyId: string;
  imageProfileId: string;
  connectionProfileId: string;
  characterAId: string;
  characterBId: string;
  characterCId: string;
  characterDId: string;
  chatFullId: string;
  chatLegacyId: string;
  chatEmptyId: string;
  chatDetailedId: string;
  chatRegenId: string;
  chatNoCharsId: string;
  chatTitleId: string;
  chatHelpId: string;
  chatAutonomousId: string;
  chatRenamedId: string;
  chatMixedStatusId: string;
  chatAllLeftId: string;
  missingId: string;
  detailedMessages: DetailedMessageSeed[];
  titleMessages: PlainMessageSeed[];
  helpMessages: PlainMessageSeed[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'cost-background-web.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_CB_MAIN;
  const mountOut = process.env.QT_FIXTURE_CB_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_CB_MAIN and QT_FIXTURE_CB_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cb-fixture-'));
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
  const { CharacterSchema, ChatMetadataSchema, FileEntrySchema, BackgroundJobSchema } =
    await import('@/lib/schemas/types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
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
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('chats', ChatMetadataSchema);
  await ensureCollection('files', FileEntrySchema);
  // The regenerate edge + the title handler both ENQUEUE — the table must exist
  // before either side writes into a copy (the port does not own DDL).
  await ensureCollection('background_jobs', BackgroundJobSchema);

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

  const repos = getRepositories();
  // doc_mount_blobs has hand-written DDL (BLOB column) — trigger CREATE via a read.
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');

  // 1. The three users (one per storyBackgroundsSettings arm).
  for (const [id, username] of [
    [spec.userEnabledId, 'friday'],
    [spec.userDisabledId, 'saturday'],
    [spec.userNoProfileId, 'sunday'],
  ] as const) {
    await repos.users.create(
      { username, email: null, name: username } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  }

  // 2. The resolvable image profile (userEnabledId's default). No `api_keys` row
  //    is seeded: `resolveImageProfileForChat` only tests `profile.apiKeyId` for
  //    truthiness, and the factory exposes no api-keys repo (the avatar-job
  //    fixture's `apiKeys` map is likewise reference-only).
  await repos.imageProfiles.create(
    {
      userId: spec.userEnabledId,
      name: 'Fixture Images',
      provider: 'OPENAI',
      modelName: 'dall-e-3',
      apiKeyId: spec.apiKeyId,
      baseUrl: null,
      parameters: {},
      isDefault: true,
      isDangerousCompatible: false,
      tags: [],
    } as never,
    { id: spec.imageProfileId, createdAt: TS, updatedAt: TS } as never,
  );

  // 3. The connection profile the TITLE_UPDATE payload names.
  await repos.connections.create(
    {
      userId: spec.userEnabledId,
      name: 'Fixture Cheap',
      provider: 'OPENAI_COMPATIBLE',
      modelName: 'mock-cheap-model',
      baseUrl: 'http://127.0.0.1:1/v1',
      apiKeyId: spec.apiKeyId,
      isCheap: true,
    } as never,
    { id: spec.connectionProfileId, createdAt: TS, updatedAt: TS } as never,
  );

  // 4. Chat settings — one row per user, one arm each.
  const cheapLLMSettings = {
    strategy: 'PROVIDER_CHEAPEST',
    fallbackToLocal: false,
    embeddingProvider: 'OPENAI',
  };
  await repos.chatSettings.create(
    {
      userId: spec.userEnabledId,
      cheapLLMSettings,
      storyBackgroundsSettings: { enabled: true, defaultImageProfileId: spec.imageProfileId },
    } as never,
    { id: 'c8000000-0000-4000-8000-000000000001', createdAt: TS, updatedAt: TS } as never,
  );
  await repos.chatSettings.create(
    {
      userId: spec.userDisabledId,
      cheapLLMSettings,
      storyBackgroundsSettings: { enabled: false, defaultImageProfileId: null },
    } as never,
    { id: 'c8000000-0000-4000-8000-000000000002', createdAt: TS, updatedAt: TS } as never,
  );
  await repos.chatSettings.create(
    {
      userId: spec.userNoProfileId,
      cheapLLMSettings,
      storyBackgroundsSettings: { enabled: true, defaultImageProfileId: null },
    } as never,
    { id: 'c8000000-0000-4000-8000-000000000003', createdAt: TS, updatedAt: TS } as never,
  );

  // 5. Two characters (REAL create → each mints its vault mount into the
  //    mount-index sibling).
  for (const [id, name] of [
    [spec.characterAId, 'Lorian'],
    [spec.characterBId, 'Riya'],
    // [P4.D146 / v4 70505745a] The two who leave the scene: the story-background
    // enqueue gates on `isParticipantPresent`, so a fixture whose participants
    // are ALL `active` cannot discriminate the filter at all.
    [spec.characterCId, 'Marisol'],
    [spec.characterDId, 'Teodor'],
  ] as const) {
    await repos.characters.create(
      {
        name,
        userId: spec.userEnabledId,
        title: null,
        description: `${name}, of the fixture.`,
        personality: null,
        aliases: [],
        controlledBy: 'llm',
        talkativeness: 0.5,
        defaultConnectionProfileId: spec.connectionProfileId,
        isFavorite: false,
        npc: false,
        tags: [],
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  }

  // 6. Chats.
  let pIdx = 0;
  const participant = (
    characterId: string,
    characterName: string,
    status: 'active' | 'silent' | 'absent' | 'removed' = 'active',
  ) => ({
    id: `c9000000-0000-4000-8000-${String(++pIdx).padStart(12, '0')}`,
    type: 'CHARACTER',
    characterId,
    characterName,
    controlledBy: 'llm',
    displayOrder: 0,
    // v4's `isActive` is the LEGACY flag; `status` is what
    // `isParticipantPresent` reads. Kept consistent with the status so the row
    // is a shape v4's own participant ops would produce.
    isActive: status === 'active' || status === 'silent',
    status,
    hasHistoryAccess: false,
    createdAt: TS,
    updatedAt: TS,
  });
  const bothCharacters = () => [
    participant(spec.characterAId, 'Lorian'),
    participant(spec.characterBId, 'Riya'),
  ];
  /** [P4.D146] One participant per status — a missing filter shows up as extra ids. */
  const oneOfEachStatus = () => [
    participant(spec.characterAId, 'Lorian', 'active'),
    participant(spec.characterBId, 'Riya', 'silent'),
    participant(spec.characterCId, 'Marisol', 'absent'),
    participant(spec.characterDId, 'Teodor', 'removed'),
  ];
  /** [P4.D146] Nobody left in the scene — the enqueue must not fire at all. */
  const nobodyPresent = () => [
    participant(spec.characterCId, 'Marisol', 'absent'),
    participant(spec.characterDId, 'Teodor', 'removed'),
  ];

  const mkChat = (
    id: string,
    title: string,
    extra: Record<string, unknown> = {},
    userId: string = spec.userEnabledId,
  ) =>
    repos.chats.create(
      {
        userId,
        title,
        chatType: 'salon',
        contextSummary: null,
        participants: [],
        tags: [],
        ...extra,
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );

  // Unit 1 — the aggregate arms.
  await mkChat(spec.chatFullId, 'A Fully Costed Affair', {
    totalPromptTokens: 1200,
    totalCompletionTokens: 800,
    estimatedCostUSD: 0.0345,
    priceSource: 'openrouter',
  });
  // Legacy: a cost with NO stored priceSource → the service infers 'registry'.
  await mkChat(spec.chatLegacyId, 'A Legacy Ledger', {
    totalPromptTokens: 300,
    totalCompletionTokens: 100,
    estimatedCostUSD: 0.0021,
    priceSource: null,
  });
  await mkChat(spec.chatEmptyId, 'An Untouched Ledger', {});
  await mkChat(spec.chatDetailedId, 'The Itemized Bill', {});

  // Unit 2 — the regenerate arms.
  await mkChat(spec.chatRegenId, 'A Scene Worth Painting', { participants: bothCharacters() });
  await mkChat(spec.chatNoCharsId, 'An Empty Stage', { participants: [] });

  // Unit 3 — the TITLE_UPDATE arms.
  await mkChat(spec.chatTitleId, 'New Chat', { participants: bothCharacters() });
  await mkChat(spec.chatHelpId, 'Help: Quilltap', {
    chatType: 'help',
    participants: bothCharacters(),
  });
  await mkChat(spec.chatAutonomousId, 'New Chat', {
    chatType: 'autonomous',
    participants: bothCharacters(),
  });
  await mkChat(spec.chatRenamedId, 'A Title I Chose Myself', {
    isManuallyRenamed: true,
    participants: bothCharacters(),
  });

  // [P4.D146 / v4 70505745a] The participant-presence arms, shared by BOTH
  // enqueue sites (the auto-trigger inside TITLE_UPDATE and the manual
  // `?action=regenerate-background` route).
  await mkChat(spec.chatMixedStatusId, 'New Chat', { participants: oneOfEachStatus() });
  await mkChat(spec.chatAllLeftId, 'New Chat', { participants: nobodyPresent() });

  // 7. Messages. `getMessageCount` first so `chat_messages` exists even if a
  //    later seed loop is empty.
  await repos.chats.getMessageCount(spec.chatFullId);

  for (const m of spec.detailedMessages) {
    if (m.kind === 'message') {
      await repos.chats.addMessage(spec.chatDetailedId, {
        type: 'message',
        id: m.id,
        role: m.role,
        content: m.content,
        attachments: [],
        promptTokens: m.promptTokens,
        completionTokens: m.completionTokens,
        tokenCount: m.tokenCount ?? null,
        createdAt: m.createdAt,
      } as never);
    } else {
      await repos.chats.addMessage(spec.chatDetailedId, {
        type: 'system',
        id: m.id,
        systemEventType: m.systemEventType,
        description: m.description,
        promptTokens: m.promptTokens,
        completionTokens: m.completionTokens,
        totalTokens: m.totalTokens ?? null,
        provider: m.provider ?? null,
        modelName: m.modelName ?? null,
        estimatedCostUSD: m.estimatedCostUSD ?? null,
        createdAt: m.createdAt,
      } as never);
    }
  }

  const seedPlain = async (chatId: string, rows: PlainMessageSeed[]) => {
    for (const m of rows) {
      await repos.chats.addMessage(chatId, {
        type: 'message',
        id: m.id,
        role: m.role,
        content: m.content,
        attachments: [],
        createdAt: m.createdAt,
      } as never);
    }
  };
  // The title arms all read the same visible conversation; the renamed +
  // autonomous chats reuse the normal corpus so only the arm under test differs.
  await seedPlain(spec.chatTitleId, spec.titleMessages);
  await seedPlain(spec.chatHelpId, spec.helpMessages);
  await seedPlain(
    spec.chatAutonomousId,
    spec.titleMessages.map((m) => ({ ...m, id: m.id.replace(/^c7000000/, 'c7a00000') })),
  );
  await seedPlain(
    spec.chatRenamedId,
    spec.titleMessages.map((m) => ({ ...m, id: m.id.replace(/^c7000000/, 'c7b00000') })),
  );
  // [P4.D146] The presence arms need a visible conversation, or the TITLE_UPDATE
  // handler takes the empty-conversation return and never reaches the enqueue.
  await seedPlain(
    spec.chatMixedStatusId,
    spec.titleMessages.map((m) => ({ ...m, id: m.id.replace(/^c7000000/, 'c7c00000') })),
  );
  await seedPlain(
    spec.chatAllLeftId,
    spec.titleMessages.map((m) => ({ ...m, id: m.id.replace(/^c7000000/, 'c7d00000') })),
  );

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built cost-background fixture: ${mainOut} (+${mountOut}) — 3 users, 1 image profile, ` +
      `4 characters, 12 chats, ${spec.detailedMessages.length} detailed rows\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`cost-background fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
