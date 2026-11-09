/**
 * P4.D374 fixture builder — the seed for `brahma_inline_profile_equivalence`
 * (v4 `c539c82e0` `resolveInlineBrahmaConnectionProfile`).
 *
 * Bakes, through v4's REAL repositories (MAIN + MOUNT-INDEX), the rows named in
 * `brahma-inline-profile.json`: three users' connection profiles, six projects
 * (a set / stale / foreign / null / absent `brahmaConnectionProfileId`, and one
 * whose `properties.json` is then overwritten with unparseable bytes through the
 * REAL `writeDatabaseDocument`, so `projects.findById` throws), eight chats, and
 * an EMPTY `instance_settings` table (memory note
 * `builder-fixtures-lack-instance-settings` — a builder boot never creates it).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_BIP_MAIN=/tmp/qt-bip-main.db QT_FIXTURE_BIP_MOUNT=/tmp/qt-bip-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-brahma-inline-profile-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  provider: string;
  modelName: string;
  users: Record<string, string>;
  profiles: Array<{ id: string; user: string; name: string; isDefault: boolean }>;
  projects: Array<{
    label: string;
    name: string;
    brahmaConnectionProfileId?: string | null;
    corruptProperties?: string;
  }>;
  chats: Array<{ id: string; user: string; project: string | null }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'brahma-inline-profile.json'), 'utf8')) as Spec;
  const outMain = process.env.QT_FIXTURE_BIP_MAIN;
  const outMount = process.env.QT_FIXTURE_BIP_MOUNT;
  if (!outMain || !outMount) {
    throw new Error('QT_FIXTURE_BIP_MAIN and QT_FIXTURE_BIP_MOUNT must point at the .db files to write');
  }
  for (const base of [outMain, outMount]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      if (existsSync(base + suffix)) rmSync(base + suffix);
    }
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-bip-build-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = outMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = outMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { ProjectSchema } = await import('@/lib/schemas/project.types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const mi = await import('@/lib/schemas/mount-index.types');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');

  await initializeDatabase();
  await ensureCollection('projects', ProjectSchema);
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
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
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  // The blobs table v4's blobs repository creates lazily (build-projects-tier2-fixture.ts).
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
  for (const p of spec.profiles) {
    await repos.connections.create(
      {
        userId: spec.users[p.user],
        name: p.name,
        provider: spec.provider,
        modelName: spec.modelName,
        isDefault: p.isDefault,
        apiKeyId: null,
      } as never,
      { id: p.id },
    );
  }
  const projectIds = new Map<string, string>();
  for (const p of spec.projects) {
    const input: Record<string, unknown> = { name: p.name, state: {} };
    if ('brahmaConnectionProfileId' in p) input.brahmaConnectionProfileId = p.brahmaConnectionProfileId;
    const created = (await repos.projects.create(input as never)) as { id: string; officialMountPointId: string };
    projectIds.set(p.label, created.id);
    if (p.corruptProperties !== undefined) {
      await writeDatabaseDocument(created.officialMountPointId, 'properties.json', p.corruptProperties);
    }
  }
  for (const c of spec.chats) {
    await repos.chats.create(
      {
        userId: spec.users[c.user],
        title: `Inline chat ${c.id.slice(-2)}`,
        participants: [],
        chatType: 'salon',
        contextSummary: null,
        tags: [],
        projectId: c.project ? projectIds.get(c.project) : null,
      } as never,
      { id: c.id } as never,
    );
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(`built brahma-inline-profile fixture: ${outMain} + ${outMount}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`brahma-inline-profile fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
