/**
 * @jest-environment node
 *
 * P4.31 STORE-DELETE cascade ORACLE — drives v4's REAL
 * `DELETE /api/v1/mount-points/[id]` handler over a FRESH copy of the committed
 * `store-delete-*` fixture per case, and dumps a WHOLE-TABLE census of the mount
 * partition afterwards.
 *
 * Why a whole-table census and not the scoped counts the P4.6p route family
 * uses: every scoped count keys off the deleted store's own link rows, which the
 * cascade has just emptied, so it reads 0 on both sides no matter how much each
 * app leaked. The rows this lane is about — a document whose file is gone, a
 * group link whose store is gone — are invisible to any query scoped that way.
 * The census counts, and dumps, the entire partition.
 *
 * v4's leaks, reproduced here rather than argued (see the Rust side's
 * `V4_LEAKS`): `doc_mount_documents` is deleted through a subquery over the link
 * table AFTER the links have gone, so it deletes nothing; and
 * `group_doc_mount_links` is never touched by the delete at all. v5 diverges on
 * both, deliberately, under the standing 2026-08-03 backup/restore ruling.
 *
 * `reap_orphans` is the reaper arm: the fixture ships with planted orphans (a
 * store deleted without its children — see the builder), v5 reaps them at boot,
 * and v4 has no such pass, so this case's census is simply the pristine one.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-store-delete-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp $V5W/harness/oracle/cases/store-delete.test.ts "$TMPO/cases/"
 *   cp $V5W/harness/oracle/fixtures/store-delete.json "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/store-delete-main.db \
 *   QT_FIXTURE_SD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/store-delete-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-store-delete.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- store-delete
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

const MP_TARGET = 'b9000000-0000-4000-8000-000000000001';
const MP_KEEPER = 'b9000000-0000-4000-8000-000000000002';
/** The planted orphans' vanished parent — no `doc_mount_points` row. */
const MP_GHOST = 'b9000000-0000-4000-8000-0000000000ee';
/** Never existed at all — the 404 arm. */
const MP_BOGUS = 'b9000000-0000-4000-8000-0000000000ff';

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'GET',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
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
  // The watcher pulls chokidar (ESM); the delete route only fires it
  // fire-and-forget, so stub the seams rather than load chokidar under jest CJS.
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

type Raw = {
  prepare: (s: string) => { all: (...a: unknown[]) => unknown[]; get: (...a: unknown[]) => unknown };
};

async function mountDb(): Promise<Raw> {
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  return getRawMountIndexDatabase() as never;
}

/**
 * The columns dumped per table. Deliberately excludes `createdAt`/`updatedAt`
 * (no delete path rewrites them, so they carry no signal) and the `data` /
 * `embedding` BLOBs (compared by their sha/size columns and content instead).
 */
const TABLES: Array<[string, string]> = [
  ['doc_mount_points', 'id, name, mountType, storeType, enabled, fileCount, chunkCount, totalSizeBytes'],
  ['doc_mount_files', 'id, sha256, fileSizeBytes, fileType, source'],
  [
    'doc_mount_file_links',
    'id, fileId, linkGroupId, mountPointId, relativePath, fileName, folderId, chunkCount',
  ],
  ['doc_mount_documents', 'id, fileId, contentSha256, plainTextLength'],
  ['doc_mount_blobs', 'id, fileId, sha256, sizeBytes, storedMimeType'],
  ['doc_mount_folders', 'id, mountPointId, parentId, name, path'],
  ['doc_mount_chunks', 'id, linkId, mountPointId, chunkIndex, content, tokenCount'],
  ['group_doc_mount_links', 'id, groupId, mountPointId'],
  ['project_doc_mount_links', 'id, projectId, mountPointId'],
];

/** Tables carrying a `mountPointId` that must resolve to a live store. */
const ORPHANABLE = ['doc_mount_file_links', 'doc_mount_folders', 'doc_mount_chunks'];

async function census(): Promise<unknown> {
  const db = await mountDb();
  const counts: Record<string, number> = {};
  const rows: Record<string, unknown[]> = {};
  for (const [table, cols] of TABLES) {
    counts[table] = (db.prepare(`SELECT COUNT(*) AS c FROM "${table}"`).get() as { c: number }).c;
    rows[table] = db.prepare(`SELECT ${cols} FROM "${table}" ORDER BY id`).all();
  }
  const orphans: Record<string, number> = {};
  for (const table of ORPHANABLE) {
    orphans[table] = (
      db
        .prepare(
          `SELECT COUNT(*) AS c FROM "${table}" t
             WHERE NOT EXISTS (SELECT 1 FROM doc_mount_points p WHERE p.id = t.mountPointId)`,
        )
        .get() as { c: number }
    ).c;
  }
  return { counts, orphans, rows };
}

interface CaseSpec {
  name: string;
  run: () => Promise<{ status: number; body: unknown }>;
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'sd-'));
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
    return { name: c.name, status: out.status, body: out.body, census: await census() };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

const B = 'http://localhost/api/v1/mount-points';

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

async function del(id: string): Promise<{ status: number; body: unknown }> {
  const route = (await import('@/app/api/v1/mount-points/[id]/route')) as unknown as {
    DELETE: (r: unknown, ctx: unknown) => Promise<unknown>;
  };
  return respond(await route.DELETE(mockRequest(`${B}/${id}`), { params: Promise.resolve({ id }) }));
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'store-delete.json'), 'utf8'),
  ) as Spec;
  const fixtures = {
    main: process.env.QT_FIXTURE_SD_MAIN ?? '',
    mount: process.env.QT_FIXTURE_SD_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-store-delete-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cases: CaseSpec[] = [
    // The untouched fixture, so both sides agree on the starting state — and so
    // the planted orphans are proven present before anything reaps them.
    { name: 'pristine', run: async () => ({ status: 200, body: null }) },
    // The delete this lane is about: a store carrying documents, a blob, a
    // cross-store hard link, folders, chunks, a group link and a project link.
    { name: 'delete_target', run: async () => del(MP_TARGET) },
    // The other end of the shared file: deleting the KEEPER must collect its own
    // readme and spare the appendix, which the target still links.
    { name: 'delete_keeper', run: async () => del(MP_KEEPER) },
    // Deleting the same store twice — the second is a 404 and writes nothing.
    {
      name: 'delete_twice',
      run: async () => {
        await del(MP_TARGET);
        return del(MP_TARGET);
      },
    },
    { name: 'delete_404', run: async () => del(MP_BOGUS) },
    // The vanished parent of the planted orphans is itself a 404 — nothing in
    // either app can reach those rows through the route.
    { name: 'delete_ghost_404', run: async () => del(MP_GHOST) },
    // The reaper arm. v4 has no boot repair pass, so its census is pristine;
    // v5's reaps the planted orphans to zero.
    { name: 'reap_orphans', run: async () => ({ status: 200, body: null }) },
  ];

  const out: string[] = [];
  for (const c of cases) {
    out.push(JSON.stringify(await runCase(spec, c, scratch, fixtures)));
    process.stderr.write(`  case ${c.name} ok\n`);
  }
  fs.writeFileSync(outPath, out.join('\n') + '\n', 'utf8');
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(`wrote ${out.length} store-delete oracle cases to ${outPath}\n`);
}

it('emits the store-delete cascade oracle', async () => {
  await main();
}, 120000);
