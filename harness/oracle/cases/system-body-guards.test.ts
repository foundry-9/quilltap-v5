/**
 * @jest-environment node
 *
 * P4.62 ORACLE — the Data & System edges' BODY GUARDS: every arm the
 * wrong-type-collapse census named in `crates/quilltap-web/src/system_data_routes.rs`,
 * measured against v4's REAL route handlers.
 *
 * Three routes, all driven by their real exported handler:
 *   - `app/api/v1/system/tools/route.ts` POST — `tasks-queue`,
 *     `job-concurrency`, `capabilities-report-delete`, `delete-data`,
 *     `memory-dedup`
 *   - `app/api/v1/system/jobs/route.ts` POST — the enqueue body
 *   - `app/api/v1/system/unlock/route.ts` POST — the action + body-shape gates
 *
 * Every arm here either REFUSES or lands in a mocked service, so the oracle is
 * DB-free: the only repository call any of them makes is the context's own
 * `users.findById` plus `files.findByCategory` on the report-delete lookup, and
 * both are stubbed. **No arm may pass `delete-data`'s confirm gate** — the real
 * `deleteAllUserData` is not mocked, and must never be reached.
 *
 * `progressid_gate_*` is not a route call: it records what
 * `z.object({ progressId: z.uuid().optional() })` ACCEPTS, so the Rust side can
 * prove its 36-char + `Uuid::parse_str` filter agrees with Zod arm for arm.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-sys-guards-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/system-body-guards.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server            # or a pinned worktree
 *   QT_ORACLE_OUT=/tmp/oracle-system-body-guards.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- system-body-guards
 */

import * as fs from 'fs';

const USER = 'aa000000-0000-4000-8000-0000000000aa';

const PINNED_STATUS = {
  running: false,
  processing: false,
  inFlight: 0,
  childCrashed: false,
};

type Route = 'tools' | 'jobs' | 'unlock';

interface CaseSpec {
  name: string;
  route: Route;
  /** `?action=` for tools/unlock; ignored for the jobs collection. */
  action?: string;
  body: unknown;
  /** The body's `json()` REJECTS — v4's `await req.json()` on malformed bytes. */
  badJson?: boolean;
}

