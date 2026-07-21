/**
 * Fixture builder for the doc-edit HOST-FILESYSTEM tool differential (P4.6bg unit 4).
 *
 * Seeds the two-database starting state (a superset of the path-resolver fixture,
 * plus a chat so the tool handlers have a real chat context):
 *   1. A user (pinned id).
 *   2. Character A (systemTransparency: true, vault minted by the REAL create).
 *   3. The General store (its id recorded in instance_settings.generalMountPointId).
 *   4. Project P via the REAL create → a DATABASE official store; an fs-backed
 *      store 'FS Store' project-linked to P (mountType 'filesystem', sentinel
 *      basePath rewritten per-side by the case).
 *   5. A chat with A as a CHARACTER participant (controlledBy 'llm').
 *   6. Legacy project L via the REAL create; its minted official store is DISABLED,
 *      forcing resolveProjectPath to the <filesDir>/<projectId> on-disk fallback.
 * No documents are seeded — the doc_mount_documents / doc_mount_file_links tables
 * stay empty, so the differential can prove the fs ops never write DB rows.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_DFS_MAIN=/tmp/qt-dfs-main.db QT_FIXTURE_DFS_MOUNT=/tmp/qt-dfs-mount.db \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-doc-fs-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  charAId: string;
  chatId: string;
  characterParticipantId: string;
  generalMountPointId: string;
  projectId: string;
  normalStore: string;
  fsStore: string;
  fsStoreName: string;
  legacyProjectId: string;
  characterA: Record<string, unknown>;
}

/// The sentinel basePath baked for the `mountType: 'filesystem'` store — both
/// differential sides rewrite it to their own per-side copy of the temp tree.
const FS_SENTINEL = '__DFS_FS_TREE__';
const PINNED_TS = '2026-02-01T00:00:00.000Z';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'doc-fs.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_DFS_MAIN;
  const mountOut = process.env.QT_FIXTURE_DFS_MOUNT;
  if (!mainOut || !mountOut) throw new Error('QT_FIXTURE_DFS_MAIN and QT_FIXTURE_DFS_MOUNT required');
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-dfs-fixture-build-'));
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
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { ChatDocumentSchema } = await import('@/lib/schemas/chat-document.types');
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
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('projects', ProjectSchema);
  // The doc_open_document new-blank/open path writes chat_documents; v4 creates it
  // lazily on init, but the Rust Writer opens the file as-is — bake the table.
  await ensureCollection('chat_documents', ChatDocumentSchema);

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
    ['group_doc_mount_links', GroupDocMountLinkSchema],
  ] as Array<[string, unknown]>) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }

  const repos = getRepositories();

  // 1. User.
  await repos.users.create(
    { username: 'doc-fs-tester', name: 'Doc FS Tester' } as never,
    { id: spec.userId, createdAt: PINNED_TS, updatedAt: PINNED_TS } as never,
  );

  // 2. Character A with a full provisioned vault.
  await repos.characters.create(spec.characterA as never, {
    id: spec.charAId,
    createdAt: PINNED_TS,
    updatedAt: PINNED_TS,
  } as never);
  const charA = await repos.characters.findByIdRaw(spec.charAId);
  const charAVault = charA?.characterDocumentMountPointId as string | null;
  if (!charAVault) throw new Error('character vault not minted');

  const provision = async (
    id: string,
    name: string,
    enabled: boolean,
    mountType: 'database' | 'filesystem' = 'database',
    basePath = '',
  ): Promise<void> => {
    await repos.docMountPoints.create(
      {
        name,
        basePath,
        mountType,
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
  await provision(spec.fsStore, spec.fsStoreName, true, 'filesystem', FS_SENTINEL);

  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'generalMountPointId',
    spec.generalMountPointId,
  ]);

  // 3. Project P (real create → real official store), link the normal + fs stores.
  await repos.projects.create({ userId: spec.userId, name: 'Project P' } as never, {
    id: spec.projectId,
    createdAt: PINNED_TS,
    updatedAt: PINNED_TS,
  } as never);
  const project = await repos.projects.findByIdRaw(spec.projectId);
  const officialStore = project?.officialMountPointId as string | null;
  for (const store of [spec.normalStore, spec.fsStore]) {
    await repos.projectDocMountLinks.link(spec.projectId, store);
  }

  // 4. Chat with A as a CHARACTER participant.
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Doc FS Fixture',
      participants: [
        {
          id: spec.characterParticipantId,
          type: 'CHARACTER',
          characterId: spec.charAId,
          controlledBy: 'llm',
          status: 'active',
          createdAt: PINNED_TS,
          updatedAt: PINNED_TS,
        },
      ],
      chatType: 'salon',
      contextSummary: null,
      avatarGenerationEnabled: false,
    } as never,
    { id: spec.chatId, createdAt: PINNED_TS, updatedAt: PINNED_TS },
  );
  await repos.chats.getMessageCount(spec.chatId);

  // 5. Legacy project L: its minted official store is DISABLED, so resolveProjectPath
  //    falls through to the <filesDir>/<projectId> on-disk fallback.
  await repos.projects.create({ userId: spec.userId, name: 'Project L' } as never, {
    id: spec.legacyProjectId,
    createdAt: PINNED_TS,
    updatedAt: PINNED_TS,
  } as never);
  const legacyProject = await repos.projects.findByIdRaw(spec.legacyProjectId);
  const legacyOfficial = legacyProject?.officialMountPointId as string | null;
  if (!legacyOfficial) throw new Error('project L official store not minted');
  midb.prepare('UPDATE doc_mount_points SET enabled = 0 WHERE id = ?').run(legacyOfficial);

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built doc-fs fixtures: main=${mainOut} mount=${mountOut} charAVault=${charAVault} officialStore=${officialStore} legacyOfficial=${legacyOfficial}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`doc-fs fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
