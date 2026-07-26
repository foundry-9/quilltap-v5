/**
 * The committed chat-CAST + avatar-override fixture builder (P4.9E1A) — the
 * shared test-pepper substrate for `chat_cast_routes_equivalence` (tier 2).
 *
 * Everything is baked through v4's REAL repositories (never hand-rolled SQL —
 * `project-store-fixture-needs-real-create`), with every id and timestamp pinned
 * so both differential sides read byte-identical rows: only the MUTATING cases
 * mint anything, and the harness tokenizes those.
 *
 * ## What is in it and why
 *
 *   1. One user + an api key + TWO connection profiles. `CONN_A` is
 *      `isDefault: true` so `resolveFallbackConnectionProfile`'s FIRST leg
 *      (`connections.findDefault`) is the one the add path exercises; `CONN_B`
 *      is the destination of the connection-profile CHANGE that fires the
 *      Prospero announcement.
 *   2. One image profile, `isDefault: true` — both the `imageProfileId` gate on
 *      update-participant and `toggle-avatar-generation`'s "no chat-level
 *      profile → the marked default" resolution.
 *   3. Six characters, every vault provisioned by v4's own `create`:
 *        - ARIA  — `controlledBy:'user'`, impersonating in CHAT_MAIN and the
 *                  ACTIVE typist (so the `controlledBy:'llm'` flip hits the
 *                  promotion branch).
 *        - GAIL  — `controlledBy:'user'`, the SECOND impersonator, so the
 *                  promotion has someone to promote (`newImpersonating[0]`).
 *        - BRAM  — `controlledBy:'llm'`, carries `systemPrompts` + a
 *                  personality so the identity stack it compiles is non-empty,
 *                  a per-chat `talkativeness` override (the absent-key arm),
 *                  and an avatarOverride on CHAT_MAIN pointing at IMG_A.
 *        - CLEO  — `controlledBy:'llm'`, an avatarOverride pointing at IMG_B, and
 *                  the `status:'silent'` / `status:'absent'` participant in the
 *                  other two chats.
 *        - DORA  — NOT in any chat: the add-participant subject. Carries a TAG
 *                  (so the add path's chat-tag merge fires) and TWO default
 *                  wardrobe items (so `mode:'default'` writes a real outfit).
 *        - ERIS  — a SOFT-REMOVED participant of CHAT_MAIN: the reactivation
 *                  branch. Also carries a default wardrobe item.
 *   4. Two legacy `files` rows — the `resolveCharacterAvatar` fallback branch.
 *      (`FileEntrySchema.sha256` is `z.string().length(64)`, i.e. REQUIRED, so a
 *      sha-less legacy file is unrepresentable and `linkSummary: null` is
 *      unreachable through this path — noted rather than faked.) Neither sha has
 *      a `doc_mount_files` twin, so the summary resolves to the empty shape on
 *      both sides without any mount-index seeding.
 *   5. Three chats:
 *        - CHAT_MAIN  — ARIA(user) + GAIL(user) + BRAM(llm) + CLEO(llm) +
 *          ERIS(llm, `status:'removed'`), `impersonatingParticipantIds:
 *          [P_ARIA, P_GAIL]`, `activeTypingParticipantId: P_ARIA`.
 *        - CHAT_QUIET — BRAM(llm) + CLEO(llm, `status:'silent'`) + ARIA(user),
 *          NOBODY impersonating and `activeTypingParticipantId: null`, so the
 *          `controlledBy:'user'` flip takes the "nobody is typing → become the
 *          typist" branch the main chat cannot reach. `avatarGenerationEnabled:
 *          true` so the toggle's OFF direction has a home.
 *        - CHAT_SOLO  — BRAM(llm) + CLEO(llm, `status:'absent'`): exactly ONE
 *          present character, so `remove-participant` hits the action layer's
 *          last-CHARACTER guard AND the chat-PUT bag (which has no such guard)
 *          reaches the repo's last-PARTICIPANT throw. The absent CLEO also
 *          gives `add-participant` its "already in this chat (currently
 *          deactivated)" arm.
 *
 * The minted character-vault ids are echoed to a JSON sidecar
 * (QT_FIXTURE_CAST_MAIN + '.meta.json') the oracle case and the Rust harness
 * both read.
 *
 * Regenerate (Node 24, from the v4 checkout) — the committed .db files land in
 * place:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   TZ=UTC \
 *   QT_FIXTURE_CAST_MAIN=$W/crates/quilltap-web/tests/fixtures/chat-cast-main.db \
 *   QT_FIXTURE_CAST_MOUNT=$W/crates/quilltap-web/tests/fixtures/chat-cast-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-chat-cast-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  ids: Record<string, string>;
}

async function main(): Promise<void> {
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) {
    throw new Error(`chat-cast fixture must be built under TZ=UTC (getTimezoneOffset=${offset})`);
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'chat-cast.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;
  const I = spec.ids;

  const mainOut = process.env.QT_FIXTURE_CAST_MAIN;
  const mountOut = process.env.QT_FIXTURE_CAST_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_CAST_{MAIN,MOUNT} must point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }
  if (existsSync(mainOut + '.meta.json')) rmSync(mainOut + '.meta.json');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cast-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, getCollection, closeDatabase, rawQuery } =
    await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const {
    CharacterSchema,
    ChatMetadataSchema,
    FileEntrySchema,
    BackgroundJobSchema,
    TagSchema,
  } = await import('@/lib/schemas/types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { ApiKeySchema, ImageProfileSchema } = await import('@/lib/schemas/profile.types');
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
  await ensureCollection('background_jobs', BackgroundJobSchema);
  await ensureCollection('api_keys', ApiKeySchema);
  await ensureCollection('image_profiles', ImageProfileSchema);
  await ensureCollection('tags', TagSchema);

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
  // doc_mount_blobs has hand-written DDL (BLOB column) — trigger the CREATE via a read.
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');
  // `instance_settings` is lazily created in v4 and read by the wardrobe
  // resolvers; materialize it EMPTY so both sides read the same schema (v4
  // tolerates the missing table and logs, v5 has no lazy CREATE).
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );

  // 1. The user, the api key, the two connection profiles, the image profile.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );
  const apiKeyCol = await getCollection('api_keys');
  await apiKeyCol.insertOne(
    ApiKeySchema.parse({
      id: I.apiKey,
      userId: spec.userId,
      label: 'mock-key',
      provider: 'OPENAI',
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
      apiKeyId: I.apiKey,
      isDefault: true,
    } as never,
    { id: I.connA, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.connections.create(
    {
      userId: spec.userId,
      name: 'Second Desk',
      provider: 'OPENAI',
      modelName: 'gpt-4o-mini',
      apiKeyId: I.apiKey,
    } as never,
    { id: I.connB, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.imageProfiles.create(
    {
      userId: spec.userId,
      name: 'Fixture Images',
      provider: 'OPENAI',
      modelName: 'dall-e-3',
      apiKeyId: I.apiKey,
      baseUrl: null,
      parameters: {},
      isDefault: true,
      isDangerousCompatible: false,
      tags: [],
    } as never,
    { id: I.imageProfile, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.tags.create(
    { userId: spec.userId, name: 'Aeronauts', nameLower: 'aeronauts', quickHide: false } as never,
    { id: I.tag, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Characters — `create` owns vault provisioning.
  const vaults: Record<string, string> = {};
  const mkChar = async (
    key: string,
    id: string,
    name: string,
    controlledBy: string,
    extra: object = {},
  ) => {
    await repos.characters.create(
      { name, userId: spec.userId, controlledBy, ...extra } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
    const raw = await repos.characters.findByIdRaw(id);
    const vault = raw?.characterDocumentMountPointId as string | null;
    if (!vault) throw new Error(`vault not minted for ${name}`);
    vaults[key] = vault;
  };
  await mkChar('aria', I.aria, 'Aria', 'user', {
    description: 'The operator’s aeronaut persona.',
    personality: 'Precise, wry, and fond of a well-turned postscript.',
  });
  await mkChar('gail', I.gail, 'Gail', 'user');
  await mkChar('bram', I.bram, 'Bramwell', 'llm', {
    description: 'A lamplighter who keeps to the far side of the river.',
    personality: 'Formal to a fault; speaks in short, weighted sentences.',
    manifesto: 'I keep the lamps lit, whatever the hour asks of me.',
    talkativeness: 0.6,
    systemPrompts: [
      {
        id: I.bramPrompt,
        name: 'Lamplighter',
        content: 'Speak as the lamplighter.',
        isDefault: true,
        createdAt: TS,
        updatedAt: TS,
      },
      {
        id: I.bramPromptAlt,
        name: 'Ferryman',
        content: 'Speak as the ferryman.',
        isDefault: false,
        createdAt: TS,
        updatedAt: TS,
      },
    ],
  });
  await mkChar('cleo', I.cleo, 'Cleo', 'llm', {
    description: 'A cartographer with an unhelpful sense of scale.',
  });
  await mkChar('dora', I.dora, 'Dora', 'llm', {
    description: 'An airship quartermaster.',
    tags: [I.tag],
  });
  await mkChar('eris', I.eris, 'Eris', 'llm', {
    description: 'A signaller who left and may yet return.',
  });

  // 3. Two default wardrobe items for DORA and one for ERIS, so `mode:'default'`
  //    writes a real outfit rather than four empty slots.
  const seedItem = async (
    characterId: string,
    id: string,
    title: string,
    types: string[],
  ): Promise<void> => {
    await repos.wardrobe.create(
      {
        characterId,
        title,
        description: null,
        imagePrompt: null,
        types,
        componentItemIds: [],
        appropriateness: null,
        isDefault: true,
        replace: false,
        migratedFromClothingRecordId: null,
        archivedAt: null,
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  };
  await seedItem(I.dora, I.wDoraTop, 'Quartermaster’s coat', ['top']);
  await seedItem(I.dora, I.wDoraBoots, 'Deck boots', ['footwear']);
  await seedItem(I.eris, I.wErisCoat, 'Signaller’s cape', ['top']);

  // 4. Two files: one WITH a sha256, one without.
  const filesCol = await getCollection('files');
  const mkFile = async (id: string, filename: string, sha256: string) => {
    await filesCol.insertOne(
      FileEntrySchema.parse({
        id,
        userId: spec.userId,
        sha256,
        originalFilename: filename,
        mimeType: 'image/webp',
        size: 1024,
        source: 'UPLOADED',
        category: 'IMAGE',
        createdAt: TS,
        updatedAt: TS,
      }) as never,
    );
  };
  await mkFile(I.imgA, 'bram-portrait.webp', 'a'.repeat(64));
  await mkFile(I.imgB, 'cleo-portrait.webp', 'b'.repeat(64));

  // 5. The three chats.
  const mkParticipant = (
    id: string,
    characterId: string,
    controlledBy: string,
    extra: object = {},
  ) => ({
    id,
    type: 'CHARACTER',
    characterId,
    controlledBy,
    status: 'active',
    isActive: true,
    connectionProfileId: controlledBy === 'user' ? null : I.connA,
    imageProfileId: null,
    selectedSystemPromptId: null,
    displayOrder: 0,
    hasHistoryAccess: false,
    // NOTE: `joinScenario` is deliberately ABSENT rather than an explicit
    // `null`. v4 keeps a stored `null` (`.nullable().optional()`) where v5's
    // `ChatParticipant` collapses it — the escalated gap the differential's
    // `PARTICIPANT_NULL_COLLAPSE` tripwire records. Seeding it absent keeps the
    // BASELINE identical on both sides, so the tripwire only fires on the rows
    // this lane's own writes produce.
    createdAt: TS,
    updatedAt: TS,
    ...extra,
  });

  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'The Salon',
      chatType: 'salon',
      contextSummary: null,
      participants: [
        mkParticipant(I.pAria, I.aria, 'user', { displayOrder: 0 }),
        mkParticipant(I.pGail, I.gail, 'user', { displayOrder: 1 }),
        mkParticipant(I.pBram, I.bram, 'llm', {
          displayOrder: 2,
          talkativeness: 0.9,
          selectedSystemPromptId: I.bramPrompt,
        }),
        mkParticipant(I.pCleo, I.cleo, 'llm', { displayOrder: 3 }),
        mkParticipant(I.pEris, I.eris, 'llm', {
          displayOrder: 4,
          status: 'removed',
          isActive: false,
          removedAt: TS,
        }),
      ],
      impersonatingParticipantIds: [I.pAria, I.pGail],
      activeTypingParticipantId: I.pAria,
      tags: [],
    } as never,
    { id: I.chatMain, createdAt: TS, updatedAt: TS } as never,
  );

  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'The Quiet Room',
      chatType: 'salon',
      contextSummary: null,
      participants: [
        mkParticipant(I.qBram, I.bram, 'llm', { displayOrder: 0 }),
        mkParticipant(I.qCleo, I.cleo, 'llm', { displayOrder: 1, status: 'silent' }),
        mkParticipant(I.qAria, I.aria, 'user', { displayOrder: 2 }),
      ],
      impersonatingParticipantIds: [],
      activeTypingParticipantId: null,
      avatarGenerationEnabled: true,
      tags: [],
    } as never,
    { id: I.chatQuiet, createdAt: TS, updatedAt: TS } as never,
  );

  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'The Solo Watch',
      chatType: 'salon',
      contextSummary: null,
      participants: [
        mkParticipant(I.sBram, I.bram, 'llm', { displayOrder: 0 }),
        mkParticipant(I.sCleo, I.cleo, 'llm', {
          displayOrder: 1,
          status: 'absent',
          isActive: false,
        }),
      ],
      impersonatingParticipantIds: [],
      activeTypingParticipantId: null,
      tags: [],
    } as never,
    { id: I.chatSolo, createdAt: TS, updatedAt: TS } as never,
  );

  // 6. Pre-existing avatar overrides for CHAT_MAIN (the `get-avatars` read and
  //    `set-avatar`'s replace-in-place branch).
  await repos.characters.update(I.bram, {
    avatarOverrides: [{ chatId: I.chatMain, imageId: I.imgA }],
  } as never);
  await repos.characters.update(I.cleo, {
    avatarOverrides: [{ chatId: I.chatMain, imageId: I.imgB }],
  } as never);

  // Materialize the empty background-jobs + chat_messages tables so both sides'
  // fresh copies read identical schemas (v4 lazily creates a collection on first
  // touch; the Rust side has no lazy CREATE TABLE).
  await repos.backgroundJobs.findByUserId(spec.userId, 'PENDING');
  for (const chatId of [I.chatMain, I.chatQuiet, I.chatSolo]) {
    await repos.chats.getMessages(chatId);
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  writeFileSync(mainOut + '.meta.json', JSON.stringify({ vaults }));
  process.stderr.write(
    `built chat-cast fixture: main=${mainOut} mount=${mountOut} vaults=${JSON.stringify(vaults)}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-cast fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
