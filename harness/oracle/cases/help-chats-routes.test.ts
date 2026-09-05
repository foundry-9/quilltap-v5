/**
 * @jest-environment node
 *
 * P4.9I2A HELP-CHATS route-surface ORACLE: drives v4's REAL `app/api/v1/help-chats`
 * route handlers over the committed `help-chat-{main,mount}.db` fixture, mocking
 * ONLY the DB path + the session (per-case userId) + the startup gate:
 *
 *   - GET  /help-chats                      → handleList (per user: A/B/C)
 *   - GET  /help-chats?action=eligibility   → handleEligibility (A/B/C — the four arms)
 *   - GET  /help-chats?action=bogus         → the envelope 400
 *   - POST /help-chats                      → handleCreate (+ the Zod arms, the
 *     first-miss 404 BEFORE the help-enabled 400, the six-null echo, and the
 *     SYSTEM row it writes — dumped rowid-ordered as `messagesAfter`)
 *   - GET/PATCH/DELETE /help-chats/[id]     → get / rename / update-context /
 *     delete + verifyHelpChat's two 404 arms + verify-then-parse ordering
 *   - GET/POST /help-chats/[id]/messages    → messages + the send prologue's
 *     refusal arms (verify FIRST, then sendMessageSchema)
 *
 * Each case runs isolated (resetModules + a fresh fixture copy + per-user
 * session); minted ids/stamps are blanked by the harness. `messagesAfter` is
 * `getMessages(chatId)` projected to `[role, content]` in rowid order.
 *
 * Run (Node 24, from the v4 checkout — mirror to /tmp; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   mkdir -p /tmp/help-chats-routes/cases /tmp/help-chats-routes/fixtures
 *   cp $V5W/harness/oracle/cases/help-chats-routes.test.ts /tmp/help-chats-routes/cases/
 *   cp $V5W/harness/oracle/fixtures/help-chat-web.json /tmp/help-chats-routes/fixtures/
 *   QT_FIXTURE_HELP_CHAT_MAIN=$V5W/crates/quilltap-web/tests/fixtures/help-chat-main.db \
 *   QT_FIXTURE_HELP_CHAT_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/help-chat-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-help-chats-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots /tmp/help-chats-routes/cases -- help-chats-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec { testPepperBase64: string; users: Record<'A' | 'B' | 'C', { id: string }> }

const C1 = 'b0000002-0000-4000-8000-000000000001';
const C2 = 'b0000002-0000-4000-8000-000000000002';
const C3 = 'b0000002-0000-4000-8000-000000000003';
const H1 = 'c1000002-0000-4000-8000-000000000001';
const H2 = 'c1000002-0000-4000-8000-000000000002';
const H3 = 'c1000002-0000-4000-8000-000000000003';
const SALON = 'c1000002-0000-4000-8000-000000000031';
const MISSING = '00000000-0000-4000-8000-0000000000ff';
const BASE = 'http://localhost/api/v1/help-chats';

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'GET',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: async () => body ?? {},
  };
}

function applyMocks(userId: string): void {
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
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

type RouteMod = Record<string, (...a: unknown[]) => Promise<unknown>>;
interface Routes { collection: RouteMod; item: RouteMod; messages: RouteMod }
type Repos = { chats: { getMessages: (id: string) => Promise<Array<Record<string, unknown>>> } };
interface Out { status: number; body: unknown; messagesAfter?: Array<[string, string]> }
interface CaseSpec { name: string; user?: 'A' | 'B' | 'C'; run: (r: Routes, repos: Repos) => Promise<Out> }

const params = (id: string) => ({ params: Promise.resolve({ id }) });
const itemUrl = (id: string) => `${BASE}/${id}`;
async function respond(r: unknown): Promise<Out> {
  const resp = (await r) as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}
async function messagesOf(repos: Repos, chatId: string): Promise<Array<[string, string]>> {
  const rows = await repos.chats.getMessages(chatId);
  return rows.map((m) => [String(m.role ?? m.type), String(m.content ?? '')]);
}

const CASES: CaseSpec[] = [
  // --- list / eligibility (per user) + the envelope 400 ---
  { name: 'list_user_a', run: (r) => respond(r.collection.GET(mockRequest(BASE))) },
  { name: 'list_user_b', user: 'B', run: (r) => respond(r.collection.GET(mockRequest(BASE))) },
  { name: 'list_user_c_empty', user: 'C', run: (r) => respond(r.collection.GET(mockRequest(BASE))) },
  { name: 'list_empty_action_lists', run: (r) => respond(r.collection.GET(mockRequest(`${BASE}?action=`))) },
  { name: 'eligibility_user_a', run: (r) => respond(r.collection.GET(mockRequest(`${BASE}?action=eligibility`))) },
  { name: 'eligibility_user_b_no_tool_capable', user: 'B', run: (r) => respond(r.collection.GET(mockRequest(`${BASE}?action=eligibility`))) },
  { name: 'eligibility_user_c_no_help_chars', user: 'C', run: (r) => respond(r.collection.GET(mockRequest(`${BASE}?action=eligibility`))) },
  { name: 'get_unknown_action_400', run: (r) => respond(r.collection.GET(mockRequest(`${BASE}?action=bogus`))) },
  // --- create ---
  {
    name: 'create_two_characters',
    run: async (r, repos) => {
      const out = await respond(r.collection.POST(mockRequest(BASE, { characterIds: [C1, C2], pageUrl: '/salon/new-1' })));
      const id = (out.body as { chat?: { id?: string } }).chat?.id;
      if (id) out.messagesAfter = await messagesOf(repos, id);
      return out;
    },
  },
  {
    name: 'create_help_disabled_first_then_enabled',
    run: async (r, repos) => {
      // C3 (help off) first, C1 (help on) second — still creates; title from C3.
      const out = await respond(r.collection.POST(mockRequest(BASE, { characterIds: [C3, C1], pageUrl: '' })));
      const id = (out.body as { chat?: { id?: string } }).chat?.id;
      if (id) out.messagesAfter = await messagesOf(repos, id);
      return out;
    },
  },
  { name: 'create_none_help_enabled_400', run: (r) => respond(r.collection.POST(mockRequest(BASE, { characterIds: [C3], pageUrl: '/x' }))) },
  // The guard-ORDER arm: the missing id sits FIRST, before a help-enabled one.
  { name: 'create_first_missing_404_before_help_check', run: (r) => respond(r.collection.POST(mockRequest(BASE, { characterIds: [MISSING, C1], pageUrl: '/x' }))) },
  { name: 'create_missing_after_valid_404', run: (r) => respond(r.collection.POST(mockRequest(BASE, { characterIds: [C1, MISSING], pageUrl: '/x' }))) },
  // THE order-measuring row: a missing id + a help-DISABLED one. v4's loop 404s
  // on the miss before it ever evaluates "none help-enabled"; an inverted order
  // answers the 400. (With a help-ENABLED partner both orders answer 404.)
  { name: 'create_missing_and_help_disabled_404_not_400', run: (r) => respond(r.collection.POST(mockRequest(BASE, { characterIds: [MISSING, C3], pageUrl: '/x' }))) },
  { name: 'create_empty_ids_400', run: (r) => respond(r.collection.POST(mockRequest(BASE, { characterIds: [], pageUrl: '/x' }))) },
  { name: 'create_non_uuid_400', run: (r) => respond(r.collection.POST(mockRequest(BASE, { characterIds: ['nope'], pageUrl: '/x' }))) },
  { name: 'create_ids_wrong_type_400', run: (r) => respond(r.collection.POST(mockRequest(BASE, { characterIds: C1, pageUrl: '/x' }))) },
  { name: 'create_page_url_missing_400', run: (r) => respond(r.collection.POST(mockRequest(BASE, { characterIds: [C1] }))) },
  { name: 'create_page_url_wrong_type_400', run: (r) => respond(r.collection.POST(mockRequest(BASE, { characterIds: [C1], pageUrl: 7 }))) },
  { name: 'create_body_empty_400', run: (r) => respond(r.collection.POST(mockRequest(BASE, {}))) },
  // --- get ---
  { name: 'get_h1', run: (r) => respond(r.item.GET(mockRequest(itemUrl(H1)), params(H1))) },
  { name: 'get_h2_null_page_url', run: (r) => respond(r.item.GET(mockRequest(itemUrl(H2)), params(H2))) },
  { name: 'get_salon_404', run: (r) => respond(r.item.GET(mockRequest(itemUrl(SALON)), params(SALON))) },
  { name: 'get_missing_404', run: (r) => respond(r.item.GET(mockRequest(itemUrl(MISSING)), params(MISSING))) },
  // v4's verifyHelpChat has NO userId gate: user B reads A's chat.
  { name: 'get_other_user_still_200', user: 'B', run: (r) => respond(r.item.GET(mockRequest(itemUrl(H1)), params(H1))) },
  // --- rename ---
  { name: 'rename', run: (r) => respond(r.item.PATCH(mockRequest(itemUrl(H2), { title: 'Renamed Help' }), params(H2))) },
  { name: 'rename_empty_title_400', run: (r) => respond(r.item.PATCH(mockRequest(itemUrl(H2), { title: '' }), params(H2))) },
  { name: 'rename_wrong_type_400', run: (r) => respond(r.item.PATCH(mockRequest(itemUrl(H2), { title: 5 }), params(H2))) },
  { name: 'rename_missing_chat_bad_body_404', run: (r) => respond(r.item.PATCH(mockRequest(itemUrl(MISSING), { title: 5 }), params(MISSING))) },
  { name: 'rename_salon_bad_body_404', run: (r) => respond(r.item.PATCH(mockRequest(itemUrl(SALON), { title: '' }), params(SALON))) },
  { name: 'patch_empty_action_renames', run: (r) => respond(r.item.PATCH(mockRequest(`${itemUrl(H3)}?action=`, { title: 'Via empty action' }), params(H3))) },
  // --- update-context ---
  {
    name: 'update_context',
    run: async (r, repos) => {
      const out = await respond(r.item.PATCH(mockRequest(`${itemUrl(H2)}?action=update-context`, { pageUrl: '/files' }), params(H2)));
      out.messagesAfter = await messagesOf(repos, H2);
      return out;
    },
  },
  { name: 'update_context_empty_400', run: (r) => respond(r.item.PATCH(mockRequest(`${itemUrl(H2)}?action=update-context`, { pageUrl: '' }), params(H2))) },
  { name: 'update_context_wrong_type_400', run: (r) => respond(r.item.PATCH(mockRequest(`${itemUrl(H2)}?action=update-context`, { pageUrl: 1 }), params(H2))) },
  { name: 'update_context_missing_chat_404', run: (r) => respond(r.item.PATCH(mockRequest(`${itemUrl(MISSING)}?action=update-context`, { pageUrl: '' }), params(MISSING))) },
  { name: 'patch_unknown_action_400', run: (r) => respond(r.item.PATCH(mockRequest(`${itemUrl(H2)}?action=bogus`, { title: 'x' }), params(H2))) },
  // --- delete ---
  { name: 'delete_h3', run: (r) => respond(r.item.DELETE(mockRequest(itemUrl(H3)), params(H3))) },
  { name: 'delete_missing_404', run: (r) => respond(r.item.DELETE(mockRequest(itemUrl(MISSING)), params(MISSING))) },
  { name: 'delete_salon_404', run: (r) => respond(r.item.DELETE(mockRequest(itemUrl(SALON)), params(SALON))) },
  // --- messages ---
  { name: 'messages_h1', run: (r) => respond(r.messages.GET(mockRequest(`${itemUrl(H1)}/messages`), params(H1))) },
  { name: 'messages_salon_404', run: (r) => respond(r.messages.GET(mockRequest(`${itemUrl(SALON)}/messages`), params(SALON))) },
  // --- the send prologue: verify FIRST, then sendMessageSchema (refusal arms only) ---
  ...([
    ['send_content_wrong_type', H1, { content: 123 }],
    ['send_content_empty', H1, { content: '' }],
    ['send_content_missing', H1, {}],
    ['send_file_ids_string', H1, { content: 'hi', fileIds: 'x' }],
    ['send_file_ids_bad_uuid', H1, { content: 'hi', fileIds: ['not-a-uuid'] }],
    ['send_file_ids_null', H1, { content: 'hi', fileIds: null }],
    ['send_missing_chat_bad_body', MISSING, { content: '' }],
    ['send_salon_chat_bad_body', SALON, { content: 123 }],
  ] as Array<[string, string, unknown]>).map(([name, chat, body]) => ({
    name,
    run: (r: Routes) => respond(r.messages.POST(mockRequest(`${itemUrl(chat)}/messages`, body), params(chat))),
  })),
];

async function runCase(spec: Spec, c: CaseSpec, scratch: string, fixtures: { main: string; mount: string }): Promise<Out> {
  jest.resetModules();
  applyMocks(spec.users[c.user ?? 'A'].id);
  const { initializePlugins } = await import('@/lib/startup/plugin-initialization');
  await initializePlugins();
  const work = mkdtempSync(join(scratch, 'help-chats-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
  const { getRepositories } = await import('@/lib/repositories/factory');
  await initializeDatabase();
  const routes: Routes = {
    collection: (await import('@/app/api/v1/help-chats/route')) as never,
    item: (await import('@/app/api/v1/help-chats/[id]/route')) as never,
    messages: (await import('@/app/api/v1/help-chats/[id]/messages/route')) as never,
  };
  const out = await c.run(routes, getRepositories() as unknown as Repos);
  closeMountIndexSQLiteClient();
  await closeDatabase();
  rmSync(work, { recursive: true, force: true });
  return out;
}

test('help-chats-routes oracle', async () => {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'help-chat-web.json'), 'utf8')) as Spec;
  const fixtureMain = process.env.QT_FIXTURE_HELP_CHAT_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_HELP_CHAT_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_HELP_CHAT_MAIN / QT_FIXTURE_HELP_CHAT_MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  const scratch = mkdtempSync(join(tmpdir(), 'qt-help-chats-routes-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';
  const lines: string[] = [];
  for (const c of CASES) {
    const out = await runCase(spec, c, scratch, { main: fixtureMain, mount: fixtureMount });
    lines.push(JSON.stringify({ kind: 'case', name: c.name, ...out }));
  }
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`help-chats-routes oracle wrote ${outPath} (${lines.length} cases)\n`);
});
