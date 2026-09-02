/**
 * @jest-environment node
 *
 * P4.6ae FILES route-surface ORACLE: drives v4's REAL files-family route handlers
 * over a FRESH copy of the committed files fixture per case, emitting each response
 * (status + body + optional post-mutation table dumps) so the Rust ports
 * (`api::files::*`) diff byte-for-byte.
 *
 * Only the seams the Rust harness also neutralizes are mocked (auth session +
 * startup gate); the DB stack is doMocked to the REAL modules past jest.setup.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-files-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/files-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/files-web.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_FILES_MAIN=$V5W/crates/quilltap-web/tests/fixtures/files-main.db \
 *   QT_FIXTURE_FILES_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/files-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-files-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- files-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  userIdB: string;
  projectId: string;
}

const PROJECT = '90000000-0000-4000-8000-000000000001';
const MISSING_PROJECT = '90000000-0000-4000-8000-0000000000ff';
const F_ROOT = 'f0000000-0000-4000-8000-000000000001';
const F_DOCS = 'f0000000-0000-4000-8000-000000000002';
const F_UNLINKED = 'f0000000-0000-4000-8000-000000000021';
const F_STALE = 'f0000000-0000-4000-8000-000000000022';
const PF_1 = 'f0000000-0000-4000-8000-000000000011';
const F_USERB = 'f0000000-0000-4000-8000-0000000000b1';
// P4.6ah
const F_ASSOC = 'f0000000-0000-4000-8000-000000000031';
const F_IMG1 = 'f0000000-0000-4000-8000-000000000061';
const F_IMG2 = 'f0000000-0000-4000-8000-000000000062';
const F_IMG_USERB = 'f0000000-0000-4000-8000-0000000000b2';
const CHAT_G = 'c0000000-0000-4000-8000-000000000001';
const CHAT_P = 'c0000000-0000-4000-8000-000000000002';
const PF_DUP = 'f0000000-0000-4000-8000-000000000041';

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'GET',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

interface UploadFile {
  name: string;
  type: string;
  bytes: Buffer;
}
function mockMultipartRequest(
  url: string,
  fields: { file?: UploadFile; [k: string]: unknown },
): unknown {
  const fd = new FormData();
  for (const [k, v] of Object.entries(fields)) {
    if (k === 'file' && v) {
      const f = v as UploadFile;
      fd.append('file', new File([f.bytes], f.name, { type: f.type }));
    } else if (v != null) {
      fd.append(k, String(v));
    }
  }
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'multipart/form-data; boundary=x' }),
    formData: async () => fd,
    json: async () => ({}),
  };
}

function applyMocks(spec: Spec): void {
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories'),
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store'),
  );
  // The upload legs write real blobs to doc_mount_blobs — un-mock the storage
  // write path (jest.setup stubs fileStorageManager) so v4 and the v5 seams both
  // land bytes in the mount store. Text uploads never touch sharp.
  jest.doMock('@/lib/file-storage/manager', () =>
    jest.requireActual('@/lib/file-storage/manager'),
  );
  jest.doMock('@/lib/file-storage/user-uploads-bridge', () =>
    jest.requireActual('@/lib/file-storage/user-uploads-bridge'),
  );
  jest.doMock('@/lib/file-storage/project-store-bridge', () =>
    jest.requireActual('@/lib/file-storage/project-store-bridge'),
  );
  jest.doMock('@/lib/mount-index/store-file', () =>
    jest.requireActual('@/lib/mount-index/store-file'),
  );
  jest.doMock('@/lib/mount-index/mount-chunk-cache', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/mount-index/mount-chunk-cache'),
    invalidateMountPoint: jest.fn(),
  }));
  jest.doMock('@/lib/mount-index/embedding-scheduler', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/mount-index/embedding-scheduler'),
    enqueueEmbeddingJobsForMountPoint: jest.fn().mockResolvedValue(undefined),
  }));
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: spec.userId } }),
  }));
  jest.doMock('@/lib/startup/startup-state', () => {
    const actual = jest.requireActual('@/lib/startup/startup-state');
    return {
      __esModule: true,
      ...actual,
      startupState: {
        ...actual.startupState,
        isReady: () => true,
        waitForReady: async () => true,
        isPepperResolved: () => true,
        getPepperState: () => 'resolved',
        getPhase: () => 'ready',
        isLockedMode: () => false,
      },
    };
  });
}

/** Dump the files + folders tables (baked ids → no remap). */
async function dumpTables(): Promise<unknown> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const main = getRawDatabase() as unknown as { prepare: (s: string) => { all: () => unknown } };
  return {
    files: main
      .prepare('SELECT id, projectId, folderPath, linkedTo, fileStatus FROM files ORDER BY id')
      .all(),
    folders: main
      .prepare('SELECT id, path, name, parentFolderId, projectId FROM folders ORDER BY id')
      .all(),
  };
}

interface CaseSpec {
  name: string;
  dump?: boolean;
  assoc?: boolean;
  run: () => Promise<{ status: number; body: unknown }>;
}