const CASES: CaseSpec[] = [
  // ── tools ?action=tasks-queue — v4 `!action || !['start','stop'].includes(action)`
  { name: 'queue_action_absent', route: 'tools', action: 'tasks-queue', body: {} },
  { name: 'queue_action_null', route: 'tools', action: 'tasks-queue', body: { action: null } },
  { name: 'queue_action_number', route: 'tools', action: 'tasks-queue', body: { action: 7 } },
  { name: 'queue_action_true', route: 'tools', action: 'tasks-queue', body: { action: true } },
  { name: 'queue_action_array', route: 'tools', action: 'tasks-queue', body: { action: [] } },
  { name: 'queue_action_unknown', route: 'tools', action: 'tasks-queue', body: { action: 'go' } },
  { name: 'queue_action_start', route: 'tools', action: 'tasks-queue', body: { action: 'start' } },
  { name: 'queue_action_stop', route: 'tools', action: 'tasks-queue', body: { action: 'stop' } },
  { name: 'queue_body_not_json', route: 'tools', action: 'tasks-queue', body: null, badJson: true },

  // ── tools ?action=job-concurrency — v4 `jobConcurrencySchema.safeParse` → validationError
  { name: 'concurrency_absent', route: 'tools', action: 'job-concurrency', body: {} },
  { name: 'concurrency_null', route: 'tools', action: 'job-concurrency', body: { concurrency: null } },
  { name: 'concurrency_string', route: 'tools', action: 'job-concurrency', body: { concurrency: '4' } },
  { name: 'concurrency_bool', route: 'tools', action: 'job-concurrency', body: { concurrency: true } },
  { name: 'concurrency_float', route: 'tools', action: 'job-concurrency', body: { concurrency: 4.5 } },
  { name: 'concurrency_zero', route: 'tools', action: 'job-concurrency', body: { concurrency: 0 } },
  { name: 'concurrency_too_big', route: 'tools', action: 'job-concurrency', body: { concurrency: 33 } },
  { name: 'concurrency_ok', route: 'tools', action: 'job-concurrency', body: { concurrency: 4 } },
  // `.catch(() => ({}))` — this leg alone survives a malformed body, as a 400.
  { name: 'concurrency_body_not_json', route: 'tools', action: 'job-concurrency', body: null, badJson: true },

  // ── tools ?action=capabilities-report-delete — v4 `!reportId` is JS falsiness,
  //    so a TRUTHY non-string passes the gate and dies in the `===` lookup as a 404.
  { name: 'report_delete_absent', route: 'tools', action: 'capabilities-report-delete', body: {} },
  { name: 'report_delete_null', route: 'tools', action: 'capabilities-report-delete', body: { reportId: null } },
  { name: 'report_delete_empty', route: 'tools', action: 'capabilities-report-delete', body: { reportId: '' } },
  { name: 'report_delete_zero', route: 'tools', action: 'capabilities-report-delete', body: { reportId: 0 } },
  { name: 'report_delete_false', route: 'tools', action: 'capabilities-report-delete', body: { reportId: false } },
  { name: 'report_delete_true', route: 'tools', action: 'capabilities-report-delete', body: { reportId: true } },
  { name: 'report_delete_number', route: 'tools', action: 'capabilities-report-delete', body: { reportId: 123 } },
  { name: 'report_delete_object', route: 'tools', action: 'capabilities-report-delete', body: { reportId: {} } },
  { name: 'report_delete_array', route: 'tools', action: 'capabilities-report-delete', body: { reportId: [] } },
  { name: 'report_delete_unknown', route: 'tools', action: 'capabilities-report-delete', body: { reportId: 'nope' } },
  { name: 'report_delete_body_not_json', route: 'tools', action: 'capabilities-report-delete', body: null, badJson: true },

  // ── tools ?action=delete-data — REFUSAL ARMS ONLY (`confirm !== 'DELETE_ALL_MY_DATA'`)
  { name: 'delete_data_confirm_absent', route: 'tools', action: 'delete-data', body: {} },
  { name: 'delete_data_confirm_null', route: 'tools', action: 'delete-data', body: { confirm: null } },
  { name: 'delete_data_confirm_number', route: 'tools', action: 'delete-data', body: { confirm: 1 } },
  { name: 'delete_data_confirm_wrong', route: 'tools', action: 'delete-data', body: { confirm: 'nope' } },
  {
    name: 'delete_data_confirm_wrong_keep_false',
    route: 'tools',
    action: 'delete-data',
    body: { confirm: 'nope', keepArchivedCharacterBundles: false },
  },
  { name: 'delete_data_body_not_json', route: 'tools', action: 'delete-data', body: null, badJson: true },

  // ── tools ?action=memory-dedup — v4 coerces `typeof !== 'number'` to 0.80
  // The accepted arms RUN the deduplicator, so exactly one of them is driven over
  // HTTP (its `result` is the oracle's stub echo on one side and a real sweep on
  // the other — normalized). The refusals are what pin the coercion: a
  // wrong-typed threshold that did NOT become 0.80 would answer 400 here.
  { name: 'dedup_threshold_string', route: 'tools', action: 'memory-dedup', body: { threshold: '0.9' } },
  { name: 'dedup_threshold_low', route: 'tools', action: 'memory-dedup', body: { threshold: 0.4 } },
  { name: 'dedup_threshold_high', route: 'tools', action: 'memory-dedup', body: { threshold: 1.5 } },
  { name: 'dedup_body_not_json', route: 'tools', action: 'memory-dedup', body: null, badJson: true },

  // ── POST /api/v1/system/jobs — the enqueue body
  { name: 'jobs_type_absent', route: 'jobs', body: { payload: {} } },
  { name: 'jobs_type_null', route: 'jobs', body: { type: null, payload: {} } },
  { name: 'jobs_type_number', route: 'jobs', body: { type: 7, payload: {} } },
  { name: 'jobs_type_object', route: 'jobs', body: { type: {}, payload: {} } },
  { name: 'jobs_payload_null', route: 'jobs', body: { type: 'MEMORY_HOUSEKEEPING', payload: null } },
  { name: 'jobs_body_not_json', route: 'jobs', body: null, badJson: true },
  // The ACCEPTED enqueue arms (`payload: []`, a wrong-typed `priority`) live in
  // `system-jobs-collection`, whose comparand is the STORED ROW — the only place
  // "what reached `enqueueJob`" is visible on both sides at once.

  // ── POST /api/v1/system/unlock — the action gate, then the body-shape gate
  { name: 'unlock_action_missing', route: 'unlock', action: '', body: {} },
  // `?action=` with an EMPTY value: `searchParams.get('action')` is `''`, which
  // `!action` treats as absent — NOT as an unknown action.
  { name: 'unlock_action_empty', route: 'unlock', action: '__EMPTY__', body: {} },
  { name: 'unlock_action_unknown', route: 'unlock', action: 'bogus', body: {} },
  { name: 'unlock_body_not_json', route: 'unlock', action: 'change-passphrase', body: null, badJson: true },
  { name: 'unlock_body_null', route: 'unlock', action: 'change-passphrase', body: null },
  { name: 'unlock_body_array', route: 'unlock', action: 'change-passphrase', body: [] },
  { name: 'unlock_body_string', route: 'unlock', action: 'change-passphrase', body: 'nope' },
  { name: 'unlock_body_number', route: 'unlock', action: 'change-passphrase', body: 42 },
  // A well-shaped body, but the app is LOCKED — v4 refuses before touching the
  // pepper, which is what keeps this oracle safe to run.
  {
    name: 'unlock_change_passphrase_locked',
    route: 'unlock',
    action: 'change-passphrase',
    body: { oldPassphrase: 'a', newPassphrase: 'b' },
  },
  {
    name: 'unlock_change_passphrase_locked_wrong_types',
    route: 'unlock',
    action: 'change-passphrase',
    body: { oldPassphrase: 7, newPassphrase: null },
  },
];

