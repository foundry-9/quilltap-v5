/**
 * @jest-environment node
 *
 * P4.60 ORACLE (confirm-only pass) — the `.qtap` import legs' BODY GUARDS
 * (`app/api/v1/system/tools/route.ts?action=import-preview|import-execute`,
 * the legacy JSON-body path).
 *
 * The order lists `qtap_routes.rs:118-226` as CONFIRM-ONLY: the sub-objects are
 * passed whole, and `data_key_absent` deliberately distinguishes an absent
 * `data` key from an explicit null. This oracle is what turns "confirm" into a
 * measurement rather than a reading — it records what v4 answers for a body
 * whose `exportData` is falsy-but-not-null, for a manifest whose `format` is
 * wrong-typed, and for a body that is not JSON at all.
 *
 * Every arm stops in the route's own guards (`validateExportFile`, `!exportData`,
 * `!options`, the `conflictStrategy` allow-list) or in the outer catch, so no
 * provisioned instance is needed on either side — only a session and a
 * `users.findById`, which `buildRequestContext` consults and the guards do not.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-qtap-guards-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/qtap-import-guards.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-qtap-import-guards.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- qtap-import-guards
 */

import * as fs from 'fs';

const USER = 'aa000000-0000-4000-8000-0000000000aa';

/** A manifest that passes `validateExportFile`. */
const GOOD_MANIFEST = { format: 'quilltap-export', version: '1.0', exportType: 'characters' };

interface CaseSpec {
  name: string;
  action: 'import-preview' | 'import-execute';
  body: unknown;
  /** The body's `json()` REJECTS — v4's `await req.json()` inside the try. */
  badJson?: boolean;
}

const CASES: CaseSpec[] = [
  // --- preview: `if (!body.exportData)` is JS falsiness ---
  { name: 'preview_export_data_absent', action: 'import-preview', body: {} },
  { name: 'preview_export_data_null', action: 'import-preview', body: { exportData: null } },
  { name: 'preview_export_data_zero', action: 'import-preview', body: { exportData: 0 } },
  { name: 'preview_export_data_empty_string', action: 'import-preview', body: { exportData: '' } },
  { name: 'preview_export_data_false', action: 'import-preview', body: { exportData: false } },
  // Truthy but not an object, or an object with a bad manifest → the shared
  // `validateExportFile` sentence.
  { name: 'preview_export_data_number', action: 'import-preview', body: { exportData: 42 } },
  { name: 'preview_export_data_array', action: 'import-preview', body: { exportData: [1] } },
  {
    name: 'preview_manifest_format_wrong_type',
    action: 'import-preview',
    body: { exportData: { manifest: { format: 1, version: '1.0' }, data: {} } },
  },
  {
    name: 'preview_manifest_version_wrong_type',
    action: 'import-preview',
    body: { exportData: { manifest: { format: 'quilltap-export', version: 1.0 }, data: {} } },
  },
  {
    name: 'preview_manifest_array',
    action: 'import-preview',
    body: { exportData: { manifest: ['quilltap-export'], data: {} } },
  },
  { name: 'preview_body_not_json', action: 'import-preview', body: null, badJson: true },
  // The whole body is a scalar: `(42).exportData` is `undefined` in JS (no
  // throw), but `null.exportData` is a TypeError the outer catch turns into a
  // 500. Measured rather than reasoned about.
  { name: 'preview_body_number', action: 'import-preview', body: 42 },
  { name: 'preview_body_null', action: 'import-preview', body: null },
  { name: 'preview_body_string', action: 'import-preview', body: 'nope' },
  // --- execute: `!exportData`, then `!options`, then the strategy allow-list ---
  { name: 'execute_export_data_absent', action: 'import-execute', body: {} },
  { name: 'execute_export_data_zero', action: 'import-execute', body: { exportData: 0 } },
  {
    name: 'execute_options_absent',
    action: 'import-execute',
    body: { exportData: { manifest: GOOD_MANIFEST, data: {} } },
  },
  {
    name: 'execute_options_falsy',
    action: 'import-execute',
    body: { exportData: { manifest: GOOD_MANIFEST, data: {} }, options: 0 },
  },
  {
    name: 'execute_options_wrong_type',
    action: 'import-execute',
    body: { exportData: { manifest: GOOD_MANIFEST, data: {} }, options: 'nope' },
  },
  {
    name: 'execute_strategy_wrong_type',
    action: 'import-execute',
    body: {
      exportData: { manifest: GOOD_MANIFEST, data: {} },
      options: { conflictStrategy: 7 },
    },
  },
  {
    name: 'execute_strategy_unknown',
    action: 'import-execute',
    body: {
      exportData: { manifest: GOOD_MANIFEST, data: {} },
      options: { conflictStrategy: 'merge' },
    },
  },
  { name: 'execute_body_not_json', action: 'import-execute', body: null, badJson: true },
  { name: 'execute_body_number', action: 'import-execute', body: 42 },
  { name: 'execute_body_null', action: 'import-execute', body: null },
];

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
  // The guard arms never reach a repository — `previewImport`/`executeImport`
  // are downstream of every refusal here — so the context's own `users.findById`
  // is the only repo call and a stub keeps this oracle DB-free.
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
  const url = `http://localhost/api/v1/system/tools?action=${c.action}`;
  const { POST } = (await import('@/app/api/v1/system/tools/route')) as {
    POST: (...a: unknown[]) => Promise<unknown>;
  };
  const response = (await POST(mockRequest(url, c))) as {
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

describe('P4.60 qtap import guards oracle', () => {
  it('emits the guard arms', async () => {
    await main();
  });
});
