/**
 * P4.6v mount READ + LIST ORACLE: drives v4's REAL `readMountFile`
 * (`lib/mount-index/read-file.ts`) and the files-list route body
 * (`docMountFiles.findByMountPointId` + `docMountFolders.findByMountPointId` +
 * `listFilesystemFolders`) over a per-side COPY of the committed mounts fixture,
 * emitting `{status|err, body}` so the Rust services diff byte-for-byte.
 *
 * The fs/obsidian mounts store the sentinel basePath `__MOUNTS_FS_TREE__`; this
 * oracle copies the committed `mounts-fs-tree/` to a temp dir and rewrites those
 * mounts' basePath to it before reading (the Rust test does the same). fs-mount
 * mtimes are nondeterministic (copy time) → the read result's `mtime` is
 * normalized to 0 for filesystem/obsidian reads on both sides.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MOUNTS_MAIN=$W/crates/quilltap-web/tests/fixtures/mounts-main.db \
 *   QT_FIXTURE_MOUNTS_MOUNT=$W/crates/quilltap-web/tests/fixtures/mounts-mount.db \
 *   QT_MOUNTS_FS_TREE=$W/crates/quilltap-web/tests/fixtures/mounts-fs-tree \
 *     $N/node --import tsx $W/harness/oracle/cases/mount-read.ts > /tmp/oracle-mount-read.ndjson
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, cpSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

const MP_DB = 'b6000000-0000-4000-8000-000000000001';
const MP_EMPTY = 'b6000000-0000-4000-8000-000000000002';
const MP_FS = 'b6000000-0000-4000-8000-000000000003';
const MP_OBS = 'b6000000-0000-4000-8000-000000000004';
const BOGUS = 'b6000000-0000-4000-8000-0000000000ee';

type ReadOpts = { encoding?: 'utf-8' | 'base64'; offset?: number; limit?: number };
type Row =
  | { kind: 'read'; id: string; fsNorm: boolean; out: unknown }
  | { kind: 'list'; id: string; out: unknown };

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'mounts.json'), 'utf8')) as Spec;

  const fxMain = process.env.QT_FIXTURE_MOUNTS_MAIN ?? '';
  const fxMount = process.env.QT_FIXTURE_MOUNTS_MOUNT ?? '';
  const fsTree = process.env.QT_MOUNTS_FS_TREE ?? '';
  for (const [k, v] of Object.entries({ fxMain, fxMount, fsTree })) {
    if (!v || !existsSync(v)) throw new Error(`missing ${k}: ${v}`);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-mount-read-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'main.db');
  const mountWork = join(scratch, 'mount.db');
  copyFileSync(fxMain, mainWork);
  copyFileSync(fxMount, mountWork);
  const treeWork = join(scratch, 'fs-tree');
  cpSync(fsTree, treeWork, { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();

  // Rewrite the sentinel basePath to this side's fs-tree copy.
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB unavailable');
  midb
    .prepare('UPDATE doc_mount_points SET basePath = ? WHERE id IN (?, ?)')
    .run(treeWork, MP_FS, MP_OBS);

  const { readMountFile } = await import('@/lib/mount-index/read-file');
  const { listFilesystemFolders } = await import('@/lib/mount-index/scanner');
  const repos = getRepositories();

  const rows: Row[] = [];

  const readCase = async (id: string, mp: string, path: string, opts: ReadOpts, fsNorm: boolean) => {
    try {
      const r = await readMountFile(mp, path, opts as never);
      const body: Record<string, unknown> = { ...r };
      if (fsNorm) body.mtime = 0;
      rows.push({ kind: 'read', id, fsNorm, out: { ok: body } });
    } catch (e) {
      const code = (e as { code?: string })?.code ?? String(e);
      rows.push({ kind: 'read', id, fsNorm, out: { err: code } });
    }
  };

  const listCase = async (id: string, mp: string) => {
    const mountPoint = await repos.docMountPoints.findById(mp);
    if (!mountPoint) {
      rows.push({ kind: 'list', id, out: { err: 'MOUNT_NOT_FOUND' } });
      return;
    }
    const [files, folderRows] = await Promise.all([
      repos.docMountFiles.findByMountPointId(mp),
      repos.docMountFolders.findByMountPointId(mp),
    ]);
    const folderSet = new Set<string>();
    for (const row of folderRows) {
      if (typeof row.path === 'string' && row.path.length > 0) folderSet.add(row.path);
    }
    if (mountPoint.mountType !== 'database' && mountPoint.basePath) {
      const fsFolders = await listFilesystemFolders(mountPoint.basePath, mountPoint.excludePatterns ?? []);
      for (const f of fsFolders) folderSet.add(f);
    }
    rows.push({ kind: 'list', id, out: { files, folders: Array.from(folderSet) } });
  };

  // ── read cases ──
  await readCase('db-doc-read', MP_DB, 'notes/intro.md', {}, false);
  await readCase('db-doc-window', MP_DB, 'notes/intro.md', { offset: 1, limit: 2 }, false);
  await readCase('db-doc-window-over', MP_DB, 'notes/intro.md', { offset: 0, limit: 999 }, false);
  await readCase('db-txt-read', MP_DB, 'reference.txt', {}, false);
  await readCase('db-blob-read', MP_DB, 'images/logo.png', {}, false);
  await readCase('db-blob-force-utf8', MP_DB, 'images/logo.png', { encoding: 'utf-8' }, false);
  await readCase('fs-md-read', MP_FS, 'notes/alpha.md', {}, true);
  await readCase('fs-window', MP_FS, 'notes/sub/deep.md', { offset: 2, limit: 2 }, true);
  await readCase('fs-json-read', MP_FS, 'data/config.json', {}, true);
  await readCase('fs-force-base64', MP_FS, 'notes/beta.txt', { encoding: 'base64' }, true);
  await readCase('obs-read', MP_OBS, 'notes/alpha.md', {}, true);
  await readCase('fs-notfound', MP_FS, 'missing.md', {}, true);
  await readCase('db-notfound', MP_DB, 'nope.md', {}, false);
  await readCase('mount-notfound', BOGUS, 'x.md', {}, false);

  // ── list cases ──
  await listCase('list-db', MP_DB);
  await listCase('list-empty', MP_EMPTY);
  await listCase('list-fs', MP_FS);
  await listCase('list-obs', MP_OBS);

  closeMountIndexSQLiteClient();
  await closeDatabase();
  for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`mount-read oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
