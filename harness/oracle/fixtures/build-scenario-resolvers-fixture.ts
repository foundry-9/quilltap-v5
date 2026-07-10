/**
 * Fixture builder for the scenario-resolver read-differential (P4.4 unit 2,
 * sub-unit 1).
 *
 * Bakes two document stores across the mount-index DB (with the General store's
 * pointer in MAIN `instance_settings`) and seeds their `Scenarios/` folders via
 * v4's REAL `linkDocumentContent`:
 *   - the Quilltap General singleton store:
 *       Scenarios/welcome.md   — frontmatter + a real body
 *       Scenarios/empty.md     — frontmatter, whitespace-only body (→ null)
 *   - a project's official store:
 *       Scenarios/tavern.md    — no frontmatter, a real body
 *
 * Both v4's real resolvers and the Rust port
 * (db::scenarios::resolve_{general,project}_scenario_body) read the SAME baked
 * fixture and must produce the same body (or null). All ids pinned.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SCEN_MAIN=/tmp/qt-scen-main.db QT_FIXTURE_SCEN_MOUNT=/tmp/qt-scen-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-scenario-resolvers-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { createHash } from 'node:crypto';

interface Spec {
  testPepperBase64: string;
  generalMountPointId: string;
  generalStoreName: string;
  projectMountPointId: string;
  projectStoreName: string;
  generalWelcomeBody: string;
  generalEmptyDoc: string;
  projectTavernBody: string;
}

const TS = '2026-02-01T00:00:00.000Z';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'scenario-resolvers.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_SCEN_MAIN;
  const mountOut = process.env.QT_FIXTURE_SCEN_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_SCEN_MAIN and QT_FIXTURE_SCEN_MOUNT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-scen-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
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
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }

  const repos = getRepositories();

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
      { id, createdAt: TS, updatedAt: TS },
    );
  };
  await provisionStore(spec.generalMountPointId, spec.generalStoreName);
  await provisionStore(spec.projectMountPointId, spec.projectStoreName);

  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'generalMountPointId',
    spec.generalMountPointId,
  ]);

  const seed = async (mountPointId: string, relativePath: string, content: string): Promise<void> => {
    const fileName = relativePath.split('/').pop() as string;
    await repos.docMountFileLinks.linkDocumentContent({
      mountPointId,
      relativePath,
      fileName,
      folderId: null,
      fileType: 'markdown',
      content,
      contentSha256: createHash('sha256').update(content, 'utf-8').digest('hex'),
      plainTextLength: content.length,
      fileSizeBytes: Buffer.byteLength(content, 'utf-8'),
    } as never);
  };

  await seed(
    spec.generalMountPointId,
    'Scenarios/welcome.md',
    `---\nname: Welcome\ndescription: A market opener\n---\n\n${spec.generalWelcomeBody}\n`,
  );
  await seed(spec.generalMountPointId, 'Scenarios/empty.md', spec.generalEmptyDoc);
  await seed(spec.projectMountPointId, 'Scenarios/tavern.md', `${spec.projectTavernBody}\n`);

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(`built scenario-resolver fixtures: main=${mainOut} mount=${mountOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`scenario-resolvers fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
