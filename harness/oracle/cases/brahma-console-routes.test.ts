/**
 * @jest-environment node
 *
 * P4.9I1A BRAHMA-CONSOLE CRUD route-surface ORACLE: drives v4's REAL route
 * handlers over the committed `brahma-{main,mount}.db` fixture, mocking ONLY the
 * DB path + the session (per-case userId) + the startup gate. Records `{ status,
 * body }` per case; the Rust differential (`brahma_console_routes_equivalence`)
 * direct-drives `api::brahma::*` over a fresh copy and diffs.
 *
 *   - GET  /api/v1/brahma-console               → handleList
 *   - POST /api/v1/brahma-console               → handleCreate (seed title)
 *   - GET  /api/v1/brahma-console/[id]          → handleGet
 *   - PATCH /api/v1/brahma-console/[id]         → handleRename
 *   - PATCH /api/v1/brahma-console/[id]?action=set-model → handleSetModel
 *   - DELETE /api/v1/brahma-console/[id]        → handleDelete
 *   - GET  /api/v1/brahma-console/[id]/messages → handleGetMessages
 *   - the verifyBrahmaChat 404 arms (non-brahma / missing / not-owner)
 *
 * Each case runs isolated (resetModules + a fresh fixture copy + per-user
 * session); the create/rename/set-model/delete mutations mint fresh ids/stamps,
 * blanked by the harness.
 *
 * Run (Node 24, from the v4 checkout — mirror to /tmp; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}   # the v5 checkout (or your worktree)
 *   cd ~/source/quilltap-server
 *   mkdir -p /tmp/brahma-routes/cases /tmp/brahma-routes/fixtures
 *   cp $V5W/harness/oracle/cases/brahma-console-routes.test.ts /tmp/brahma-routes/cases/
 *   cp $V5W/harness/oracle/fixtures/brahma-console-web.json /tmp/brahma-routes/fixtures/
 *   QT_FIXTURE_BRAHMA_MAIN=$V5W/crates/quilltap-web/tests/fixtures/brahma-main.db \
 *   QT_FIXTURE_BRAHMA_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/brahma-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-brahma-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots /tmp/brahma-routes/cases -- brahma-console-routes
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

const P2 = 'c0000001-0000-4000-8000-000000000002';
const CHAT_A = 'c1000000-0000-4000-8000-00000000000a';
const CHAT_B = 'c1000000-0000-4000-8000-00000000000b';
const CHAT_C = 'c1000000-0000-4000-8000-00000000000c';
const CHAT_SALON = 'c1000000-0000-4000-8000-00000000000d';
const MISSING = '00000000-0000-4000-8000-0000000000ff';
const BASE = 'http://localhost/api/v1/brahma-console';

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'GET',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: async () => body ?? {},
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
  user?: 'A' | 'B';
  run: (routes: Routes) => Promise<{ status: number; body: unknown }>;
}

type RouteMod = Record<string, (...a: unknown[]) => Promise<unknown>>;
interface Routes {
  collection: RouteMod;
  item: RouteMod;
  messages: RouteMod;
}

const params = (id: string) => ({ params: Promise.resolve({ id }) });
const itemUrl = (id: string) => `${BASE}/${id}`;

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = (await r) as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

const CASES: CaseSpec[] = [
  { name: 'list', run: (r) => respond((r.collection as RouteMod).GET(mockRequest(BASE))) },
  { name: 'list_other_user', user: 'B', run: (r) => respond((r.collection as RouteMod).GET(mockRequest(BASE))) },
  { name: 'create_default', run: (r) => respond((r.collection as RouteMod).POST(mockRequest(BASE, {}))) },
  {
    name: 'create_with_profile',
    run: (r) => respond((r.collection as RouteMod).POST(mockRequest(BASE, { connectionProfileId: P2 }))),
  },
  {
    name: 'create_bad_profile',
    run: (r) => respond((r.collection as RouteMod).POST(mockRequest(BASE, { connectionProfileId: MISSING }))),
  },
  { name: 'get', run: (r) => respond(r.item.GET(mockRequest(itemUrl(CHAT_A)), params(CHAT_A))) },
  { name: 'get_salon_404', run: (r) => respond(r.item.GET(mockRequest(itemUrl(CHAT_SALON)), params(CHAT_SALON))) },
  { name: 'get_missing_404', run: (r) => respond(r.item.GET(mockRequest(itemUrl(MISSING)), params(MISSING))) },
  { name: 'get_other_user_404', user: 'B', run: (r) => respond(r.item.GET(mockRequest(itemUrl(CHAT_A)), params(CHAT_A))) },
  {
    name: 'rename',
    run: (r) => respond(r.item.PATCH(mockRequest(itemUrl(CHAT_B), { title: 'Renamed Console' }), params(CHAT_B))),
  },
  {
    name: 'set_model',
    run: (r) =>
      respond(
        r.item.PATCH(mockRequest(`${itemUrl(CHAT_A)}?action=set-model`, { connectionProfileId: P2 }), params(CHAT_A)),
      ),
  },
  {
    name: 'set_model_bad_profile',
    run: (r) =>
      respond(
        r.item.PATCH(
          mockRequest(`${itemUrl(CHAT_A)}?action=set-model`, { connectionProfileId: MISSING }),
          params(CHAT_A),
        ),
      ),
  },
  { name: 'delete', run: (r) => respond(r.item.DELETE(mockRequest(itemUrl(CHAT_C)), params(CHAT_C))) },
  { name: 'get_messages', run: (r) => respond(r.messages.GET(mockRequest(`${itemUrl(CHAT_A)}/messages`), params(CHAT_A))) },
  // -------------------------------------------------------------------------
  // P4.60 — the create / rename / set-model bodies. Same class as the send
  // body: every schema is `.parse`d UNCAUGHT, so a wrong-typed value is the
  // flat 400 `Validation error`; rename and set-model parse AFTER
  // `verifyBrahmaChat`, create parses BEFORE any lookup.
  // -------------------------------------------------------------------------
  {
    name: 'create_profile_wrong_type',
    run: (r) => respond((r.collection as RouteMod).POST(mockRequest(BASE, { connectionProfileId: 7 }))),
  },
  {
    name: 'create_profile_empty',
    run: (r) => respond((r.collection as RouteMod).POST(mockRequest(BASE, { connectionProfileId: '' }))),
  },
  {
    name: 'create_profile_null',
    run: (r) => respond((r.collection as RouteMod).POST(mockRequest(BASE, { connectionProfileId: null }))),
  },
  {
    name: 'rename_title_wrong_type',
    run: (r) => respond(r.item.PATCH(mockRequest(itemUrl(CHAT_B), { title: 5 }), params(CHAT_B))),
  },
  {
    name: 'rename_title_empty',
    run: (r) => respond(r.item.PATCH(mockRequest(itemUrl(CHAT_B), { title: '' }), params(CHAT_B))),
  },
  {
    name: 'rename_missing_chat_bad_body',
    run: (r) => respond(r.item.PATCH(mockRequest(itemUrl(MISSING), { title: 5 }), params(MISSING))),
  },
  {
    name: 'set_model_profile_wrong_type',
    run: (r) =>
      respond(
        r.item.PATCH(
          mockRequest(`${itemUrl(CHAT_A)}?action=set-model`, { connectionProfileId: 5 }),
          params(CHAT_A),
        ),
      ),
  },
  {
    name: 'set_model_profile_not_uuid',
    run: (r) =>
      respond(
        r.item.PATCH(
          mockRequest(`${itemUrl(CHAT_A)}?action=set-model`, { connectionProfileId: 'nope' }),
          params(CHAT_A),
        ),
      ),
  },
  // -------------------------------------------------------------------------
  // P4.60 — the send body's wrong-type-collapse arms. `handleSendMessage`
  // calls `sendMessageSchema.parse` UNCAUGHT, so every refusal is the flat 400
  // `Validation error` (the schema's own 'Message content is required' lives
  // only in the deferred `details`) — and it parses AFTER `verifyBrahmaChat`,
  // so a bad body on a chat that is not a Brahma console is a 404.
  // -------------------------------------------------------------------------
  ...([
    ['send_content_wrong_type', CHAT_A, { content: 123 }],
    ['send_content_empty', CHAT_A, { content: '' }],
    ['send_content_missing', CHAT_A, {}],
    ['send_file_ids_string', CHAT_A, { content: 'hi', fileIds: 'x' }],
    ['send_file_ids_bad_uuid', CHAT_A, { content: 'hi', fileIds: ['not-a-uuid'] }],
    ['send_file_ids_element_number', CHAT_A, { content: 'hi', fileIds: [1] }],
    ['send_file_ids_null', CHAT_A, { content: 'hi', fileIds: null }],
    // The guard-ORDER arms: the verify runs first, so these are 404s even
    // though the body would also have failed.
    ['send_missing_chat_bad_body', MISSING, { content: '' }],
    ['send_salon_chat_bad_body', CHAT_SALON, { content: 123 }],
  ] as Array<[string, string, unknown]>).map(([name, chat, body]) => ({
    name,
    run: (r: Routes) =>
      respond(r.messages.POST(mockRequest(`${itemUrl(chat)}/messages`, body), params(chat))),
  })),
];

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<{ status: number; body: unknown }> {
  jest.resetModules();
  const userId = c.user === 'B' ? spec.userIdB : spec.userId;
  applyMocks(spec, userId);
  const { initializePlugins } = await import('@/lib/startup/plugin-initialization');
  await initializePlugins();

  const work = mkdtempSync(join(scratch, 'brahma-'));
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

  const routes: Routes = {
    collection: (await import('@/app/api/v1/brahma-console/route')) as never,
    item: (await import('@/app/api/v1/brahma-console/[id]/route')) as never,
    messages: (await import('@/app/api/v1/brahma-console/[id]/messages/route')) as never,
  };

  const out = await c.run(routes);

  closeMountIndexSQLiteClient();
  await closeDatabase();
  rmSync(work, { recursive: true, force: true });
  return out;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'brahma-console-web.json'), 'utf8'),
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_BRAHMA_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_BRAHMA_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_BRAHMA_MAIN / QT_FIXTURE_BRAHMA_MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-brahma-routes-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const lines: string[] = [];
  for (const c of CASES) {
    const { status, body } = await runCase(spec, c, scratch, { main: fixtureMain, mount: fixtureMount });
    lines.push(JSON.stringify({ kind: 'case', name: c.name, status, body }));
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`brahma-console-routes oracle wrote ${outPath}\n`);
}

test('brahma-console-routes oracle', async () => {
  await main();
});
