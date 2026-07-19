/**
 * P4.9f1 fixture builder (lane F1) — the committed test-pepper substrate for
 * the wardrobe-routes differential (`wardrobe_routes_equivalence`). Everything
 * is baked through v4's REAL repositories (character creates mint real vaults;
 * wardrobe items run the whole vault projection), so both differential sides
 * start from byte-identical state.
 *
 * ── TWO FILES, ONE FAMILY ───────────────────────────────────────────────────
 *   - `wardrobe-routes-main.db`  — users, characters, chats (one with a preset
 *     `equippedOutfit`), image profiles, one api_keys row, and the empty
 *     `files` / `background_jobs` tables the avatar legs touch.
 *   - `wardrobe-routes-mount.db` — the mount-index sibling: the two character
 *     vaults, Quilltap General (`instance_settings.generalMountPointId`), the
 *     project + group official stores, and the seeded wardrobe items across
 *     three tiers (character / General / project).
 *
 * ── THE STAGED TIERS ────────────────────────────────────────────────────────
 *   Aria's vault: W_TOP [top], W_DRESS [top,bottom] replace:true,
 *                 W_BOOTS [footwear], W_SHARED [accessories] (the id-collision
 *                 seed — Bramwell's vault holds the SAME id).
 *   Quilltap General: G_HAT [accessories] (equipped in chat1's preset — the
 *                 delete_ok cleanup arm), G_COAT [top] (the CRUD subject),
 *                 G_COMP [top] (composite of G_COAT — the update_cycle
 *                 subject), G_ARCH [bottom] ARCHIVED (list excludes it, the
 *                 single-item GET still resolves it — a pinned asymmetry).
 *   Project store: P_SCARF [accessories] (the tri-tier equip arm).
 *
 * Aria carries a `physicalDescription` (the preview hasAppearance=true arm);
 * Bramwell deliberately has none (the hasAppearance=false arm). The stranger's
 * character pins the ownership guards.
 *
 * Image profiles: P_OK (default, apiKeyId → the seeded api_keys row),
 * P_NOKEY (apiKeyId null), P_BADKEY (apiKeyId → a nonexistent row).
 *
 * Regenerate (Node 24, from the v4 checkout — if the checkout is dirty, from
 * the pinned worktree at the baseline) + the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this v5 worktree>
 *   cd /private/tmp/qt-v4-pin-616930db   # or ~/source/quilltap-server when clean
 *   QT_FIXTURE_WROUTES_MAIN=$W/crates/quilltap-web/tests/fixtures/wardrobe-routes-main.db \
 *   QT_FIXTURE_WROUTES_MOUNT=$W/crates/quilltap-web/tests/fixtures/wardrobe-routes-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-wardrobe-routes-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  otherUserId: string;
  ids: Record<string, string>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'wardrobe-routes.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;
  const I = spec.ids;

  const mainOut = process.env.QT_FIXTURE_WROUTES_MAIN;
  const mountOut = process.env.QT_FIXTURE_WROUTES_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_WROUTES_{MAIN,MOUNT} must point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }
  if (existsSync(mainOut + '.meta.json')) rmSync(mainOut + '.meta.json');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wroutes-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery, getCollection } =
    await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { CharacterSchema, ChatMetadataSchema, FileEntrySchema, BackgroundJobSchema } =
    await import('@/lib/schemas/types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { ApiKeySchema } = await import('@/lib/schemas/profile.types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
    ProjectDocMountLinkSchema,
    GroupDocMountLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('chats', ChatMetadataSchema);
  await ensureCollection('files', FileEntrySchema);
  await ensureCollection('background_jobs', BackgroundJobSchema);
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
    ['project_doc_mount_links', ProjectDocMountLinkSchema],
    ['group_doc_mount_links', GroupDocMountLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  const repos = getRepositories();
  // doc_mount_blobs has hand-written DDL (BLOB column) — trigger CREATE via a
  // read, or the Rust side (no lazy CREATE TABLE) fails the preview vault write.
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');

  // 1. The owner + the stranger.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.users.create(
    { username: 'stranger', email: null, name: 'Stranger' } as never,
    { id: spec.otherUserId, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Characters — each create mints a linked vault. Aria carries the
  //    appearance data; Bramwell deliberately none.
  await repos.characters.create(
    {
      name: 'Aria',
      userId: spec.userId,
      controlledBy: 'llm',
      physicalDescription: {
        id: 'ab000000-0000-4000-8000-000000000001',
        name: 'Aria portrait',
        headAndShouldersPrompt:
          'A tall aeronaut with copper hair, storm-grey eyes, and a high brass-clasped collar.',
        createdAt: TS,
        updatedAt: TS,
      },
    } as never,
    { id: I.aria, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.characters.create(
    { name: 'Bramwell', userId: spec.userId, controlledBy: 'llm' } as never,
    { id: I.bram, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.characters.create(
    { name: 'Vesper', userId: spec.otherUserId, controlledBy: 'llm' } as never,
    { id: I.strangerChar, createdAt: TS, updatedAt: TS } as never,
  );

  // 3. Quilltap General (an ordinary documents store + the instance_settings
  //    key — the transfers-fixture recipe).
  await repos.docMountPoints.create(
    {
      name: 'Quilltap General',
      basePath: '',
      mountType: 'database',
      storeType: 'documents',
      includePatterns: [],
      excludePatterns: [],
      enabled: true,
      lastScannedAt: null,
      scanStatus: 'idle',
      lastScanError: null,
      conversionStatus: 'idle',
      conversionError: null,
      fileCount: 0,
      chunkCount: 0,
      totalSizeBytes: 0,
    } as never,
    { id: I.generalMountPointId, createdAt: TS, updatedAt: TS },
  );
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'generalMountPointId',
    I.generalMountPointId,
  ]);

  // 4. The project + group (store-backed creates provision official stores).
  const project = await repos.projects.create(
    { name: 'The Lantern Project' } as never,
    { id: I.project, createdAt: TS, updatedAt: TS } as never,
  );
  const projectMp = project.officialMountPointId as string;
  if (!projectMp) throw new Error('project official store not minted');
  await repos.groups.create(
    { name: 'The Aeronauts' } as never,
    { id: I.group, createdAt: TS, updatedAt: TS } as never,
  );

  // 5. Wardrobe items via the REAL public create (full vault projection).
  const seedItem = async (
    characterId: string | null,
    id: string,
    title: string,
    types: string[],
    extra: Record<string, unknown> = {},
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
        isDefault: false,
        replace: false,
        migratedFromClothingRecordId: null,
        archivedAt: null,
        ...extra,
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  };

  // Aria's vault.
  await seedItem(I.aria, I.wTop, 'Brass-button blouse', ['top']);
  await seedItem(I.aria, I.wDress, 'Aviator dress', ['top', 'bottom'], { replace: true });
  await seedItem(I.aria, I.wBoots, 'Lantern boots', ['footwear']);
  await seedItem(I.aria, I.wShared, 'Shared cravat', ['accessories']);
  // The collision twin in Bramwell's vault (SAME id).
  await seedItem(I.bram, I.wShared, 'Shared cravat (Bramwell)', ['accessories']);

  // Quilltap General.
  await seedItem(null, I.gHat, 'General topper', ['accessories']);
  await seedItem(null, I.gCoat, 'General greatcoat', ['top'], {
    description: 'A general-issue greatcoat',
  });
  await seedItem(null, I.gComp, 'General ensemble', ['top'], {
    componentItemIds: [I.gCoat],
  });
  await seedItem(null, I.gArch, 'Retired breeches', ['bottom'], { archivedAt: TS });

  // The project store's Wardrobe/ + P_SCARF.
  const { ensureProjectWardrobeFolder } = await import('@/lib/mount-index/project-wardrobe');
  const { createProjectWardrobeItem } = await import(
    '@/lib/database/repositories/vault-overlay/wardrobe-writes'
  );
  await ensureProjectWardrobeFolder(projectMp);
  await createProjectWardrobeItem(projectMp, {
    id: I.pScarf,
    characterId: null,
    title: 'Project scarf',
    description: null,
    imagePrompt: null,
    types: ['accessories'],
    componentItemIds: [],
    appropriateness: null,
    isDefault: false,
    replace: false,
    migratedFromClothingRecordId: null,
    archivedAt: null,
    createdAt: TS,
    updatedAt: TS,
  } as never);

  // 6. Chats. chat1 = the project chat with the preset outfit (W_TOP worn,
  //    G_HAT accessory — the delete_ok cleanup subject); chat2 = bare.
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Fitting Room One',
      projectId: I.project,
      participants: [
        {
          id: 'fa000000-0000-4000-8000-000000000001',
          type: 'CHARACTER',
          characterId: I.aria,
          createdAt: TS,
          updatedAt: TS,
        },
      ],
      equippedOutfit: {
        [I.aria]: {
          top: [I.wTop],
          bottom: [],
          footwear: [],
          accessories: [I.gHat],
        },
      },
    } as never,
    { id: I.chat1, createdAt: TS, updatedAt: TS },
  );
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Bare Deck',
      participants: [
        {
          id: 'fa000000-0000-4000-8000-000000000002',
          type: 'CHARACTER',
          characterId: I.aria,
          createdAt: TS,
          updatedAt: TS,
        },
        {
          id: 'fa000000-0000-4000-8000-000000000003',
          type: 'CHARACTER',
          characterId: I.bram,
          createdAt: TS,
          updatedAt: TS,
        },
      ],
    } as never,
    { id: I.chat2, createdAt: TS, updatedAt: TS },
  );

  // 7. Image profiles + the one api_keys row.
  await repos.imageProfiles.create(
    {
      userId: spec.userId,
      name: 'Portrait OK',
      provider: 'OPENAI',
      modelName: 'dall-e-3',
      apiKeyId: I.apiKey,
      baseUrl: null,
      parameters: { quality: 'hd' },
      isDefault: true,
      isDangerousCompatible: false,
      tags: [],
    } as never,
    { id: I.profileOk, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.imageProfiles.create(
    {
      userId: spec.userId,
      name: 'No Key',
      provider: 'OPENAI',
      modelName: 'dall-e-3',
      apiKeyId: null,
      baseUrl: null,
      parameters: {},
      isDefault: false,
      isDangerousCompatible: false,
      tags: [],
    } as never,
    { id: I.profileNoKey, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.imageProfiles.create(
    {
      userId: spec.userId,
      name: 'Bad Key',
      provider: 'OPENAI',
      modelName: 'dall-e-3',
      apiKeyId: I.apiKeyMissing,
      baseUrl: null,
      parameters: {},
      isDefault: false,
      isDangerousCompatible: false,
      tags: [],
    } as never,
    { id: I.profileBadKey, createdAt: TS, updatedAt: TS } as never,
  );
  const apiKeyCol = await getCollection('api_keys');
  await apiKeyCol.insertOne(
    ApiKeySchema.parse({
      id: I.apiKey,
      userId: spec.userId,
      label: 'portrait-key',
      provider: 'OPENAI',
      key_value: 'sk-synthetic-portrait-key',
      isActive: true,
      lastUsed: null,
      createdAt: TS,
      updatedAt: TS,
    }) as never,
  );

  // 8. The sidecar (minted mount ids, for debugging + future consumers).
  const aria = await repos.characters.findByIdRaw(I.aria);
  const bram = await repos.characters.findByIdRaw(I.bram);
  writeFileSync(
    mainOut + '.meta.json',
    JSON.stringify({
      ariaVault: aria?.characterDocumentMountPointId ?? null,
      bramVault: bram?.characterDocumentMountPointId ?? null,
      projectMp,
    }),
  );

  closeMountIndexSQLiteClient();
  await closeDatabase();
  for (const out of [mainOut, mountOut]) {
    const j = out + '-journal';
    if (existsSync(j)) rmSync(j);
  }

  process.stderr.write(
    `built wardrobe-routes fixture: ${mainOut} (+${mountOut}); project store ${projectMp}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`wardrobe-routes fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