async function loadRoute(path: string): Promise<Record<string, (...a: unknown[]) => Promise<unknown>>> {
  return (await import(path)) as never;
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

const FILES = 'http://localhost/api/v1/files';
const FOLDERS = 'http://localhost/api/v1/files/folders';

const filesGet = (q: string) =>
  async () => respond(await (await loadRoute('@/app/api/v1/files/route')).GET(mockRequest(`${FILES}${q}`)));
const foldersGet = (q: string) =>
  async () => respond(await (await loadRoute('@/app/api/v1/files/folders/route')).GET(mockRequest(`${FOLDERS}${q}`)));
const itemPost = (id: string, action: string, body: unknown) =>
  async () =>
    respond(
      await (await loadRoute('@/app/api/v1/files/[id]/route')).POST(
        mockRequest(`${FILES}/${id}?action=${action}`, body),
        { params: Promise.resolve({ id }) },
      ),
    );
const itemDelete = (id: string, q = '') =>
  async () =>
    respond(
      await (await loadRoute('@/app/api/v1/files/[id]/route')).DELETE(
        mockRequest(`${FILES}/${id}${q}`),
        { params: Promise.resolve({ id }) },
      ),
    );
const foldersPost = (action: string, body: unknown) =>
  async () =>
    respond(
      await (await loadRoute('@/app/api/v1/files/folders/route')).POST(
        mockRequest(`${FOLDERS}?action=${action}`, body),
      ),
    );
const filesPost = (action: string, body: unknown) =>
  async () =>
    respond(
      await (await loadRoute('@/app/api/v1/files/route')).POST(
        mockRequest(`${FILES}?action=${action}`, body),
      ),
    );
const filesUpload = (fields: { file?: UploadFile; [k: string]: unknown }) =>
  async () =>
    respond(
      await (await loadRoute('@/app/api/v1/files/route')).POST(
        mockMultipartRequest(`${FILES}?action=upload`, fields),
      ),
    );
const chatFilesUpload = (chatId: string, fields: { file?: UploadFile; [k: string]: unknown }) =>
  async () =>
    respond(
      await (await loadRoute('@/app/api/v1/chats/[id]/files/route')).POST(
        mockMultipartRequest(`http://localhost/api/v1/chats/${chatId}/files`, fields),
        { params: Promise.resolve({ id: chatId }) },
      ),
    );
const chatFilesLink = (chatId: string, fileId: string) =>
  async () =>
    respond(
      await (await loadRoute('@/app/api/v1/chats/[id]/files/route')).POST(
        mockRequest(`http://localhost/api/v1/chats/${chatId}/files?action=link`, { fileId }),
        { params: Promise.resolve({ id: chatId }) },
      ),
    );

/** Dump characters (image refs) + chat_messages (attachments) — the dissociate arm. */
async function dumpAssoc(): Promise<unknown> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const main = getRawDatabase() as unknown as { prepare: (s: string) => { all: () => unknown } };
  return {
    characters: main
      .prepare('SELECT id, defaultImageId, avatarOverrides FROM characters ORDER BY id')
      .all(),
    messages: main
      .prepare('SELECT id, chatId, content, attachments FROM chat_messages ORDER BY id')
      .all(),
    files: main.prepare('SELECT id FROM files ORDER BY id').all(),
  };
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'files-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();
  try {
    const out = await c.run();
    const payload: Record<string, unknown> = { name: c.name, status: out.status, body: out.body };
    if (c.dump) payload.tables = await dumpTables();
    if (c.assoc) payload.assoc = await dumpAssoc();
    return payload;
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'files-web.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_FILES_MAIN ?? '',
    mount: process.env.QT_FIXTURE_FILES_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-files-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cases: CaseSpec[] = [
    // ── reads ──
    { name: 'list_general', run: filesGet('?filter=general') },
    { name: 'list_project', run: filesGet(`?projectId=${PROJECT}`) },
    { name: 'list_folder', run: filesGet('?folderPath=/docs/') },
    { name: 'folders_general', run: foldersGet('') },
    { name: 'folders_project', run: foldersGet(`?projectId=${PROJECT}`) },
    // ── move / promote ──
    { name: 'move_folder', dump: true, run: itemPost(F_DOCS, 'move', { folderPath: '/moved/' }) },
    { name: 'move_filename', dump: true, run: itemPost(F_DOCS, 'move', { filename: 'renamed.txt' }) },
    { name: 'move_to_project', dump: true, run: itemPost(F_ROOT, 'move', { projectId: PROJECT }) },
    { name: 'move_to_general', dump: true, run: itemPost(PF_1, 'move', { projectId: null }) },
    { name: 'move_project_missing', run: itemPost(F_ROOT, 'move', { projectId: MISSING_PROJECT }) },
    { name: 'move_none', run: itemPost(F_ROOT, 'move', {}) },
    { name: 'promote_to_project', dump: true, run: itemPost(F_ROOT, 'promote', { targetProjectId: PROJECT }) },
    { name: 'promote_to_general', dump: true, run: itemPost(PF_1, 'promote', {}) },
    // ── delete ──
    { name: 'delete_happy', dump: true, run: itemDelete(F_UNLINKED) },
    { name: 'delete_stale', dump: true, run: itemDelete(F_STALE) },
    { name: 'delete_forbidden', run: itemDelete(F_USERB) },
    // ── folders ──
    { name: 'folder_create_new', dump: true, run: foldersPost('create', { path: '/new/' }) },
    { name: 'folder_create_idempotent', run: foldersPost('create', { path: '/docs/' }) },
    // P4.D145 (v4 `a5df98b3f`, bug 114): the same path INSIDE a project is a
    // different folder — the unique index keys on (userId, COALESCE(projectId,
    // ''), path), so the general `/docs/` row must not make this one look like a
    // duplicate. Reaching the chokepoint's create branch with a colliding path
    // is the closest a sequential route call gets to the race it exists for; a
    // literal pre-planted duplicate is unreachable now that the index forbids
    // one (see the folders remap family's constraint arm).
    { name: 'folder_create_same_path_in_project', dump: true, run: foldersPost('create', { path: '/docs/', projectId: PROJECT }) },
    { name: 'folder_create_parent_chain', dump: true, run: foldersPost('create', { path: '/a/b/c/' }) },
    { name: 'folder_rename', dump: true, run: foldersPost('rename', { path: '/docs/', newName: 'documents' }) },
    { name: 'folder_rename_root', run: foldersPost('rename', { path: '/', newName: 'x' }) },
    { name: 'folder_delete_happy', dump: true, run: foldersPost('delete', { path: '/empty/' }) },
    { name: 'folder_delete_has_files', run: foldersPost('delete', { path: '/trash/' }) },
    // /docs/ has 3 files (incl. /docs/reports/) → the files-count check fires FIRST.
    { name: 'folder_delete_files_before_subfolders', run: foldersPost('delete', { path: '/docs/' }) },
    // /sub/ has a child but NO files → the pure-subfolders 400 arm.
    { name: 'folder_delete_has_subfolders', run: foldersPost('delete', { path: '/sub/' }) },
    // ── P4.6ah: delete envelope (associations / force / dissociate) ──
    { name: 'delete_associations', run: itemDelete(F_ASSOC) },
    { name: 'delete_force', assoc: true, run: itemDelete(F_ASSOC, '?force=true') },
    { name: 'delete_dissociate', assoc: true, run: itemDelete(F_ASSOC, '?dissociate=true') },
    // ── P4.6ah: maintenance ──
    { name: 'thumbnails', run: filesPost('generate-thumbnails', { fileIds: [F_IMG1, F_IMG2, F_ROOT, F_IMG_USERB] }) },
    { name: 'cleanup_stale_dryrun', run: filesPost('cleanup-stale', { dryRun: true }) },
    { name: 'cleanup_orphans_dryrun', run: filesPost('cleanup-orphans', { mode: 'delete', dryRun: true }) },
    { name: 'cleanup_orphans_move', dump: true, run: filesPost('cleanup-orphans', { mode: 'move', dryRun: false }) },
    // ── P4.6ah: general upload (multipart) ──
    { name: 'upload_new_project', run: filesUpload({ file: { name: 'newdoc.txt', type: 'text/plain', bytes: Buffer.from('a fresh document body') }, projectId: PROJECT }) },
    { name: 'upload_overwrite_project', dump: true, run: filesUpload({ file: { name: 'p1.txt', type: 'text/plain', bytes: Buffer.from('overwritten p1 body') }, projectId: PROJECT }) },
    { name: 'upload_new_general', run: filesUpload({ file: { name: 'gen.txt', type: 'text/plain', bytes: Buffer.from('general upload body') } }) },
    // ── P4.6ah: chat upload (multipart) + link ──
    { name: 'chat_upload_nonproject', run: chatFilesUpload(CHAT_G, { file: { name: 'note.txt', type: 'text/plain', bytes: Buffer.from('a chat note') } }) },
    { name: 'chat_upload_conflict', run: chatFilesUpload(CHAT_P, { file: { name: 'dup.txt', type: 'text/plain', bytes: Buffer.from('new dup content') } }) },
    { name: 'chat_upload_skip', run: chatFilesUpload(CHAT_P, { file: { name: 'dup.txt', type: 'text/plain', bytes: Buffer.from('skip me') }, resolution: 'skip' }) },
    { name: 'chat_upload_keepboth', run: chatFilesUpload(CHAT_P, { file: { name: 'dup.txt', type: 'text/plain', bytes: Buffer.from('kept both body') }, resolution: 'keepBoth' }) },
    { name: 'chat_upload_replace', run: chatFilesUpload(CHAT_P, { file: { name: 'dup.txt', type: 'text/plain', bytes: Buffer.from('replacement body') }, resolution: 'replace', conflictingFileId: PF_DUP }) },
    { name: 'chat_upload_link', run: chatFilesLink(CHAT_G, F_UNLINKED) },
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`files-routes oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('files-routes oracle', async () => {
  await main();
});
