/**
 * Fixture builder for the Inventory TITLE LINT families (P4.D340 — v4
 * `7263105c6` at `322a5488e`): ONE main + mount-index pair for
 * `inventory_title_lint_equivalence` and `tool_gate_keys_equivalence`.
 * Rebuilt from v4 at the pin, NOT committed (the keystone's convention). The
 * preamble (the DB open, the schema, General + `instance_settings`) is
 * COPIED from `build-inventory-mutations-fixture.ts`, which exports no build
 * function.
 *
 * Bakes, all ids and timestamps pinned (`inventory-title-lint.json`):
 *   1. a users row; Friday (REAL `repos.characters.create` — her vault is
 *      "Friday Character Vault");
 *   2. Quilltap General (`instance_settings.generalMountPointId`); project
 *      "The Quest" and group "The Covenant" (Friday a member), each with its
 *      official store through v4's REAL `resolveInventoryLocation(…,
 *      { ensure: true })`;
 *   3. the objects through v4's REAL `location.create`: General's `tape
 *      measure` archetype; in the project store `ansible resonator`, `lamp`
 *      ("Brass Lamp"), `compass` / `map` (both "North Star"), `key` ("Brass
 *      Key"); in Friday's vault an instance of the tape measure titled
 *      "Friday's Yellow Tape Measure";
 *   4. the documents through v4's REAL `writeDatabaseDocument` INSIDE
 *      `withInventoryWritesLinted` (an unlinted write of a shadowing title
 *      would itself be refused at the pin): the pre-existing collisions
 *      `Inventory/lantern.json` ("Lantern" / `lantern`) and
 *      `Inventory/decoy.json` (`decoy` titled "ansible resonator"), a belt of
 *      two objects under `Inventory/Belt/`, and General's ONE keyed tool
 *      `Tools/measure_twice.tool.json` (`availableWhen.inventory` keyed on
 *      the tape measure's title).
 * The group's dangling store (the `missingMounts` shape) is planted PER OP
 * (a `group_doc_mount_links` row), not here. Each plant self-checks; the last
 * line prints the counts.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-itl-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-itl-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-inventory-title-lint-fixture.ts
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
  friday: { id: string; name: string };
  project: { id: string; name: string };
  group: { id: string; name: string; memberId: string; member: string };
  generalMountPointId: string;
  generalStoreName: string;
  objects: Array<{ scope: string; owner: string | null; record: Record<string, unknown> }>;
  docs: Array<{ scope: string; owner: string | null; path: string; content: string }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'inventory-title-lint.json'), 'utf8')) as Spec;
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
  const scratch = mkdtempSync(join(tmpdir(), 'qt-itl-fixture-build-'));
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
  await repos.characters.create(
    { userId: spec.userId, name: spec.friday.name, title: null, description: null } as never,
    { id: spec.friday.id, createdAt: TS, updatedAt: TS } as never,
  );

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

  await repos.projects.create({ name: spec.project.name } as never, {
    id: spec.project.id,
    createdAt: TS,
    updatedAt: TS,
  } as never);
  await repos.groups.create({ name: spec.group.name } as never, {
    id: spec.group.id,
    createdAt: TS,
    updatedAt: TS,
  } as never);
  await repos.groupCharacterMembers.create(
    { groupId: spec.group.id, characterId: spec.group.member } as never,
    { id: spec.group.memberId, createdAt: TS, updatedAt: TS } as never,
  );

  const { resolveInventoryLocation } = await import('@/lib/inventory/location');
  const locationOf = async (scope: string, owner: string | null) => {
    const loc = await resolveInventoryLocation(scope as never, owner, repos, spec.userId, { ensure: true });
    if (!loc) throw new Error(`no location for ${scope}:${owner}`);
    return loc;
  };
  // Every store the ops address, ensured up front (the group's too).
  for (const [scope, owner] of [
    ['general', null],
    ['project', spec.project.id],
    ['group', spec.group.id],
    ['character', spec.friday.id],
  ] as Array<[string, string | null]>) {
    await locationOf(scope, owner);
  }
  let objects = 0;
  for (const o of spec.objects) {
    const loc = await locationOf(o.scope, o.owner);
    await loc.create(o.record as never);
    if (!(await loc.findItem(o.record.id as string))) throw new Error(`object ${o.record.id} did not land`);
    objects++;
  }

  const { withInventoryWritesLinted } = await import('@/lib/inventory/lint-scope');
  const { writeDatabaseDocument, readDatabaseDocument } = await import('@/lib/mount-index/database-store');
  let docs = 0;
  for (const d of spec.docs) {
    const loc = await locationOf(d.scope, d.owner);
    await withInventoryWritesLinted(() => writeDatabaseDocument(loc.mountPointId, d.path, d.content));
    const back = await readDatabaseDocument(loc.mountPointId, d.path);
    if (back.content !== d.content) throw new Error(`document ${d.path} did not land`);
    docs++;
  }

  // The keyed tool must load as LIVE at the pin, or every keyed row tests nothing.
  const { collectToolGateKeys } = await import('@/lib/pascal/tool-gate-keys');
  const census = await collectToolGateKeys();
  if (census.keys.length !== 1 || census.unread.length !== 0) {
    throw new Error(`the keyed tool did not load live: ${JSON.stringify(census)}`);
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built inventory title-lint fixtures: main=${mainOut} mount=${mountOut} objects=${objects} docs=${docs} gateKeys=${census.keys.length}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`inventory title-lint fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
