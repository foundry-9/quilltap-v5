/**
 * Fixture builder for `inventory_pool_order_tier2_equivalence` (P4.D378
 * item 7 + Tier 2 item 17 — v4 `0c47f4539` at `624d0d25c`): ONE main +
 * mount-index pair. Rebuilt from v4 at the pin per run, NOT committed (the
 * keystone's convention). The preamble (the DB open, the schema, General +
 * `instance_settings`) is COPIED from `build-inventory-title-lint-fixture.ts`,
 * which exports no build function.
 *
 * Bakes, ids and timestamps pinned (`inventory-pool-order.json`) except the
 * official stores' (minted by v4's REAL store-backed `create`) and the twins'
 * object ids (minted by v4's REAL `createInventoryItem`):
 *   1. a users row; Vivienne (REAL `repos.characters.create` — her vault);
 *   2. Quilltap General (`instance_settings.generalMountPointId`);
 *   3. groups "First" and "Second", each with its own official store, and
 *      Vivienne a member of First THEN Second (membership order);
 *   4. project "The Expedition" with its official store, plus a store LINKED
 *      to it ("Expedition Annex", pinned id) — not its official store;
 *   5. v4's `pool-order.test.ts` `twin` vectors: an object written through
 *      v4's REAL `createInventoryItem` into First's store ("compass") and the
 *      project's official store ("lantern"), then the SAME id under another
 *      name raw-planted through v4's REAL `writeDatabaseDocument` into
 *      Second's store ("sextant") and the linked store ("candle").
 * Each plant self-checks; the last line prints the counts.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-ipo-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-ipo-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-inventory-pool-order-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

type Json = any; // eslint-disable-line @typescript-eslint/no-explicit-any

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'inventory-pool-order.json'), 'utf8')) as Json;
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
  const scratch = mkdtempSync(join(tmpdir(), 'qt-ipo-fixture-build-'));
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
    // The wardrobe wear ledger in v4's MIGRATION shape (a migrated instance's).
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
  // v4's blobs repository creates this lazily from hand-written DDL, copied verbatim.
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
  await repos.characters.create(
    { userId: spec.userId, name: spec.character.name, title: null, description: null } as never,
    { id: spec.character.id, createdAt: TS, updatedAt: TS } as never,
  );

  const storeRow = (name: string) =>
    ({
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
    }) as never;

  // Quilltap General.
  await repos.docMountPoints.create(storeRow(spec.generalStoreName), {
    id: spec.generalMountPointId,
    createdAt: TS,
    updatedAt: TS,
  });
  await rawQuery('CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)');
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'generalMountPointId',
    spec.generalMountPointId,
  ]);

  // The groups, Vivienne a member of First THEN Second.
  for (const g of spec.groups) {
    await repos.groups.create({ name: g.name } as never, { id: g.id, createdAt: TS, updatedAt: TS } as never);
  }
  for (const g of spec.groups) {
    await repos.groupCharacterMembers.create(
      { groupId: g.id, characterId: spec.character.id } as never,
      { id: g.memberId, createdAt: g.memberCreatedAt, updatedAt: g.memberCreatedAt } as never,
    );
  }

  // The project, its official store, and a store merely LINKED to it.
  await repos.projects.create({ name: spec.project.name } as never, {
    id: spec.project.id,
    createdAt: TS,
    updatedAt: TS,
  } as never);
  await repos.docMountPoints.create(storeRow(spec.linked.name), { id: spec.linked.id, createdAt: TS, updatedAt: TS });
  await repos.projectDocMountLinks.create(
    { projectId: spec.project.id, mountPointId: spec.linked.id } as never,
    { id: spec.linked.linkId, createdAt: TS, updatedAt: TS } as never,
  );

  const { resolveInventoryLocation } = await import('@/lib/inventory/location');
  const locationOf = async (scope: string, owner: string | null) => {
    const loc = await resolveInventoryLocation(scope as never, owner, repos, spec.userId, { ensure: true });
    if (!loc) throw new Error(`no location for ${scope}:${owner}`);
    return loc;
  };
  const mounts: Record<string, string> = {
    own: (await locationOf('character', spec.character.id)).mountPointId,
    first: (await locationOf('group', spec.groups[0].id)).mountPointId,
    second: (await locationOf('group', spec.groups[1].id)).mountPointId,
    project: (await locationOf('project', spec.project.id)).mountPointId,
    general: (await locationOf('general', null)).mountPointId,
    annex: spec.linked.id,
  };
  if (new Set(Object.values(mounts)).size !== 6) throw new Error(`stores collide: ${JSON.stringify(mounts)}`);
  const { resolveProjectMountPointIds } = await import('@/lib/mount-index/tiered-mount-pool');
  const projectTier = await resolveProjectMountPointIds(spec.project.id);
  if (JSON.stringify(projectTier) !== JSON.stringify([mounts.project, mounts.annex])) {
    throw new Error(`project tier is not [official, linked]: ${JSON.stringify(projectTier)}`);
  }
  const { groupMountsFor } = await import('@/lib/mount-index/reach');
  const groupTier = (await groupMountsFor(spec.character.id)).flatMap((g: Json) => g.mountPointIds);
  if (JSON.stringify(groupTier) !== JSON.stringify([mounts.first, mounts.second])) {
    throw new Error(`group tier is not [First, Second]: ${JSON.stringify(groupTier)}`);
  }

  // v4's `twin` vectors: REAL createInventoryItem, then the same id raw-planted elsewhere.
  const { createInventoryItem } = await import('@/lib/inventory/item-mutations');
  const { withInventoryWritesLinted } = await import('@/lib/inventory/lint-scope');
  const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
  const { readInventoryRecords } = await import('@/lib/database/repositories/vault-overlay/inventory-readers');
  const body = (name: string) =>
    ({
      kind: 'item',
      equippedByDefault: false,
      maxEquipped: 1,
      canCopy: false,
      isArchetype: false,
      parentId: null,
      itemIds: [],
      metadata: {},
      name,
    }) as never;
  let twins = 0;
  for (const t of spec.twins) {
    const from = await locationOf(t.at.scope, t.at.owner);
    const record = await createInventoryItem(repos as never, from, body(t.names[0]));
    const docs = await repos.docMountDocuments.findManyByMountPointsInFolder([from.mountPointId], 'Inventory', '.json', {
      recursive: true,
    });
    const doc = docs.find((d: Json) => {
      try {
        return JSON.parse(d.content).id === record.id;
      } catch {
        return false;
      }
    });
    if (!doc) throw new Error(`twin ${t.label}: no document for ${record.id}`);
    const content = JSON.parse(doc.content as string);
    content.name = t.names[1];
    const toMp = mounts[t.twinMount];
    const path = doc.relativePath.replace(t.names[0], t.names[1]);
    await withInventoryWritesLinted(() => writeDatabaseDocument(toMp, path, JSON.stringify(content, null, 2) + '\n'));
    const back = (await readInventoryRecords(toMp)).find((r: Json) => r.id === record.id);
    if (!back || back.name !== t.names[1]) throw new Error(`twin ${t.label}: the raw plant did not land`);
    twins++;
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built inventory pool-order fixtures: main=${mainOut} mount=${mountOut} twins=${twins} mounts=${JSON.stringify(mounts)}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`inventory pool-order fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
