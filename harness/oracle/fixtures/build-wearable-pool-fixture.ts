/**
 * Fixture builder for the Wearable Pool LOADER differential (P4.D279, keystone
 * C1 §1 — v4 `lib/wardrobe/pool.ts` at `4a38222e6`).
 *
 * Bakes a FOUR-tier world across TWO databases (main + mount-index) with the
 * REAL repositories, so v4 and the Rust port load the same pools:
 *   1. A user; ADA and BEA via the REAL `repos.characters.create` (each mints
 *      and links a vault; pinned ids + stamps).
 *   2. Quilltap General — an ordinary documents store whose id lives in MAIN
 *      `instance_settings.generalMountPointId` (the public-read builder's idiom).
 *   3. The Guild — REAL `repos.groups.create` (the store-backed create provisions
 *      its official store through `ensureOwnerOfficialStore('group', …)`) and
 *      Ada's membership through the REAL `group_character_members` repository.
 *   4. ADMIT (`allowAnyCharacter: false`, roster [Ada]) and EXCLUDE (`false`,
 *      roster []) — REAL `repos.projects.create`, official stores minted.
 *   5. Chats on ADMIT / EXCLUDE / none (Ada the participant), REAL `repos.chats.create`.
 *   6. Items in every tier: the shared tiers through `ensureSharedWardrobeFolder`
 *      + `createMountWardrobeItem(mountPointId, item)` (full `WardrobeItem`s,
 *      pinned ids); the vaults through `repos.wardrobe.create(data, { id, … })`.
 *      The shapes the pool exists for: the SAME id archived in Ada's vault and
 *      live in the group store (the unmask case); the same id in the ADMIT store
 *      and General (project wins); a General composite whose component is the
 *      GROUP sash (a cross-tier UUID ref kept as written, bug 187); a filed own
 *      item under `Formal/Evening`.
 *
 * The three minted store ids are read back by the oracle case (its meta row).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WPOOL_MAIN=/tmp/qt-wpool-main.db QT_FIXTURE_WPOOL_MOUNT=/tmp/qt-wpool-mount.db \
 *     $N/node --import tsx <v5>/harness/oracle/fixtures/build-wearable-pool-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  adaId: string;
  adaName: string;
  beaId: string;
  beaName: string;
  groupId: string;
  groupName: string;
  groupMemberId: string;
  admitProjectId: string;
  admitProjectName: string;
  excludeProjectId: string;
  excludeProjectName: string;
  generalMountPointId: string;
  generalStoreName: string;
  admitChatId: string;
  excludeChatId: string;
  noProjectChatId: string;
  participantIds: { adaAdmit: string; adaExclude: string; adaNoProject: string };
  items: Record<string, string>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'wearable-pool-tier2.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;
  const I = spec.items;

  const mainOut = process.env.QT_FIXTURE_WPOOL_MAIN;
  const mountOut = process.env.QT_FIXTURE_WPOOL_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_WPOOL_MAIN and QT_FIXTURE_WPOOL_MOUNT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wpool-fixture-build-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
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
  const { CharacterSchema, ProjectSchema, ChatMetadataSchema } = await import('@/lib/schemas/types');
  const { GroupSchema } = await import('@/lib/schemas/group.types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
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
  await ensureCollection('projects', ProjectSchema);
  await ensureCollection('groups', GroupSchema);
  await ensureCollection('chats', ChatMetadataSchema);

  // MOUNT-INDEX db: every store table the create / provision / write path
  // touches, via v4's own generated DDL (the roster-access builder's list).
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
  // The blobs table a real instance has (hand-written DDL in v4; see the
  // public-read builder's note on `gcOrphanedFileRow`).
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

  // 1. The user, Ada and Bea.
  await repos.users.create(
    { username: 'pool-tester', name: 'Pool Tester' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.characters.create(
    { name: spec.adaName, userId: spec.userId, controlledBy: 'llm' } as never,
    { id: spec.adaId, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.characters.create(
    { name: spec.beaName, userId: spec.userId, controlledBy: 'llm' } as never,
    { id: spec.beaId, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Quilltap General + its MAIN pointer.
  await repos.docMountPoints.create(
    {
      name: spec.generalStoreName,
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

  // 3. The Guild (store-backed create provisions the official store) + Ada's
  //    membership. Bea belongs to nothing — the cast-pool contrast.
  await repos.groups.create(
    { name: spec.groupName } as never,
    { id: spec.groupId, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.groupCharacterMembers.create(
    { groupId: spec.groupId, characterId: spec.adaId } as never,
    { id: spec.groupMemberId, createdAt: TS, updatedAt: TS } as never,
  );
  const groupRow = await repos.groups.findByIdRaw(spec.groupId);
  const groupStore = (groupRow as { officialMountPointId?: string } | null)?.officialMountPointId;
  if (!groupStore) throw new Error('group official store not minted');

  // 4. The two projects — the roster flag EXPLICIT (v4 `9753d0eb2` flipped the
  //    create default to `true`; a flag-less create would admit everyone).
  const createProject = async (
    id: string,
    name: string,
    characterRoster: string[],
  ): Promise<string> => {
    await repos.projects.create(
      { name, userId: spec.userId, allowAnyCharacter: false, characterRoster } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
    const project = (await repos.projects.findById(id)) as {
      officialMountPointId?: string;
      allowAnyCharacter?: boolean;
      characterRoster?: string[];
    } | null;
    if (!project?.officialMountPointId) throw new Error(`project ${name} has no officialMountPointId`);
    if (project.allowAnyCharacter !== false) throw new Error(`project ${name} must be allowAnyCharacter:false`);
    return project.officialMountPointId;
  };
  const admitStore = await createProject(spec.admitProjectId, spec.admitProjectName, [spec.adaId]);
  const excludeStore = await createProject(spec.excludeProjectId, spec.excludeProjectName, []);

  // 5. Chats (Ada the participant on each).
  const participant = (id: string, characterId: string) => ({
    id,
    type: 'CHARACTER',
    characterId,
    controlledBy: 'llm',
    status: 'active',
    createdAt: TS,
    updatedAt: TS,
  });
  const createChat = async (id: string, title: string, projectId: string | null, participantId: string) => {
    await repos.chats.create(
      {
        userId: spec.userId,
        title,
        ...(projectId ? { projectId } : {}),
        participants: [participant(participantId, spec.adaId)],
        chatType: 'salon',
        contextSummary: null,
        avatarGenerationEnabled: false,
      } as never,
      { id, createdAt: TS, updatedAt: TS },
    );
  };
  await createChat(spec.admitChatId, 'Open Ledger Chat', spec.admitProjectId, spec.participantIds.adaAdmit);
  await createChat(spec.excludeChatId, 'Shut Ledger Chat', spec.excludeProjectId, spec.participantIds.adaExclude);
  await createChat(spec.noProjectChatId, 'No Project Chat', null, spec.participantIds.adaNoProject);

  // 6. Items. A full `WardrobeItem` per tier write (the wardrobe-routes idiom).
  const full = (
    id: string,
    title: string,
    types: string[],
    extra: Record<string, unknown> = {},
  ) => ({
    id,
    characterId: null,
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
    createdAt: TS,
    updatedAt: TS,
    ...extra,
  });

  const { ensureSharedWardrobeFolder } = await import('@/lib/mount-index/shared-wardrobe');
  const { createMountWardrobeItem } = await import(
    '@/lib/database/repositories/vault-overlay/wardrobe-writes'
  );
  const seedShared = async (mountPointId: string, items: Array<Record<string, unknown>>) => {
    await ensureSharedWardrobeFolder(mountPointId);
    for (const item of items) await createMountWardrobeItem(mountPointId, item as never);
  };

  // The GROUP store first: the General composite's component is the group sash.
  await seedShared(groupStore, [
    full(I.groupSash, 'Guild sash', ['accessories']),
    full(I.unmaskLivery, 'Guild livery', ['top']),
  ]);
  // Quilltap General.
  await seedShared(spec.generalMountPointId, [
    full(I.generalTopper, 'General topper', ['accessories']),
    full(I.duplicateProjectGeneral, 'General badge', ['accessories']),
    full(I.generalEnsemble, 'General ensemble', ['top'], { componentItemIds: [I.groupSash] }),
    full(I.generalArchivedBreeches, 'Retired breeches', ['bottom'], { archivedAt: TS }),
  ]);
  // The ADMIT project's store — the duplicate id wins over General here.
  await seedShared(admitStore, [
    full(I.duplicateProjectGeneral, 'Project badge', ['accessories']),
    full(I.projectApron, 'Project apron', ['top']),
  ]);
  // The EXCLUDE project's store — what the roster withholds.
  await seedShared(excludeStore, [full(I.excludeVeil, 'Shut veil', ['accessories'])]);

  // The vaults through the REAL owner-addressed create.
  const seedOwn = async (
    characterId: string,
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
  // Ada: the ARCHIVED personal copy of the group's livery (the unmask case), her
  // boots, and a gown FILED under Formal/Evening.
  await seedOwn(spec.adaId, I.unmaskLivery, 'My livery', ['top'], { archivedAt: TS });
  await seedOwn(spec.adaId, I.adaBoots, 'Ada boots', ['footwear']);
  await seedOwn(spec.adaId, I.adaEveningGown, 'Evening gown', ['top', 'bottom'], {
    folder: 'Formal/Evening',
  });
  // Bea: one coat.
  await seedOwn(spec.beaId, I.beaCoat, 'Bea coat', ['top']);

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built wearable-pool fixtures: main=${mainOut} mount=${mountOut} ` +
      `(admitStore=${admitStore}, excludeStore=${excludeStore}, groupStore=${groupStore})\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`wearable-pool fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
