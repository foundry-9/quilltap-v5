/**
 * Fixture builder for the PROJECT ROSTER ACCESS differential (P4.D245 — v4
 * `9753d0eb2`, `lib/projects/roster-access.ts`).
 *
 * Bakes a world where the roster is the ONLY axis that varies, across TWO
 * databases (main + mount-index), with the REAL repositories:
 *   1. A users row (pinned id).
 *   2. ADA (rostered) and BEA (the stranger) — REAL `repos.characters.create`
 *      mints and links each vault. Both `systemTransparency: true`, so the opacity
 *      covenant (P4.D200) never subtracts a vault and the roster gate is the only
 *      thing that can remove a store from an enumeration here.
 *   3. CLOSED ("Closed Ledger"): `allowAnyCharacter: false` EXPLICITLY, roster
 *      [Ada]. Its REAL official store holds `Docs/plan.md` (the word "quarry", so
 *      one grep discriminates) and a project `Wardrobe/` item (the
 *      `build-wardrobe-routes-fixture.ts` idiom — `ensureProjectWardrobeFolder`
 *      + `createProjectWardrobeItem`, pinned id).
 *   4. OPEN ("Open Commons"): `allowAnyCharacter: true` EXPLICITLY, roster [];
 *      `Docs/notes.md` in its store.
 *   5. BROKEN ("Broken Vault"): `allowAnyCharacter: false`, roster [Ada]; AFTER
 *      the build its official store is WIPED from the mount index (the
 *      `doc_mount_points` row and every `doc_mount_file_links` row), so the
 *      overlay raises `properties.json missing` → v4's OUTER fail-closed line
 *      (`Error checking character participation`) and a refusal.
 *   6. The Quilltap General singleton (id in MAIN `instance_settings`) with
 *      `notes.md` ("quarry").
 *   7. Chats: one in CLOSED carrying BOTH characters as active `llm`
 *      participants (the shared wardrobe tiers key on chat × character), one in
 *      OPEN (Bea), one with NO project (Bea).
 *
 * `allowAnyCharacter` is set EXPLICITLY on every create because v4 `9753d0eb2`
 * flipped the create default `false` → `true`: a flag-less create bakes a
 * different project at each pin, and an off-roster arm over a target-built
 * fixture would be ADMITTED and prove nothing.
 *
 * The three official store ids and the Closed store's NAME are MINTED; both sides
 * read them back and substitute the `{{...}}` placeholders in the op matrix.
 *
 * Run (Node 24, from the v4 checkout — or a pinned worktree):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_PRA_MAIN=/tmp/qt-pra-main.db QT_FIXTURE_PRA_MOUNT=/tmp/qt-pra-mount.db \
 *     $N/node --import tsx <v5>/harness/oracle/fixtures/build-project-roster-access-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join, basename as pathBasename } from 'node:path';
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
  closedProjectId: string;
  closedProjectName: string;
  openProjectId: string;
  openProjectName: string;
  brokenProjectId: string;
  brokenProjectName: string;
  closedCloakId: string;
  closedCloakTitle: string;
  generalMountPointId: string;
  generalStoreName: string;
  closedChatId: string;
  openChatId: string;
  noProjectChatId: string;
  adaClosedParticipantId: string;
  beaClosedParticipantId: string;
  beaOpenParticipantId: string;
  beaNoProjectParticipantId: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'project-roster-access.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_PRA_MAIN;
  const mountOut = process.env.QT_FIXTURE_PRA_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_PRA_MAIN and QT_FIXTURE_PRA_MOUNT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-pra-fixture-build-'));
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
  const { ChatDocumentSchema } = await import('@/lib/schemas/chat-document.types');
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
  const { sha256OfString } = await import('@/lib/utils/sha256');

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('projects', ProjectSchema);
  await ensureCollection('chats', ChatMetadataSchema);
  // `doc_open_document` records the open in `chat_documents` (v4 creates the
  // collection lazily on first access; v5 expects the table a provisioned
  // instance has).
  await ensureCollection('chat_documents', ChatDocumentSchema);

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
  // The blobs table a real instance has (the doc-opacity builder's note).
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

  // 1. User.
  await repos.users.create(
    { username: 'roster-tester', name: 'Roster Tester' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Ada (rostered) and Bea (the stranger), both transparent.
  await repos.characters.create(
    { name: spec.adaName, userId: spec.userId, systemTransparency: true } as never,
    { id: spec.adaId, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.characters.create(
    { name: spec.beaName, userId: spec.userId, systemTransparency: true } as never,
    { id: spec.beaId, createdAt: TS, updatedAt: TS } as never,
  );

  // 3. The General singleton + its MAIN pointer.
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

  // 4. The three projects — the flag EXPLICIT on every create (see the header).
  const createProject = async (
    id: string,
    name: string,
    allowAnyCharacter: boolean,
    characterRoster: string[],
  ): Promise<string> => {
    await repos.projects.create(
      { name, userId: spec.userId, allowAnyCharacter, characterRoster } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
    const project = await repos.projects.findById(id);
    const official = (project as { officialMountPointId?: string } | null)?.officialMountPointId;
    if (!official) throw new Error(`project ${name} has no officialMountPointId`);
    return official;
  };
  const closedStore = await createProject(spec.closedProjectId, spec.closedProjectName, false, [spec.adaId]);
  const openStore = await createProject(spec.openProjectId, spec.openProjectName, true, []);
  const brokenStore = await createProject(spec.brokenProjectId, spec.brokenProjectName, false, [spec.adaId]);

  // The roster as stored — the premise of every off-roster arm.
  const closed = (await repos.projects.findById(spec.closedProjectId)) as {
    allowAnyCharacter?: boolean;
    characterRoster?: string[];
  } | null;
  if (closed?.allowAnyCharacter !== false || !closed.characterRoster?.includes(spec.adaId)) {
    throw new Error(`Closed must be allowAnyCharacter:false with Ada rostered; got ${JSON.stringify(closed)}`);
  }
  if (closed.characterRoster.includes(spec.beaId)) throw new Error('Bea must NOT be on the Closed roster');

  // 5. Documents: one "quarry" sentence per store, each naming its store.
  const seedDoc = async (mountPointId: string, relativePath: string, where: string): Promise<void> => {
    const content = `# Notes\n\nThe quarry stone was cut for ${where}.\n`;
    await repos.docMountFileLinks.linkDocumentContent({
      mountPointId,
      relativePath,
      fileName: pathBasename(relativePath),
      folderId: null,
      fileType: 'markdown',
      content,
      contentSha256: sha256OfString(content),
      plainTextLength: content.length,
      fileSizeBytes: Buffer.byteLength(content, 'utf-8'),
    });
  };
  await seedDoc(closedStore, 'Docs/plan.md', 'the Closed Ledger');
  await seedDoc(openStore, 'Docs/notes.md', 'the Open Commons');
  await seedDoc(spec.generalMountPointId, 'notes.md', 'Quilltap General');

  // 6. The Closed project's Wardrobe/ item (the wardrobe-routes idiom).
  const { ensureProjectWardrobeFolder } = await import('@/lib/mount-index/project-wardrobe');
  const { createProjectWardrobeItem } = await import(
    '@/lib/database/repositories/vault-overlay/wardrobe-writes'
  );
  await ensureProjectWardrobeFolder(closedStore);
  await createProjectWardrobeItem(closedStore, {
    id: spec.closedCloakId,
    characterId: null,
    title: spec.closedCloakTitle,
    description: null,
    imagePrompt: null,
    types: ['top'],
    componentItemIds: [],
    appropriateness: null,
    isDefault: false,
    replace: false,
    migratedFromClothingRecordId: null,
    archivedAt: null,
    createdAt: TS,
    updatedAt: TS,
  } as never);

  // 7. Chats.
  const participant = (id: string, characterId: string) => ({
    id,
    type: 'CHARACTER',
    characterId,
    controlledBy: 'llm',
    status: 'active',
    createdAt: TS,
    updatedAt: TS,
  });
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Closed Ledger Chat',
      projectId: spec.closedProjectId,
      participants: [
        participant(spec.adaClosedParticipantId, spec.adaId),
        participant(spec.beaClosedParticipantId, spec.beaId),
      ],
      chatType: 'salon',
      contextSummary: null,
      avatarGenerationEnabled: false,
    } as never,
    { id: spec.closedChatId, createdAt: TS, updatedAt: TS },
  );
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Open Commons Chat',
      projectId: spec.openProjectId,
      participants: [participant(spec.beaOpenParticipantId, spec.beaId)],
      chatType: 'salon',
      contextSummary: null,
      avatarGenerationEnabled: false,
    } as never,
    { id: spec.openChatId, createdAt: TS, updatedAt: TS },
  );
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'No Project Chat',
      participants: [participant(spec.beaNoProjectParticipantId, spec.beaId)],
      chatType: 'salon',
      contextSummary: null,
      avatarGenerationEnabled: false,
    } as never,
    { id: spec.noProjectChatId, createdAt: TS, updatedAt: TS },
  );
  for (const id of [spec.closedChatId, spec.openChatId, spec.noProjectChatId]) {
    await repos.chats.getMessageCount(id);
  }

  // 8. The Broken plant — AFTER everything else: wipe its official store from the
  //    mount index (the store row AND its file links), so the overlay finds no
  //    `properties.json` and raises the store-unavailable error both sides render
  //    as `Project <id> has no usable document store (officialMountPointId=<mp>):
  //    properties.json missing`.
  midb.prepare('DELETE FROM doc_mount_file_links WHERE mountPointId = ?').run(brokenStore);
  midb.prepare('DELETE FROM doc_mount_points WHERE id = ?').run(brokenStore);

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built project-roster-access fixtures: main=${mainOut} mount=${mountOut} ` +
      `(closedStore=${closedStore}, openStore=${openStore}, brokenStore=${brokenStore} [wiped])\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`project-roster-access fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