/** What `z.object({ progressId: z.uuid().optional() })` accepts, arm by arm. */
const PROGRESS_ID_ARMS: Array<{ name: string; value: unknown; present: boolean }> = [
  { name: 'absent', value: undefined, present: false },
  { name: 'null', value: null, present: true },
  { name: 'number', value: 7, present: true },
  { name: 'empty', value: '', present: true },
  { name: 'v4_uuid', value: '2f1c9a2e-6c3b-4f8a-9c1d-3a5b7e9f0d21', present: true },
  { name: 'nil_uuid', value: '00000000-0000-0000-0000-000000000000', present: true },
  { name: 'max_uuid', value: 'ffffffff-ffff-ffff-ffff-ffffffffffff', present: true },
  { name: 'uppercase', value: '2F1C9A2E-6C3B-4F8A-9C1D-3A5B7E9F0D21', present: true },
  { name: 'braced', value: '{2f1c9a2e-6c3b-4f8a-9c1d-3a5b7e9f0d21}', present: true },
  { name: 'simple', value: '2f1c9a2e6c3b4f8a9c1d3a5b7e9f0d21', present: true },
  { name: 'urn', value: 'urn:uuid:2f1c9a2e-6c3b-4f8a-9c1d-3a5b7e9f0d21', present: true },
  { name: 'not_a_uuid', value: 'nope', present: true },
  // 36 chars of valid hex, but the VERSION nibble is 9 and the VARIANT nibble
  // is 1 — Zod 4's `z.uuid()` is RFC-strict about both; `Uuid::parse_str` is
  // not. These two arms are what turn v5's `len() == 36 && parse_str` gate from
  // an assertion into a measurement.
  { name: 'bad_version', value: '2f1c9a2e-6c3b-9f8a-9c1d-3a5b7e9f0d21', present: true },
  { name: 'bad_variant', value: '2f1c9a2e-6c3b-4f8a-1c1d-3a5b7e9f0d21', present: true },
  { name: 'non_hex_36', value: 'gggggggg-6c3b-4f8a-9c1d-3a5b7e9f0d21', present: true },
];

function routeUrl(c: CaseSpec): string {
  if (c.route === 'jobs') return 'http://localhost/api/v1/system/jobs';
  const base = c.route === 'tools' ? 'system/tools' : 'system/unlock';
  if (c.action === '') return `http://localhost/api/v1/${base}`;
  if (c.action === '__EMPTY__') return `http://localhost/api/v1/${base}?action=`;
  return `http://localhost/api/v1/${base}?action=${c.action}`;
}

