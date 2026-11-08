/**
 * @jest-environment node
 *
 * P4.6y MOUNT-OPS route-surface ORACLE (the mutation family: move / copy /
 * link / delete file, delete / move / create folder, the item-route PATCH):
 * drives v4's REAL handlers (`[id]/route.ts` action dispatch,
 * `files/[...path]/route.ts` PATCH, `folders/route.ts` POST) over a FRESH copy
 * of the committed mounts fixture per case, emitting `{status, body, tables}` —
 * the RAW eight-table dump. Normalization happens ONCE, in the Rust test
 * (`tests/mount_common/mod.rs`), applied to BOTH sides.
 *
 * [ed8934f1 / Bug 56] Three folder-create cases point MP_FS's basePath at
 * PLANTED unreachable conditions (missing / not-a-directory / denied) and pin
 * the new 409 diagnosis — see the `PLANT_ROOT` block below.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-mount-ops-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/mount-ops.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/mounts.json" "$TMPO/fixtures/"
 *   cp "$V5W/harness/oracle/fixtures/docx-sample.b64" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MOUNTS_MAIN=$V5W/crates/quilltap-web/tests/fixtures/mounts-main.db \
 *   QT_FIXTURE_MOUNTS_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/mounts-mount.db \
 *   QT_MOUNTS_FS_TREE=$V5W/crates/quilltap-web/tests/fixtures/mounts-fs-tree \
 *   QT_ORACLE_OUT=/tmp/oracle-mount-ops.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- mount-ops
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, cpSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

const MP_DB = 'b6000000-0000-4000-8000-000000000001';
const MP_EMPTY = 'b6000000-0000-4000-8000-000000000002';
const MP_FS = 'b6000000-0000-4000-8000-000000000003';

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
    text: jest.fn().mockResolvedValue(body === undefined ? '' : JSON.stringify(body)),
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
  jest.doMock('@/lib/embedding/embedding-service', () =>
    jest.requireActual('@/lib/embedding/embedding-service'),
  );
  // Keep the in-process job dispatcher OFF (enqueued rows stay PENDING).
  jest.doMock('@/lib/background-jobs/host/processor-host', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/background-jobs/host/processor-host'),
    ensureProcessorRunning: () => {},
  }));
  jest.doMock('@/lib/mount-index/watcher', () => ({
    __esModule: true,
    attachMountPoint: async () => {},
    detachMountPoint: async () => {},
    refreshMountPoint: async () => {},
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

const MOUNT_TABLES: Record<string, string> = {
  points:
    'SELECT id, name, basePath, mountType, storeType, includePatterns, excludePatterns, enabled, ' +
    'lastScannedAt, scanStatus, lastScanError, conversionStatus, conversionError, fileCount, ' +
    'chunkCount, totalSizeBytes, createdAt, updatedAt FROM doc_mount_points ORDER BY id',
  files:
    'SELECT id, sha256, fileSizeBytes, fileType, source, createdAt, updatedAt ' +
    'FROM doc_mount_files ORDER BY sha256, source',
  links:
    'SELECT id, fileId, mountPointId, relativePath, fileName, folderId, originalFileName, ' +
    'originalMimeType, description, descriptionUpdatedAt, conversionStatus, conversionError, ' +
    'plainTextLength, extractedText, extractedTextSha256, extractionStatus, extractionError, ' +
    'chunkCount, allowEmbed, allowCharacterRead, allowCharacterWrite, lastModified, createdAt, ' +
    'updatedAt, linkGroupId FROM doc_mount_file_links ORDER BY mountPointId, relativePath',
  documents:
    'SELECT id, fileId, content, contentSha256, plainTextLength, createdAt, updatedAt ' +
    'FROM doc_mount_documents ORDER BY contentSha256',
  folders:
    'SELECT id, mountPointId, parentId, name, path, createdAt, updatedAt ' +
    'FROM doc_mount_folders ORDER BY mountPointId, path',
  chunks:
    'SELECT c.id, c.linkId, c.mountPointId, c.chunkIndex, c.content, c.tokenCount, ' +
    'c.headingContext, (CASE WHEN c.embedding IS NULL THEN NULL ELSE length(c.embedding) END) ' +
    'AS embeddingLength, c.createdAt, c.updatedAt, l.relativePath AS sortPath ' +
    'FROM doc_mount_chunks c LEFT JOIN doc_mount_file_links l ON l.id = c.linkId ' +
    'ORDER BY c.mountPointId, sortPath, c.chunkIndex',
  blobs:
    'SELECT id, fileId, sha256, sizeBytes, storedMimeType, length(data) AS dataLength, ' +
    'createdAt, updatedAt FROM doc_mount_blobs ORDER BY sha256',
};

const JOBS_SQL =
  'SELECT id, userId, type, status, payload, priority, attempts, maxAttempts, lastError, ' +
  'scheduledAt, startedAt, completedAt, createdAt, updatedAt FROM background_jobs ORDER BY payload';

async function dumpTables(): Promise<Record<string, unknown>> {
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const midb = getRawMountIndexDatabase() as never as {
    prepare: (s: string) => { all: () => unknown[] };
  };
  const main = getRawDatabase() as never as { prepare: (s: string) => { all: () => unknown[] } };
  const out: Record<string, unknown> = {};
  for (const [name, sql] of Object.entries(MOUNT_TABLES)) {
    out[name] = midb.prepare(sql).all();
  }
  out.jobs = main.prepare(JOBS_SQL).all();
  return out;
}

interface CaseSpec {
  name: string;
  run: () => Promise<{ status: number; body: unknown }>;
  /**
   * [ed8934f1] Point MP_FS's basePath somewhere other than the per-case fs-tree
   * copy — the planted Bug-56 reachability conditions. Applied AFTER the
   * standard rewrite, and only to MP_FS (MP_OBS keeps the tree).
   */
  basePath?: string;
  // === P4.D381 ===
  /** In-case plant on this case's fs-tree copy (before the run). */
  plant?: (treeWork: string) => void;
  /** Undo the plant AFTER the dump (modes restored before the copy goes). */
  unplant?: (treeWork: string) => void;
  // === /P4.D381 ===
}

