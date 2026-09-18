/**
 * Fixture builder for the doc-edit OPACITY-COVENANT differential
 * (P4.D200 — v4 bugs 152 `1065a1f53` + 153 `89fcc3c0d`).
 *
 * Bakes the world of v4's two regression suites
 * (`lib/doc-edit/__tests__/path-resolver-opacity-{group-stores,enumeration}.test.ts`)
 * across TWO databases (main + mount-index), but with the REAL repositories and a
 * REAL tiered pool rather than the mocks those suites use — the composition over a
 * real DB being exactly what they cannot reach:
 *   1. A users row (pinned id).
 *   2. LEILANI — the ACTING character, `systemTransparency` ABSENT (v4's default,
 *      the shape a real instance carries; `!== true` → opaque). REAL
 *      repos.characters.create, so her vault is minted and linked.
 *   3. ABIGAIL — the TRANSPARENT peer (`systemTransparency: true`), same creator,
 *      her own minted vault. She is the control: every covenant row must be green
 *      for her both before AND after the fix.
 *   4. A GROUP ("Severed") with an official database store ("Group Files: Severed")
 *      that is linked to NO project — the store bug 152 made unreachable — PLUS one
 *      store LINKED (not official) to the group ("Group Files: Severed (Linked)",
 *      P4.100 item ii: the fix is at the flatten, which never distinguishes official
 *      from linked, so a linked store must be reachable by an OPAQUE member exactly
 *      like the official one). BOTH characters are members (Abigail's membership is
 *      what lets the transparent control reach the same stores).
 *   5. A PROJECT ("Papers") via REAL repos.projects.create (its official store) PLUS
 *      one linked database store ("Project Papers") — the control that kept working
 *      throughout, which is why the bug looked intermittent.
 *   6. A "stranger" enabled store ("Someone Elses Papers") linked to NOTHING — the
 *      out-of-scope ACCESS_DENIED subject.
 *   7. The Quilltap General singleton (id in MAIN `instance_settings`).
 *   8. A chat with BOTH characters as active `llm` participants and
 *      `allowCrossCharacterVaultReads: true` (so peers are admitted for a
 *      TRANSPARENT actor — the flag bug 153's enumeration must still subtract for
 *      an opaque one).
 *   9. `notes.md` in EVERY store including both vaults, each with a DISTINCT
 *      sentence naming its store, all containing the word "quarry" — so a single
 *      doc_grep discriminates which stores were enumerated rather than merely
 *      returning something.
 *
 * No blob rows are seeded: the corpus's own write_blob ops create them (Abigail
 * writes and reads hers back, which is what keeps the four Leilani blob refusals
 * non-vacuous — they fail at MOUNT resolution, not for want of a blob).
 *
 * The two vault ids, their names and the project's official store id are MINTED;
 * both sides read them back and substitute the `{{...}}` placeholders in the op
 * matrix (the doc-edit-path-resolver idiom), so nothing is transcribed.
 *
 * Run (Node 24, from the v4 checkout — or a pinned worktree):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_DOPA_MAIN=/tmp/qt-dopa-main.db QT_FIXTURE_DOPA_MOUNT=/tmp/qt-dopa-mount.db \
 *     $N/npx tsx <v5>/harness/oracle/fixtures/build-doc-opacity-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join, basename as pathBasename } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  leilaniId: string;
  abigailId: string;
  leilaniName: string;
  abigailName: string;
  groupId: string;
  groupName: string;
  groupOfficialMountPointId: string;
  groupStoreName: string;
  groupLinkedMountPointId: string;
  groupLinkedStoreName: string;
  projectId: string;
  projectName: string;
  projectLinkedMountPointId: string;
  projectStoreName: string;
  strangerMountPointId: string;
  strangerStoreName: string;
  generalMountPointId: string;
  generalStoreName: string;
  chatId: string;
  leilaniParticipantId: string;
  abigailParticipantId: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'doc-opacity.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_DOPA_MAIN;
  const mountOut = process.env.QT_FIXTURE_DOPA_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error(
      'QT_FIXTURE_DOPA_MAIN and QT_FIXTURE_DOPA_MOUNT must both point at the .db files to write',
    );
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-dopa-fixture-build-'));
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
  const { CharacterSchema, GroupSchema } = await import('@/lib/schemas/types');
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

  // [40319484] `gcOrphanedFileRow` runs inside every content-addressed rewrite and
  // deletes from `doc_mount_blobs` unconditionally, so a mount index that lacks the
  // table throws `no such table` on the SECOND write to any path. v4's blobs
  // repository creates it lazily from hand-written DDL
  // (`doc-mount-blobs.repository.ts:113`); copied verbatim so the fixture carries
  // exactly the table a real instance has.
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
    { username: 'opacity-tester', name: 'Opacity Tester' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. The two characters. Leilani's `systemTransparency` key is ABSENT (not
  //    `false`): v4's opacity test is `!== true`, and absent is the shape real
  //    instances carry for a character who never touched the toggle.
  await repos.characters.create(
    { name: spec.leilaniName, userId: spec.userId } as never,
    { id: spec.leilaniId, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.characters.create(
    { name: spec.abigailName, userId: spec.userId, systemTransparency: true } as never,
    { id: spec.abigailId, createdAt: TS, updatedAt: TS } as never,
  );
  const leilani = await repos.characters.findByIdRaw(spec.leilaniId);
  const abigail = await repos.characters.findByIdRaw(spec.abigailId);
  const leilaniVault = leilani?.characterDocumentMountPointId as string | null;
  const abigailVault = abigail?.characterDocumentMountPointId as string | null;
  if (!leilaniVault || !abigailVault) throw new Error('character vault(s) not minted');
  if ((leilani as { systemTransparency?: unknown }).systemTransparency === true) {
    throw new Error('Leilani must NOT be transparent — the opaque arm is the point');
  }

  // 3. The standalone stores (group official / project linked / stranger / General).
  const provisionStore = async (id: string, name: string): Promise<void> => {
    await repos.docMountPoints.create(
      {
        name,
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
      { id, createdAt: TS, updatedAt: TS },
    );
  };
  await provisionStore(spec.groupOfficialMountPointId, spec.groupStoreName);
  await provisionStore(spec.groupLinkedMountPointId, spec.groupLinkedStoreName);
  await provisionStore(spec.projectLinkedMountPointId, spec.projectStoreName);
  await provisionStore(spec.strangerMountPointId, spec.strangerStoreName);
  await provisionStore(spec.generalMountPointId, spec.generalStoreName);

  // 4. The General singleton pointer (MAIN db).
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'generalMountPointId',
    spec.generalMountPointId,
  ]);

  // 5. The group, its official store, ONE LINKED store (P4.100 item ii — bug
  //    152's fix is at the flatten, which never distinguishes official from
  //    linked; this store proves the group tier carries BOTH kinds for an
  //    OPAQUE member, not just the official pointer), and BOTH members. The
  //    official store is deliberately NOT project-linked: that is the store
  //    bug 152 erased.
  await rawQuery(
    'INSERT INTO "groups" ("id", "name", "officialMountPointId", "createdAt", "updatedAt") VALUES (?, ?, ?, ?, ?)',
    [spec.groupId, spec.groupName, spec.groupOfficialMountPointId, TS, TS],
  );
  await repos.groupDocMountLinks.link(spec.groupId, spec.groupLinkedMountPointId);
  await repos.groupCharacterMembers.addMember(spec.groupId, spec.leilaniId);
  await repos.groupCharacterMembers.addMember(spec.groupId, spec.abigailId);

  // 6. The project (REAL create → official store) + one linked store.
  await repos.projects.create(
    { name: spec.projectName, userId: spec.userId } as never,
    { id: spec.projectId, createdAt: TS, updatedAt: TS } as never,
  );
  const project = await repos.projects.findById(spec.projectId);
  const projectOfficial = (project as { officialMountPointId?: string } | null)
    ?.officialMountPointId;
  if (!projectOfficial) throw new Error('project has no officialMountPointId');
  await repos.projectDocMountLinks.link(spec.projectId, spec.projectLinkedMountPointId);

  // 7. The chat: both characters active `llm` participants, cross-character vault
  //    reads ENABLED (so peers are admitted for a transparent actor — the flag the
  //    covenant must still subtract for an opaque one).
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Doc Opacity Fixture',
      projectId: spec.projectId,
      allowCrossCharacterVaultReads: true,
      participants: [
        {
          id: spec.leilaniParticipantId,
          type: 'CHARACTER',
          characterId: spec.leilaniId,
          controlledBy: 'llm',
          status: 'active',
          createdAt: TS,
          updatedAt: TS,
        },
        {
          id: spec.abigailParticipantId,
          type: 'CHARACTER',
          characterId: spec.abigailId,
          controlledBy: 'llm',
          status: 'active',
          createdAt: TS,
          updatedAt: TS,
        },
      ],
      chatType: 'salon',
      contextSummary: null,
      avatarGenerationEnabled: false,
    } as never,
    { id: spec.chatId, createdAt: TS, updatedAt: TS },
  );
  await repos.chats.getMessageCount(spec.chatId);

  // 8. `notes.md` in EVERY store, each naming its own store and all containing
  //    "quarry" — one grep then discriminates WHICH stores were enumerated.
  const seedNotes = async (mountPointId: string, where: string): Promise<void> => {
    const content = `# Notes\n\nThe quarry stone was cut for ${where}.\n`;
    await repos.docMountFileLinks.linkDocumentContent({
      mountPointId,
      relativePath: 'notes.md',
      fileName: pathBasename('notes.md'),
      folderId: null,
      fileType: 'markdown',
      content,
      contentSha256: sha256OfString(content),
      plainTextLength: content.length,
      fileSizeBytes: Buffer.byteLength(content, 'utf-8'),
    });
  };
  await seedNotes(leilaniVault, 'Leilani');
  await seedNotes(abigailVault, 'Abigail');
  await seedNotes(spec.groupOfficialMountPointId, 'the Severed group');
  await seedNotes(spec.groupLinkedMountPointId, 'the Severed group (linked store)');
  await seedNotes(spec.projectLinkedMountPointId, 'the Papers project');
  await seedNotes(projectOfficial, 'the Papers official store');
  await seedNotes(spec.strangerMountPointId, 'someone else entirely');
  await seedNotes(spec.generalMountPointId, 'Quilltap General');

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built doc-opacity fixtures: main=${mainOut} mount=${mountOut} ` +
      `(leilaniVault=${leilaniVault}, abigailVault=${abigailVault}, projectOfficial=${projectOfficial})\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`doc-opacity fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
