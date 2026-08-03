/**
 * Fixture builder for the tiered-mount-pool read-differential (W4.1d batch 3a).
 *
 * Bakes shared state across TWO databases (main + mount-index) so BOTH v4 and the
 * Rust port resolve it identically:
 *   - Two characters (A responding, B participant) via the REAL
 *     repos.characters.create (pinned ids/ts) — each mints a linked vault mount.
 *   - Five provisioned document stores (pinned ids): the Quilltap General
 *     singleton (its id persisted into MAIN instance_settings), the group G1
 *     official + linked stores, and two project stores.
 *   - A group G1 (slim row raw-inserted with a pinned officialMountPointId) with
 *     charA as a member (NOT charB) + a linked store.
 *   - Project P1 links: the two project stores PLUS colliding links to
 *     charA-vault / G1-official / the General store (so the dedup drops them).
 *
 * All non-vault ids pinned; the char-vault ids are minted then echoed to stderr
 * (the oracle re-reads them, and the Rust port reads the same fixture).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_TMP_MAIN=/tmp/qt-tmp-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-tmp-mount.db \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-tiered-mount-pool-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  generalMountPointId: string;
  generalStoreName: string;
  groupId: string;
  groupOfficialMountPointId: string;
  groupLinkedMountPointId: string;
  projectId: string;
  projectStoreA: string;
  projectStoreB: string;
  charAId: string;
  charBId: string;
  characterA: Record<string, unknown>;
  characterB: Record<string, unknown>;
}

const PINNED_TS = '2026-02-01T00:00:00.000Z';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'tiered-mount-pool.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_TMP_MAIN;
  const mountOut = process.env.QT_FIXTURE_TMP_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_TMP_MAIN and QT_FIXTURE_TMP_MOUNT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-tmp-fixture-build-'));
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

  await initializeDatabase();

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

  // 1. Bake both characters (mints + links their vaults).
  await repos.characters.create(spec.characterA as never, {
    id: spec.charAId,
    createdAt: PINNED_TS,
    updatedAt: PINNED_TS,
  } as never);
  await repos.characters.create(spec.characterB as never, {
    id: spec.charBId,
    createdAt: PINNED_TS,
    updatedAt: PINNED_TS,
  } as never);
  const charA = await repos.characters.findByIdRaw(spec.charAId);
  const charB = await repos.characters.findByIdRaw(spec.charBId);
  const charAVault = charA?.characterDocumentMountPointId as string | null;
  const charBVault = charB?.characterDocumentMountPointId as string | null;
  if (!charAVault || !charBVault) throw new Error('character vault(s) not minted');

  // 2. Provision the standalone stores (General + group + project), pinned ids.
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
      { id, createdAt: PINNED_TS, updatedAt: PINNED_TS },
    );
  };
  await provisionStore(spec.generalMountPointId, spec.generalStoreName);
  await provisionStore(spec.groupOfficialMountPointId, 'Group 1 Official');
  await provisionStore(spec.groupLinkedMountPointId, 'Group 1 Linked');
  await provisionStore(spec.projectStoreA, 'Project Store A');
  await provisionStore(spec.projectStoreB, 'Project Store B');

  // 3. instance_settings generalMountPointId (MAIN db).
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'generalMountPointId',
    spec.generalMountPointId,
  ]);

  // 4. The group slim row (raw insert with a pinned officialMountPointId).
  await rawQuery(
    'INSERT INTO "groups" ("id", "name", "officialMountPointId", "createdAt", "updatedAt") VALUES (?, ?, ?, ?, ?)',
    [spec.groupId, 'Group 1', spec.groupOfficialMountPointId, PINNED_TS, PINNED_TS],
  );

  // 5. charA is a member of G1 (charB is NOT); G1 links its linked store.
  await repos.groupCharacterMembers.addMember(spec.groupId, spec.charAId);
  await repos.groupDocMountLinks.link(spec.groupId, spec.groupLinkedMountPointId);

  // 6. Project links: the two project stores PLUS colliding refs (charA-vault,
  //    G1-official, the General store) that the dedup must drop from the project.
  await repos.projectDocMountLinks.link(spec.projectId, spec.projectStoreA);
  await repos.projectDocMountLinks.link(spec.projectId, spec.projectStoreB);
  await repos.projectDocMountLinks.link(spec.projectId, charAVault);
  await repos.projectDocMountLinks.link(spec.projectId, spec.groupOfficialMountPointId);
  await repos.projectDocMountLinks.link(spec.projectId, spec.generalMountPointId);

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built tiered-mount-pool fixtures: main=${mainOut} mount=${mountOut} charAVault=${charAVault} charBVault=${charBVault}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`tiered-mount-pool fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
