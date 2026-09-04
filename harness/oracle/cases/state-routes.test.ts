/**
 * @jest-environment node
 *
 * Differential ORACLE for the P4.d10 §A state dispatch verbs: v4's REAL route
 * modules — `app/api/v1/chats/[id]` (`?action=get-state|set-state|reset-state`),
 * `app/api/v1/groups/[id]` (same actions), and
 * `app/api/v1/settings/general-state` (GET/PUT/DELETE) — driven directly (the
 * P4.d7 direct-drive precedent) over a FRESH copy of the state-sql-tools
 * fixture family per case, at the round baseline `7e6d13e5`.
 *
 * One NDJSON row per case: `{ label, status, body, afterBody? }` — `afterBody`
 * is the corresponding GET re-read after a mutation (persistence proof).
 * Validation-failure bodies drop the Zod `details` array (v4-implementation-
 * specific; the settings-routes precedent).
 *
 * Run (Node 24; the v4 checkout must sit at the oracle baseline — pin a
 * detached worktree only on drift/dirty; stage outside .claude/ first).
 * Self-contained: builds the /tmp fixture itself rather than assuming the
 * state-cascade recipe ran first.
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   STAGE=/tmp/qt-state-routes-stage
 *   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
 *   cp $V5W/harness/oracle/cases/state-routes.test.ts $STAGE/harness/oracle/cases/
 *   cp $V5W/harness/oracle/fixtures/state-sql-tools.json $STAGE/harness/oracle/fixtures/
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_TMP_MAIN=/tmp/qt-state-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-state-mount.db \
 *   QT_FIXTURE_TMP_LLM=/tmp/qt-state-llm.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-state-sql-tools-fixture.ts
 *   QT_FIXTURE_TMP_MAIN=/tmp/qt-state-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-state-mount.db \
 *   QT_FIXTURE_TMP_LLM=/tmp/qt-state-llm.db QT_ORACLE_OUT=/tmp/oracle-state-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- state-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  chatProjectId: string;
  chatSoloId: string;
  chatUnionId: string;
  groupAlphaId: string;
  generalMountPointId: string;
}

type Method = 'GET' | 'PUT' | 'DELETE';

interface CaseSpec {
  label: string;
  route: 'chat' | 'group' | 'general';
  method: Method;
  /** ?action= for chat/group routes (absent for the general route). */
  action?: string;
  id?: (spec: Spec) => string;
  body?: unknown;
  /** Clear the generalMountPointId pointer first (the unprovisioned arm). */
  clearGeneralPointer?: boolean;
  /** Re-read via the tier's GET after the mutation. */
  after?: boolean;
}

const CASES: CaseSpec[] = [
  // ── chat (the merged cascade GET + own-column set/reset) ──
  { label: 'chat_get_state_full', route: 'chat', method: 'GET', action: 'get-state', id: (s) => s.chatProjectId },
  { label: 'chat_get_state_solo', route: 'chat', method: 'GET', action: 'get-state', id: (s) => s.chatSoloId },
  { label: 'chat_get_state_union_ambiguous', route: 'chat', method: 'GET', action: 'get-state', id: (s) => s.chatUnionId },
  { label: 'chat_get_state_missing', route: 'chat', method: 'GET', action: 'get-state', id: () => 'deadbeef-0000-4000-8000-000000000000' },
  { label: 'chat_set_state', route: 'chat', method: 'PUT', action: 'set-state', id: (s) => s.chatProjectId, body: { state: { hp: 1, fresh: true } }, after: true },
  { label: 'chat_set_state_invalid', route: 'chat', method: 'PUT', action: 'set-state', id: (s) => s.chatProjectId, body: { state: 5 } },
  { label: 'chat_set_state_missing', route: 'chat', method: 'PUT', action: 'set-state', id: () => 'deadbeef-0000-4000-8000-000000000000', body: { state: {} } },
  { label: 'chat_reset_state', route: 'chat', method: 'DELETE', action: 'reset-state', id: (s) => s.chatProjectId, after: true },
  // ── group (own state, no cascade) ──
  { label: 'group_get_state', route: 'group', method: 'GET', action: 'get-state', id: (s) => s.groupAlphaId },
  { label: 'group_get_state_missing', route: 'group', method: 'GET', action: 'get-state', id: () => 'deadbeef-0000-4000-8000-000000000000' },
  { label: 'group_set_state', route: 'group', method: 'PUT', action: 'set-state', id: (s) => s.groupAlphaId, body: { state: { banner: 'up' } }, after: true },
  { label: 'group_set_state_invalid', route: 'group', method: 'PUT', action: 'set-state', id: (s) => s.groupAlphaId, body: { state: [1, 2] } },
  { label: 'group_reset_state', route: 'group', method: 'DELETE', action: 'reset-state', id: (s) => s.groupAlphaId, after: true },
  // ── general (instance-wide, bespoke route) ──
  { label: 'general_get_state', route: 'general', method: 'GET' },
  { label: 'general_set_state', route: 'general', method: 'PUT', body: { state: { fog: 3, nested: { a: 1 } } }, after: true },
  { label: 'general_set_state_invalid', route: 'general', method: 'PUT', body: { state: 'nope' } },
  { label: 'general_reset_state', route: 'general', method: 'DELETE', after: true },
  { label: 'general_set_unprovisioned', route: 'general', method: 'PUT', body: { state: { a: 1 } }, clearGeneralPointer: true },
  { label: 'general_get_unprovisioned', route: 'general', method: 'GET', clearGeneralPointer: true },
];

