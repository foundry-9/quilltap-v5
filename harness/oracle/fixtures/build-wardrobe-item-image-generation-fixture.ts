/**
 * Fixture builder for the wardrobe item-image GENERATION tier-3 differential
 * (P4.D263; v4 `lib/wardrobe/item-image-generation.ts`, `7c8572869`, and the
 * `WARDROBE_ITEM_IMAGE_GENERATION` handler, `b3f937076`).
 *
 * Bakes, across TWO databases (main + mount-index):
 *   - the owner (a users row), four API keys and five image profiles (the
 *     designated one, the user default, an override, one whose key ROW is gone,
 *     and a dangerous-compatible understudy) — raw `api_keys` inserts through
 *     v4's `ApiKeySchema`, profiles through REAL `repos.imageProfiles.create`;
 *   - the owner's `chat_settings` row (logging ON, `wardrobeImageSettings`
 *     naming the designated profile) through REAL `repos.chatSettings.create`;
 *   - the owner's character (a physical description, she/her) and an
 *     ARCHIVED character, each with a linked vault (REAL
 *     `repos.characters.create`);
 *   - a "Quilltap General" store and a project with its official store;
 *   - wardrobe items: a garment, two outfit leaves, a hair item and an outfit
 *     whose components include a General archetype (the shared-archetype
 *     merge), a General garment, a project garment and the archived
 *     character's garment — REAL `repos.wardrobe.create` /
 *     `createProjectWardrobeItem`, pinned ids + timestamps.
 *
 * No pictures: the generation adds them. The llm-logs DB is a fresh scratch
 * file per scenario (both sides). The wear ledger rides in v4's MIGRATION
 * shape (C1 §6).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WIIG_MAIN=/tmp/qt-wiig-main.db QT_FIXTURE_WIIG_MOUNT=/tmp/qt-wiig-mount.db \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-wardrobe-item-image-generation-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface ItemSpec {
  id: string;
  home: 'character' | 'general' | 'project' | 'archived';
  title: string;
  types: string[];
  imagePrompt?: string;
  componentItemIds?: string[];
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  user: Record<string, unknown>;
  characterId: string;
  archivedCharacterId: string;
  projectId: string;
  generalMountPointId: string;
  characterTemplate: Record<string, unknown>;
  character: Record<string, unknown>;
  archivedCharacter: Record<string, unknown>;
  apiKeys: Array<{ id: string; provider: string; key_value: string }>;
  imageProfiles: Array<Record<string, unknown> & { id: string }>;
  items: Record<string, ItemSpec>;
  chatSettings: Record<string, unknown>;
}

const PINNED_TS = '2026-02-01T00:00:00.000Z';
const ARCHIVED_AT = '2026-03-01T00:00:00.000Z';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'wardrobe-item-image-generation-tier3.json'), 'utf8'),
  ) as Spec;

  const mainOut = process.env.QT_FIXTURE_WIIG_MAIN;
  const mountOut = process.env.QT_FIXTURE_WIIG_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_WIIG_MAIN and QT_FIXTURE_WIIG_MOUNT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-wiig-fixture-build-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  // A scratch llm-logs DB: the builder must never open the developer's own.
  process.env.SQLITE_LLM_LOGS_PATH = join(scratch, 'llm-logs.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, getCollection, closeDatabase, rawQuery } =
    await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { FileEntrySchema } = await import('@/lib/schemas/file.types');
  const { ImageProfileSchema, ApiKeySchema } = await import('@/lib/schemas/profile.types');
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
    GroupCharacterMemberSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  {
    const { WARDROBE_WEAR_STATS_DDL } = await import(
      '@/lib/database/backends/sqlite/wardrobe-wear-stats-ddl'
    );
    const raw = getRawDatabase();
    if (!raw) throw new Error('main DB handle unavailable for the wear ledger');
    for (const sql of WARDROBE_WEAR_STATS_DDL) raw.exec(sql);
  }
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('files', FileEntrySchema);
  await ensureCollection('image_profiles', ImageProfileSchema);
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
    ['group_character_members', GroupCharacterMemberSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
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
    'CREATE UNIQUE INDEX IF NOT EXISTS "idx_doc_mount_blobs_fileId" ON "doc_mount_blobs" ("fileId")',
  );

  const repos = getRepositories();
  const pin = (id: string) => ({ id, createdAt: PINNED_TS, updatedAt: PINNED_TS }) as never;

  await repos.users.create(spec.user as never, pin(spec.userId));
  const apiKeyCol = await getCollection('api_keys');
  for (const k of spec.apiKeys) {
    await apiKeyCol.insertOne(
      ApiKeySchema.parse({
        id: k.id,
        userId: spec.userId,
        label: k.key_value,
        provider: k.provider,
        key_value: k.key_value,
        isActive: true,
        lastUsed: null,
        createdAt: PINNED_TS,
        updatedAt: PINNED_TS,
      }) as never,
    );
  }
  for (const p of spec.imageProfiles) {
    const { id, ...rest } = p;
    await repos.imageProfiles.create(
      { userId: spec.userId, baseUrl: null, tags: [], ...rest } as never,
      pin(id),
    );
  }
  await repos.chatSettings.create(
    { userId: spec.userId, ...spec.chatSettings } as never,
    pin('d4d4d4d4-0001-4000-8000-000000000001'),
  );

  const character = (over: Record<string, unknown>) =>
    ({ ...spec.characterTemplate, ...over, userId: spec.userId }) as never;
  await repos.characters.create(character(spec.character), pin(spec.characterId));
  await repos.characters.create(character(spec.archivedCharacter), pin(spec.archivedCharacterId));

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
    pin(spec.generalMountPointId),
  );
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'generalMountPointId',
    spec.generalMountPointId,
  ]);
  await repos.projects.create({ name: 'Generation Project' } as never, pin(spec.projectId));
  const projectMp = (await repos.projects.findByIdRaw(spec.projectId))
    ?.officialMountPointId as string;
  if (!projectMp) throw new Error('project official store not minted');
  const { ensureProjectWardrobeFolder } = await import('@/lib/mount-index/project-wardrobe');
  await ensureProjectWardrobeFolder(projectMp);

  const { createProjectWardrobeItem } = await import(
    '@/lib/database/repositories/vault-overlay/wardrobe-writes'
  );
  const body = (item: ItemSpec, characterId: string | null) => ({
    characterId,
    title: item.title,
    description: null,
    imagePrompt: item.imagePrompt ?? null,
    types: item.types,
    componentItemIds: item.componentItemIds ?? [],
    appropriateness: null,
    isDefault: false,
    replace: false,
    migratedFromClothingRecordId: null,
    archivedAt: null,
  });
  // General and leaves first, so every component reference resolves when its
  // outfit is written.
  const order = Object.values(spec.items).sort(
    (a, b) => (a.componentItemIds?.length ?? 0) - (b.componentItemIds?.length ?? 0),
  );
  for (const item of order) {
    if (item.home === 'project') {
      await createProjectWardrobeItem(projectMp, {
        id: item.id,
        ...body(item, null),
        createdAt: PINNED_TS,
        updatedAt: PINNED_TS,
      } as never);
    } else {
      const owner =
        item.home === 'character'
          ? spec.characterId
          : item.home === 'archived'
            ? spec.archivedCharacterId
            : null;
      await repos.wardrobe.create(body(item, owner) as never, pin(item.id));
    }
  }

  await rawQuery('UPDATE "characters" SET "archivedAt" = ? WHERE "id" = ?', [
    ARCHIVED_AT,
    spec.archivedCharacterId,
  ]);

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built wardrobe-item-image-generation fixtures: main=${mainOut} mount=${mountOut}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`wardrobe-item-image-generation fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
