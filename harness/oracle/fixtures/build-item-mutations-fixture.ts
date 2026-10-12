/**
 * Fixture builder for the wardrobe ITEM + FOLDER MUTATIONS family (P4.D279
 * keystone C1 §11; v4 `lib/wardrobe/item-mutations.ts` + `folder-mutations.ts`).
 *
 * Bakes the SHARED starting state across TWO databases (main + mount-index):
 *   1. A character baked via the REAL `repos.characters.create` (pinned id +
 *      stamps), which mints a linked vault mount + scaffolds an empty
 *      `Wardrobe/`.
 *   2. A provisioned "Quilltap General" store (a `doc_mount_points` row with a
 *      pinned id; `instance_settings.generalMountPointId` pointing at it) —
 *      the shape `build-wardrobe-public-read-fixture.ts` uses.
 *   3. FOUR items seeded through v4's REAL `createItem(location, body,
 *      { preserve })` over `resolveWardrobeLocation(..., { ensure: true })`:
 *      two in General (a `top`, a `footwear` — components a character composite
 *      can borrow), two in the character's vault (a leaf `top`, a leaf
 *      `accessories`). `preserve` pins every id and stamp, so the baked state
 *      compares with ZERO normalisation.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_ITEM_MUTATIONS_MAIN=/tmp/qt-itemmut-main.db \
 *   QT_FIXTURE_ITEM_MUTATIONS_MOUNT=/tmp/qt-itemmut-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-item-mutations-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedItem {
  scope: 'character' | 'general';
  body: Record<string, unknown>;
  preserve: { id: string; createdAt: string; updatedAt: string };
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  characterId: string;
  generalMountPointId: string;
  generalStore: { id: string; name: string };
  pinnedTs: string;
  character: Record<string, unknown>;
  seed: SeedItem[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'item-mutations-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_ITEM_MUTATIONS_MAIN;
  const mountOut = process.env.QT_FIXTURE_ITEM_MUTATIONS_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error(
      'QT_FIXTURE_ITEM_MUTATIONS_MAIN and QT_FIXTURE_ITEM_MUTATIONS_MOUNT must both point at the .db files to write',
    );
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-itemmut-fixture-build-'));
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
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();

  // MAIN db: the slim `characters` table (vault-managed columns stay NULL/default).
  await ensureCollection('characters', CharacterSchema);

  // MOUNT-INDEX db: every store table the create/provision/write path touches,
  // via v4's own generated DDL.
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) {
      midb.exec(sql);
    }
  }

  // [40319484] `gcOrphanedFileRow` deletes from `doc_mount_blobs` unconditionally
  // inside every content-addressed rewrite; v4 creates the table lazily from
  // hand-written DDL (`doc-mount-blobs.repository.ts:113`) — copied verbatim.
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

  // The character + its empty vault via the REAL create (mints the mount point,
  // scaffolds the preset folders incl. an empty Wardrobe/).
  await repos.characters.create(spec.character as never, {
    id: spec.characterId,
    createdAt: spec.pinnedTs,
    updatedAt: spec.pinnedTs,
  } as never);

  // Quilltap General: an ordinary documents store whose id lives in
  // instance_settings.generalMountPointId (the schema has no 'general' storeType).
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
    { id: spec.generalMountPointId, createdAt: spec.pinnedTs, updatedAt: spec.pinnedTs },
  );
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'generalMountPointId',
    spec.generalMountPointId,
  ]);

  // The four seed items through the REAL createItem, every id + stamp pinned.
  const { resolveWardrobeLocation } = await import('@/lib/wardrobe/location');
  const { createItem } = await import('@/lib/wardrobe/item-mutations');
  for (const seed of spec.seed) {
    const location = await resolveWardrobeLocation(
      seed.scope,
      seed.scope === 'character' ? spec.characterId : null,
      repos,
      spec.userId,
      { ensure: true },
    );
    if (!location) throw new Error(`seed: no ${seed.scope} location`);
    await createItem(location, seed.body as never, { preserve: seed.preserve });
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(`built item-mutations fixtures: main=${mainOut} mount=${mountOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`item-mutations fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
