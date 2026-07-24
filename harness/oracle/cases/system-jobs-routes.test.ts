/**
 * @jest-environment node
 *
 * P4.9G1 tasks-queue / jobs route-surface ORACLE: drives v4's REAL route
 * handlers (`system/tools?action=tasks-queue|job-concurrency` +
 * `system/jobs/[id]` GET/DELETE/POST) over a FRESH copy of the committed
 * `system-data-*` fixture family per case, and emits each response {status,
 * body}. The Rust port (`api::system_data::*`, direct-driven) diffs each.
 *
 * ── PROCESSOR STATUS IS PINNED ───────────────────────────────────────────────
 * v4's `getProcessorStatus()` reports a forked-child snapshot; in v5 the host
 * owns the pump (the `JobPumpControl` seam). The oracle MOCKS
 * `@/lib/background-jobs/processor` so getProcessorStatus returns a fixed
 * `{running:false, processing:false, inFlight:0, childCrashed:false}` and
 * start/stop/wake/ensure are no-ops; the Rust differential pins the SAME status.
 * So the diff proves the DB-derived fields (stats, active set, per-job
 * serialization, tokens, concurrency) — not the nondeterministic pump snapshot.
 *
 * ── MUTATION CASES ───────────────────────────────────────────────────────────
 * pause/resume/delete/concurrency-set each run on a FRESH fixture copy. The
 * pause/resume result job carries a fresh `updatedAt` (+ `scheduledAt` on
 * resume); the diff normalizes those. delete/pause re-GET the id and emit
 * `extra.afterStatus`/`extra.afterBody` so the diff sees the DB effect.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-sysjobs-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/system-jobs-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/system-data.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/system-data-main.db \
 *   QT_FIXTURE_SD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/system-data-mount.db \
 *   QT_FIXTURE_SD_LLM=$V5W/crates/quilltap-web/tests/fixtures/system-data-llmlogs.db \
 *   QT_ORACLE_OUT=/tmp/oracle-system-jobs.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- system-jobs-routes
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

// A PENDING job (b6..01), the PROCESSING one (b6..02), a retry-eligible FAILED
// (b6..03), a retry-exhausted FAILED (b6..04), COMPLETED (b6..05), DEAD (b6..06),
// PAUSED (b6..07), another PENDING (b6..08).
const JOB_PENDING = 'b6000000-0000-4000-8000-000000000001';
const JOB_PROCESSING = 'b6000000-0000-4000-8000-000000000002';
const JOB_PAUSED = 'b6000000-0000-4000-8000-000000000007';
const JOB_PENDING_2 = 'b6000000-0000-4000-8000-000000000008';
const JOB_MISSING = 'b6000000-0000-4000-8000-0000000000ff';

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
  // Pin the processor snapshot + no-op the lifecycle controls.
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

async function loadRoute(
  path: string,
): Promise<Record<string, (...a: unknown[]) => Promise<unknown>>> {
  return (await import(path)) as never;
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  if (resp.status === 204) return { status: 204, body: null };
  return { status: resp.status, body: await resp.json() };
}

const TOOLS_ROUTE = '@/app/api/v1/system/tools/route';
const JOB_ROUTE = '@/app/api/v1/system/jobs/[id]/route';
const B = 'http://localhost/api/v1';

const toolsGet = async (action: string, extra = '') =>
  respond(
    await (await loadRoute(TOOLS_ROUTE)).GET(
      mockRequest(`${B}/system/tools?action=${action}${extra}`, 'GET', {}),
    ),
  );
const toolsPost = async (action: string, body: unknown) =>
  respond(
    await (await loadRoute(TOOLS_ROUTE)).POST(
      mockRequest(`${B}/system/tools?action=${action}`, 'POST', body),
    ),
  );
const jobGet = async (id: string) =>
  respond(
    await (await loadRoute(JOB_ROUTE)).GET(mockRequest(`${B}/system/jobs/${id}`, 'GET', {}), {
      params: Promise.resolve({ id }),
    }),
  );
const jobDelete = async (id: string) =>
  respond(
    await (await loadRoute(JOB_ROUTE)).DELETE(mockRequest(`${B}/system/jobs/${id}`, 'DELETE', {}), {
      params: Promise.resolve({ id }),
    }),
  );
const jobPost = async (id: string, action: string) =>
  respond(
    await (await loadRoute(JOB_ROUTE)).POST(
      mockRequest(`${B}/system/jobs/${id}?action=${action}`, 'POST', {}),
      { params: Promise.resolve({ id }) },
    ),
  );

interface CaseSpec {
  name: string;
  /** Fields on the response body whose values are nondeterministic (normalize). */
  normalize?: string[];
  /** Compare only the status (documented body divergence — e.g. Zod details). */
  statusOnly?: boolean;
  run: () => Promise<{ status: number; body: unknown; extra?: unknown }>;
}

