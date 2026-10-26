/**
 * Fixture builder for the Inventory TOOLS family (P4.D328 — v4 `e1872c94f` +
 * `102a23648` at `322a5488e`): ONE main + mount-index pair
 * `inventory_tools_equivalence` runs every case against (each case on its own
 * fresh copy, both sides).
 *
 * Bakes, all ids and timestamps pinned (`inventory-tools.json`), through v4's
 * REAL repositories and `resolveInventoryLocation(…, { ensure: true })` +
 * `location.create`:
 *   1. a users row; three characters (REAL `repos.characters.create` — each
 *      mints its vault): Vivienne (the caller), Marcus (the receiver), and
 *      Oldwyn — ARCHIVED after seeding (the archived-receiver refusal);
 *   2. Quilltap General; an OPEN project (`allowAnyCharacter: true`) and a
 *      CLOSED one (`allowAnyCharacter: false`, a roster of Marcus alone — the
 *      roster shelf closed to Vivienne, ⟨102a⟩); a group whose member is
 *      Vivienne;
 *   3. the objects: General's apple + an archetype lamp; the group's rope (a
 *      description longer than the list's excerpt); the open project's
 *      archetypes (magic bow, torch), a shared brazier and a shared titled
 *      lantern; the closed project's shield (CARRIED by Vivienne there), a
 *      titled helm and an archetype horn (both on the closed shelf);
 *      Vivienne's vault — kin swords titled Excalibur / Durendal (cap 1), a
 *      kit of two, a kit naming a filed object + a MISSING id, objects filed
 *      `Weapons` and `Weapons/Bows`, an ARCHIVED map, an instance of the
 *      project's bow, a titled-"42" token with metadata, and a pebble whose id
 *      Marcus's vault ALSO claims (the give's id collision);
 *   4. four chats (REAL `repos.chats.create`) with their carried sets written
 *      through v4's REAL `setEquippedInventory`: the open-project hall (with
 *      Oldwyn seated), the closed-project vault, a project-less road, and a
 *      room where Vivienne sits alone.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_IT_MAIN=/tmp/qt-it-main.db QT_FIXTURE_IT_MOUNT=/tmp/qt-it-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-inventory-tools-fixture.ts
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
  projects: Array<{ id: string; name: string; allowAnyCharacter: boolean; characterRoster: string[] }>;
  group: { id: string; name: string; memberId: string; member: string };
  generalMountPointId: string;
  generalStoreName: string;
  objects: Array<{ scope: string; owner: string | null; record: Record<string, unknown> }>;
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
  const spec = JSON.parse(readFileSync(join(here, 'inventory-tools.json'), 'utf8')) as Spec;
  const mainOut = process.env.QT_FIXTURE_IT_MAIN;
  const mountOut = process.env.QT_FIXTURE_IT_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_IT_MAIN and QT_FIXTURE_IT_MOUNT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      if (existsSync(out + suffix)) rmSync(out + suffix);
    }
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-it-fixture-build-'));
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

  for (const project of spec.projects) {
    await repos.projects.create(
      {
        name: project.name,
        allowAnyCharacter: project.allowAnyCharacter,
        characterRoster: project.characterRoster,
      } as never,
      { id: project.id, createdAt: TS, updatedAt: TS } as never,
    );
  }
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

  // v4 creates `chat_messages` and `background_jobs` lazily; force both so the
  // Rust side's Pascal post (and any job) can INSERT (the wardrobe-tools
  // builder's idiom).
  await repos.chats.getMessageCount(spec.chats[0].id);
  await repos.backgroundJobs.findByUserId(spec.userId, 'PENDING');

  // Oldwyn's tombstone, AFTER his vault was seeded.
  for (const c of Object.values(spec.characters)) {
    if (c.archived) {
      await rawQuery('UPDATE "characters" SET "archivedAt" = ? WHERE "id" = ?', ['2026-03-01T00:00:00.000Z', c.id]);
    }
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(`built inventory tools fixtures: main=${mainOut} mount=${mountOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`inventory tools fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
