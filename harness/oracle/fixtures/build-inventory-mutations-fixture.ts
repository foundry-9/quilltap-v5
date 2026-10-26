/**
 * Fixture builder for the Inventory KEYSTONE families (P4.D324 — v4
 * `4fbdad186` at `322a5488e`): ONE main + mount-index pair every keystone
 * family runs against — location, pool, dependents, the equipped set, the
 * mutations, the pictures, the transfers and equip / arrival defaults
 * (`inventory_keystone_tier2_equivalence`). It is routes-shaped, so
 * P4.D325's `inventory_routes_equivalence` may REUSE it.
 *
 * Bakes, all ids and timestamps pinned (`inventory-keystone.json`):
 *   1. a users row; three characters (REAL `repos.characters.create` — each
 *      mints its vault): Vivienne, Wren, and Zed — ARCHIVED after seeding,
 *      whose vault holds an instance of the General archetype (the
 *      `ARCHIVED_DEPENDENTS` arm);
 *   2. Quilltap General (`instance_settings.generalMountPointId`); a project
 *      with `allowAnyCharacter: false` and a roster of Vivienne alone (the
 *      `OFF_ROSTER` both-ways rows); a group whose one member is Vivienne;
 *   3. every object written through v4's REAL `resolveInventoryLocation(…,
 *      { ensure: true })` + `location.create` — archetypes, instances (one in
 *      the group store whose archetype is the project's: two dependents in
 *      two mounts), kin by name (`sword` / `Sword `), caps 1 / 2, a kit naming
 *      an archetype + an object + a missing id + an archived object + a kit, a
 *      kit with exactly ONE member, a kit naming a project object + a General
 *      object (the strand case), equipped-by-default objects in the vault and
 *      on the project shelf, a not-copyable object filed two folders deep, and
 *      in Wren's vault a SECOND file claiming the sword's id (the transfer
 *      collision);
 *   4. pictures through v4's REAL `addInventoryItemImage` (`Inventory/images/
 *      <id>/`), their `files` rows' timestamps pinned one second apart;
 *   5. four chats (REAL `repos.chats.create`) with their carried sets written
 *      through v4's REAL `setEquippedInventory` — one in the project with a
 *      USER-type seat carrying Wren's `characterId` (R-18), a continuation
 *      source carrying a dangling id, one with an empty entry, one with no
 *      entries at all and a REMOVED seat.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-inv-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-inv-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-inventory-mutations-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  user: Record<string, unknown>;
  characters: Record<string, { id: string; name: string; archived: boolean }>;
  project: { id: string; name: string; allowAnyCharacter: boolean; characterRoster: string[] };
  group: { id: string; name: string; memberId: string; member: string };
  generalMountPointId: string;
  generalStoreName: string;
  objects: Array<{ scope: string; owner: string | null; record: Record<string, unknown> }>;
  pictures: Array<{
    scope: string;
    owner: string | null;
    item: string;
    kind: 'generated' | 'uploaded' | 'imported';
    webp: number;
    generationPrompt?: string;
    generationModel?: string;
  }>;
  webp: string[];
  chats: Array<{
    id: string;
    title: string;
    projectId: string | null;
    participants: Array<{ id: string; type: string; characterId: string; controlledBy?: string; status?: string }>;
    equipped: Record<string, string[]> | null;
  }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'inventory-keystone.json'), 'utf8')) as Spec;
  const mainOut = process.env.QT_FIXTURE_OUT;
  const mountOut = process.env.QT_FIXTURE_MOUNT_OUT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_OUT and QT_FIXTURE_MOUNT_OUT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      if (existsSync(out + suffix)) rmSync(out + suffix);
    }
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-inv-fixture-build-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';
  const TS = spec.seedTimestamp;

  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { FileEntrySchema } = await import('@/lib/schemas/file.types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const mi = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  {
    // The wardrobe wear ledger in v4's MIGRATION shape (a migrated instance's),
    // so any wardrobe path a family reaches finds its table.
    const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
    const { WARDROBE_WEAR_STATS_DDL } = await import('@/lib/database/backends/sqlite/wardrobe-wear-stats-ddl');
    const raw = getRawDatabase();
    if (!raw) throw new Error('main DB handle unavailable');
    for (const sql of WARDROBE_WEAR_STATS_DDL) raw.exec(sql);
  }
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('files', FileEntrySchema);

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', mi.DocMountPointSchema],
    ['doc_mount_files', mi.DocMountFileSchema],
    ['doc_mount_documents', mi.DocMountDocumentSchema],
    ['doc_mount_folders', mi.DocMountFolderSchema],
    ['doc_mount_file_links', mi.DocMountFileLinkSchema],
    ['doc_mount_chunks', mi.DocMountChunkSchema],
    ['project_doc_mount_links', mi.ProjectDocMountLinkSchema],
    ['group_doc_mount_links', mi.GroupDocMountLinkSchema],
    ['group_character_members', mi.GroupCharacterMemberSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  // v4's blobs repository creates this lazily from hand-written DDL
  // (`doc-mount-blobs.repository.ts`), copied verbatim (the wardrobe builders').
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
  midb.exec('CREATE UNIQUE INDEX IF NOT EXISTS "idx_doc_mount_blobs_fileId" ON "doc_mount_blobs" ("fileId")');

  const repos = getRepositories();
  await repos.users.create(spec.user as never, { id: spec.userId, createdAt: TS, updatedAt: TS } as never);
  for (const c of Object.values(spec.characters)) {
    await repos.characters.create(
      { userId: spec.userId, name: c.name, title: null, description: null } as never,
      { id: c.id, createdAt: TS, updatedAt: TS } as never,
    );
  }

  // Quilltap General.
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
  await rawQuery('CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)');
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'generalMountPointId',
    spec.generalMountPointId,
  ]);

  await repos.projects.create(
    {
      name: spec.project.name,
      allowAnyCharacter: spec.project.allowAnyCharacter,
      characterRoster: spec.project.characterRoster,
    } as never,
    { id: spec.project.id, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.groups.create({ name: spec.group.name } as never, {
    id: spec.group.id,
    createdAt: TS,
    updatedAt: TS,
  } as never);
  await repos.groupCharacterMembers.create(
    { groupId: spec.group.id, characterId: spec.group.member } as never,
    { id: spec.group.memberId, createdAt: TS, updatedAt: TS } as never,
  );

  // The objects, through v4's REAL location (the projection writes the files).
  const { resolveInventoryLocation } = await import('@/lib/inventory/location');
  const locationOf = async (scope: string, owner: string | null) => {
    const loc = await resolveInventoryLocation(scope as never, owner, repos, spec.userId, { ensure: true });
    if (!loc) throw new Error(`no location for ${scope}:${owner}`);
    return loc;
  };
  for (const o of spec.objects) {
    await (await locationOf(o.scope, o.owner)).create(o.record as never);
  }

  // Pictures through v4's REAL `addInventoryItemImage`.
  const { addInventoryItemImage, resolveInventoryItemHome } = await import('@/lib/inventory/item-images');
  for (const p of spec.pictures) {
    const home = await resolveInventoryItemHome(repos, spec.userId, p.scope as never, p.owner, p.item);
    if (!home) throw new Error(`no home for picture item ${p.item}`);
    await addInventoryItemImage(repos, home, {
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
  {
    // Pin every `files` row's timestamps one second apart in seeding order,
    // so "newest first" is unambiguous.
    const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
    const raw = getRawDatabase();
    if (!raw) throw new Error('main DB handle unavailable');
    const rows = raw.prepare('SELECT id FROM files ORDER BY rowid').all() as Array<{ id: string }>;
    const base = Date.parse(TS);
    rows.forEach((r, i) => {
      const ts = new Date(base + (i + 1) * 1000).toISOString();
      raw.prepare('UPDATE files SET createdAt = ?, updatedAt = ? WHERE id = ?').run(ts, ts, r.id);
    });
  }

  // The chats and their carried sets (REAL `setEquippedInventory`).
  for (const c of spec.chats) {
    await repos.chats.create(
      {
        userId: spec.userId,
        title: c.title,
        projectId: c.projectId,
        participants: c.participants.map((p) => ({
          id: p.id,
          type: p.type,
          characterId: p.characterId,
          ...(p.controlledBy ? { controlledBy: p.controlledBy } : {}),
          ...(p.status ? { status: p.status } : {}),
          createdAt: TS,
          updatedAt: TS,
        })),
      } as never,
      { id: c.id, createdAt: TS, updatedAt: TS },
    );
    for (const [characterId, ids] of Object.entries(c.equipped ?? {})) {
      const ok = await repos.chats.setEquippedInventory(c.id, characterId, ids);
      if (!ok) throw new Error(`carried-set plant failed for ${c.id}`);
    }
  }

  // Zed's tombstone, AFTER his vault was seeded.
  for (const c of Object.values(spec.characters)) {
    if (c.archived) {
      await rawQuery('UPDATE "characters" SET "archivedAt" = ? WHERE "id" = ?', ['2026-03-01T00:00:00.000Z', c.id]);
    }
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(`built inventory keystone fixtures: main=${mainOut} mount=${mountOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`inventory keystone fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
