/**
 * @jest-environment node
 *
 * P4.6ad AUTONOMOUS-ROOMS route-surface ORACLE: drives v4's REAL route handlers
 * over a FRESH copy of the committed autonomous-{main,mount}.db fixture per case,
 * and emits each response body (+ status) + optional structural table dumps so
 * the Rust ports (`api::autonomous_rooms::*`) diff byte-for-byte.
 *
 * v4 routes:
 *   - GET  app/api/v1/system/autonomous-rooms/route            → the listing
 *   - GET  app/api/v1/chats/[id]/autonomous-room/route         → status
 *   - POST app/api/v1/chats/[id]/autonomous-room?action=…      → the run verbs
 *
 * Only the seams the Rust harness also neutralizes are mocked: the auth session
 * (per-case userId) + the startup gate. The DB stack + repositories are doMocked
 * to the REAL modules (past jest.setup). Mutation minted ids/jobs/timestamps are
 * blanked by the harness (both sides run on the real clock — no clock pin).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-auto-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/autonomous-rooms-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/autonomous-rooms-web.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_AUTO_MAIN=$V5W/crates/quilltap-web/tests/fixtures/autonomous-main.db \
 *   QT_FIXTURE_AUTO_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/autonomous-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-autonomous-rooms-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- autonomous-rooms-routes
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
}

const ROOM_IDLE = 'c1000000-0000-4000-8000-000000000001';
const ROOM_RUNNING = 'c1000000-0000-4000-8000-000000000004';
const ROOM_PAUSED = 'c1000000-0000-4000-8000-000000000005';
const CHAT_SALON = 'c1000000-0000-4000-8000-000000000009';
const ROOM_USER_B = 'c1000000-0000-4000-8000-00000000000b';
const MISSING = '00000000-0000-4000-8000-0000000000ff';

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'GET',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

function applyMocks(spec: Spec, userId: string): void {
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
  // The forked background-job child, silenced (the sibling idiom — see
  // courier-images-routes.test.ts). `queueService.enqueueJob` calls
  // `ensureProcessorRunning()` after every insert, which `child_process.fork`s
  // `lib/background-jobs/child/child-entry.ts` with `execArgv: process.execArgv`.
  // Under jest that carries no tsx loader and no tsconfig-paths resolver, so the
  // child dies on `Cannot find package '@/lib'` — and its crash/restart loop
  // races the parent's DB singleton, which is how a mid-list case ends up
  // reading `no such table: chats`. P4.27 recorded that failure and deferred it;
  // it reproduces deterministically (P4.34 measured 0/4 green under load and a
  // fork failure on EVERY run, including the ones that happened to pass).
  //
  // Silencing it is also the FAITHFUL choice: the Rust harness has no forked
  // child either, so a child that actually claimed jobs would make the two
  // sides diverge on `background_jobs` state. The differential compares the
  // enqueue, never the execution.
  jest.doMock('@/lib/background-jobs/processor', () => {
    const actual = jest.requireActual('@/lib/background-jobs/processor');
    return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
  });
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

interface CaseSpec {
  name: string;
  /** The acting user (defaults to USER_A). */
  user?: 'A' | 'B';
  run: () => Promise<{ status: number; body: unknown; tables?: unknown }>;
}

async function loadRoute(
  path: string,
): Promise<Record<string, (...a: unknown[]) => Promise<unknown>>> {
  return (await import(path)) as never;
}

/** Raw dump of the autonomous columns of one chat row (both differential sides
 *  read raw columns → identical INTEGER/REAL/TEXT/NULL types; the harness blanks
 *  the minted/computed volatile fields). */
