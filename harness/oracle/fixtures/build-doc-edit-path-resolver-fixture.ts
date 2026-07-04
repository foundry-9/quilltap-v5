/**
 * Fixture builder for the doc-edit path-resolver + URI-producer read-differential.
 *
 * Bakes: character A (vault minted by the REAL create); the General store; a
 * project P with a database official store; and a set of P-linked database stores
 * — a normal store, TWO with the same name ("Shared Name", ambiguous), and one
 * DISABLED store. Every store is `mountType: 'database'` (the FS scopes stay a
 * documented seam). All non-vault ids pinned; the vault id is echoed to stderr.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_DPR_MAIN=/tmp/qt-dpr-main.db QT_FIXTURE_DPR_MOUNT=/tmp/qt-dpr-mount.db \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-doc-edit-path-resolver-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  charAId: string;
  generalMountPointId: string;
  projectId: string;
  normalStore: string;
  sharedStoreA: string;
  sharedStoreB: string;
  disabledStore: string;
  characterA: Record<string, unknown>;
}

const PINNED_TS = '2026-02-01T00:00:00.000Z';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'doc-edit-path-resolver.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_DPR_MAIN;
  const mountOut = process.env.QT_FIXTURE_DPR_MOUNT;
  if (!mainOut || !mountOut) throw new Error('QT_FIXTURE_DPR_MAIN and QT_FIXTURE_DPR_MOUNT required');
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-dpr-fixture-build-'));
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
  const { CharacterSchema, ProjectSchema } = await import('@/lib/schemas/types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
    ProjectDocMountLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('projects', ProjectSchema);

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  for (const [name, schema] of [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
    ['project_doc_mount_links', ProjectDocMountLinkSchema],
  ] as Array<[string, unknown]>) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }

  const repos = getRepositories();

  await repos.characters.create(spec.characterA as never, {
    id: spec.charAId,
    createdAt: PINNED_TS,
    updatedAt: PINNED_TS,
  } as never);
  const charA = await repos.characters.findByIdRaw(spec.charAId);
  const charAVault = charA?.characterDocumentMountPointId as string | null;
  if (!charAVault) throw new Error('character vault not minted');

  const provision = async (id: string, name: string, enabled: boolean): Promise<void> => {
    await repos.docMountPoints.create(
      {
        name,
        basePath: '',
        mountType: 'database',
        storeType: 'documents',
        includePatterns: [],
        excludePatterns: [],
        enabled,
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
  await provision(spec.generalMountPointId, 'Quilltap General', true);
  await provision(spec.normalStore, 'Project Docs', true);
  await provision(spec.sharedStoreA, 'Shared Name', true);
  await provision(spec.sharedStoreB, 'Shared Name', true);
  await provision(spec.disabledStore, 'Disabled Store', false);

  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'generalMountPointId',
    spec.generalMountPointId,
  ]);

  // Project P via the REAL create — provisions a real official store (with the
  // properties.json the overlaid `projects.findById` reads in resolveProjectPath).
  await repos.projects.create({ userId: spec.userId, name: 'Project P' } as never, {
    id: spec.projectId,
    createdAt: PINNED_TS,
    updatedAt: PINNED_TS,
  } as never);
  const project = await repos.projects.findByIdRaw(spec.projectId);
  const officialStore = project?.officialMountPointId as string | null;

  for (const store of [spec.normalStore, spec.sharedStoreA, spec.sharedStoreB, spec.disabledStore]) {
    await repos.projectDocMountLinks.link(spec.projectId, store);
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built doc-edit-path-resolver fixtures: main=${mainOut} mount=${mountOut} charAVault=${charAVault} officialStore=${officialStore}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`doc-edit-path-resolver fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