// [ed8934f1] The planted base-path conditions for the folder-create refusals.
// Fixed literal paths, identical on both sides of the diff, so the diagnosis
// sentences (which embed the path) compare byte-for-byte with no
// normalization. The root is per-family (`qt-bug56-ops`, not the
// mount-points family's `qt-bug56-mp`) because cargo runs the two Rust
// differentials as concurrent test binaries.
//
// `denied` is planted as an unreadable PARENT, not a `chmod 000` on the base
// itself: `fs.stat` takes its EACCES from the parent directory's search
// permission, and a 000 directory still stats fine and reads as available.
//
// ⚠ `containerized` is false in BOTH environments, so the container-variant
// sentences are pinned by UNIT tests on the message builder, not here.
const PLANT_ROOT = '/tmp/qt-bug56-ops';
const PLANT_NOT_A_DIRECTORY = `${PLANT_ROOT}/notdir`;
const PLANT_DENIED_PARENT = `${PLANT_ROOT}/denied`;
const PLANT_DENIED = `${PLANT_DENIED_PARENT}/inner`;
const PLANT_MISSING = `${PLANT_ROOT}/gone`;

function plantBasePaths(): void {
  unplantBasePaths();
  mkdirSync(PLANT_DENIED, { recursive: true });
  fs.writeFileSync(PLANT_NOT_A_DIRECTORY, 'not a directory');
  fs.chmodSync(PLANT_DENIED_PARENT, 0o000);
}

function unplantBasePaths(): void {
  // Restore the denied parent before the recursive remove, or the remove
  // itself fails and leaks a 000 directory into every later suite.
  try {
    fs.chmodSync(PLANT_DENIED_PARENT, 0o755);
  } catch {
    /* not planted */
  }
  rmSync(PLANT_ROOT, { recursive: true, force: true });
}

