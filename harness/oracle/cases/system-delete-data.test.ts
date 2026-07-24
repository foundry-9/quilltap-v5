/**
 * @jest-environment node
 *
 * P4.9G3 delete-all-data ORACLE (tier-2 DB-state): drives v4's REAL
 * `GET/POST /api/v1/system/tools?action=delete-data-preview|delete-data` route
 * handlers (which compose the REAL `lib/backup/restore/delete-service.ts`) over a
 * FRESH copy of the committed `system-data-*` fixture family per case, and emits
 *
 *   { name, status, body, counts }
 *
 * where `counts` is a row-count map over EVERY table in all three partitions
 * (`main.<table>` / `mount.<table>` / `llm.<table>`, enumerated from
 * `sqlite_master`) taken AFTER the case ran. The full map — not just the tables
 * the wipe is expected to touch — is what catches a repo whose v5 delete cascades
 * differently, and it pins the tables the wipe must NOT touch
 * (`instance_settings` above all).
 *
 * Cases: pristine (no op — the preview baseline), preview (must equal pristine),
 * the full delete, the delete run twice (idempotence), and the wrong-sentinel
 * refusal (400, no writes).
 *
 * The session + startup-state mocks mirror `system-jobs-routes.test.ts`; the
 * background-job processor is pinned/no-oped so nothing starts a child.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-sysdel-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/system-delete-data.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/system-data.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/system-data-main.db \
 *   QT_FIXTURE_SD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/system-data-mount.db \
 *   QT_FIXTURE_SD_LLM=$V5W/crates/quilltap-web/tests/fixtures/system-data-llmlogs.db \
 *   QT_ORACLE_OUT=/tmp/oracle-system-delete-data.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- system-delete-data
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

const PINNED_STATUS = {
  running: false,
  processing: false,
  inFlight: 0,
  childCrashed: false,
};

function mockRequest(url: string, method: string, body: unknown): unknown {
  return {
    method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

function applyMocks(userId: string): void {
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/background-jobs/processor', () => ({
    __esModule: true,
    getProcessorStatus: () => ({ ...PINNED_STATUS }),
    startProcessor: () => {},
    stopProcessor: () => {},
    wakeProcessor: () => {},
    ensureProcessorRunning: () => {},
  }));
  jest.doMock('@/lib/background-jobs', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/background-jobs'),
    ensureProcessorRunning: () => {},
    getProcessorStatus: () => ({ ...PINNED_STATUS }),
  }));
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: userId } }),
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

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  if (resp.status === 204) return { status: 204, body: null };
  return { status: resp.status, body: await resp.json() };
}

const TOOLS_ROUTE = '@/app/api/v1/system/tools/route';
const B = 'http://localhost/api/v1';

async function toolsGet(action: string): Promise<{ status: number; body: unknown }> {
  const route = (await import(TOOLS_ROUTE)) as never as Record<
    string,
    (...a: unknown[]) => Promise<unknown>
  >;
  return respond(await route.GET(mockRequest(`${B}/system/tools?action=${action}`, 'GET', {})));
}

async function toolsPost(
  action: string,
  body: unknown,
): Promise<{ status: number; body: unknown }> {
  const route = (await import(TOOLS_ROUTE)) as never as Record<
    string,
    (...a: unknown[]) => Promise<unknown>
  >;
  return respond(await route.POST(mockRequest(`${B}/system/tools?action=${action}`, 'POST', body)));
}

/** Row counts for every table in one partition handle, prefixed `<tag>.<table>`. */
function countPartition(
  db: { prepare: (sql: string) => { all: () => unknown[]; get: () => unknown } } | null,
  tag: string,
  out: Record<string, number>,
): void {
  if (!db) return;
  const tables = db
    .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
    .all() as Array<{ name: string }>;
  for (const { name } of tables) {
    const row = db.prepare(`SELECT COUNT(*) AS n FROM "${name}"`).get() as { n: number };
    out[`${tag}.${name}`] = row.n;
  }
}

async function countAll(): Promise<Record<string, number>> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawLLMLogsDatabase } = await import('@/lib/database/backends/sqlite/llm-logs-client');
  const out: Record<string, number> = {};
  countPartition(getRawDatabase() as never, 'main', out);
  countPartition(getRawMountIndexDatabase() as never, 'mount', out);
  countPartition(getRawLLMLogsDatabase() as never, 'llm', out);
  return out;
}

interface CaseSpec {
  name: string;
  run: () => Promise<{ status: number; body: unknown }>;
}

/** A no-op "case" so the pristine fixture's counts are on record. */
const NOOP = async () => ({ status: 0, body: null });

function buildCases(): CaseSpec[] {
  return [
    { name: 'pristine_counts', run: NOOP },
    { name: 'delete_data_preview', run: () => toolsGet('delete-data-preview') },
    {
      name: 'delete_data_wrong_confirm',
      run: () => toolsPost('delete-data', { confirm: 'nope' }),
    },
    {
      name: 'delete_data',
      run: () => toolsPost('delete-data', { confirm: 'DELETE_ALL_MY_DATA' }),
    },
    {
      name: 'delete_data_twice',
      run: async () => {
        await toolsPost('delete-data', { confirm: 'DELETE_ALL_MY_DATA' });
        return toolsPost('delete-data', { confirm: 'DELETE_ALL_MY_DATA' });
      },
    },
    {
      // `instance_settings` is the table the wipe must NOT touch. The committed
      // fixture deliberately carries no row there (the concurrency default), so
      // the case WRITES one first (through the concurrency verb) and then wipes:
      // a surviving row is the assertion, not a coincidental 0 == 0.
      name: 'delete_data_keeps_instance_settings',
      run: async () => {
        await toolsPost('job-concurrency', { concurrency: 7 });
        return toolsPost('delete-data', { confirm: 'DELETE_ALL_MY_DATA' });
      },
    },
    {
      // The preview after a full wipe: every count zero, and it still writes
      // nothing.
      name: 'delete_data_preview_after_wipe',
      run: async () => {
        await toolsPost('delete-data', { confirm: 'DELETE_ALL_MY_DATA' });
        return toolsGet('delete-data-preview');
      },
    },
  ];
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string; llm: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec.userId);

  const work = mkdtempSync(join(scratch, 'sd-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  const llmWork = join(work, 'llm.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  copyFileSync(fixtures.llm, llmWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.SQLITE_LLM_LOGS_PATH = llmWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  await initializeDatabase();

  try {
    const out = await c.run();
    const counts = await countAll();
    return {
      name: c.name,
      status: out.status,
      body: out.body,
      counts,
    };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    closeLLMLogsSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'system-data.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_SD_MAIN ?? '',
    mount: process.env.QT_FIXTURE_SD_MOUNT ?? '',
    llm: process.env.QT_FIXTURE_SD_LLM ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-sysdel-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const outLines: string[] = [];
  for (const c of buildCases()) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`system-delete-data oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('system-delete-data oracle', async () => {
  await main();
});