function mockRequest(url: string, c: CaseSpec): unknown {
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: c.badJson
      ? async () => {
          throw new SyntaxError('Unexpected end of JSON input');
        }
      : async () => c.body,
  };
}

function applyMocks(): void {
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: USER } }),
  }));
  // Every arm refuses or lands in a mocked service; `users.findById` is the
  // context's own call and `files.findByCategory` the report lookup.
  const repos = {
    users: { findById: async () => ({ id: USER }) },
    files: { findByCategory: async () => [] as unknown[] },
  };
  jest.doMock('@/lib/repositories/factory', () => ({
    __esModule: true,
    getRepositoriesSafe: async () => repos,
    getRepositories: () => repos,
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
    enqueueJob: async (
      _userId: string,
      _type: string,
      _payload: unknown,
      opts: { priority?: number; maxAttempts?: number },
    ) => `JOB<${JSON.stringify(opts)}>`,
  }));
  // The concurrency write must not touch a database — the SHAPE of v4's refusal
  // is what this oracle measures, and the accepted arm's echo.
  jest.doMock('@/lib/instance-settings', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/instance-settings'),
    getMaxConcurrentJobs: async () => 4,
    setMaxConcurrentJobs: async () => {},
  }));
  jest.doMock('@/lib/tools/memory-dedup', () => ({
    __esModule: true,
    deduplicateAllMemories: async (_userId: string, threshold: number) => ({
      threshold,
      totalOriginal: 0,
      totalRemoved: 0,
      totalMergedDetails: 0,
      totalFinal: 0,
    }),
  }));
  // The app is LOCKED, so `handleChangePassphrase` refuses before the dbkey is
  // touched. Nothing in this oracle may re-wrap a pepper.
  jest.doMock('@/lib/startup/dbkey', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/startup/dbkey'),
    getDbKeyState: () => 'locked',
  }));
}

async function runCase(c: CaseSpec): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks();
  const url = routeUrl(c);
  const mod =
    c.route === 'tools'
      ? '@/app/api/v1/system/tools/route'
      : c.route === 'jobs'
        ? '@/app/api/v1/system/jobs/route'
        : '@/app/api/v1/system/unlock/route';
  const { POST } = (await import(mod)) as { POST: (...a: unknown[]) => Promise<unknown> };
  const response = (await POST(mockRequest(url, c), { params: Promise.resolve({}) })) as {
    status: number;
    json: () => Promise<unknown>;
  };
  let body: unknown;
  try {
    body = await response.json();
  } catch {
    body = null;
  }
  return { name: c.name, status: response.status, body };
}

async function runProgressIdArms(): Promise<Record<string, unknown>[]> {
  jest.resetModules();
  // The oracle file is copied to a /tmp mirror (jest ignores `.claude/`), so a
  // bare `import 'zod'` cannot resolve from there — reach the v4 checkout's own
  // copy by absolute path, exactly as the real-DB oracles reach the cipher driver.
  const zodPath = require('node:path').join(process.cwd(), 'node_modules/zod');
  const { z } = require(zodPath) as typeof import('zod');
  const schema = z.object({ progressId: z.uuid().optional() });
  return PROGRESS_ID_ARMS.map((arm) => {
    const body = arm.present ? { progressId: arm.value } : {};
    const parsed = schema.safeParse(body);
    return {
      name: `progressid_gate_${arm.name}`,
      // v4: `if (parsed.success) progressId = parsed.data.progressId ?? null;`
      tracked: parsed.success ? (parsed.data.progressId ?? null) : null,
    };
  });
}

async function main(): Promise<void> {
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  process.env.LOG_LEVEL = 'error';
  const out = fs.createWriteStream(outPath);
  for (const c of CASES) {
    out.write(JSON.stringify(await runCase(c)) + '\n');
  }
  for (const row of await runProgressIdArms()) {
    out.write(JSON.stringify(row) + '\n');
  }
  out.end();
  await new Promise((r) => out.on('finish', r));
}

describe('P4.62 system body guards oracle', () => {
  it('emits the guard arms', async () => {
    await main();
  });
});