async function loadRoute(
  path: string,
): Promise<Record<string, (...a: unknown[]) => Promise<unknown>>> {
  return (await import(path)) as never;
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

async function drain(): Promise<void> {
  for (let i = 0; i < 60; i++) {
    await new Promise((r) => setImmediate(r));
  }
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string; tree: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'mo-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  const treeWork = join(work, 'fs-tree');
  cpSync(fixtures.tree, treeWork, { recursive: true });
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  // === P4.D381 === the level BEFORE any module reads it; the capture after.
  process.env.LOG_LEVEL = 'debug';
  const logs = await installP4d381Capture();
  // === /P4.D381 ===

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();
  const { initializePlugins } = await import('@/lib/startup/plugin-initialization');
  await initializePlugins();

  const midb = getRawMountIndexDatabase() as never as {
    prepare: (s: string) => { run: (...a: unknown[]) => unknown };
  };
  midb
    .prepare('UPDATE doc_mount_points SET basePath = ? WHERE id IN (?, ?)')
    .run(treeWork, MP_FS, 'b6000000-0000-4000-8000-000000000004');
  if (c.basePath !== undefined) {
    midb.prepare('UPDATE doc_mount_points SET basePath = ? WHERE id = ?').run(c.basePath, MP_FS);
  }

  // === P4.D381 ===
  process.env.QT_P4D381_TREE = treeWork;
  c.plant?.(treeWork);
  logs.length = 0; // only the case's own lines
  // === /P4.D381 ===
  try {
    const out = await c.run();
    await drain();
    const tables = await dumpTables();
    // === P4.D381 === the on-disk tree of this case's fs-tree copy.
    const tree = listTree(treeWork);
    c.unplant?.(treeWork);
    return { name: c.name, status: out.status, body: out.body, tables, tree, logs };
    // === /P4.D381 ===
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

// === P4.D381 (v4 aec626eff / c34986421): helpers ===
/** The fs-tree copy, recursively: `dir/` for a directory, `file:<size>` for a
 *  file, sorted by code unit (both sides list the same way). */
function listTree(root: string): string[] {
  const out: string[] = [];
  const walk = (rel: string): void => {
    const abs = rel ? join(root, rel) : root;
    let names: string[];
    try {
      names = fs.readdirSync(abs);
    } catch {
      out.push(`${rel}/<unreadable>`);
      return;
    }
    for (const name of names) {
      const childRel = rel ? `${rel}/${name}` : name;
      const st = fs.lstatSync(join(root, childRel));
      if (st.isDirectory()) {
        out.push(`${childRel}/`);
        walk(childRel);
      } else {
        out.push(`${childRel}:${st.size}`);
      }
    }
  };
  walk('');
  return out.sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
}
const p4d381Tree = (): string => process.env.QT_P4D381_TREE ?? '';
/** The lines the store-browser rows compare, by message (`MountIndex:FolderOps`,
 *  the action / files routes, the repository's clear DEBUG). */
const P4D381_LINES = new Set([
  'Removed filesystem folder row with the folder',
  'Moved filesystem folder rows with the folder',
  'Deleted folder recursively',
  'Folder copy failed; undoing',
  'Copied folder',
  'Folder move failed; moving back',
  'Could not move a file back after a failed folder move',
  'Moved folder between stores',
  '[Mount Points v1] Deleted folder',
  '[Mount Points v1] Moved folder',
  '[Mount Points v1] Copied folder',
  '[Mount Points v1] Unlinked file from its hard-link group',
  '[Mount Points v1] Error deleting folder',
  '[Mount Points v1] Error moving folder',
  '[Mount Points v1] Error trying to copy folder',
  '[Mount Points v1] Error render file',
  '[Mount Points v1] Listed files for browser',
  'Cleared hard-link group from link',
]);
async function installP4d381Capture(): Promise<Array<{ level: string; message: string; fields: Record<string, unknown> }>> {
  const { logger } = await import('@/lib/logger');
  const lines: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown>; error?: { message: string } };
      if (!P4D381_LINES.has(data.message)) return;
      const { service: _s, environment: _e, module: _m, ...fields } = (data.context ?? {}) as Record<string, unknown>;
      if (data.error) fields.error = data.error.message;
      lines.push(JSON.parse(JSON.stringify({ level: data.level, message: data.message, fields })));
    },
  });
  return lines;
}
const docxSample = (): Buffer => {
  const here = dirname(fileURLToPath(new URL(`file://${__filename}`).href));
  const text = fs.readFileSync(join(here, '..', 'fixtures', 'docx-sample.b64'), 'utf8');
  const payload = text.split('\n').filter((l) => l && !l.startsWith('#')).join('');
  return Buffer.from(payload, 'base64');
};
// === /P4.D381 ===

