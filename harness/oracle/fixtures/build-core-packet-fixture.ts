/**
 * Tier-2 fixture builder (P4.D378 item 12) — the main + mount-index pair for
 * `core_packet_tier2_equivalence`: v4's REAL `assembleCorePacket` (the Core
 * whisper's packet, `lib/services/aurora-notifications/core-whisper.ts`) whose
 * group tier goes through the shared walk `groupMountsFor` at `0c47f4539`.
 *
 * Seeds, through v4's REAL repositories and document store (spec:
 * `core-packet.json`):
 *   - one character per corpus row (`repos.characters.create`, pinned ids —
 *     each provisions + scaffolds its own vault; the scaffold writes no `Core/`);
 *   - three plain database stores (`repos.docMountPoints.create`, pinned ids):
 *     the orphan group's store and two shelves shared by the sound shapes;
 *   - the groups (`repos.groups.create` — each provisions an official store);
 *   - the `Core/*.md` documents (`writeDatabaseDocument`) in vaults, official
 *     stores and plain stores;
 *   - the links (`repos.groupDocMountLinks.create`) and memberships (REAL
 *     `GroupCharacterMembersRepository`, in join order — the walk reads them in
 *     rowid order), INCLUDING an orphan membership + link for a group id with
 *     NO `groups` row;
 *   - R-F: a group whose official store's `properties.json` is malformed (the
 *     hydrated `groups.findById` throws; the raw row is whole — self-checked
 *     here, and probed in-case on both sides by the oracle / the Rust test);
 *   - the `nullOfficial` groups' `officialMountPointId` NULLed by a raw UPDATE
 *     AFTER their links exist (the order's sound shapes need a group whose
 *     stores are exactly its links).
 *
 * Per-run pair under /tmp, never committed. Both sides then READ a COPY.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   rm -f /tmp/qt-corepacket-main.db /tmp/qt-corepacket-mount.db
 *   QT_FIXTURE_COREPACKET_MAIN=/tmp/qt-corepacket-main.db \
 *   QT_FIXTURE_COREPACKET_MOUNT=/tmp/qt-corepacket-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-core-packet-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  characters: Array<{ id: string; name: string }>;
  stores: Array<{ id: string; name: string }>;
  groups: Array<{ id: string; name: string; nullOfficial?: boolean; corruptProperties?: string }>;
  memberships: Array<{ id: string; groupId: string; characterId: string }>;
  links: Array<{ id: string; groupId: string; mountPointId: string }>;
  docs: Array<{ mount: string; path: string; content: string }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'core-packet.json'), 'utf8')) as Spec;

  const outMain = process.env.QT_FIXTURE_COREPACKET_MAIN;
  const outMount = process.env.QT_FIXTURE_COREPACKET_MOUNT;
  if (!outMain || !outMount) {
    throw new Error('QT_FIXTURE_COREPACKET_MAIN and QT_FIXTURE_COREPACKET_MOUNT must be set');
  }
  for (const base of [outMain, outMount]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = base + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-corepacket-build-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = outMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = outMount;
  // The llm-logs sibling is not part of the pair (nothing here reads it).
  process.env.SQLITE_LLM_LOGS_PATH = join(scratch, 'llm-logs.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, ensureCollection, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { GroupCharacterMembersRepository } = await import(
    '@/lib/database/repositories/group-character-members.repository'
  );
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const { UserSchema, CharacterSchema } = await import('@/lib/schemas/types');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
    ProjectDocMountLinkSchema,
    GroupDocMountLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);

  // Materialize the mount-index store tables (`build-self-inventory-fixture.ts`'s
  // block, verbatim — the ports never issue CREATE TABLE).
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
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  // v4's blobs repository creates this lazily from hand-written DDL
  // (`doc-mount-blobs.repository.ts`); a second write to any path GCs through it.
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
  const stamp = { createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp };

  // 1. User.
  await repos.users.create(
    { username: 'reader', name: 'Reader' } as never,
    { id: spec.userId, ...stamp } as never
  );

  // 2. Characters (each provisions + scaffolds a vault).
  for (const c of spec.characters) {
    await repos.characters.create(
      {
        name: c.name,
        userId: spec.userId,
        description: null,
        identity: null,
        personality: null,
        controlledBy: 'llm',
        canBeCarina: false,
      } as never,
      { id: c.id, ...stamp }
    );
  }

  // 3. Plain database stores.
  for (const s of spec.stores) {
    await repos.docMountPoints.create(
      {
        name: s.name,
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
      { id: s.id, ...stamp }
    );
  }

  // 4. Groups (each provisions an official store).
  for (const g of spec.groups) {
    await repos.groups.create({ name: g.name } as never, { id: g.id, ...stamp } as never);
  }

  // `official:<groupId>` / `vault:<characterId>` / a literal store id.
  const resolveMount = async (ref: string): Promise<string> => {
    if (ref.startsWith('official:')) {
      const row = await repos.groups.findByIdRaw(ref.slice('official:'.length));
      if (!row?.officialMountPointId) throw new Error(`no official store for ${ref}`);
      return row.officialMountPointId;
    }
    if (ref.startsWith('vault:')) {
      const row = await repos.characters.findByIdRaw(ref.slice('vault:'.length));
      if (!row?.characterDocumentMountPointId) throw new Error(`no vault for ${ref}`);
      return row.characterDocumentMountPointId;
    }
    return ref;
  };

  // 5. Documents, through the REAL document store.
  const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
  for (const d of spec.docs) {
    await writeDatabaseDocument(await resolveMount(d.mount), d.path, d.content);
  }

  // 6. Links (before any official pointer is NULLed).
  for (const l of spec.links) {
    await repos.groupDocMountLinks.create(
      { groupId: l.groupId, mountPointId: await resolveMount(l.mountPointId) } as never,
      { id: l.id, ...stamp }
    );
  }

  // 7. Memberships, in join order.
  const members = new GroupCharacterMembersRepository();
  for (const m of spec.memberships) {
    await members.create({ groupId: m.groupId, characterId: m.characterId }, { id: m.id, ...stamp });
  }

  // 8. R-F: malformed `properties.json` (the `store-unavailable-routes` plant),
  //    self-checked — the hydrated read must throw, the raw row stay whole.
  for (const g of spec.groups) {
    if (g.corruptProperties === undefined) continue;
    await writeDatabaseDocument(
      await resolveMount(`official:${g.id}`),
      'properties.json',
      g.corruptProperties
    );
    let threw = false;
    try {
      await repos.groups.findById(g.id);
    } catch {
      threw = true;
    }
    if (!threw) throw new Error(`R-F plant did not break the hydrated read of ${g.id}`);
    if ((await repos.groups.findByIdRaw(g.id))?.name !== g.name) {
      throw new Error(`R-F plant broke the raw read of ${g.id}`);
    }
  }

  // 9. The sound shapes' groups: stores = exactly their links.
  for (const g of spec.groups) {
    if (!g.nullOfficial) continue;
    await rawQuery('UPDATE groups SET officialMountPointId = NULL WHERE id = ?', [g.id]);
  }

  closeMountIndexSQLiteClient();
  closeLLMLogsSQLiteClient();
  await closeDatabase();

  process.stderr.write(`built core-packet fixtures: main=${outMain} mount=${outMount}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`core-packet fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
