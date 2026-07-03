/**
 * @jest-environment node
 *
 * Tier-2 ORACLE for the wardrobe TRANSFERS service (v4
 * app/api/v1/wardrobe/transfers/route.ts POST), ported to
 * quilltap-core::services::wardrobe_transfers::transfer_wardrobe_item.
 *
 * Drives v4's REAL POST handler (the actual route module, via the real
 * createAuthenticatedHandler middleware) against a REAL fixture DB. Only the auth
 * seam is stubbed: getServerSession -> { user: { id: userId } }, and the startup
 * gate is forced ready/pepper-resolved. The whole DB stack is doMocked to the REAL
 * modules (past jest.setup's global mocks) plus the real
 * better-sqlite3-multiple-ciphers cipher binding, so the store provisioning + the
 * vault read/write overlay all run genuinely. See [[jest-real-db-oracle]].
 *
 * Per scenario the oracle resets to a FRESH copy of the baked fixture (so
 * scenarios are independent), calls POST, then dumps the mount-index tables. Emits
 * one NDJSON line per scenario: { name, ok, status, body, tables }.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WTR_MAIN=/tmp/qt-wtr-main.db QT_FIXTURE_WTR_MOUNT=/tmp/qt-wtr-mount.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-wardrobe-transfers-fixture.ts
 *   QT_FIXTURE_WTR_MAIN=/tmp/qt-wtr-main.db QT_FIXTURE_WTR_MOUNT=/tmp/qt-wtr-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-wtr.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- wardrobe-transfers
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

// Inlined canonicalizer (jest can't resolve the `.js` ESM specifier of
// ../lib/tier2.ts). BLOBs -> lowercase hex, nulls explicit, everything else
// as-is, rows sorted by `orderBy` (code-unit string order on the canonicalized
// cell so a BLOB sorts by hex == the Rust dump's BLOB memcmp order).
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

interface Destination {
  scope: string;
  id?: string;
}
interface Scenario {
  name: string;
  action: string;
  itemId: string;
  sourceCharacterId: string;
  sourceProjectId?: string | null;
  destination: Destination;
  expect: string;
  expectMessage?: string;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  scenarios: Scenario[];
}

// The mount-index tables to dump after each scenario, with the sort key the Rust
// harness uses. doc_mount_chunks is EXCLUDED (v4-only reindex artifact).
const MOUNT_TABLES: Array<{ key: string; table: string; orderBy: string }> = [
  { key: 'points', table: 'doc_mount_points', orderBy: 'name' },
  { key: 'files', table: 'doc_mount_files', orderBy: 'sha256' },
  { key: 'documents', table: 'doc_mount_documents', orderBy: 'contentSha256' },
  { key: 'links', table: 'doc_mount_file_links', orderBy: 'relativePath' },
  { key: 'folders', table: 'doc_mount_folders', orderBy: 'path' },
  { key: 'projectLinks', table: 'project_doc_mount_links', orderBy: 'createdAt' },
  { key: 'groupLinks', table: 'group_doc_mount_links', orderBy: 'createdAt' },
];

function createMockRequest(body: unknown): unknown {
  return {
    method: 'POST',
    url: 'http://localhost/api/v1/wardrobe/transfers',
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body),
  };
}

/**
 * Run ONE scenario in a FRESH jest module graph. The SQLite backend registers a
 * collection's JSON-column type mapping only on the FIRST init of a module graph;
 * re-initing within the same graph loses it (JSON columns then read back as raw
 * strings and Zod-parse to null). So each scenario resetModules() + re-doMocks +
 * re-imports the whole chain, guaranteeing a first-init graph and true scenario
 * independence (plus a fresh fixture copy). Returns the NDJSON payload object.
 */
async function runScenario(
  spec: Spec,
  scenario: Scenario,
  scratch: string,
  mainFixture: string,
  mountFixture: string,
): Promise<Record<string, unknown>> {
  // Restore the REAL DB stack (past jest.setup's global mocks) + the real cipher
  // driver, stub the auth seam, and force the startup gate open. doMock is runtime
  // (not hoisted), so it runs now and wins over jest.setup.
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

  // Fresh copy of BOTH fixture DBs -> independence from other scenarios.
  const work = mkdtempSync(join(scratch, 'sc-'));
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
  const { POST } = await import('@/app/api/v1/wardrobe/transfers/route');

  await initializeDatabase();

  try {
    const req = createMockRequest({
      action: scenario.action,
      itemId: scenario.itemId,
      sourceCharacterId: scenario.sourceCharacterId,
      sourceProjectId: scenario.sourceProjectId ?? null,
      destination: scenario.destination,
    });
    const response = await POST(req as never);
    const status: number = response.status;
    const body = await response.json();
    const ok = status >= 200 && status < 300;

    if (scenario.expect === 'ok' && !ok) {
      throw new Error(
        `scenario ${scenario.name}: expected ok, got status ${status}: ${JSON.stringify(body)}`,
      );
    }
    if (scenario.expect === 'error') {
      if (ok) {
        throw new Error(
          `scenario ${scenario.name}: expected an error, got ok ${JSON.stringify(body)}`,
        );
      }
      if (scenario.expectMessage && body?.error !== scenario.expectMessage) {
        throw new Error(
          `scenario ${scenario.name}: expected error message "${scenario.expectMessage}", got "${body?.error}"`,
        );
      }
    }

    // Dump the mount-index tables (raw handle).
    const midb = getRawMountIndexDatabase();
    if (!midb) throw new Error('mount-index DB handle unavailable for dump');
    const tables: Record<string, unknown> = {};
    for (const t of MOUNT_TABLES) {
      const columns = (
        midb.prepare(`PRAGMA table_info(${t.table})`).all() as Array<{ name: string }>
      ).map((c) => c.name);
      const rawRows = midb.prepare(`SELECT * FROM ${t.table}`).all() as Array<
        Record<string, unknown>
      >;
      tables[t.key] = canonicalizeRows({ table: t.table, columns, rawRows, orderBy: t.orderBy });
    }

    return { name: scenario.name, ok, status, body, tables };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'wardrobe-transfers-tier2.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_WTR_MAIN;
  const mountFixture = process.env.QT_FIXTURE_WTR_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_WTR_MAIN and QT_FIXTURE_WTR_MOUNT must point at the seed fixtures from build-wardrobe-transfers-fixture.ts',
    );
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wtr-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const outLines: string[] = [];
  for (const scenario of spec.scenarios) {
    const payload = await runScenario(spec, scenario, scratch, mainFixture, mountFixture);
    outLines.push(JSON.stringify(payload));
  }

  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`wardrobe-transfers oracle wrote ${outPath} (${outLines.length} scenarios)\n`);
}

test('wardrobe-transfers tier-2 oracle', async () => {
  await main();
});
