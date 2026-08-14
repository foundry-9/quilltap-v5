/**
 * Fixture builder for the wardrobe TOOL-HANDLER differential (W4.1d batch 2).
 *
 * Bakes the shared starting state across TWO databases (main + mount-index) so
 * BOTH v4 and the Rust port drive the same seven wardrobe tool handlers against a
 * copy of byte-identical vault state:
 *   1. A users row (pinned id).
 *   2. The CALLER character (Rosalind) + a RECIPIENT character (Basil), each with a
 *      full provisioned vault (REAL repos.characters.create, pinned ids).
 *   3. A provisioned "Quilltap General" store (instance_settings.generalMountPointId).
 *   4. Caller vault items (leaf + a composite + an archived item) and a General
 *      archetype, all via the REAL public repos.wardrobe.create (full vault
 *      projection), pinned ids/timestamps.
 *   5. A chat with two CHARACTER participants and a seeded equippedOutfit for the
 *      caller (Blue Blouse worn on top). avatarGenerationEnabled is false so the
 *      handlers' avatar-generation seam is a v4 no-op (matching the Rust port).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WT_MAIN=/tmp/qt-wt-main.db QT_FIXTURE_WT_MOUNT=/tmp/qt-wt-mount.db \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-wardrobe-tools-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface ItemSpec {
  id: string;
  title: string;
  types: string[];
  description?: string;
  imagePrompt?: string;
  appropriateness?: string;
  isDefault?: boolean;
  replace?: boolean;
  componentItemIds?: string[];
  archived?: boolean;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  user: Record<string, unknown>;
  callerCharacterId: string;
  recipientCharacterId: string;
  caller: { name: string };
  recipient: { name: string };
  generalMountPointId: string;
  generalStore: { name: string };
  callerGroupId: string;
  callerGroupName: string;
  callerGroupMemberId: string;
  recipientGroupId: string;
  recipientGroupName: string;
  recipientGroupMemberId: string;
  chatId: string;
  callerParticipantId: string;
  recipientParticipantId: string;
  callerItems: ItemSpec[];
  generalItems: ItemSpec[];
  callerGroupItems: ItemSpec[];
  recipientGroupItems: ItemSpec[];
  seedEquipped: Record<string, string[]>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'wardrobe-tools.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_WT_MAIN;
  const mountOut = process.env.QT_FIXTURE_WT_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_WT_MAIN and QT_FIXTURE_WT_MOUNT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wt-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
    GroupDocMountLinkSchema,
    GroupCharacterMemberSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
    ['group_doc_mount_links', GroupDocMountLinkSchema],
    ['group_character_members', GroupCharacterMemberSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
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

  const repos = getRepositories();

  // 1. User.
  await repos.users.create(spec.user as never, {
    id: spec.userId,
    createdAt: TS,
    updatedAt: TS,
  } as never);

  // 2. Caller + recipient characters (full vault provisioning, pinned ids).
  await repos.characters.create({ name: spec.caller.name, userId: spec.userId } as never, {
    id: spec.callerCharacterId,
    createdAt: TS,
    updatedAt: TS,
  } as never);
  await repos.characters.create({ name: spec.recipient.name, userId: spec.userId } as never, {
    id: spec.recipientCharacterId,
    createdAt: TS,
    updatedAt: TS,
  } as never);

  // 3. Provision "Quilltap General" (ordinary documents store + the settings key).
  await repos.docMountPoints.create(
    {
      name: spec.generalStore.name,
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
    { id: spec.generalMountPointId, createdAt: TS, updatedAt: TS },
  );
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'generalMountPointId',
    spec.generalMountPointId,
  ]);

  // 4. Seed wardrobe items via the REAL public repos.wardrobe.create.
  const seedItem = async (item: ItemSpec, characterId: string | null): Promise<void> => {
    await repos.wardrobe.create(
      {
        characterId,
        title: item.title,
        description: item.description ?? null,
        imagePrompt: item.imagePrompt ?? null,
        types: item.types,
        componentItemIds: item.componentItemIds ?? [],
        appropriateness: item.appropriateness ?? null,
        isDefault: item.isDefault ?? false,
        replace: item.replace ?? false,
        migratedFromClothingRecordId: null,
        archivedAt: item.archived ? TS : null,
      } as never,
      { id: item.id, createdAt: TS, updatedAt: TS } as never,
    );
  };
  for (const item of spec.callerItems) await seedItem(item, spec.callerCharacterId);
  for (const item of spec.generalItems) await seedItem(item, null);

  // 4b. [P4.D71 / v4 8600c83f] The GROUP tier. Two groups, each with its own
  // official store and exactly one member:
  //   - "The Household" holds the caller. Its `Wardrobe/` carries a group-only
  //     garment plus a copy that SHADOWS a General archetype by id.
  //   - "The Regiment" holds the recipient. The caller is NOT a member, which
  //     is what proves group stores follow the CHARACTER (never a
  //     co-participant) and that `wardrobe_create` keys component resolution on
  //     the recipient.
  // The official store is provisioned by hand rather than via
  // `ensureGroupOfficialStore` so its mount-point id can be pinned.
  const { createProjectWardrobeItem } = await import(
    '@/lib/database/repositories/vault-overlay/wardrobe-writes'
  );
  const { ensureFolderPath } = await import('@/lib/mount-index/folder-paths');
  const { ensureGroupOfficialStore } = await import('@/lib/mount-index/ensure-group-store');

  const seedGroup = async (
    groupId: string,
    groupName: string,
    memberId: string,
    memberCharacterId: string,
    items: ItemSpec[],
  ): Promise<string> => {
    await repos.groups.create({ name: groupName } as never, {
      id: groupId,
      createdAt: TS,
      updatedAt: TS,
    } as never);
    // Groups are store-backed entities: their content lives in the official
    // store's `properties.json`, so the store must be provisioned through v4's
    // own `ensureGroupOfficialStore` (a hand-inserted mount point leaves the
    // group unhydratable). Its mount-point id is therefore MINTED, not pinned —
    // harmless here, since both sides read this same built fixture and the
    // wardrobe item ids inside it stay pinned.
    const ensured = await ensureGroupOfficialStore(groupId, groupName);
    if (!ensured) throw new Error(`failed to provision the official store for ${groupName}`);
    await repos.groupCharacterMembers.create(
      { groupId, characterId: memberCharacterId } as never,
      { id: memberId, createdAt: TS, updatedAt: TS } as never,
    );
    await ensureFolderPath(ensured.mountPointId, 'Wardrobe');
    for (const item of items) {
      await createProjectWardrobeItem(ensured.mountPointId, {
        id: item.id,
        characterId: null,
        title: item.title,
        description: item.description ?? null,
        imagePrompt: item.imagePrompt ?? null,
        types: item.types,
        componentItemIds: item.componentItemIds ?? [],
        appropriateness: item.appropriateness ?? null,
        isDefault: item.isDefault ?? false,
        replace: item.replace ?? false,
        migratedFromClothingRecordId: null,
        archivedAt: item.archived ? TS : null,
        createdAt: TS,
        updatedAt: TS,
      } as never);
    }
    return ensured.mountPointId;
  };

  await seedGroup(
    spec.callerGroupId,
    spec.callerGroupName,
    spec.callerGroupMemberId,
    spec.callerCharacterId,
    spec.callerGroupItems,
  );
  await seedGroup(
    spec.recipientGroupId,
    spec.recipientGroupName,
    spec.recipientGroupMemberId,
    spec.recipientCharacterId,
    spec.recipientGroupItems,
  );

  // 5. Chat with two CHARACTER participants + a seeded equipped outfit for the caller.
  const participants = [
    {
      id: spec.callerParticipantId,
      type: 'CHARACTER',
      characterId: spec.callerCharacterId,
      controlledBy: 'llm',
      status: 'active',
      createdAt: TS,
      updatedAt: TS,
    },
    {
      id: spec.recipientParticipantId,
      type: 'CHARACTER',
      characterId: spec.recipientCharacterId,
      controlledBy: 'llm',
      status: 'active',
      createdAt: TS,
      updatedAt: TS,
    },
  ];
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Wardrobe Tools Fixture',
      participants,
      chatType: 'salon',
      contextSummary: null,
      avatarGenerationEnabled: false,
      equippedOutfit: { [spec.callerCharacterId]: spec.seedEquipped },
    } as never,
    { id: spec.chatId, createdAt: TS, updatedAt: TS },
  );
  await repos.chats.getMessageCount(spec.chatId);

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(`built wardrobe-tools fixtures: main=${mainOut} mount=${mountOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`wardrobe-tools fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
