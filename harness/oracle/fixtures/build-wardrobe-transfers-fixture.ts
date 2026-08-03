/**
 * Fixture builder for the wardrobe TRANSFERS tier-2 differential.
 *
 * Bakes the shared starting state across TWO databases (main + mount-index) so
 * BOTH v4 and the Rust port drive the same transfer ops against a copy:
 *   1. A users row (pinned id = spec.userId) so buildRequestContext's
 *      repos.users.findById(session.user.id) resolves.
 *   2. A SOURCE character owned by that user (REAL repos.characters.create, pinned
 *      id/ts) with one wardrobe SOURCE item seeded into its vault (REAL
 *      repos.wardrobe.create, pinned id/ts).
 *   3. A DESTINATION character (same owner) holding a pre-existing item whose id
 *      collides with the source item's id (the id-collision scenario seed).
 *   4. A slim project + slim group (REAL repos.projects/groups.create). These are
 *      store-backed, so each is created WITH a provisioned official store already;
 *      the transfer's ensure_official_store then FINDS it (idempotent) and just
 *      ensures the Wardrobe/ folder.
 *   5. A provisioned "Quilltap General" store (ordinary documents store; the
 *      General-ness is instance_settings.generalMountPointId, NOT a storeType) with
 *      one archetype item seeded into General/Wardrobe/ via repos.wardrobe.create
 *      ({ characterId: null }).
 *
 * All ids + timestamps are pinned; every minted store/mount id is remapped by the
 * harness. Files are seeded via the REAL public repos.wardrobe.create (which runs
 * the whole vault projection), so both sides start from byte-identical vault state.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WTR_MAIN=/tmp/qt-wtr-main.db QT_FIXTURE_WTR_MOUNT=/tmp/qt-wtr-mount.db \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-wardrobe-transfers-fixture.ts
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
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  user: Record<string, unknown>;
  characterId: string;
  destCharacterId: string;
  projectId: string;
  groupId: string;
  generalMountPointId: string;
  generalStore: { id: string; name: string };
  character: Record<string, unknown>;
  destCharacter: Record<string, unknown>;
  sourceItem: ItemSpec;
  generalItem: ItemSpec;
  destCharacterItem: ItemSpec;
}

const PINNED_TS = '2026-02-01T00:00:00.000Z';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'wardrobe-transfers-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_WTR_MAIN;
  const mountOut = process.env.QT_FIXTURE_WTR_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_WTR_MAIN and QT_FIXTURE_WTR_MOUNT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wtr-fixture-build-'));
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
  } = await import('@/lib/schemas/mount-index.types');
  const {
    ProjectDocMountLinkSchema,
    GroupDocMountLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();

  // MAIN db: the slim tables the create/provision path touches.
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  // projects/groups slim tables are ensured by their repositories on first create.

  // MOUNT-INDEX db: materialize every store table via v4's own generated DDL.
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
    for (const sql of generateDDL(name, schema as never)) {
      midb.exec(sql);
    }
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

  // 1. Users row.
  await repos.users.create(spec.user as never, {
    id: spec.userId,
    createdAt: PINNED_TS,
    updatedAt: PINNED_TS,
  } as never);

  // 2. Source + destination characters (each mints a linked vault mount +
  // scaffolds an empty Wardrobe/). Pin the ids.
  await repos.characters.create(spec.character as never, {
    id: spec.characterId,
    createdAt: PINNED_TS,
    updatedAt: PINNED_TS,
  } as never);
  await repos.characters.create(spec.destCharacter as never, {
    id: spec.destCharacterId,
    createdAt: PINNED_TS,
    updatedAt: PINNED_TS,
  } as never);

  // 3. Provision the "Quilltap General" store (ordinary documents store; the
  // General-ness is the instance_settings key). Pin its id.
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
    { id: spec.generalMountPointId, createdAt: PINNED_TS, updatedAt: PINNED_TS },
  );
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'generalMountPointId',
    spec.generalMountPointId,
  ]);

  // 4. Slim project + group. These are store-backed: create provisions each an
  // official store already; the transfer's ensure_official_store finds it.
  await repos.projects.create(
    { name: 'Transfer Project' } as never,
    { id: spec.projectId, createdAt: PINNED_TS, updatedAt: PINNED_TS } as never,
  );
  await repos.groups.create(
    { name: 'Transfer Group' } as never,
    { id: spec.groupId, createdAt: PINNED_TS, updatedAt: PINNED_TS } as never,
  );

  // 5. Seed wardrobe items via the REAL public repos.wardrobe.create (which runs
  // the full vault projection). Pinned id/timestamps -> deterministic bytes.
  const seedItem = async (
    item: ItemSpec,
    characterId: string | null,
  ): Promise<void> => {
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
        archivedAt: null,
      } as never,
      { id: item.id, createdAt: PINNED_TS, updatedAt: PINNED_TS } as never,
    );
  };

  // Source item -> source character's vault.
  await seedItem(spec.sourceItem, spec.characterId);
  // Collision seed -> destination character's vault (shares the source id).
  await seedItem(spec.destCharacterItem, spec.destCharacterId);
  // Archetype item -> Quilltap General.
  await seedItem(spec.generalItem, null);

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built wardrobe-transfers fixtures: main=${mainOut} mount=${mountOut}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`wardrobe-transfers fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
