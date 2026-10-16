/**
 * Fixture builder for the wardrobe FOLDER FILING tier-2 differential (P4.D309 —
 * v4 `bbbcd2388`, `lib/wardrobe/folder-filing.ts`).
 *
 * Bakes the shared starting state across TWO databases (main + mount-index) so
 * BOTH v4 and the Rust port file the same items against a copy:
 *   1. A users row and TWO characters owned by that user (REAL
 *      `repos.characters.create`, pinned id/ts — each mints a linked vault and
 *      scaffolds an empty `Wardrobe/`); the second is ARCHIVED at the end (the
 *      409 ordering rows).
 *   2. A provisioned "Quilltap General" store (an ordinary documents store whose
 *      id is `instance_settings.generalMountPointId`), a project and a group
 *      (REAL `repos.projects/groups.create` — each provisions an official store).
 *   3. Every `items` entry written through v4's REAL location writer
 *      (`resolveWardrobeLocation(..., { ensure: true })` → `location.create`,
 *      the keystone's `createInMount`) with its pinned id + stamps and its
 *      folder — leaf-first, so a ref resolves when its holder is written.
 *   4. A hand-made item file under `Wardrobe/hats: summer/` (raw, as the
 *      Scriptorium would write it).
 *   5. Pictures through v4's REAL `addWardrobeItemImage` (the transfers
 *      family's three `webp` samples, copied); the Shirt's pointer re-aimed at
 *      its FIRST picture; every `files` row stamped one second apart.
 *   6. Wear rows raw in v4's migration-shaped `wardrobe_wear_stats` (a copy's
 *      fresh id must not inherit them).
 * Every plant SELF-CHECKS (a malformed plant would fail both sides identically
 * and pass — the vacuous-equal trap).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_FFL_MAIN=/tmp/qt-ffl-main.db QT_FIXTURE_FFL_MOUNT=/tmp/qt-ffl-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-folder-filing-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

type Scope = 'character' | 'general' | 'project' | 'group' | 'archived';
interface ItemSpec {
  id: string;
  scope: Scope;
  title: string;
  types: string[];
  description?: string;
  imagePrompt?: string;
  appropriateness?: string;
  isDefault?: boolean;
  replace?: boolean;
  componentItemIds?: string[];
  folder?: string;
  archived?: boolean;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  user: Record<string, unknown>;
  characterId: string;
  character: Record<string, unknown>;
  archivedCharacterId: string;
  archivedCharacter: Record<string, unknown>;
  archivedAt: string;
  projectId: string;
  groupId: string;
  generalMountPointId: string;
  generalStore: { id: string; name: string };
  pinnedTs: string;
  items: Record<string, ItemSpec>;
  handmade: { path: string; id: string; content: string };
  pictures: Array<{
    item: string;
    kind: 'generated' | 'uploaded' | 'imported';
    webp: number;
    generationPrompt?: string;
    generationModel?: string;
  }>;
  webp: string[];
  wearRows: Array<Record<string, unknown>>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'folder-filing-tier2.json'), 'utf8')) as Spec;
  const PINNED_TS = spec.pinnedTs;

  const mainOut = process.env.QT_FIXTURE_FFL_MAIN;
  const mountOut = process.env.QT_FIXTURE_FFL_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_FFL_MAIN and QT_FIXTURE_FFL_MOUNT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ffl-fixture-build-'));
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
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { FileEntrySchema } = await import('@/lib/schemas/file.types');
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
  // The wear ledger in v4's MIGRATION shape (`WARDROBE_WEAR_STATS_DDL`), as a
  // migrated instance has it (the transfers builder's reasoning).
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
  // v4's blobs repository creates this table lazily from hand-written DDL
  // (`doc-mount-blobs.repository.ts:113`) — copied verbatim (the transfers
  // builder's note).
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
  const pinned = { createdAt: PINNED_TS, updatedAt: PINNED_TS };

  await repos.users.create(spec.user as never, { id: spec.userId, ...pinned } as never);
  await repos.characters.create(spec.character as never, { id: spec.characterId, ...pinned } as never);
  await repos.characters.create(spec.archivedCharacter as never, {
    id: spec.archivedCharacterId,
    ...pinned,
  } as never);

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
    { id: spec.generalMountPointId, ...pinned },
  );
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'generalMountPointId',
    spec.generalMountPointId,
  ]);
  await repos.projects.create({ name: 'Filing Project' } as never, { id: spec.projectId, ...pinned } as never);
  await repos.groups.create({ name: 'Filing Group' } as never, { id: spec.groupId, ...pinned } as never);

  const { resolveWardrobeLocation } = await import('@/lib/wardrobe/location');
  const locationOf = async (scope: Scope) => {
    const [s, id] =
      scope === 'character'
        ? ['character', spec.characterId]
        : scope === 'archived'
          ? ['character', spec.archivedCharacterId]
          : scope === 'project'
            ? ['project', spec.projectId]
            : scope === 'group'
              ? ['group', spec.groupId]
              : ['general', null];
    const location = await resolveWardrobeLocation(s as never, id, repos, spec.userId, { ensure: true });
    if (!location) throw new Error(`seed: no ${scope} location`);
    return location;
  };

  for (const [key, item] of Object.entries(spec.items)) {
    const location = await locationOf(item.scope);
    await location.create({
      id: item.id,
      characterId: location.characterId,
      title: item.title,
      description: item.description ?? null,
      imagePrompt: item.imagePrompt ?? null,
      types: item.types,
      componentItemIds: item.componentItemIds ?? [],
      appropriateness: item.appropriateness ?? null,
      isDefault: item.isDefault ?? false,
      replace: item.replace ?? false,
      migratedFromClothingRecordId: null,
      imageFileId: null,
      archivedAt: item.archived ? spec.archivedAt : null,
      folder: item.folder ?? '',
      ...pinned,
    } as never);
    const back = (await location.readItems(true)).find((i) => i.id === item.id);
    if (!back || back.title !== item.title || (back.folder ?? '') !== (item.folder ?? '')) {
      throw new Error(`seed self-check: ${key} did not read back as written`);
    }
  }

  // The hand-made item, written raw where the Scriptorium would.
  {
    const vault = await locationOf('character');
    const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
    await writeDatabaseDocument(vault.mountPointId, spec.handmade.path, spec.handmade.content);
    const back = (await vault.readItems(true)).find((i) => i.id === spec.handmade.id);
    if (!back) throw new Error('seed self-check: the hand-made item did not read back');
  }

  // Pictures through v4's REAL addWardrobeItemImage.
  const { resolveWardrobeItemHome, addWardrobeItemImage, listWardrobeItemImages } = await import(
    '@/lib/wardrobe/item-images'
  );
  const homeOf = async (key: string) => {
    const item = spec.items[key];
    const [scope, container] =
      item.scope === 'character'
        ? ['character', spec.characterId]
        : item.scope === 'archived'
          ? ['character', spec.archivedCharacterId]
          : item.scope === 'project'
            ? ['project', spec.projectId]
            : item.scope === 'group'
              ? ['group', spec.groupId]
              : ['general', null];
    const home = await resolveWardrobeItemHome(repos, spec.userId, scope as never, container, item.id);
    if (!home) throw new Error(`no home for picture item ${key}`);
    return home;
  };
  for (const p of spec.pictures) {
    await addWardrobeItemImage(repos, await homeOf(p.item), {
      userId: spec.userId,
      kind: p.kind,
      content: Buffer.from(spec.webp[p.webp], 'base64'),
      contentType: 'image/webp',
      width: null,
      height: null,
      generationPrompt: p.generationPrompt ?? null,
      generationModel: p.generationModel ?? null,
      generationRevisedPrompt: null,
    });
  }
  // The Shirt's pointer aimed at its FIRST picture (addWardrobeItemImage
  // points at the newest), so the copy's repoint is the non-trivial one.
  {
    const raw = getRawDatabase();
    if (!raw) throw new Error('main DB handle unavailable');
    const first = raw
      .prepare("SELECT f.id AS id FROM files f, json_each(f.linkedTo) j WHERE j.value = ? ORDER BY f.rowid LIMIT 1")
      .get(spec.items.shirt.id) as { id: string } | undefined;
    if (!first) throw new Error('seed self-check: the Shirt has no picture');
    const home = await homeOf('shirt');
    await home.update({ imageFileId: first.id } as never);
    const listed = await listWardrobeItemImages(repos, spec.items.shirt.id);
    if (listed.length !== 2) throw new Error(`seed self-check: the Shirt has ${listed.length} pictures`);
    const again = await homeOf('shirt');
    if (again.item.imageFileId !== first.id) throw new Error('seed self-check: the Shirt pointer did not move');
  }
  // Every `files` row stamped one second apart in seeding order, and the wear
  // rows raw.
  {
    const raw = getRawDatabase();
    if (!raw) throw new Error('main DB handle unavailable');
    const rows = raw.prepare('SELECT id FROM files ORDER BY rowid').all() as Array<{ id: string }>;
    if (rows.length !== spec.pictures.length) {
      throw new Error(`seed self-check: ${rows.length} files rows for ${spec.pictures.length} pictures`);
    }
    const base = Date.parse(PINNED_TS);
    rows.forEach((r, i) => {
      const ts = new Date(base + (i + 1) * 1000).toISOString();
      raw.prepare('UPDATE files SET createdAt = ?, updatedAt = ? WHERE id = ?').run(ts, ts, r.id);
    });
    for (const w of spec.wearRows) {
      raw
        .prepare(
          'INSERT INTO "wardrobe_wear_stats" ("id","itemId","wearerCharacterId","wearCount","firstWornAt","lastWornAt","lastWornChatId","createdAt","updatedAt") VALUES (?,?,?,?,?,?,?,?,?)',
        )
        .run(
          w.id,
          w.itemId,
          w.wearerCharacterId,
          w.wearCount,
          w.firstWornAt,
          w.lastWornAt,
          w.lastWornChatId,
          PINNED_TS,
          PINNED_TS,
        );
    }
    const n = (raw.prepare('SELECT COUNT(*) AS n FROM wardrobe_wear_stats').get() as { n: number }).n;
    if (n !== spec.wearRows.length) throw new Error('seed self-check: wear rows');
  }

  await rawQuery('UPDATE "characters" SET "archivedAt" = ? WHERE "id" = ?', [
    spec.archivedAt,
    spec.archivedCharacterId,
  ]);

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(`built folder-filing fixtures: main=${mainOut} mount=${mountOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`folder-filing fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
