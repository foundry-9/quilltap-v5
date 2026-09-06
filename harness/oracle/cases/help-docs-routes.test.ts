/**
 * @jest-environment node
 *
 * P4.9I2A HELP-DOCS route-surface ORACLE: drives v4's REAL `app/api/v1/help-docs`
 * route handlers (`route.ts` GET — list / `?action=chat-count` / `?action=search`
 * / the default-serving unknown-action fallthrough; `[id]/route.ts` GET — by id,
 * by slug, 404) over the committed `help-chat-{main,mount}.db` fixture, mocking
 * ONLY the DB path + the session (per-case userId) + the startup gate. v4's
 * `HelpSearch` loads from the fixture's 17 synced `help_docs` rows (the ensure is
 * a no-op: disk and table agree — the scratch `help/` the fixture was built
 * from is NOT the cwd, so v4's `ensureHelpDocsSynced` sees the FULL shipped tree
 * and would SYNC IT; the mock below pins `ensureHelpDocsSynced` to a no-op so
 * the route reads the fixture's 17 rows, exactly as v5's per-call table read
 * does). Records `{ status, body }` per case.
 *
 * Run (Node 24, from the v4 checkout — mirror to /tmp; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   mkdir -p /tmp/help-docs-routes/cases /tmp/help-docs-routes/fixtures
 *   cp $V5W/harness/oracle/cases/help-docs-routes.test.ts /tmp/help-docs-routes/cases/
 *   cp $V5W/harness/oracle/fixtures/help-chat-web.json /tmp/help-docs-routes/fixtures/
 *   QT_FIXTURE_HELP_CHAT_MAIN=$V5W/crates/quilltap-web/tests/fixtures/help-chat-main.db \
 *   QT_FIXTURE_HELP_CHAT_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/help-chat-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-help-docs-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots /tmp/help-docs-routes/cases -- help-docs-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  users: Record<'A' | 'B' | 'C', { id: string }>;
}

const BASE = 'http://localhost/api/v1/help-docs';
// The fixture's minted id for help/brahma-console.md (help-chat-main.db.meta.json).
/**
 * The fixture's `help/brahma-console.md` row id — DERIVED from the committed
 * `<main>.meta.json` the builder writes beside the databases, never
 * transcribed. v4's real `syncHelpDocs()` mints doc ids with `randomUUID`, so
 * every fixture rebuild re-mints them; a literal here goes stale SILENTLY and
 * turns `get_by_id` into a 404-vs-404 agreement that passes while measuring
 * nothing (it did, at P4.D162's rebuild — caught by consequence).
 */
function brahmaDocId(): string {
  const metaPath = `${process.env.QT_FIXTURE_HELP_CHAT_MAIN}.meta.json`;
  const meta = JSON.parse(fs.readFileSync(metaPath, 'utf8')) as {
    helpDocs: { byPath: Record<string, { id: string }> };
  };
  const row = meta.helpDocs.byPath['help/brahma-console.md'];
  if (!row?.id) throw new Error(`no brahma-console doc id in ${metaPath}`);
  return row.id;
}

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
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories'),
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  // The fixture's help_docs ARE the corpus; v4's lazy ensure would otherwise
  // walk cwd/help (the full shipped tree) and re-sync 120 docs over them.
  jest.doMock('@/lib/help/help-doc-sync', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/help/help-doc-sync'),
    ensureHelpDocsSynced: async () => undefined,
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

type RouteMod = Record<string, (...a: unknown[]) => Promise<unknown>>;
interface Routes { collection: RouteMod; item: RouteMod }
interface CaseSpec { name: string; user?: 'A' | 'B' | 'C'; run: (r: Routes) => Promise<{ status: number; body: unknown }> }

const params = (id: string) => ({ params: Promise.resolve({ id }) });
async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = (await r) as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}
const search = (q: string) => `${BASE}?action=search&q=${encodeURIComponent(q)}`;

const CASES: CaseSpec[] = [
  { name: 'list', run: (r) => respond(r.collection.GET(mockRequest(BASE))) },
  { name: 'list_unknown_action_falls_through', run: (r) => respond(r.collection.GET(mockRequest(`${BASE}?action=bogus`))) },
  { name: 'list_empty_action_falls_through', run: (r) => respond(r.collection.GET(mockRequest(`${BASE}?action=`))) },
  { name: 'chat_count_user_a', run: (r) => respond(r.collection.GET(mockRequest(`${BASE}?action=chat-count`))) },
  { name: 'chat_count_user_c', user: 'C', run: (r) => respond(r.collection.GET(mockRequest(`${BASE}?action=chat-count`))) },
  { name: 'get_by_id', run: (r) => respond(r.item.GET(mockRequest(`${BASE}/${brahmaDocId()}`), params(brahmaDocId()))) },
  { name: 'get_by_slug', run: (r) => respond(r.item.GET(mockRequest(`${BASE}/brahma-console`), params('brahma-console'))) },
  { name: 'get_missing_404', run: (r) => respond(r.item.GET(mockRequest(`${BASE}/no-such-doc`), params('no-such-doc'))) },
  ...([
    ['search_title_and_content', 'Brahma'],
    ['search_content_only', 'wildcard'],
    ['search_case_insensitive', 'SALON'],
    ['search_none', 'zzqx-nothing-here'],
    ['search_one_char_short_circuit', 'a'],
    ['search_one_astral_char', '😀'],
    ['search_padded_trims', '  taboo  '],
    ['search_common_word_many_hits', 'the'],
    ['search_fenced_code_word', 'quilltap'],
    ['search_q_absent', null],
    ['search_q_empty', ''],
  ] as Array<[string, string | null]>).map(([name, q]) => ({
    name,
    run: (r: Routes) =>
      respond(r.collection.GET(mockRequest(q === null ? `${BASE}?action=search` : search(q)))),
  })),
];

async function runCase(spec: Spec, c: CaseSpec, scratch: string, fixtures: { main: string; mount: string }) {
  jest.resetModules();
  applyMocks(spec.users[c.user ?? 'A'].id);
  const { initializePlugins } = await import('@/lib/startup/plugin-initialization');
  await initializePlugins();
  const work = mkdtempSync(join(scratch, 'help-docs-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
  await initializeDatabase();
  const routes: Routes = {
    collection: (await import('@/app/api/v1/help-docs/route')) as never,
    item: (await import('@/app/api/v1/help-docs/[id]/route')) as never,
  };
  const out = await c.run(routes);
  closeMountIndexSQLiteClient();
  await closeDatabase();
  rmSync(work, { recursive: true, force: true });
  return out;
}

test('help-docs-routes oracle', async () => {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'help-chat-web.json'), 'utf8')) as Spec;
  const fixtureMain = process.env.QT_FIXTURE_HELP_CHAT_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_HELP_CHAT_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_HELP_CHAT_MAIN / QT_FIXTURE_HELP_CHAT_MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  const scratch = mkdtempSync(join(tmpdir(), 'qt-help-docs-routes-'));
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
  process.stderr.write(`help-docs-routes oracle wrote ${outPath} (${lines.length} cases)\n`);
});