const B = 'http://localhost/api/v1/mount-points';
const idRoute = () => loadRoute('@/app/api/v1/mount-points/[id]/route');
const itemRoute = () => loadRoute('@/app/api/v1/mount-points/[id]/files/[...path]/route');
const foldersRoute = () => loadRoute('@/app/api/v1/mount-points/[id]/folders/route');
const params = (id: string) => ({ params: Promise.resolve({ id }) });
const itemParams = (id: string, path: string[]) => ({ params: Promise.resolve({ id, path }) });

const act = (id: string, action: string, body?: unknown) => async () =>
  respond(
    await (await idRoute()).POST(mockRequest(`${B}/${id}?action=${action}`, body), params(id)),
  );
const patch = (id: string, path: string[], body: unknown) => async () =>
  respond(
    await (await itemRoute()).PATCH(
      mockRequest(`${B}/${id}/files/${path.join('/')}`, body),
      itemParams(id, path),
    ),
  );
// === P4.D381 ===
const filesRoute = () => loadRoute('@/app/api/v1/mount-points/[id]/files/route');
const getReq = (url: string): unknown => ({
  method: 'GET',
  url,
  nextUrl: new URL(url),
  headers: new Headers(),
});
const itemGet = (id: string, path: string[], query: string) => async () =>
  respond(
    await (await itemRoute()).GET(
      getReq(`${B}/${id}/files/${path.join('/')}${query}`),
      itemParams(id, path),
    ),
  );
const listFiles = (id: string) => async () =>
  respond(await (await filesRoute()).GET(getReq(`${B}/${id}/files`), params(id)));
/** Run `steps` in order and answer the LAST one's response. */
const seq = (...steps: Array<() => Promise<{ status: number; body: unknown }>>) => async () => {
  let last: { status: number; body: unknown } = { status: 0, body: null };
  for (const step of steps) last = await step();
  return last;
};
const chmodIn = (rel: string, mode: number) => async () => {
  fs.chmodSync(join(p4d381Tree(), rel), mode);
  return { status: 0, body: null };
};
const writeIn = (rel: string, data: string | Buffer) => async () => {
  fs.mkdirSync(dirname(join(p4d381Tree(), rel)), { recursive: true });
  fs.writeFileSync(join(p4d381Tree(), rel), data);
  return { status: 0, body: null };
};
// === /P4.D381 ===
const mkFolder = (id: string, body: unknown) => async () =>
  respond(
    await (await foldersRoute()).POST(mockRequest(`${B}/${id}/folders`, body), params(id)),
  );

