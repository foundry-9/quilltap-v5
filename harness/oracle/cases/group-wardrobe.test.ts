/**
 * @jest-environment node
 *
 * Tier-2 ORACLE for the GROUP WARDROBE CRUD (v4 d7263f39
 * app/api/v1/groups/[id]/wardrobe/route.ts + [itemId]/route.ts), ported to
 * quilltap-core::api::groups::group_wardrobe_{list,create,get,update,delete}.
 *
 * Drives v4's REAL route handlers (via the real createContextParamsHandler
 * middleware — the schema 400s come out of THAT layer, not the routes) against
 * a REAL fixture DB pair: the SAME baked wardrobe-transfers fixture, whose
 * group already carries a provisioned official store, a Wardrobe/ folder, and
 * the Household Livery item. Only the auth seam is stubbed and the startup
 * gate forced open; the whole DB stack is doMocked to the REAL modules plus
 * the real better-sqlite3-multiple-ciphers binding. See [[jest-real-db-oracle]].
 *
 * Per case the oracle resets to a FRESH copy of the fixture pair, calls the
 * route, then dumps the seven mount-index tables. Emits one NDJSON line per
 * case: { name, ok, status, body, tables }.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores
 * .claude/ paths, and this case reads its spec from `../fixtures/`):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<this checkout>
 *   TMPO=/tmp/qt-gw-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5/harness/oracle/cases/group-wardrobe.test.ts" "$TMPO/cases/"
 *   cp "$V5/harness/oracle/fixtures/group-wardrobe.json" "$TMPO/fixtures/"
 *   cp "$V5/harness/oracle/fixtures/wardrobe-transfers-tier2.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_GW_MAIN=/tmp/qt-gw-main.db QT_FIXTURE_GW_MOUNT=/tmp/qt-gw-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-wardrobe-transfers-fixture.ts
 *   ... (the builder reads QT_FIXTURE_WTR_*; pass those spellings — see the
 *   .rs family header for the exact recipe)
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}
function canonicalizeRows(opts: {
  table: string;
  columns: string[];
  rawRows: Array<Record<string, unknown>>;
  orderBy?: string;
}): { table: string; columns: string[]; rows: Array<Record<string, unknown>> } {
  const { table, columns, rawRows, orderBy = 'id' } = opts;
  const rows = rawRows
    .map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of columns) out[col] = canonValue(r[col]);
      return out;
    })
    .sort((a, b) => {
      const av = String(a[orderBy] ?? '');
      const bv = String(b[orderBy] ?? '');
      return av < bv ? -1 : av > bv ? 1 : 0;
    });
  return { table, columns, rows };
}

interface Case {
  name: string;
  kind: string;
  groupId?: string;
  itemId?: string;
  body?: unknown;
  normalize?: string[];
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  groupId: string;
  itemId: string;
  missingGroupId: string;
  missingItemId: string;
  cases: Case[];
}

const MOUNT_TABLES: Array<{ key: string; table: string; orderBy: string }> = [
  { key: 'points', table: 'doc_mount_points', orderBy: 'name' },
  { key: 'files', table: 'doc_mount_files', orderBy: 'sha256' },
  { key: 'documents', table: 'doc_mount_documents', orderBy: 'contentSha256' },
  { key: 'links', table: 'doc_mount_file_links', orderBy: 'relativePath' },
  { key: 'folders', table: 'doc_mount_folders', orderBy: 'path' },
  { key: 'projectLinks', table: 'project_doc_mount_links', orderBy: 'createdAt' },
  { key: 'groupLinks', table: 'group_doc_mount_links', orderBy: 'createdAt' },
];

function mockRequest(url: string, method: string, body?: unknown): unknown {
  return {
    method,
    url,
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body),
  };
}

/**
 * Run ONE case in a FRESH jest module graph (the wardrobe-transfers idiom —
 * the SQLite backend registers JSON-column mappings only on the first init of
 * a graph, so each case resets modules, re-doMocks, and re-imports).
 */
