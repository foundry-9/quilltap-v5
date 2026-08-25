/**
 * @jest-environment node
 *
 * P4.60 ORACLE — the restore route's BODY GUARDS
 * (`app/api/v1/system/restore/route.ts`, the `preview` and default actions).
 *
 * These arms never reach the restore service: every one of them stops at the
 * route's own `uploadId` / `mode` checks, or at `getPendingUpload`, whose
 * module-local `pendingUploads` map is EMPTY in a fresh import. That is what
 * makes this oracle cheap — it needs no provisioned instance and no fixture,
 * only a session and a `users.findById`, both of which the guard arms never
 * consult beyond `buildRequestContext`'s own check.
 *
 * What it measures: v4 destructures `{ uploadId, mode }` with NO Zod at all, so
 * the questions are about JS semantics rather than a schema. `!uploadId` is JS
 * FALSINESS (so `0`, `false` and `''` are all "required"), while a truthy
 * wrong-typed value goes on to `UUID_REGEX.test(uploadId)`, which coerces it
 * with `String()` — and therefore answers the OTHER sentence. v5's edge read the
 * key with `and_then(Value::as_str)`, collapsing both into "uploadId is
 * required".
 *
 * `keepArchivedCharacterBundles` rides along as a recorded FAITHFUL verdict:
 * v4's `!== false` means only a literal `false` disables it, which is exactly
 * what an `as_bool` read leaves as `Some(false)`.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-restore-guards-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/system-restore-guards.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-system-restore-guards.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- system-restore-guards
 */

import * as fs from 'fs';

const USER = 'aa000000-0000-4000-8000-0000000000aa';

interface CaseSpec {
  name: string;
  /** `?action=preview`, or omitted for the default restore action. */
  action?: 'preview';
  body: unknown;
}

const CASES: CaseSpec[] = [
  // --- `uploadId`: JS falsiness vs a truthy wrong type ---
  { name: 'preview_upload_id_absent', action: 'preview', body: {} },
  { name: 'preview_upload_id_null', action: 'preview', body: { uploadId: null } },
  { name: 'preview_upload_id_empty', action: 'preview', body: { uploadId: '' } },
  { name: 'preview_upload_id_zero', action: 'preview', body: { uploadId: 0 } },
  { name: 'preview_upload_id_false', action: 'preview', body: { uploadId: false } },
  { name: 'preview_upload_id_number', action: 'preview', body: { uploadId: 123 } },
  { name: 'preview_upload_id_true', action: 'preview', body: { uploadId: true } },
  { name: 'preview_upload_id_object', action: 'preview', body: { uploadId: { a: 1 } } },
  { name: 'preview_upload_id_array', action: 'preview', body: { uploadId: ['x'] } },
  {
    name: 'preview_upload_id_unknown_uuid',
    action: 'preview',
    body: { uploadId: '11111111-1111-4111-8111-111111111111' },
  },
  // --- the default (restore) action: `uploadId` is checked BEFORE `mode` ---
  { name: 'restore_upload_id_absent_bad_mode', body: { mode: 'nope' } },
  { name: 'restore_upload_id_number_bad_mode', body: { uploadId: 123, mode: 'nope' } },
  {
    name: 'restore_mode_absent',
    body: { uploadId: '11111111-1111-4111-8111-111111111111' },
  },
  {
    name: 'restore_mode_wrong_type',
    body: { uploadId: '11111111-1111-4111-8111-111111111111', mode: 7 },
  },
  {
    name: 'restore_mode_unknown',
    body: { uploadId: '11111111-1111-4111-8111-111111111111', mode: 'merge' },
  },
  // A well-formed request that only fails at the map lookup — the arm that
  // proves `mode` passed and `keepArchivedCharacterBundles` was accepted in
  // every shape (v4's `!== false`).
  {
    name: 'restore_valid_shape_unknown_upload',
    body: { uploadId: '11111111-1111-4111-8111-111111111111', mode: 'replace' },
  },
  {
    name: 'restore_keep_bundles_wrong_type',
    body: {
      uploadId: '11111111-1111-4111-8111-111111111111',
      mode: 'replace',
      keepArchivedCharacterBundles: 'no',
    },
  },
];

function mockRequest(url: string, body: unknown): unknown {
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: async () => body,
  };
}

function applyMocks(): void {
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: USER } }),
  }));
  // The guard arms never touch a repository — they answer before
  // `getPendingUpload` can find anything — so the context's own `users.findById`
  // is the only repo call, and a stub for it keeps this oracle DB-free.
  jest.doMock('@/lib/repositories/factory', () => ({
    __esModule: true,
    getRepositoriesSafe: async () => ({ users: { findById: async () => ({ id: USER }) } }),
    getRepositories: () => ({ users: { findById: async () => ({ id: USER }) } }),
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

async function runCase(c: CaseSpec): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks();
  const url = `http://localhost/api/v1/system/restore${c.action ? `?action=${c.action}` : ''}`;
  const { POST } = (await import('@/app/api/v1/system/restore/route')) as {
    POST: (...a: unknown[]) => Promise<unknown>;
  };
  const response = (await POST(mockRequest(url, c.body))) as {
    status: number;
    json: () => Promise<unknown>;
  };
  return { name: c.name, status: response.status, body: await response.json() };
}

async function main(): Promise<void> {
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  process.env.LOG_LEVEL = 'error';
  const out = fs.createWriteStream(outPath);
  for (const c of CASES) {
    out.write(JSON.stringify(await runCase(c)) + '\n');
  }
  out.end();
  await new Promise((r) => out.on('finish', r));
}

describe('P4.60 restore route guards oracle', () => {
  it('emits the guard arms', async () => {
    await main();
  });
});
