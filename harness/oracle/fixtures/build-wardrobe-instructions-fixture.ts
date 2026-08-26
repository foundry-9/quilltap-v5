/**
 * P4.D119 fixture builder — the committed `wardrobe-instructions-{main,mount}.db`
 * pair behind BOTH dressing-instructions differentials (v4 `b86bb1a5`).
 *
 * Bakes, across the two partitions:
 *   1. A users row (pinned id) so `buildRequestContext` resolves.
 *   2. `Bertie` — a character WITH a vault and one garment, so the reader-skip
 *      and projection-preserve arms have a real `Wardrobe/` folder to work in.
 *   3. `Jeeves` — a character whose `characterDocumentMountPointId` is NULLed
 *      after creation: the vault-less arms (GET reads nothing, a clearing POST
 *      is a 200 no-op, a writing POST is the 500).
 *   4. `Archie` — a character with `archivedAt` set: the tombstone arms (the SET
 *      409s through `resolveWardrobeMount`; the GET still answers 200).
 *   5. A slim project + a slim group (each store-backed, so each is created WITH
 *      a provisioned official store), Bertie a member of the group.
 *   6. A provisioned "Quilltap General" store + `instance_settings.
 *      generalMountPointId` (General-ness is the setting, not a storeType).
 *   7. FOUR extra plain document stores with pinned ids that sort
 *      sA < sB < sY < sZ. The cascade's dedupe-then-code-unit-sort is only
 *      observable through WHICH tier mount wins when two in the same tier both
 *      carry a file, so these are what the multi-mount cases probe.
 *
 * No `Wardrobe/instructions.md` is seeded here: every case seeds its own (both
 * sides through the raw document-store write, so the seeding stays distinct from
 * the write helper under test).
 *
 * Run (Node 24, from the v4 checkout or a pinned worktree):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WI_MAIN=$V5W/crates/quilltap-web/tests/fixtures/wardrobe-instructions-main.db \
 *   QT_FIXTURE_WI_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/wardrobe-instructions-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-wardrobe-instructions-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  user: Record<string, unknown>;
  characterId: string;
  vaultlessCharacterId: string;
  archivedCharacterId: string;
  projectId: string;
  groupId: string;
  groupMemberId: string;
  generalMountPointId: string;
  generalStore: { id: string; name: string };
  extraStores: Array<{ label: string; id: string; name: string }>;
  character: Record<string, unknown>;
  vaultlessCharacter: Record<string, unknown>;
  archivedCharacter: Record<string, unknown>;
  characterItem: { id: string; title: string; types: string[]; description?: string };
}

const PINNED_TS = '2026-02-01T00:00:00.000Z';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'wardrobe-instructions.json'), 'utf8'),
  ) as Spec;

  const mainOut = process.env.QT_FIXTURE_WI_MAIN;
  const mountOut = process.env.QT_FIXTURE_WI_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error(
      'QT_FIXTURE_WI_MAIN and QT_FIXTURE_WI_MOUNT must both point at the .db files to write',
    );
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wi-fixture-build-'));
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
    ProjectDocMountLinkSchema,
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
    ['project_doc_mount_links', ProjectDocMountLinkSchema],
    ['group_doc_mount_links', GroupDocMountLinkSchema],
    ['group_character_members', GroupCharacterMemberSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) {
      midb.exec(sql);
    }
  }

  // [40319484] `gcOrphanedFileRow` deletes from `doc_mount_blobs` on every
  // content-addressed rewrite, so a mount index lacking the table throws on the
  // SECOND write to any path. v4's blobs repository creates it lazily from
  // hand-written DDL (`doc-mount-blobs.repository.ts:113`); copied verbatim.
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

  await repos.users.create(spec.user as never, {
    id: spec.userId,
    createdAt: PINNED_TS,
    updatedAt: PINNED_TS,
  } as never);

  for (const [row, id] of [
    [spec.character, spec.characterId],
    [spec.vaultlessCharacter, spec.vaultlessCharacterId],
    [spec.archivedCharacter, spec.archivedCharacterId],
  ] as Array<[Record<string, unknown>, string]>) {
    await repos.characters.create(row as never, {
      id,
      createdAt: PINNED_TS,
      updatedAt: PINNED_TS,
    } as never);
  }

  // Jeeves loses his vault link (the route's `?? null` arm); the vault mount
  // itself stays behind, unreferenced, exactly as an unlinked character's would.
  await rawQuery(
    'UPDATE "characters" SET "characterDocumentMountPointId" = NULL WHERE "id" = ?',
    [spec.vaultlessCharacterId],
  );
  // Archie is tombstoned — `resolveWardrobeMount` throws CharacterArchivedError.
  await rawQuery('UPDATE "characters" SET "archivedAt" = ? WHERE "id" = ?', [
    PINNED_TS,
    spec.archivedCharacterId,
  ]);

  const makeStore = async (id: string, name: string): Promise<void> => {
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
      { id, createdAt: PINNED_TS, updatedAt: PINNED_TS },
    );
  };

  await makeStore(spec.generalStore.id, spec.generalStore.name);
  for (const s of spec.extraStores) await makeStore(s.id, s.name);

  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'generalMountPointId',
    spec.generalMountPointId,
  ]);

  await repos.projects.create(
    { name: 'Instructions Project' } as never,
    { id: spec.projectId, createdAt: PINNED_TS, updatedAt: PINNED_TS } as never,
  );
  await repos.groups.create(
    { name: 'Instructions Group' } as never,
    { id: spec.groupId, createdAt: PINNED_TS, updatedAt: PINNED_TS } as never,
  );
  await repos.groupCharacterMembers.create(
    { groupId: spec.groupId, characterId: spec.characterId } as never,
    { id: spec.groupMemberId, createdAt: PINNED_TS, updatedAt: PINNED_TS } as never,
  );

  // One garment in Bertie's vault, through the REAL public create (the whole
  // vault projection), so the folder is a real wardrobe folder.
  await repos.wardrobe.create(
    {
      characterId: spec.characterId,
      title: spec.characterItem.title,
      description: spec.characterItem.description ?? null,
      imagePrompt: null,
      types: spec.characterItem.types,
      componentItemIds: [],
      appropriateness: null,
      isDefault: false,
      replace: false,
      migratedFromClothingRecordId: null,
      archivedAt: null,
    } as never,
    { id: spec.characterItem.id, createdAt: PINNED_TS, updatedAt: PINNED_TS } as never,
  );

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built wardrobe-instructions fixtures: main=${mainOut} mount=${mountOut}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`wardrobe-instructions fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
