/**
 * Fixture builder — the WARDROBE KEYSTONE four-tier seed (P4.D279, C1 §2 / §4).
 *
 * The round's ONE shared fixture: the three new tier-2 families
 * (`wardrobe_location_tier2`, `wardrobe_folder_writes_tier2`,
 * `vault_projection_recursive_tier2`) and every consumer lane READ it; nobody
 * changes it. Bakes `wardrobe-keystone-tier2.json` into TWO databases (main +
 * mount-index) through v4's REAL repositories and writers, so both v4 and the
 * Rust port start from byte-identical state:
 *
 *   1. v4's REAL `initializeDatabase`, then the collections the public-read /
 *      roster-access builders ensure (`users`, `characters`, `chats`, `projects`,
 *      `groups`) plus the mount-index DDL via `generateDDL` (points / files /
 *      documents / folders / links / chunks, the group + project link tables,
 *      `group_character_members`) and the hand-DDL blobs table.
 *   2. Users A and B. Ada (A), Bo (B — the ownership gate's plant) and Zed (A)
 *      through `repos.characters.create` (each mints and links a vault); Zed gets
 *      one item, then is ARCHIVED through `repos.characters.update({ archivedAt })`
 *      — the write the archive service itself commits, and the one that keeps
 *      the vault (§4.2a prunes in place).
 *   3. Quilltap General (pinned id, MAIN `instance_settings`) + two items via
 *      `createMountWardrobeItem`.
 *   4. Projects: Bare Manor (create provisions a store; NO `Wardrobe/`), Null
 *      Abbey (create, then `officialMountPointId` NULLED on the slim row — v4's
 *      real `findById` throws for it), Full Abbey (`ensureOwnerOfficialStore` +
 *      `ensureSharedWardrobeFolder` + two items). The Guild group: Ada a member,
 *      `ensureOwnerOfficialStore('group', …)`, two items (one sharing Ada's
 *      archived item's id — the shadow case).
 *   5. Ada's vault through the REAL `resolveWardrobeLocation('character', A,
 *      repos, A, { ensure: true }).create(item)`: a root item, a FILED item
 *      (`Formal/Evening`), an ARCHIVED own item, a composite naming a General
 *      item's UUID. Then the hand-made rows: `Wardrobe/Hats: Summer` and
 *      `Wardrobe/Empty` via `ensureFolderPath`, the note / `.txt` / caption-under-
 *      `images/` / id-less item file via `writeDatabaseDocument`.
 *   6. The scratch projection store (pinned id) with its pre-seeded files.
 *
 * Run (Node 24, from the v4 checkout — the round's PIN):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WKS_MAIN=/tmp/qt-wks-main.db QT_FIXTURE_WKS_MOUNT=/tmp/qt-wks-mount.db \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-wardrobe-keystone-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedItem {
  id: string;
  title: string;
  types: string[];
  folder: string;
  archivedAt?: string;
  componentItemIds?: string[];
}
interface SeedFile {
  relativePath: string;
  content: string;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userAId: string;
  userBId: string;
  characterAId: string;
  characterAName: string;
  characterBId: string;
  characterBName: string;
  archivedCharacterId: string;
  archivedCharacterName: string;
  generalMountPointId: string;
  generalStoreName: string;
  scratchMountPointId: string;
  scratchStoreName: string;
  bareProjectId: string;
  bareProjectName: string;
  nullProjectId: string;
  nullProjectName: string;
  fullProjectId: string;
  fullProjectName: string;
  groupId: string;
  groupName: string;
  items: Record<string, SeedItem>;
  characterVaultFolders: string[];
  characterVaultFiles: SeedFile[];
  scratchFolders: string[];
  scratchFiles: SeedFile[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'wardrobe-keystone-tier2.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_WKS_MAIN;
  const mountOut = process.env.QT_FIXTURE_WKS_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_WKS_MAIN and QT_FIXTURE_WKS_MOUNT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wks-fixture-build-'));
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
  const { CharacterSchema, ChatMetadataSchema } = await import('@/lib/schemas/types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { GroupSchema } = await import('@/lib/schemas/group.types');
  const { ProjectSchema } = await import('@/lib/schemas/project.types');
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
  const { ensureOwnerOfficialStore } = await import('@/lib/mount-index/ensure-owner-store');
  const { ensureSharedWardrobeFolder } = await import('@/lib/mount-index/shared-wardrobe');
  const { ensureFolderPath } = await import('@/lib/mount-index/folder-paths');
  const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
  // 7417e9a1b removed createMountWardrobeItem (no production caller); rebuilt
  // verbatim from 7417e9a1b^:wardrobe-writes.ts:647-650 on the surviving exports.
  const createMountWardrobeItem = async (mountPointId: string, item: never) => {
    const w = await import('@/lib/database/repositories/vault-overlay/wardrobe-writes');
    return w.createInMount(w.mountWardrobeLocation(mountPointId), item);
  };
  const { resolveWardrobeLocation } = await import('@/lib/wardrobe/location');

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('chats', ChatMetadataSchema);
  await ensureCollection('projects', ProjectSchema);
  await ensureCollection('groups', GroupSchema);

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
  // [40319484] `gcOrphanedFileRow` deletes from `doc_mount_blobs` unconditionally
  // on every content-addressed rewrite; v4 creates the table lazily from hand
  // DDL (`doc-mount-blobs.repository.ts:113`) — copied verbatim.
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
  const item = (key: string, characterId: string | null) => {
    const s = spec.items[key];
    if (!s) throw new Error(`spec has no item ${key}`);
    return {
      id: s.id,
      characterId,
      title: s.title,
      description: null,
      imagePrompt: null,
      types: s.types,
      componentItemIds: s.componentItemIds ?? [],
      appropriateness: null,
      isDefault: false,
      replace: false,
      migratedFromClothingRecordId: null,
      archivedAt: s.archivedAt ?? null,
      folder: s.folder,
      createdAt: TS,
      updatedAt: TS,
    };
  };

  // 1. Users.
  await repos.users.create(
    { username: 'keystone-a', name: 'Keystone A' } as never,
    { id: spec.userAId, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.users.create(
    { username: 'keystone-b', name: 'Keystone B' } as never,
    { id: spec.userBId, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Characters — each REAL create mints and links a vault.
  const createCharacter = async (id: string, name: string, userId: string): Promise<string> => {
    await repos.characters.create(
      { name, userId, systemTransparency: true } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
    const raw = await repos.characters.findByIdRaw(id);
    const mp = raw?.characterDocumentMountPointId;
    if (!mp) throw new Error(`character ${name} has no linked vault after create`);
    return mp;
  };
  const adaMount = await createCharacter(spec.characterAId, spec.characterAName, spec.userAId);
  const boMount = await createCharacter(spec.characterBId, spec.characterBName, spec.userBId);
  const zedMount = await createCharacter(
    spec.archivedCharacterId,
    spec.archivedCharacterName,
    spec.userAId,
  );

  // 3. Quilltap General (an ordinary documents store; General-ness is the MAIN key).
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
  await ensureSharedWardrobeFolder(spec.generalMountPointId);
  await createMountWardrobeItem(spec.generalMountPointId, item('generalWatch', null) as never);
  await createMountWardrobeItem(spec.generalMountPointId, item('generalBoots', null) as never);

  // The scratch projection store (pinned id; the recursive family's canvas).
  await repos.docMountPoints.create(
    {
      name: spec.scratchStoreName,
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
    { id: spec.scratchMountPointId, createdAt: TS, updatedAt: TS },
  );

  // 4. Projects + the group.
  const createProject = async (id: string, name: string): Promise<string> => {
    await repos.projects.create(
      { name, userId: spec.userAId, allowAnyCharacter: true, characterRoster: [] } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
    const raw = await repos.projects.findByIdRaw(id);
    const mp = (raw as { officialMountPointId?: string | null } | null)?.officialMountPointId;
    if (!mp) throw new Error(`project ${name} has no officialMountPointId after create`);
    return mp;
  };
  const bareMount = await createProject(spec.bareProjectId, spec.bareProjectName);
  const nullMount = await createProject(spec.nullProjectId, spec.nullProjectName);
  await rawQuery('UPDATE "projects" SET "officialMountPointId" = NULL WHERE "id" = ?', [
    spec.nullProjectId,
  ]);
  {
    const raw = await repos.projects.findByIdRaw(spec.nullProjectId);
    if ((raw as { officialMountPointId?: string | null } | null)?.officialMountPointId) {
      throw new Error('Null Abbey still carries an officialMountPointId');
    }
  }
  await createProject(spec.fullProjectId, spec.fullProjectName);
  const fullEnsured = await ensureOwnerOfficialStore('project', spec.fullProjectId, spec.fullProjectName);
  if (!fullEnsured) throw new Error('Full Abbey could not be ensured');
  const fullMount = fullEnsured.mountPointId;
  await ensureSharedWardrobeFolder(fullMount);
  await createMountWardrobeItem(fullMount, item('projectLivery', null) as never);
  await createMountWardrobeItem(fullMount, item('projectBoots', null) as never);

  await repos.groups.create(
    { name: spec.groupName, description: 'The keystone guild.', state: {} } as never,
    { id: spec.groupId, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.groupCharacterMembers.addMember(spec.groupId, spec.characterAId);
  const groupEnsured = await ensureOwnerOfficialStore('group', spec.groupId, spec.groupName);
  if (!groupEnsured) throw new Error('Guild could not be ensured');
  const groupMount = groupEnsured.mountPointId;
  await ensureSharedWardrobeFolder(groupMount);
  await createMountWardrobeItem(groupMount, item('groupShadowHat', null) as never);
  await createMountWardrobeItem(groupMount, item('groupCloak', null) as never);

  // 5. Ada's vault through the REAL location; Zed's one item, then the archive.
  const ada = await resolveWardrobeLocation('character', spec.characterAId, repos, spec.userAId, {
    ensure: true,
  });
  if (!ada) throw new Error('Ada has no wardrobe location');
  if (ada.mountPointId !== adaMount) throw new Error('Ada location mount drift');
  for (const key of ['rootCoat', 'filedGown', 'archivedHat', 'composite']) {
    await ada.create(item(key, spec.characterAId) as never);
  }
  const zed = await resolveWardrobeLocation(
    'character',
    spec.archivedCharacterId,
    repos,
    spec.userAId,
    { ensure: true },
  );
  if (!zed) throw new Error('Zed has no wardrobe location');
  await zed.create(item('archivedCape', spec.archivedCharacterId) as never);
  await repos.characters.update(spec.archivedCharacterId, { archivedAt: TS } as never);
  {
    const raw = await repos.characters.findByIdRaw(spec.archivedCharacterId);
    if (!raw?.archivedAt) throw new Error('Zed is not archived');
    if (raw.characterDocumentMountPointId !== zedMount) throw new Error('Zed lost the vault');
  }

  for (const folder of spec.characterVaultFolders) await ensureFolderPath(adaMount, folder);
  for (const f of spec.characterVaultFiles) await writeDatabaseDocument(adaMount, f.relativePath, f.content);

  // 6. The scratch store's canvas.
  for (const folder of spec.scratchFolders) await ensureFolderPath(spec.scratchMountPointId, folder);
  for (const f of spec.scratchFiles) {
    await writeDatabaseDocument(spec.scratchMountPointId, f.relativePath, f.content);
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built wardrobe-keystone fixtures: main=${mainOut} mount=${mountOut}\n` +
      `  ada=${adaMount} bo=${boMount} zed=${zedMount}\n` +
      `  general=${spec.generalMountPointId} scratch=${spec.scratchMountPointId}\n` +
      `  bare=${bareMount} null(orphaned store)=${nullMount} full=${fullMount} group=${groupMount}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`wardrobe-keystone fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