function mockRequest(url: string, method: Method, body?: unknown): unknown {
  return {
    method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

function dropDetails(body: unknown): unknown {
  if (body && typeof body === 'object' && 'details' in (body as Record<string, unknown>)) {
    const copy = { ...(body as Record<string, unknown>) };
    delete copy.details;
    return copy;
  }
  return body;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'state-sql-tools.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_TMP_MAIN;
  const mountFixture = process.env.QT_FIXTURE_TMP_MOUNT;
  const llmFixture = process.env.QT_FIXTURE_TMP_LLM;
  if (!mainFixture || !mountFixture || !llmFixture) {
    throw new Error('QT_FIXTURE_TMP_MAIN / _MOUNT / _LLM must point at the seeded fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );

  const lines: string[] = [];

  for (const c of CASES) {
    const scratch = mkdtempSync(join(tmpdir(), 'qt-stateroutes-oracle-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const mainWork = join(scratch, 'main-work.db');
    const mountWork = join(scratch, 'mount-work.db');
    const llmWork = join(scratch, 'llm-work.db');
    copyFileSync(mainFixture, mainWork);
    copyFileSync(mountFixture, mountWork);
    copyFileSync(llmFixture, llmWork);

    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.SQLITE_PATH = mainWork;
    process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
    process.env.SQLITE_LLM_LOGS_PATH = llmWork;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';

    jest.resetModules();
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
    jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
    jest.doMock('@/lib/database/backends/sqlite/client', () =>
      jest.requireActual('@/lib/database/backends/sqlite/client'),
    );
    jest.doMock('@/lib/database/backends/sqlite/llm-logs-client', () =>
      jest.requireActual('@/lib/database/backends/sqlite/llm-logs-client'),
    );
    jest.doMock('@/lib/database/backends/sqlite/mount-index-client', () =>
      jest.requireActual('@/lib/database/backends/sqlite/mount-index-client'),
    );
    // The chats/[id] route module transitively imports the markdown renderer
    // (npm-native ESM jest can't compile). Stub it — the state actions never
    // touch it (the salon-reads precedent).
    jest.doMock('@/lib/services/markdown-renderer.service', () => ({
      __esModule: true,
      renderMarkdownToHtml: async () => null,
      canPreRenderMessage: () => false,
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

    const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
    await initializeDatabase();

    if (c.clearGeneralPointer) {
      const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
      const db = getRawDatabase();
      if (!db) throw new Error('main DB handle unavailable');
      db.prepare('DELETE FROM "instance_settings" WHERE "key" = ?').run('generalMountPointId');
    }

    const id = c.id ? c.id(spec) : '';
    const params = { params: Promise.resolve({ id }) };

    const call = async (
      method: Method,
      action: string | undefined,
      body?: unknown,
    ): Promise<{ status: number; body: unknown }> => {
      let response: { status: number; json: () => Promise<unknown> };
      if (c.route === 'chat') {
        const mod = await import('@/app/api/v1/chats/[id]/route');
        const url = `http://x/api/v1/chats/${id}${action ? `?action=${action}` : ''}`;
        const req = mockRequest(url, method, body);
        const fn = method === 'GET' ? mod.GET : method === 'PUT' ? mod.PUT : mod.DELETE;
        response = (await fn(req as never, params as never)) as never;
      } else if (c.route === 'group') {
        const mod = await import('@/app/api/v1/groups/[id]/route');
        const url = `http://x/api/v1/groups/${id}${action ? `?action=${action}` : ''}`;
        const req = mockRequest(url, method, body);
        const fn = method === 'GET' ? mod.GET : method === 'PUT' ? mod.PUT : mod.DELETE;
        response = (await fn(req as never, params as never)) as never;
      } else {
        const mod = await import('@/app/api/v1/settings/general-state/route');
        const url = 'http://x/api/v1/settings/general-state';
        const req = mockRequest(url, method, body);
        const fn = method === 'GET' ? mod.GET : method === 'PUT' ? mod.PUT : mod.DELETE;
        response = (await fn(req as never)) as never;
      }
      return { status: response.status, body: dropDetails(await response.json()) };
    };

    const { status, body } = await call(c.method, c.action, c.body);
    let afterBody: unknown;
    if (c.after) {
      afterBody = (await call('GET', c.route === 'general' ? undefined : 'get-state')).body;
    }

    lines.push(
      JSON.stringify({
        label: c.label,
        status,
        body,
        ...(c.after ? { afterBody } : {}),
      }),
    );

    const { closeMountIndexSQLiteClient } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    closeMountIndexSQLiteClient();
    await closeDatabase();
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`state-routes oracle wrote ${outPath} (${lines.length} cases)\n`);
}

test('state-routes oracle', async () => {
  await main();
});