function buildCases(): CaseSpec[] {
  return [
    { name: 'tasks_queue_get', run: () => toolsGet('tasks-queue') },
    { name: 'job_concurrency_get_default', run: () => toolsGet('job-concurrency') },
    { name: 'tasks_queue_start', run: () => toolsPost('tasks-queue', { action: 'start' }) },
    { name: 'tasks_queue_stop', run: () => toolsPost('tasks-queue', { action: 'stop' }) },
    {
      name: 'tasks_queue_bogus_action',
      statusOnly: true,
      run: () => toolsPost('tasks-queue', { action: 'nope' }),
    },
    {
      name: 'job_concurrency_set_8',
      run: async () => {
        const set = await toolsPost('job-concurrency', { concurrency: 8 });
        const after = await toolsGet('job-concurrency');
        return { ...set, extra: { afterConcurrency: (after.body as { concurrency?: number }).concurrency } };
      },
    },
    {
      name: 'job_concurrency_set_out_of_range',
      statusOnly: true,
      run: () => toolsPost('job-concurrency', { concurrency: 0 }),
    },
    { name: 'job_get_existing', run: () => jobGet(JOB_PENDING) },
    { name: 'job_get_missing', run: () => jobGet(JOB_MISSING) },
    {
      name: 'job_delete_pending',
      run: async () => {
        const del = await jobDelete(JOB_PENDING);
        const after = await jobGet(JOB_PENDING);
        return { ...del, extra: { afterStatus: after.status } };
      },
    },
    { name: 'job_delete_processing_blocked', run: () => jobDelete(JOB_PROCESSING) },
    { name: 'job_delete_missing', run: () => jobDelete(JOB_MISSING) },
    {
      name: 'job_pause_pending',
      normalize: ['updatedAt'],
      run: () => jobPost(JOB_PENDING_2, 'pause'),
    },
    {
      name: 'job_resume_paused',
      normalize: ['updatedAt', 'scheduledAt'],
      run: () => jobPost(JOB_PAUSED, 'resume'),
    },
    { name: 'job_pause_processing_blocked', run: () => jobPost(JOB_PROCESSING, 'pause') },
    { name: 'job_resume_pending_blocked', run: () => jobPost(JOB_PENDING, 'resume') },
    { name: 'job_bogus_action', run: () => jobPost(JOB_PENDING, 'frobnicate') },
    { name: 'job_get_missing_after_nothing', run: () => jobGet(JOB_MISSING) },
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
  await initializeDatabase();

  try {
    const out = await c.run(spec);
    return {
      name: c.name,
      status: out.status,
      body: out.body,
      ...(out.extra !== undefined ? { extra: out.extra } : {}),
      ...(c.normalize ? { normalize: c.normalize } : {}),
      ...(c.statusOnly ? { statusOnly: true } : {}),
    };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-sysjobs-oracle-'));
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
  process.stderr.write(`system-jobs-routes oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('system-jobs-routes oracle', async () => {
  await main();
});
