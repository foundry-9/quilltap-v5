/**
 * @jest-environment node
 *
 * P4.9G3 jobs-COLLECTION route ORACLE: drives v4's REAL
 * `GET/POST /api/v1/system/jobs` handlers (`app/api/v1/system/jobs/route.ts`)
 * over a FRESH copy of the committed `system-data-*` fixture family per case and
 * emits each response `{status, body}` (+ `extra` where a case reads the DB
 * effect back). The Rust port direct-drives `api::system_data::{jobs_list,
 * jobs_enqueue_now}` — the free functions the new web-edge-only collection route
 * calls — and diffs each.
 *
 * These two handlers are the `system_jobs_routes_equivalence` sibling's blind
 * spot: P4.9G1 wrote `jobs_list` / `jobs_enqueue` with no edge and no case, so
 * they had never been diffed against v4. This file closes that.
 *
 * ── PROCESSOR STATUS IS PINNED ───────────────────────────────────────────────
 * As in `system-jobs-routes.test.ts`: v4's processor module is mocked so
 * `getProcessorStatus()` is a fixed snapshot and `ensureProcessorRunning` is a
 * no-op; the Rust side pins the SAME status. The diff therefore proves the
 * DB-derived fields, not the pump snapshot.
 *
 * ── THE REGISTRY IS NOT IN THIS DIFF ─────────────────────────────────────────
 * `activeByKind` merges active job ROWS with the in-flight activity registry,
 * and `startedByKind` is the registry's monotonic blip totals. In an oracle run
 * no inline work is in flight on EITHER side, so `activeByKind` here is purely
 * job-derived and `startedByKind` is all zeros. This family therefore proves the
 * DB leg, the opt-in gating, and the response shape — NOT the merge. The merge
 * and every span site are pinned by unit tests (P4.D123), since in-flight
 * counters never touch DB state.
 *
 * ── THE ENQUEUE CASE ─────────────────────────────────────────────────────────
 * The created job's id and its three timestamps are minted at call time, so the
 * case normalizes them: the body's `jobId` and, in `extra`, the read-back row's
 * `id`/`scheduledAt`/`createdAt`/`updatedAt`. Everything else about the row
 * (type, status, payload, priority, attempts, maxAttempts) is diffed verbatim.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   TMPO=/tmp/qt-sysjobscoll-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/system-jobs-collection.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/system-data.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/system-data-main.db \
 *   QT_FIXTURE_SD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/system-data-mount.db \
 *   QT_FIXTURE_SD_LLM=$V5W/crates/quilltap-web/tests/fixtures/system-data-llmlogs.db \
 *   QT_ORACLE_OUT=/tmp/oracle-system-jobs-collection.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- system-jobs-collection
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

const CHAT_1 = 'c1000000-0000-4000-8000-000000000001';
const LORIAN = 'a1000000-0000-4000-8000-000000000001';

/** Minted-at-call-time fields the diff replaces with a sentinel. */
const MINTED = ['id', 'scheduledAt', 'createdAt', 'updatedAt'];

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

const JOBS_ROUTE = '@/app/api/v1/system/jobs/route';
const B = 'http://localhost/api/v1';

async function jobsGet(
  query: string,
): Promise<{ status: number; body: unknown; extra: unknown }> {
  const route = (await import(JOBS_ROUTE)) as never as Record<
    string,
    (...a: unknown[]) => Promise<unknown>
  >;
  const out = await respond(await route.GET(mockRequest(`${B}/system/jobs${query}`, 'GET', {})));
  // The key ORDER is part of the contract (§Shared contract §A): v4 builds
  // `{stats, activeByKind, startedByKind, processor}` and then appends
  // activeByType / jobs / pendingForChat in that order. Object equality alone
  // proves the key SET; this proves the sequence, so a leaked unconditional
  // `activeByType` cannot hide behind a reordering.
  return { ...out, extra: { keys: Object.keys(out.body as Record<string, unknown>) } };
}

async function jobsPost(body: unknown): Promise<{ status: number; body: unknown }> {
  const route = (await import(JOBS_ROUTE)) as never as Record<
    string,
    (...a: unknown[]) => Promise<unknown>
  >;
  return respond(await route.POST(mockRequest(`${B}/system/jobs`, 'POST', body)));
}

interface CaseSpec {
  name: string;
  /** Top-level body fields to normalize (the minted `jobId`). */
  normalize?: string[];
  run: () => Promise<{ status: number; body: unknown; extra?: unknown }>;
}

function buildCases(): CaseSpec[] {
  return [
    // `includeByType` absent — the toolbar's own poll shape. `activeByType` is
    // withheld (664cfca84 made it opt-in); `activeByKind`/`startedByKind` are
    // always present.
    { name: 'jobs_collection_get', run: () => jobsGet('') },
    // Explicitly opted in.
    { name: 'jobs_collection_get_include_by_type', run: () => jobsGet('?includeByType=true') },
    // Anything but the literal 'true' is not an opt-in.
    { name: 'jobs_collection_get_include_by_type_junk', run: () => jobsGet('?includeByType=1') },
    // THE QUIRK: `includeJobs=true` implies `includeByType` even when the latter
    // is absent (v4: `param === 'true' || includeJobs`).
    { name: 'jobs_collection_get_include_jobs', run: () => jobsGet('?includeJobs=true') },
    // …and the quirk is one-directional: byType does NOT imply jobs.
    {
      name: 'jobs_collection_get_by_type_does_not_imply_jobs',
      run: () => jobsGet('?includeByType=true&includeJobs=false'),
    },
    { name: 'jobs_collection_get_chat', run: () => jobsGet(`?chatId=${CHAT_1}`) },
    {
      name: 'jobs_collection_get_both',
      run: () => jobsGet(`?includeJobs=true&chatId=${CHAT_1}`),
    },
    {
      name: 'jobs_collection_post',
      normalize: ['jobId'],
      run: async () => {
        const created = await jobsPost({
          type: 'MEMORY_HOUSEKEEPING',
          payload: { characterId: LORIAN },
          priority: 3,
          maxAttempts: 5,
        });
        const jobId = (created.body as { jobId?: string }).jobId ?? '';
        const after = await jobsGet('?includeJobs=true');
        const jobs = ((after.body as { jobs?: Array<Record<string, unknown>> }).jobs ??
          []) as Array<Record<string, unknown>>;
        const row = jobs.find((j) => j.id === jobId) ?? null;
        if (row) for (const k of MINTED) if (k in row) row[k] = '<NORM>';
        return { ...created, extra: { row, jobCount: jobs.length } };
      },
    },
    {
      name: 'jobs_collection_post_bad_type',
      run: () => jobsPost({ type: 'NOT_A_JOB', payload: {} }),
    },
    {
      name: 'jobs_collection_post_no_payload',
      run: () => jobsPost({ type: 'MEMORY_HOUSEKEEPING' }),
    },
    {
      name: 'jobs_collection_post_payload_not_object',
      run: () => jobsPost({ type: 'MEMORY_HOUSEKEEPING', payload: 'nope' }),
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
    return {
      name: c.name,
      status: out.status,
      body: out.body,
      ...(out.extra !== undefined ? { extra: out.extra } : {}),
      ...(c.normalize ? { normalize: c.normalize } : {}),
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-sysjobscoll-oracle-'));
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
  process.stderr.write(
    `system-jobs-collection oracle wrote ${outPath} (${outLines.length} cases)\n`,
  );
}

test('system-jobs-collection oracle', async () => {
  await main();
});