function cases(): CaseSpec[] {
  const mv = (src: string, dmp: string, dst: string) => ({
    sourcePath: src,
    destMountPointId: dmp,
    destPath: dst,
  });
  return [
    // ── copy ──
    { name: 'copy_db_db', run: act(MP_DB, 'copy-file', mv('notes/intro.md', MP_EMPTY, 'notes/copied.md')) },
    {
      name: 'copy_db_db_force',
      run: act(MP_DB, 'copy-file', { ...mv('notes/intro.md', MP_EMPTY, 'notes/copied.md'), force: true }),
    },
    { name: 'copy_dest_exists', run: act(MP_DB, 'copy-file', mv('notes/intro.md', MP_DB, 'reference.txt')) },
    { name: 'copy_same_path', run: act(MP_DB, 'copy-file', mv('notes/intro.md', MP_DB, 'notes/intro.md')) },
    { name: 'copy_fs_fs', run: act(MP_FS, 'copy-file', mv('notes/alpha.md', MP_FS, 'docs/alpha-copy.md')) },
    { name: 'copy_fs_db', run: act(MP_FS, 'copy-file', mv('notes/alpha.md', MP_DB, 'imported/alpha.md')) },
    { name: 'copy_db_fs', run: act(MP_DB, 'copy-file', mv('notes/intro.md', MP_FS, 'out/intro.md')) },
    { name: 'copy_blob_db_db', run: act(MP_DB, 'copy-file', mv('images/logo.png', MP_EMPTY, 'images/logo.png')) },
    { name: 'copy_missing_source', run: act(MP_DB, 'copy-file', mv('nope.md', MP_EMPTY, 'x.md')) },
    // ── move ──
    { name: 'move_db_db', run: act(MP_DB, 'move-file', mv('reference.txt', MP_EMPTY, 'moved/reference.txt')) },
    { name: 'move_fs_fs', run: act(MP_FS, 'move-file', mv('notes/beta.txt', MP_FS, 'notes/beta-renamed.txt')) },
    { name: 'move_cross_db_fs', run: act(MP_DB, 'move-file', mv('notes/intro.md', MP_FS, 'moved-intro.md')) },
    { name: 'move_dest_exists', run: act(MP_DB, 'move-file', mv('notes/intro.md', MP_DB, 'reference.txt')) },
    // ── link ──
    { name: 'link_db_db', run: act(MP_DB, 'link-file', mv('notes/intro.md', MP_EMPTY, 'linked/intro.md')) },
    { name: 'link_fs_fs', run: act(MP_FS, 'link-file', mv('notes/alpha.md', MP_FS, 'linked-alpha.md')) },
    { name: 'link_cross', run: act(MP_DB, 'link-file', mv('notes/intro.md', MP_FS, 'x.md')) },
    { name: 'link_dest_exists', run: act(MP_DB, 'link-file', mv('notes/intro.md', MP_DB, 'reference.txt')) },
    // ── delete file ──
    { name: 'delete_file_db', run: act(MP_DB, 'delete-file', { path: 'reference.txt' }) },
    { name: 'delete_file_fs', run: act(MP_FS, 'delete-file', { path: 'notes/alpha.md' }) },
    { name: 'delete_file_missing', run: act(MP_DB, 'delete-file', { path: 'nope.md' }) },
    // ── folders ──
    { name: 'folder_delete_db_nonempty', run: act(MP_DB, 'delete-folder', { path: 'notes' }) },
    { name: 'folder_delete_db_missing', run: act(MP_DB, 'delete-folder', { path: 'ghost' }) },
    { name: 'folder_delete_fs_nonempty', run: act(MP_FS, 'delete-folder', { path: 'notes' }) },
    { name: 'folder_delete_fs_missing', run: act(MP_FS, 'delete-folder', { path: 'ghost' }) },
    { name: 'folder_move_db', run: act(MP_DB, 'move-folder', { fromPath: 'notes', toPath: 'notes2' }) },
    {
      name: 'folder_move_fs_after_scan',
      run: async () => {
        await act(MP_FS, 'scan')();
        return act(MP_FS, 'move-folder', { fromPath: 'notes', toPath: 'moved-notes' })();
      },
    },
    { name: 'folder_move_same', run: act(MP_DB, 'move-folder', { fromPath: 'notes', toPath: 'notes' }) },
    { name: 'folder_move_dest_exists', run: act(MP_FS, 'move-folder', { fromPath: 'notes', toPath: 'data' }) },
    // ── item-route PATCH (rename / describe) ──
    { name: 'patch_rename', run: patch(MP_DB, ['notes', 'intro.md'], { rename: 'notes/intro2.md' }) },
    { name: 'patch_desc_blob', run: patch(MP_DB, ['images', 'logo.png'], { description: 'A fresh caption.' }) },
    { name: 'patch_desc_text', run: patch(MP_DB, ['reference.txt'], { description: 'nope' }) },
    {
      name: 'patch_rename_and_desc',
      run: patch(MP_DB, ['images', 'logo.png'], { rename: 'images/badge.png', description: 'Moved & described.' }),
    },
    { name: 'patch_no_fields', run: patch(MP_DB, ['notes', 'intro.md'], {}) },
    { name: 'patch_rename_missing', run: patch(MP_DB, ['ghost.md'], { rename: 'ghost2.md' }) },
    // ── folder create ──
    { name: 'folder_create_db', run: mkFolder(MP_DB, { path: 'fresh/sub' }) },
    { name: 'folder_create_fs', run: mkFolder(MP_FS, { path: 'fresh/sub' }) },
    { name: 'folder_create_unsafe', run: mkFolder(MP_DB, { path: '../evil' }) },
    { name: 'folder_create_bad_chars', run: mkFolder(MP_DB, { path: 'a<b' }) },
    // [ed8934f1 / Bug 56] The store's own root is unreachable: refuse with a
    // 409 carrying the diagnosis, instead of letting the recursive mkdir walk
    // up to the topmost missing ancestor and fabricate the whole chain.
    {
      name: 'folder_create_fs_base_missing',
      run: mkFolder(MP_FS, { path: 'fresh/sub' }),
      basePath: PLANT_MISSING,
    },
    {
      name: 'folder_create_fs_base_not_a_directory',
      run: mkFolder(MP_FS, { path: 'fresh/sub' }),
      basePath: PLANT_NOT_A_DIRECTORY,
    },
    {
      name: 'folder_create_fs_base_denied',
      run: mkFolder(MP_FS, { path: 'fresh/sub' }),
      basePath: PLANT_DENIED,
    },
    {
      name: 'folder_create_then_delete_db',
      run: async () => {
        await mkFolder(MP_DB, { path: 'scratch' })();
        return act(MP_DB, 'delete-folder', { path: 'scratch' })();
      },
    },
    // === P4.D381 (v4 aec626eff / c34986421): the store-browser API rows ===
    // ── item 5: the browser listing with a non-trivial group ──
    {
      name: 'files_list_grouped',
      run: seq(act(MP_DB, 'link-file', mv('notes/intro.md', MP_EMPTY, 'linked/intro.md')), listFiles(MP_DB)),
    },
    // ── item 6: filesystem folder rows follow the folder (c34986421) ──
    {
      name: 'folder_move_fs_nested_to_top',
      run: seq(act(MP_FS, 'scan'), act(MP_FS, 'move-folder', { fromPath: 'notes/sub', toPath: 'sub' })),
    },
    {
      name: 'folder_delete_fs_empty_with_row',
      run: seq(
        act(MP_FS, 'scan'),
        act(MP_FS, 'delete-file', { path: 'notes/sub/deep.md' }),
        act(MP_FS, 'delete-folder', { path: 'notes/sub' }),
      ),
    },
    // ── item 7: the folder family ──
    { name: 'folder_delete_db_recursive', run: act(MP_DB, 'delete-folder', { path: 'notes', recursive: true }) },
    {
      name: 'folder_delete_fs_recursive',
      run: seq(act(MP_FS, 'scan'), act(MP_FS, 'delete-folder', { path: 'notes/sub', recursive: true })),
    },
    {
      // After a scan, `notes/blank.md` (empty → never indexed) and a planted
      // `notes/z.xyz` are the strays: two names.
      name: 'folder_delete_fs_recursive_strays',
      run: seq(
        act(MP_FS, 'scan'),
        writeIn('notes/z.xyz', 'stray'),
        act(MP_FS, 'delete-folder', { path: 'notes', recursive: true }),
      ),
    },
    {
      // No scan: every file under `notes/` is a stray (five) → three names + `, …`.
      name: 'folder_delete_fs_recursive_strays_many',
      run: act(MP_FS, 'delete-folder', { path: 'notes', recursive: true }),
    },
    { name: 'folder_copy_db_db', run: act(MP_DB, 'copy-folder', { fromPath: 'notes', toPath: 'imported', destMountPointId: MP_EMPTY }) },
    {
      name: 'folder_copy_fs_db',
      run: seq(act(MP_FS, 'scan'), act(MP_FS, 'copy-folder', { fromPath: 'notes/sub', toPath: 'fromfs', destMountPointId: MP_DB })),
    },
    { name: 'folder_copy_db_fs', run: act(MP_DB, 'copy-folder', { fromPath: 'notes', toPath: 'fromdb', destMountPointId: MP_FS }) },
    { name: 'folder_copy_into_itself', run: act(MP_DB, 'copy-folder', { fromPath: 'notes', toPath: 'notes/inner' }) },
    { name: 'folder_copy_dest_exists', run: act(MP_DB, 'copy-folder', { fromPath: 'notes', toPath: 'docs' }) },
    { name: 'folder_copy_missing', run: act(MP_DB, 'copy-folder', { fromPath: 'ghost', toPath: 'x' }) },
    { name: 'folder_move_cross_db_db', run: act(MP_DB, 'move-folder', { fromPath: 'notes', toPath: 'moved', destMountPointId: MP_EMPTY }) },
    {
      name: 'folder_move_cross_fs_db',
      run: seq(act(MP_FS, 'scan'), act(MP_FS, 'move-folder', { fromPath: 'notes/sub', toPath: 'fromfs', destMountPointId: MP_DB })),
    },
    {
      name: 'folder_move_cross_fs_strays',
      run: seq(act(MP_FS, 'scan'), act(MP_FS, 'move-folder', { fromPath: 'notes', toPath: 'x', destMountPointId: MP_DB })),
    },
    {
      // The undo plant: `notes/sub` gains a second indexed file; after the scan
      // the SECOND link in `tree.files` order (`findByMountPointId`'s rowid
      // order — `notes/sub/deep.md` first, `notes/sub/deeper.md` second,
      // measured) is made unreadable, so its copy throws after the first landed.
      name: 'folder_copy_undo',
      plant: (tree) => fs.writeFileSync(join(tree, 'notes', 'sub', 'deeper.md'), '# Deeper\n\nFurther down.\n'),
      run: seq(
        act(MP_FS, 'scan'),
        chmodIn('notes/sub/deeper.md', 0o000),
        act(MP_FS, 'copy-folder', { fromPath: 'notes/sub', toPath: 'undo', destMountPointId: MP_DB }),
      ),
      unplant: (tree) => fs.chmodSync(join(tree, 'notes', 'sub', 'deeper.md'), 0o644),
    },
    {
      name: 'folder_move_undo',
      plant: (tree) => fs.writeFileSync(join(tree, 'notes', 'sub', 'deeper.md'), '# Deeper\n\nFurther down.\n'),
      run: seq(
        act(MP_FS, 'scan'),
        chmodIn('notes/sub/deeper.md', 0o000),
        act(MP_FS, 'move-folder', { fromPath: 'notes/sub', toPath: 'undo', destMountPointId: MP_DB }),
      ),
      unplant: (tree) => fs.chmodSync(join(tree, 'notes', 'sub', 'deeper.md'), 0o644),
    },
    { name: 'folder_move_dest_same_store', run: act(MP_DB, 'move-folder', { fromPath: 'notes', toPath: 'notes3', destMountPointId: MP_DB }) },
    // ── item 8: the wire edges of the four actions ──
    { name: 'folder_delete_recursive_wrong_type', run: act(MP_DB, 'delete-folder', { path: 'notes', recursive: 'yes' }) },
    { name: 'folder_delete_recursive_null', run: act(MP_DB, 'delete-folder', { path: 'notes', recursive: null }) },
    { name: 'folder_copy_missing_to', run: act(MP_DB, 'copy-folder', { fromPath: 'notes' }) },
    { name: 'folder_move_dest_empty', run: act(MP_DB, 'move-folder', { fromPath: 'notes', toPath: 'n2', destMountPointId: '' }) },
    {
      // A three-member group (two links off `notes/intro.md`), one member leaves.
      name: 'unlink_grouped',
      run: seq(
        act(MP_DB, 'link-file', mv('notes/intro.md', MP_EMPTY, 'linked/a.md')),
        act(MP_DB, 'link-file', mv('notes/intro.md', MP_EMPTY, 'linked/b.md')),
        act(MP_EMPTY, 'unlink-file', { path: 'linked/a.md' }),
      ),
    },
    {
      name: 'unlink_dissolve',
      run: seq(
        act(MP_DB, 'link-file', mv('notes/intro.md', MP_EMPTY, 'linked/a.md')),
        act(MP_EMPTY, 'unlink-file', { path: 'linked/a.md' }),
      ),
    },
    { name: 'unlink_ungrouped', run: act(MP_DB, 'unlink-file', { path: 'reference.txt' }) },
    { name: 'unlink_missing', run: act(MP_DB, 'unlink-file', { path: 'nope.md' }) },
    { name: 'unlink_no_path', run: act(MP_DB, 'unlink-file', {}) },
    // ── item 11: render-html (`DOCX_RENDER_HTML_REFUSED` on the two DOCX rows) ──
    {
      name: 'render_html_docx_valid',
      run: seq(writeIn('docs/sample.docx', docxSample()), itemGet(MP_FS, ['docs', 'sample.docx'], '?action=render-html')),
    },
    {
      name: 'render_html_docx_corrupt',
      run: seq(writeIn('docs/bad.docx', 'not a zip at all'), itemGet(MP_FS, ['docs', 'bad.docx'], '?action=render-html')),
    },
    { name: 'render_html_md', run: itemGet(MP_FS, ['notes', 'alpha.md'], '?action=render-html') },
    { name: 'render_html_missing', run: itemGet(MP_FS, ['docs', 'missing.docx'], '?action=render-html') },
    { name: 'render_html_db_md', run: itemGet(MP_DB, ['notes', 'intro.md'], '?action=render-html') },
    // === /P4.D381 ===
  ];
}

describe('mount-ops oracle', () => {
  it('emits the differential rows', async () => {
    const here = dirname(fileURLToPath(new URL(`file://${__filename}`).href));
    const spec = JSON.parse(
      fs.readFileSync(join(here, '..', 'fixtures', 'mounts.json'), 'utf8'),
    ) as Spec;
    const fixtures = {
      main: process.env.QT_FIXTURE_MOUNTS_MAIN ?? '',
      mount: process.env.QT_FIXTURE_MOUNTS_MOUNT ?? '',
      tree: process.env.QT_MOUNTS_FS_TREE ?? '',
    };
    for (const [k, v] of Object.entries(fixtures)) {
      if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
    }
    const outPath = process.env.QT_ORACLE_OUT;
    if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

    const scratch = mkdtempSync(join(tmpdir(), 'qt-mount-ops-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';

    const rows: unknown[] = [];
    plantBasePaths();
    try {
      for (const c of cases()) {
        rows.push(await runCase(spec, c, scratch, fixtures));
      }
    } finally {
      unplantBasePaths();
    }
    fs.writeFileSync(outPath, rows.map((r) => JSON.stringify(r)).join('\n') + '\n');
    rmSync(scratch, { recursive: true, force: true });
  }, 300000);
});