async function runCase(
  spec: Spec,
  c: Case,
  scratch: string,
  mainFixture: string,
  mountFixture: string,
): Promise<Record<string, unknown>> {
  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories'),
  );
  jest.doMock('@/lib/repositories/factory', () =>
    jest.requireActual('@/lib/repositories/factory'),
  );
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store'),
  );
  jest.doMock('@/lib/embedding/embedding-service', () =>
    jest.requireActual('@/lib/embedding/embedding-service'),
  );
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

  const work = mkdtempSync(join(scratch, 'case-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const collection = (await import('@/app/api/v1/groups/[id]/wardrobe/route')) as {
    GET: (r: unknown, ctx: unknown) => Promise<{ status: number; json(): Promise<unknown> }>;
    POST: (r: unknown, ctx: unknown) => Promise<{ status: number; json(): Promise<unknown> }>;
  };
  const detail = (await import('@/app/api/v1/groups/[id]/wardrobe/[itemId]/route')) as {
    GET: (r: unknown, ctx: unknown) => Promise<{ status: number; json(): Promise<unknown> }>;
    PUT: (r: unknown, ctx: unknown) => Promise<{ status: number; json(): Promise<unknown> }>;
    DELETE: (r: unknown, ctx: unknown) => Promise<{ status: number; json(): Promise<unknown> }>;
  };

  await initializeDatabase();

  try {
    const gid = c.groupId ?? spec.groupId;
    const iid = c.itemId ?? spec.itemId;
    const B = `http://localhost/api/v1/groups/${gid}/wardrobe`;
    const collCtx = { params: Promise.resolve({ id: gid }) };
    const itemCtx = { params: Promise.resolve({ id: gid, itemId: iid }) };

    let response: { status: number; json(): Promise<unknown> };
    switch (c.kind) {
      case 'list':
        response = await collection.GET(mockRequest(B, 'GET'), collCtx);
        break;
      case 'create':
        response = await collection.POST(mockRequest(B, 'POST', c.body), collCtx);
        break;
      case 'get':
        response = await detail.GET(mockRequest(`${B}/${iid}`, 'GET'), itemCtx);
        break;
      case 'update':
        response = await detail.PUT(mockRequest(`${B}/${iid}`, 'PUT', c.body), itemCtx);
        break;
      case 'delete':
        response = await detail.DELETE(mockRequest(`${B}/${iid}`, 'DELETE'), itemCtx);
        break;
      default:
        throw new Error(`unknown case kind ${c.kind}`);
    }

    const status: number = response.status;
    const body = await response.json();
    const ok = status >= 200 && status < 300;

    const midb = getRawMountIndexDatabase();
    if (!midb) throw new Error('mount-index DB handle unavailable for dump');
    const tables: Record<string, unknown> = {};
    for (const t of MOUNT_TABLES) {
      const columns = (
        midb.prepare(`PRAGMA table_info(${t.table})`).all() as Array<{ name: string }>
      ).map((col) => col.name);
      const rawRows = midb.prepare(`SELECT * FROM ${t.table}`).all() as Array<
        Record<string, unknown>
      >;
      tables[t.key] = canonicalizeRows({ table: t.table, columns, rawRows, orderBy: t.orderBy });
    }

    return { name: c.name, ok, status, body, tables };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'group-wardrobe.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_GW_MAIN;
  const mountFixture = process.env.QT_FIXTURE_GW_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_GW_MAIN and QT_FIXTURE_GW_MOUNT must point at the seed fixtures (built by build-wardrobe-transfers-fixture.ts)',
    );
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-gw-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const outLines: string[] = [];
  for (const c of spec.cases) {
    const payload = await runCase(spec, c, scratch, mainFixture, mountFixture);
    outLines.push(JSON.stringify(payload));
  }

  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`group-wardrobe oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('group-wardrobe tier-2 oracle', async () => {
  await main();
});
