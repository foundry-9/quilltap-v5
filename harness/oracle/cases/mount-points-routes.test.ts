/**
 * @jest-environment node
 *
 * P4.6p GLOBAL MOUNT-POINTS route-surface ORACLE: drives v4's REAL mount-point
 * route handlers over a FRESH copy of the committed groups-projects fixture per
 * case, emitting `{status, body(, tables)}` so `api::mount_points::*` can be diffed
 * byte-for-byte. The fs seams (`checkBasePathAvailability` / watcher
 * attach/refresh/detach) are the real functions.
 *
 * [ed8934f1 / Bug 56] The store-create warning is now the base-path-availability
 * DIAGNOSIS, so the create differential drives all four reachability states over
 * PLANTED conditions under a fixed root (`PLANT_ROOT` below). The paths are
 * literal and identical on both sides — that is what lets the sentences, which
 * embed the path, be compared byte-for-byte with no normalization. The root is
 * per-family (`qt-bug56-mp`, not the mount-ops family's `qt-bug56-ops`) because
 * the two Rust differentials are separate test binaries and cargo runs them
 * concurrently.
 *
 * `denied` is planted as an unreadable PARENT, not a `chmod 000` on the base
 * itself: `fs.stat` takes its EACCES from the parent directory's search
 * permission, and a 000 directory still stats fine and reads as available.
 *
 * ⚠ `containerized` is false in BOTH environments (no `/.dockerenv`, no
 * `/app`), so the container-variant sentences are pinned by UNIT tests on the
 * message builder (`base_path_availability.rs`), not here. (v4 `1560bd43b`
 * dropped `isLimaEnvironment()` from `isContainerized()`; the probe is now the
 * Docker one alone.)
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-mount-points-routes-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp $V5W/harness/oracle/cases/mount-points-routes.test.ts "$TMPO/cases/"
 *   cp $V5W/harness/oracle/fixtures/groups-projects.json     "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_GP_MAIN=$V5W/crates/quilltap-web/tests/fixtures/groups-projects-main.db \
 *   QT_FIXTURE_GP_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/groups-projects-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-mount-points-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- mount-points-routes
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

// [ed8934f1] The planted base-path conditions. Fixed literal paths, identical
// on both sides of the diff, so the diagnosis sentences (which embed the path)
// compare byte-for-byte.
const PLANT_ROOT = '/tmp/qt-bug56-mp';
const PLANT_AVAILABLE = `${PLANT_ROOT}/available`;
const PLANT_NOT_A_DIRECTORY = `${PLANT_ROOT}/notdir`;
const PLANT_DENIED_PARENT = `${PLANT_ROOT}/denied`;
const PLANT_DENIED = `${PLANT_DENIED_PARENT}/inner`;
const PLANT_MISSING = `${PLANT_ROOT}/gone`;

function plantBasePaths(): void {
  unplantBasePaths();
  mkdirSync(PLANT_AVAILABLE, { recursive: true });
  mkdirSync(PLANT_DENIED, { recursive: true });
  fs.writeFileSync(PLANT_NOT_A_DIRECTORY, 'not a directory');
  // Make the PARENT unreadable — that is where stat()'s EACCES comes from.
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

const GAMMA_EXTRA_MP = 'b0000000-0000-4000-8000-000000000001'; // database/documents, linked to Gamma
const MP_INDEXED = 'b0000000-0000-4000-8000-0000000000c0'; // has an embedded chunk + a file
const BOGUS = 'b0000000-0000-4000-8000-0000000000ee';

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
  // The watcher pulls chokidar (ESM); the routes use it only fire-and-forget, so
  // stub the three seams to no-ops rather than loading chokidar under jest CJS.
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

async function mountDb(): Promise<{ prepare: (s: string) => { all: (...a: unknown[]) => unknown; get: (...a: unknown[]) => unknown } }> {
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  return getRawMountIndexDatabase() as never;
}

async function dumpFolders(mountId: string): Promise<unknown> {
  const db = await mountDb();
  return { folders: db.prepare('SELECT path FROM doc_mount_folders WHERE mountPointId = ? ORDER BY path').all(mountId) };
}

async function dumpCascade(mountId: string): Promise<unknown> {
  const db = await mountDb();
  const count = (sql: string) => (db.prepare(sql).get(mountId) as { c: number }).c;
  return {
    chunks: count('SELECT COUNT(*) c FROM doc_mount_chunks WHERE mountPointId = ?'),
    files: count('SELECT COUNT(*) c FROM doc_mount_files f WHERE EXISTS (SELECT 1 FROM doc_mount_file_links l WHERE l.fileId = f.id AND l.mountPointId = ?)'),
    fileLinks: count('SELECT COUNT(*) c FROM doc_mount_file_links WHERE mountPointId = ?'),
    documents: count('SELECT COUNT(*) c FROM doc_mount_documents WHERE fileId IN (SELECT fileId FROM doc_mount_file_links WHERE mountPointId = ?)'),
    folders: count('SELECT COUNT(*) c FROM doc_mount_folders WHERE mountPointId = ?'),
    projectLinks: count('SELECT COUNT(*) c FROM project_doc_mount_links WHERE mountPointId = ?'),
    mountPointExists: count('SELECT COUNT(*) c FROM doc_mount_points WHERE id = ?') > 0,
  };
}

interface CaseSpec {
  name: string;
  run: () => Promise<{ status: number; body: unknown; tables?: unknown }>;
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

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'mp-'));
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
    return {
      name: c.name,
      status: out.status,
      body: out.body,
      ...(out.tables !== undefined ? { tables: out.tables } : {}),
    };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

const B = 'http://localhost/api/v1/mount-points';
const coll = () => loadRoute('@/app/api/v1/mount-points/route');
const idRoute = () => loadRoute('@/app/api/v1/mount-points/[id]/route');
const params = (id: string) => ({ params: Promise.resolve({ id }) });
const createdMountId = (body: unknown) =>
  (body as { mountPoint?: { id?: string } })?.mountPoint?.id ?? '';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'groups-projects.json'), 'utf8'),
  ) as Spec;
  const fixtures = {
    main: process.env.QT_FIXTURE_GP_MAIN ?? '',
    mount: process.env.QT_FIXTURE_GP_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-mp-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cases: CaseSpec[] = [
    { name: 'list', run: async () => respond(await (await coll()).GET(mockRequest(B))) },
    {
      name: 'get',
      run: async () =>
        respond(await (await idRoute()).GET(mockRequest(`${B}/${MP_INDEXED}`), params(MP_INDEXED))),
    },
    {
      name: 'get_404',
      run: async () =>
        respond(await (await idRoute()).GET(mockRequest(`${B}/${BOGUS}`), params(BOGUS))),
    },
    {
      name: 'create_database',
      run: async () =>
        respond(
          await (await coll()).POST(
            mockRequest(B, { name: 'DB Store', mountType: 'database', storeType: 'documents' }),
          ),
        ),
    },
    // [0a0419f5] Store names are one case-insensitive namespace — create a store
    // named 'gamma extra store' clashes with the existing 'Gamma Extra Store'.
    {
      name: 'create_name_clash',
      run: async () =>
        respond(
          await (await coll()).POST(
            mockRequest(B, {
              name: 'gamma extra store',
              mountType: 'database',
              storeType: 'documents',
            }),
          ),
        ),
    },
    // [0a0419f5] Renaming 'Indexed Store' to a case-variant of another store's
    // name (GAMMA EXTRA STORE) is a 409 (the clash excludes the store itself).
    {
      name: 'patch_name_clash',
      run: async () =>
        respond(
          await (await idRoute()).PATCH(
            mockRequest(`${B}/${MP_INDEXED}`, { name: 'GAMMA EXTRA STORE' }),
            params(MP_INDEXED),
          ),
        ),
    },
    // [0a0419f5] Renaming a store to a case-variant of ITS OWN name is allowed —
    // the clash check excludes self, so 'Gamma Extra Store' → 'GAMMA EXTRA STORE'.
    {
      name: 'patch_name_case_only_self',
      run: async () =>
        respond(
          await (await idRoute()).PATCH(
            mockRequest(`${B}/${GAMMA_EXTRA_MP}`, { name: 'GAMMA EXTRA STORE' }),
            params(GAMMA_EXTRA_MP),
          ),
        ),
    },
    {
      name: 'create_filesystem_warning',
      run: async () =>
        respond(
          await (await coll()).POST(
            mockRequest(B, { name: 'FS Store', mountType: 'filesystem', basePath: PLANT_MISSING }),
          ),
        ),
    },
    // [ed8934f1] The three other reachability states. `available` is the arm
    // that used to be unreachable in v5 (its warning seam was blanket-false):
    // a store on a real directory now creates with NO warning key at all.
    {
      name: 'create_filesystem_available',
      run: async () =>
        respond(
          await (await coll()).POST(
            mockRequest(B, {
              name: 'FS Store Reachable',
              mountType: 'filesystem',
              basePath: PLANT_AVAILABLE,
            }),
          ),
        ),
    },
    {
      name: 'create_filesystem_not_a_directory',
      run: async () =>
        respond(
          await (await coll()).POST(
            mockRequest(B, {
              name: 'FS Store On A File',
              mountType: 'filesystem',
              basePath: PLANT_NOT_A_DIRECTORY,
            }),
          ),
        ),
    },
    {
      name: 'create_filesystem_denied',
      run: async () =>
        respond(
          await (await coll()).POST(
            mockRequest(B, {
              name: 'FS Store Unreadable',
              mountType: 'obsidian',
              basePath: PLANT_DENIED,
            }),
          ),
        ),
    },
    {
      name: 'create_character_scaffold',
      run: async () => {
        const r = await (await coll()).POST(
          mockRequest(B, { name: 'Char Store', mountType: 'database', storeType: 'character' }),
        );
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpFolders(createdMountId(body)) };
      },
    },
    {
      name: 'create_refine_400',
      run: async () =>
        respond(
          await (await coll()).POST(
            mockRequest(B, { name: 'Bad', mountType: 'filesystem', basePath: '' }),
          ),
        ),
    },
    {
      name: 'patch_happy',
      run: async () =>
        respond(
          await (await idRoute()).PATCH(
            mockRequest(`${B}/${GAMMA_EXTRA_MP}`, { name: 'Gamma Store Renamed', enabled: false }),
            params(GAMMA_EXTRA_MP),
          ),
        ),
    },
    {
      name: 'patch_zod_500',
      run: async () =>
        respond(
          await (await idRoute()).PATCH(
            mockRequest(`${B}/${GAMMA_EXTRA_MP}`, { mountType: 'bogus' }),
            params(GAMMA_EXTRA_MP),
          ),
        ),
    },
    {
      name: 'patch_scaffold_flip',
      run: async () => {
        const r = await (await idRoute()).PATCH(
          mockRequest(`${B}/${GAMMA_EXTRA_MP}`, { storeType: 'character' }),
          params(GAMMA_EXTRA_MP),
        );
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpFolders(GAMMA_EXTRA_MP) };
      },
    },
    {
      name: 'patch_404',
      run: async () =>
        respond(
          await (await idRoute()).PATCH(mockRequest(`${B}/${BOGUS}`, { name: 'x' }), params(BOGUS)),
        ),
    },
    {
      name: 'delete',
      run: async () => {
        const r = await (await idRoute()).DELETE(mockRequest(`${B}/${MP_INDEXED}`), params(MP_INDEXED));
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpCascade(MP_INDEXED) };
      },
    },
    {
      name: 'delete_404',
      run: async () =>
        respond(await (await idRoute()).DELETE(mockRequest(`${B}/${BOGUS}`), params(BOGUS))),
    },
  ];

  const outLines: string[] = [];
  plantBasePaths();
  try {
    for (const c of cases) {
      const payload = await runCase(spec, c, scratch, fixtures);
      outLines.push(JSON.stringify(payload));
    }
  } finally {
    unplantBasePaths();
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`mount-points oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('mount-points-routes oracle', async () => {
  await main();
});
