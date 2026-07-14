/**
 * The committed files-family server-surface fixture builder (P4.6ae deliverable) —
 * the test-pepper substrate for `files_routes_equivalence`.
 *
 * Bakes, via v4's REAL repositories (all ids/timestamps pinned so both
 * differential sides read identical rows — reads diff with no remap; only the
 * folder-create + move/promote mutations mint fresh ids/stamps, which the harness
 * blanks or ignores):
 *   - TWO users: USER_A (= v5 SINGLE_USER_ID, the fixture owner) + USER_B (the
 *     ownership/forbidden arm),
 *   - USER_A general files across folders, incl. a LEGACY row with NULL folderPath
 *     + a storageKey that DERIVES `/docs/` (the resolved-vs-raw folderPath arm), an
 *     UNLINKED file (delete-happy), and a STALE-linkedTo file (linkedTo points at a
 *     non-existent entity → the stale-clear arm),
 *   - a store-backed PROJECT + project files (the scope-collision arms),
 *   - a general folders tree (`/docs/` with files + the child `/docs/reports/`;
 *     `/empty/` empty; `/trash/` with a file) + a project folder `/plans/`,
 *   - USER_B a file (the ownership/forbidden + listing-exclusion arm).
 *
 * NOT seeded (bounded deferral this lane — the FILE_HAS_ASSOCIATIONS *real*
 * associations arm + dissociate need a character-default-image + a
 * chat-message-attachment; enumerated in the lane report): a file linked into a
 * live character/message.
 *
 * Regenerate (Node 24, from the v4 checkout) + re-copy the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_FILES_MAIN=$W/crates/quilltap-web/tests/fixtures/files-main.db \
 *   QT_FIXTURE_FILES_MOUNT=$W/crates/quilltap-web/tests/fixtures/files-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-files-web-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  userIdB: string;
  projectId: string;
  seedTimestamp: string;
}

// ── Pinned entity ids (shared verbatim with the Rust differential) ──────────
const PROJECT_1 = '90000000-0000-4000-8000-000000000001';

// General files (USER_A).
const F_ROOT = 'f0000000-0000-4000-8000-000000000001'; // '/', newest
const F_DOCS = 'f0000000-0000-4000-8000-000000000002'; // '/docs/'
const F_DOCS2 = 'f0000000-0000-4000-8000-000000000003'; // '/docs/'
const F_LEGACY = 'f0000000-0000-4000-8000-000000000004'; // folderPath NULL, storageKey derives /docs/
const F_REPORTS = 'f0000000-0000-4000-8000-000000000005'; // '/docs/reports/'
const F_UNLINKED = 'f0000000-0000-4000-8000-000000000021'; // '/trash/', linkedTo [] (delete-happy)
const F_STALE = 'f0000000-0000-4000-8000-000000000022'; // '/', linkedTo [dead] (delete-stale)

// Project files (USER_A).
const PF_1 = 'f0000000-0000-4000-8000-000000000011'; // '/'
const PF_2 = 'f0000000-0000-4000-8000-000000000012'; // '/plans/'

// USER_B file (ownership).
const F_USERB = 'f0000000-0000-4000-8000-0000000000b1';

// Folders.
const FLD_DOCS = 'd0000000-0000-4000-8000-000000000001'; // /docs/
const FLD_REPORTS = 'd0000000-0000-4000-8000-000000000002'; // /docs/reports/ (child of /docs/)
const FLD_EMPTY = 'd0000000-0000-4000-8000-000000000003'; // /empty/
const FLD_TRASH = 'd0000000-0000-4000-8000-000000000004'; // /trash/ (has F_UNLINKED)
const FLD_SUB = 'd0000000-0000-4000-8000-000000000005'; // /sub/ (no files, HAS a child → subfolders-only arm)
const FLD_SUBDEEP = 'd0000000-0000-4000-8000-000000000006'; // /sub/deep/ (child of /sub/, no files)
const FLD_PLANS = 'd0000000-0000-4000-8000-000000000011'; // /plans/ (project)

const DEAD_ENTITY = 'deaddead-0000-4000-8000-00000000dead'; // stale linkedTo target

function sha(tag: string): string {
  return (tag.repeat(64) + '0'.repeat(64)).slice(0, 64);
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'files-web.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_FILES_MAIN;
  const mountOut = process.env.QT_FIXTURE_FILES_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_FILES_MAIN and QT_FIXTURE_FILES_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-files-web-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { CharacterSchema, ChatMetadataSchema } = await import('@/lib/schemas/types');
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
  const { initializePlugins } = await import('@/lib/startup/plugin-initialization');
  await initializePlugins();
  await ensureCollection('characters', CharacterSchema);
  // The associations reads (delete arm) touch chats + chat_messages — materialize
  // both (empty) so the v5 port reads them without a "no such table" error (v4's
  // oracle auto-creates them in its working copy, masking the gap otherwise).
  await ensureCollection('chats', ChatMetadataSchema);

  // The 3-partition mount-index shape (the projects store rides it; the
  // associations reads open with the full shape).
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
  const TS = spec.seedTimestamp;

  // 1. Users.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.users.create(
    { username: 'saturday', email: null, name: 'Saturday' } as never,
    { id: spec.userIdB, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. A store-backed project (the scope arms).
  const project = await repos.projects.create(
    { name: 'The Cartographers', description: 'Mapping the archipelago.', state: {} } as never,
    { id: PROJECT_1, createdAt: TS, updatedAt: TS } as never,
  );
  if (project.id !== PROJECT_1) throw new Error(`project id drift: ${project.id}`);

  // 3. Files.
  type FileSeed = {
    id: string;
    userId?: string;
    filename: string;
    createdAt: string;
    folderPath?: string | null;
    projectId?: string | null;
    storageKey?: string | null;
    linkedTo?: string[];
    category?: string;
    fileStatus?: string;
    description?: string | null;
    width?: number | null;
    height?: number | null;
    mimeType?: string;
    size?: number;
    shaTag: string;
  };
  const mkFile = async (f: FileSeed) => {
    await repos.files.create(
      {
        userId: f.userId ?? spec.userId,
        sha256: sha(f.shaTag),
        originalFilename: f.filename,
        mimeType: f.mimeType ?? 'text/plain',
        size: f.size ?? 1024,
        source: 'UPLOADED',
        category: f.category ?? 'DOCUMENT',
        linkedTo: f.linkedTo ?? [],
        tags: [],
        projectId: f.projectId ?? null,
        folderPath: f.folderPath ?? null,
        storageKey: f.storageKey ?? null,
        fileStatus: f.fileStatus ?? 'ok',
        description: f.description ?? null,
        width: f.width ?? null,
        height: f.height ?? null,
      } as never,
      { id: f.id, createdAt: f.createdAt, updatedAt: f.createdAt } as never,
    );
  };

  await mkFile({ id: F_ROOT, filename: 'root.txt', createdAt: '2026-05-05T00:00:00.000Z', folderPath: '/', shaTag: '1' });
  await mkFile({ id: F_DOCS, filename: 'a.txt', createdAt: '2026-05-04T00:00:00.000Z', folderPath: '/docs/', shaTag: '2', description: 'A doc.' });
  await mkFile({ id: F_DOCS2, filename: 'b.txt', createdAt: '2026-05-03T00:00:00.000Z', folderPath: '/docs/', shaTag: '3' });
  // Legacy: NULL folderPath + a storageKey that derives '/docs/'.
  await mkFile({
    id: F_LEGACY,
    filename: 'legacy.txt',
    createdAt: '2026-05-02T00:00:00.000Z',
    folderPath: null,
    storageKey: '_general/docs/legacy.txt',
    shaTag: '4',
  });
  await mkFile({ id: F_REPORTS, filename: 'r.txt', createdAt: '2026-05-01T00:00:00.000Z', folderPath: '/docs/reports/', shaTag: '5' });
  // Project files.
  await mkFile({ id: PF_1, filename: 'p1.txt', createdAt: '2026-04-05T00:00:00.000Z', folderPath: '/', projectId: PROJECT_1, shaTag: '6' });
  await mkFile({ id: PF_2, filename: 'p2.txt', createdAt: '2026-04-04T00:00:00.000Z', folderPath: '/plans/', projectId: PROJECT_1, shaTag: '7' });
  // Delete arms.
  await mkFile({ id: F_UNLINKED, filename: 'trash.txt', createdAt: '2026-04-03T00:00:00.000Z', folderPath: '/trash/', shaTag: '8' });
  await mkFile({ id: F_STALE, filename: 'stale.txt', createdAt: '2026-04-02T00:00:00.000Z', folderPath: '/', linkedTo: [DEAD_ENTITY], shaTag: '9' });
  // USER_B.
  await mkFile({ id: F_USERB, userId: spec.userIdB, filename: 'b.txt', createdAt: '2026-04-01T00:00:00.000Z', folderPath: '/', shaTag: 'b' });

  // 4. Folders.
  type FolderSeed = { id: string; path: string; name: string; parentFolderId?: string | null; projectId?: string | null };
  const mkFolder = async (f: FolderSeed) => {
    await repos.folders.create(
      {
        userId: spec.userId,
        path: f.path,
        name: f.name,
        parentFolderId: f.parentFolderId ?? null,
        projectId: f.projectId ?? null,
      } as never,
      { id: f.id, createdAt: TS, updatedAt: TS } as never,
    );
  };
  await mkFolder({ id: FLD_DOCS, path: '/docs/', name: 'docs' });
  await mkFolder({ id: FLD_REPORTS, path: '/docs/reports/', name: 'reports', parentFolderId: FLD_DOCS });
  await mkFolder({ id: FLD_EMPTY, path: '/empty/', name: 'empty' });
  await mkFolder({ id: FLD_TRASH, path: '/trash/', name: 'trash' });
  await mkFolder({ id: FLD_SUB, path: '/sub/', name: 'sub' });
  await mkFolder({ id: FLD_SUBDEEP, path: '/sub/deep/', name: 'deep', parentFolderId: FLD_SUB });
  await mkFolder({ id: FLD_PLANS, path: '/plans/', name: 'plans', projectId: PROJECT_1 });

  // Materialize the (empty) chat_messages table (the P4.6w getMessageCount idiom).
  await repos.chats.getMessageCount('00000000-0000-4000-8000-000000000000');

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built files-web fixture: main=${mainOut} mount=${mountOut} ` +
      `(2 users, 1 project, 10 files, 7 folders)\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`files-web fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