async function dumpRoomRow(id: string): Promise<unknown> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const db = getRawDatabase() as unknown as {
    prepare: (s: string) => { get: (...a: unknown[]) => unknown };
  };
  const r = db
    .prepare(
      'SELECT title, isManuallyRenamed, scheduleCron, scheduleNextRunAt, scheduleFreshnessWindowMs, ' +
        'budgetMaxTurns, budgetMaxTokens, budgetMaxWallClockMs, budgetEstimatedSpendCapUSD, ' +
        'budgetExcludeCacheHits, runVisibility, runDestructiveToolsAllowed, runState, ' +
        'runStateMessage, currentRunId, runStartedAt, runEndedAt, runPausedAt, runPausedAccumMs, ' +
        'runTurnsConsumed, runTokensConsumed FROM chats WHERE id = ?',
    )
    .get(id);
  return r ?? null;
}

/** The AUTONOMOUS_ROOM_TURN jobs (enqueue proof), oldest-first. */
async function dumpAutoTurnJobs(): Promise<unknown> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const db = getRawDatabase() as unknown as {
    prepare: (s: string) => { all: (...a: unknown[]) => unknown[] };
  };
  const rows = db
    .prepare(
      "SELECT type, status, payload FROM background_jobs WHERE type = 'AUTONOMOUS_ROOM_TURN' ORDER BY createdAt",
    )
    .all() as Array<{ type: string; status: string; payload: string }>;
  return { jobs: rows.map((r) => ({ type: r.type, status: r.status, payload: JSON.parse(r.payload) })) };
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
  const userId = c.user === 'B' ? spec.userIdB : spec.userId;
  applyMocks(spec, userId);
  const { initializePlugins } = await import('@/lib/startup/plugin-initialization');
  await initializePlugins();

  const work = mkdtempSync(join(scratch, 'auto-'));
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
    // Let any fire-and-forget write (the run-begun announcement) settle before
    // teardown so it can't corrupt the next case's read.
    await new Promise((r) => setTimeout(r, 350));
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'autonomous-rooms-web.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_AUTO_MAIN ?? '',
    mount: process.env.QT_FIXTURE_AUTO_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-auto-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const LIST = 'http://localhost/api/v1/system/autonomous-rooms';
  const listRoute = () => loadRoute('@/app/api/v1/system/autonomous-rooms/route');
  const roomRoute = () => loadRoute('@/app/api/v1/chats/[id]/autonomous-room/route');
  const roomUrl = (id: string) => `http://localhost/api/v1/chats/${id}/autonomous-room`;
  const params = (id: string) => ({ params: Promise.resolve({ id }) });

  const getStatus = async (id: string) =>
    respond(await (await roomRoute()).GET(mockRequest(roomUrl(id)), params(id)));
  const postAction = async (id: string, action: string, body?: unknown) =>
    respond(
      await (await roomRoute()).POST(mockRequest(`${roomUrl(id)}?action=${action}`, body), params(id)),
    );

  const cases: CaseSpec[] = [
    // --- Listing (sort + projectName join + ownership scoping) ---
    { name: 'listing', run: async () => respond(await (await listRoute()).GET(mockRequest(LIST))) },
    {
      name: 'listing_user_b',
      user: 'B',
      run: async () => respond(await (await listRoute()).GET(mockRequest(LIST))),
    },
    // --- Status snapshot ---
    { name: 'status_running', run: () => getStatus(ROOM_RUNNING) },
    { name: 'status_idle', run: () => getStatus(ROOM_IDLE) },
    { name: 'status_missing', run: () => getStatus(MISSING) },
    { name: 'status_non_autonomous', run: () => getStatus(CHAT_SALON) },
    // --- Start ---
    {
      name: 'start_idle',
      run: async () => {
        const { status, body } = await postAction(ROOM_IDLE, 'start');
        return { status, body, tables: await dumpAutoTurnJobs() };
      },
    },
    { name: 'start_already_running', run: () => postAction(ROOM_RUNNING, 'start') },
    // --- Pause ---
    {
      name: 'pause',
      run: async () => {
        const { status, body } = await postAction(ROOM_RUNNING, 'pause');
        return { status, body, tables: await dumpRoomRow(ROOM_RUNNING) };
      },
    },
    { name: 'pause_non_autonomous', run: () => postAction(CHAT_SALON, 'pause') },
    { name: 'pause_missing', run: () => postAction(MISSING, 'pause') },
    // --- Stop ---
    {
      name: 'stop',
      run: async () => {
        const { status, body } = await postAction(ROOM_RUNNING, 'stop');
        return { status, body, tables: await dumpRoomRow(ROOM_RUNNING) };
      },
    },
    // --- Resume ---
    {
      name: 'resume_paused',
      run: async () => {
        const { status, body } = await postAction(ROOM_PAUSED, 'resume');
        return { status, body, tables: await dumpRoomRow(ROOM_PAUSED) };
      },
    },
    {
      name: 'resume_idle',
      run: async () => {
        const { status, body } = await postAction(ROOM_IDLE, 'resume');
        return { status, body, tables: await dumpAutoTurnJobs() };
      },
    },
    // --- Update settings ---
    {
      name: 'update_caps',
      run: async () => {
        const { status, body } = await postAction(ROOM_IDLE, 'update-settings', {
          budgetMaxTurns: 25,
          budgetMaxTokens: 60000,
          budgetMaxWallClockMs: 1800000,
          budgetEstimatedSpendCapUSD: 2.25,
          scheduleFreshnessWindowMs: 7200000,
          runVisibility: 'household',
        });
        return { status, body, tables: await dumpRoomRow(ROOM_IDLE) };
      },
    },
    {
      name: 'update_clear_cap',
      run: async () => {
        const { status, body } = await postAction(ROOM_RUNNING, 'update-settings', {
          budgetMaxTurns: null,
        });
        return { status, body, tables: await dumpRoomRow(ROOM_RUNNING) };
      },
    },
    {
      name: 'update_cron_set',
      run: async () => {
        const { status, body } = await postAction(ROOM_IDLE, 'update-settings', {
          scheduleCron: '0 9 * * *',
        });
        return { status, body, tables: await dumpRoomRow(ROOM_IDLE) };
      },
    },
    {
      name: 'update_cron_clear',
      run: async () => {
        const { status, body } = await postAction(ROOM_RUNNING, 'update-settings', {
          scheduleCron: null,
        });
        return { status, body, tables: await dumpRoomRow(ROOM_RUNNING) };
      },
    },
    {
      name: 'update_title',
      run: async () => {
        const { status, body } = await postAction(ROOM_IDLE, 'update-settings', {
          title: '  The Renamed Room  ',
        });
        return { status, body, tables: await dumpRoomRow(ROOM_IDLE) };
      },
    },
    {
      name: 'update_invalid_cron',
      run: () => postAction(ROOM_IDLE, 'update-settings', { scheduleCron: 'not a cron' }),
    },
    { name: 'update_non_autonomous', run: () => postAction(CHAT_SALON, 'update-settings', { budgetMaxTurns: 5 }) },
    { name: 'update_missing', run: () => postAction(MISSING, 'update-settings', { budgetMaxTurns: 5 }) },
    // --- Destructive clamp both ways ---
    {
      name: 'update_clamp_false',
      run: async () => {
        const { status, body } = await postAction(ROOM_IDLE, 'update-settings', {
          runDestructiveToolsAllowed: true,
        });
        return { status, body, tables: await dumpRoomRow(ROOM_IDLE) };
      },
    },
    {
      name: 'update_clamp_true',
      user: 'B',
      run: async () => {
        const { status, body } = await postAction(ROOM_USER_B, 'update-settings', {
          runDestructiveToolsAllowed: true,
        });
        return { status, body, tables: await dumpRoomRow(ROOM_USER_B) };
      },
    },
  ];

  const lines: string[] = [];
  for (const c of cases) {
    const rec = await runCase(spec, c, scratch, fixtures);
    lines.push(JSON.stringify(rec));
  }
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(`wrote ${cases.length} autonomous-rooms-routes oracle records to ${outPath}\n`);
}

test('autonomous-rooms-routes oracle', async () => {
  await main();
}, 120000);
