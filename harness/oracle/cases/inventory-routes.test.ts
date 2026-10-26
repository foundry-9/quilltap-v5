/**
 * @jest-environment node
 *
 * P4.D325 INVENTORY-ROUTES ORACLE: drives v4's REAL route handlers at the
 * `322a5488e` pin — the route factory through its FOUR real configs (the eight
 * thin files under `app/api/v1/{inventory,characters/[id]/inventory,
 * groups/[id]/inventory,projects/[id]/inventory}/**`), the transfers route
 * (`app/api/v1/inventory/transfers/route.ts`), and the chat actions (`actions/
 * inventory.ts` — the POSTs through `handlers/post.ts`'s `handlePost`, the chat
 * gate first) — each wrapped in v4's real context
 * middleware, so a ZodError is its `Validation error` 400 and any other throw
 * its 500. Each case runs over a FRESH copy of the committed
 * `inventory-routes-{main,mount}.db` pair; emits `{name, status, body}`.
 *
 * THE CASE LIST IS NOT DECLARED HERE: both this oracle and the Rust family
 * (`crates/quilltap-web/tests/inventory_routes_equivalence.rs`) read the SAME
 * `inventory-routes.json#cases` corpus (the shared-corpus rule); the Rust side
 * asserts its row count against this NDJSON's.
 *
 * Chained rows: `dump` emits `${name}__dump` — the store after the case
 * (`doc_mount_points` count, every `Inventory/` file's path + content, the
 * `Inventory` folders, and for `dump.chat` that chat's `equippedInventory`, its
 * `chat_messages` count and the `background_jobs` count — the last two are the
 * effects P4.D325's branch carves until the union); `thenRead: <chatId>` emits
 * `${name}__read` — the chat's `?action=inventory` on the SAME copy
 * (persistence, not just the response). `mainSql` runs raw SQL on the main
 * copy before the database opens (both sides) — how General is made
 * unprovisioned (v4's `projects.create` / `groups.create` always provision a
 * store, so the "owner exists, no store" probe arm is reachable only through
 * General's).
 *
 * v4 runs WITHOUT its title lint (`7263105c6` — P4.D340's): the two lint
 * modules are stubbed exactly as the keystone's tier-2 oracle stubs them, so
 * the mutations beneath the routes are the keystone's (which ports them
 * without the lint calls).
 *
 * jest.setup's global mocks of the vault bridge, the vector store and the
 * file-storage manager are UNDONE (memory note `shared-fixture-rebuilds-and-
 * jest-setup-mocks`) — every character-vault write is real.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror because jest
 * ignores .claude/ paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-iroutes-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures" "$TMPO/lib"
 *   cp "$V5W/harness/oracle/cases/inventory-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/lib/v4-migrations.ts" "$TMPO/lib/"
 *   cp "$V5W/harness/oracle/fixtures/inventory-routes.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_IROUTES_MAIN=$V5W/crates/quilltap-web/tests/fixtures/inventory-routes-main.db \
 *   QT_FIXTURE_IROUTES_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/inventory-routes-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-inventory-routes.ndjson TZ=UTC \
 *     $N/npx jest --silent --watchman=false --testTimeout=240000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- inventory-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CaseEntry {
  name: string;
  kind: string;
  scope?: string;
  id?: string;
  itemId?: string;
  chatId?: string;
  action?: string;
  query?: Array<[string, string]>;
  body?: unknown;
  rawBody?: string;
  dump?: boolean | { chat: string };
  thenRead?: string;
  /** Raw SQL on the main copy before the database initializes (both sides). */
  mainSql?: string[];
}

interface Spec {
  testPepperBase64: string;
  userId: string;
  cases: CaseEntry[];
}

const B = 'http://localhost/api/v1';

function mockRequest(url: string, method: string, body?: unknown, rawBody?: string): unknown {
  return {
    method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: async () => (rawBody !== undefined ? JSON.parse(rawBody) : body ?? {}),
    text: async () => (rawBody !== undefined ? rawBody : JSON.stringify(body ?? {})),
  };
}

function applyMocks(spec: Spec): void {
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  for (const p of [
    '@/lib/database/manager',
    '@/lib/database/repositories',
    '@/lib/repositories/factory',
    '@/lib/file-storage/character-vault-bridge',
    '@/lib/embedding/vector-store',
    '@/lib/file-storage/manager',
  ]) {
    jest.doMock(p, () => jest.requireActual(p));
  }
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
  // The title lint is P4.D340's — the keystone's mutations run without it.
  jest.doMock('@/lib/inventory/title-lint', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/inventory/title-lint'),
    lintInventoryTitle: async () => ({ rows: [] }),
    writeTitleLintRows: () => undefined,
  }));
  jest.doMock('@/lib/inventory/title-lint-documents', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/inventory/title-lint-documents'),
    lintInventoryDocumentChange: async () => () => undefined,
  }));
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

/** The query string both sides build: `action` first, then the case's pairs. */
function queryString(c: CaseEntry): string {
  const params = new URLSearchParams();
  if (c.action !== undefined) params.append('action', c.action);
  for (const [k, v] of c.query ?? []) params.append(k, v);
  const qs = params.toString();
  return qs ? `?${qs}` : '';
}

function tierPath(c: CaseEntry): { path: string; dir: string } {
  switch (c.scope) {
    case 'general':
      return { path: 'inventory', dir: 'inventory' };
    case 'character':
      return { path: `characters/${c.id}/inventory`, dir: 'characters/[id]/inventory' };
    case 'group':
      return { path: `groups/${c.id}/inventory`, dir: 'groups/[id]/inventory' };
    case 'project':
      return { path: `projects/${c.id}/inventory`, dir: 'projects/[id]/inventory' };
    default:
      throw new Error(`unknown scope ${c.scope}`);
  }
}

type Handler = (...a: unknown[]) => Promise<unknown>;

async function routeModule(dir: string): Promise<Record<string, Handler>> {
  // Static specifiers (jest resolves `@/` through its moduleNameMapper).
  switch (dir) {
    case 'inventory':
      return (await import('@/app/api/v1/inventory/route')) as never;
    case 'inventory/[itemId]':
      return (await import('@/app/api/v1/inventory/[itemId]/route')) as never;
    case 'characters/[id]/inventory':
      return (await import('@/app/api/v1/characters/[id]/inventory/route')) as never;
    case 'characters/[id]/inventory/[itemId]':
      return (await import('@/app/api/v1/characters/[id]/inventory/[itemId]/route')) as never;
    case 'groups/[id]/inventory':
      return (await import('@/app/api/v1/groups/[id]/inventory/route')) as never;
    case 'groups/[id]/inventory/[itemId]':
      return (await import('@/app/api/v1/groups/[id]/inventory/[itemId]/route')) as never;
    case 'projects/[id]/inventory':
      return (await import('@/app/api/v1/projects/[id]/inventory/route')) as never;
    case 'projects/[id]/inventory/[itemId]':
      return (await import('@/app/api/v1/projects/[id]/inventory/[itemId]/route')) as never;
    default:
      throw new Error(`no route module for ${dir}`);
  }
}

/**
 * The chat route's own wrapping — v4's real context middleware
 * (`createContextParamsHandler`) around `handleGetInventory` / `handlePost`.
 * (`app/api/v1/chats/[id]/route.ts` itself cannot load under jest: its GET
 * handler's graph reaches an ESM-only markdown import.)
 */
async function chatGetInventory(chatId: string): Promise<{ status: number; body: unknown }> {
  const { createContextParamsHandler } = (await import('@/lib/api/middleware')) as {
    createContextParamsHandler: (h: Handler) => Handler;
  };
  const { handleGetInventory } = (await import('@/app/api/v1/chats/[id]/actions/inventory')) as Record<
    string,
    Handler
  >;
  const GET = createContextParamsHandler(async (_req, ctx, params) =>
    handleGetInventory((params as { id: string }).id, ctx),
  );
  return respond(
    await GET(mockRequest(`${B}/chats/${chatId}?action=inventory`, 'GET'), {
      params: Promise.resolve({ id: chatId }),
    }),
  );
}

async function runKind(c: CaseEntry): Promise<{ status: number; body: unknown }> {
  const qs = queryString(c);
  switch (c.kind) {
    case 'collectionGet':
    case 'collectionPost': {
      const { path, dir } = tierPath(c);
      const mod = await routeModule(dir);
      const method = c.kind === 'collectionGet' ? 'GET' : 'POST';
      const req = mockRequest(`${B}/${path}${qs}`, method, c.body, c.rawBody);
      const handler = mod[method];
      return respond(
        c.scope === 'general' ? await handler(req) : await handler(req, { params: Promise.resolve({ id: c.id }) }),
      );
    }
    case 'itemGet':
    case 'itemPut':
    case 'itemDelete': {
      const { path, dir } = tierPath(c);
      const mod = await routeModule(`${dir}/[itemId]`);
      const method = c.kind === 'itemGet' ? 'GET' : c.kind === 'itemPut' ? 'PUT' : 'DELETE';
      const req = mockRequest(`${B}/${path}/${c.itemId}${qs}`, method, c.body, c.rawBody);
      const params = c.scope === 'general' ? { itemId: c.itemId } : { id: c.id, itemId: c.itemId };
      return respond(await mod[method](req, { params: Promise.resolve(params) }));
    }
    case 'transfer': {
      const mod = (await import('@/app/api/v1/inventory/transfers/route')) as Record<string, Handler>;
      return respond(await mod.POST(mockRequest(`${B}/inventory/transfers`, 'POST', c.body, c.rawBody)));
    }
    case 'chatGet':
      return chatGetInventory(c.chatId!);
    case 'chatPost': {
      const { createContextParamsHandler } = (await import('@/lib/api/middleware')) as {
        createContextParamsHandler: (h: Handler) => Handler;
      };
      const { handlePost } = (await import('@/app/api/v1/chats/[id]/handlers/post')) as Record<string, Handler>;
      const POST = createContextParamsHandler(async (req, ctx, params) =>
        handlePost(req, ctx, (params as { id: string }).id),
      );
      return respond(
        await POST(mockRequest(`${B}/chats/${c.chatId}${qs}`, 'POST', c.body, c.rawBody), {
          params: Promise.resolve({ id: c.chatId }),
        }),
      );
    }
    default:
      throw new Error(`unknown case kind: ${c.kind}`);
  }
}

/** The store after a case — both sides dump the same projection. */
async function dumpStore(c: CaseEntry): Promise<Record<string, unknown>> {
  const { rawQuery } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase } = await import('@/lib/database/backends/sqlite/mount-index-client');
  const mi = getRawMountIndexDatabase();
  if (!mi) throw new Error('mount-index handle unavailable');
  const out: Record<string, unknown> = {};
  out.mountPoints = (mi.prepare('SELECT COUNT(*) AS n FROM doc_mount_points').get() as { n: number }).n;
  out.docs = (
    mi
      .prepare(
        `SELECT l."mountPointId" AS mp, l."relativePath" AS path, d."content" AS content
           FROM doc_mount_file_links l JOIN doc_mount_documents d ON d."fileId" = l."fileId"
          WHERE l."relativePath" LIKE 'Inventory/%' ORDER BY l."mountPointId", l."relativePath"`,
      )
      .all() as Array<{ mp: string; path: string; content: string }>
  ).map((r) => [r.mp, r.path, r.content]);
  out.folders = (
    mi
      .prepare(
        `SELECT "mountPointId" AS mp, "path" AS path FROM doc_mount_folders
          WHERE "path" = 'Inventory' OR "path" LIKE 'Inventory/%' ORDER BY "mountPointId", "path"`,
      )
      .all() as Array<{ mp: string; path: string }>
  ).map((r) => [r.mp, r.path]);
  if (typeof c.dump === 'object' && c.dump.chat) {
    const chat = (await rawQuery('SELECT "equippedInventory" AS e FROM chats WHERE id = ?', [c.dump.chat])) as Array<{
      e: string | null;
    }>;
    out.equipped = chat[0]?.e ?? null;
    // Either table may be absent on the pair (each is created lazily) — a
    // count of an absent table is `null`, on both sides.
    const countOf = async (table: string, where: string, params: unknown[]): Promise<number | null> => {
      const present = (await rawQuery('SELECT COUNT(*) AS n FROM sqlite_master WHERE type = ? AND name = ?', [
        'table',
        table,
      ])) as Array<{ n: number }>;
      if (!present[0].n) return null;
      return ((await rawQuery(`SELECT COUNT(*) AS n FROM "${table}"${where}`, params)) as Array<{ n: number }>)[0].n;
    };
    out.messages = await countOf('chat_messages', ' WHERE "chatId" = ?', [c.dump.chat]);
    out.jobs = await countOf('background_jobs', '', []);
  }
  return out;
}

async function runCase(
  spec: Spec,
  c: CaseEntry,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Array<Record<string, unknown>>> {
  jest.resetModules();
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'iroutes-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  if (c.mainSql?.length) {
    const nodeRequire = require('node:module').createRequire(join(process.cwd(), 'noop.js'));
    const Database = nodeRequire(join(process.cwd(), 'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'));
    const conn = new Database(mainWork);
    conn.pragma(`key = "x'${Buffer.from(spec.testPepperBase64, 'base64').toString('hex')}'"`);
    try {
      for (const sql of c.mainSql) conn.exec(sql);
    } finally {
      conn.close();
    }
  }
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { closeDatabase, initializeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
  await initializeDatabase();
  const { plantChatWardrobeHistoryTable } = await import('../lib/v4-migrations');
  await plantChatWardrobeHistoryTable();

  try {
    const rows: Array<Record<string, unknown>> = [];
    const out = await runKind(c);
    rows.push({ name: c.name, status: out.status, body: out.body });
    // Drain the fire-and-forget tails (the effects' avatar refresh) first.
    await new Promise((r) => setTimeout(r, 250));
    if (c.thenRead) {
      const follow = await chatGetInventory(c.thenRead);
      rows.push({ name: `${c.name}__read`, status: follow.status, body: follow.body });
    }
    if (c.dump) {
      rows.push({ name: `${c.name}__dump`, status: 200, body: await dumpStore(c) });
    }
    return rows;
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'inventory-routes.json'), 'utf8')) as Spec;
  const fixtures = {
    main: process.env.QT_FIXTURE_IROUTES_MAIN ?? '',
    mount: process.env.QT_FIXTURE_IROUTES_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-iroutes-oracle-'));
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const only = process.env.QT_IROUTES_ONLY?.split(',');
  const outLines: string[] = [];
  for (const c of spec.cases) {
    if (only && !only.includes(c.name)) continue;
    const payloads = await runCase(spec, c, scratch, fixtures);
    for (const p of payloads) outLines.push(JSON.stringify(p));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`inventory-routes oracle wrote ${outPath} (${outLines.length} rows from ${spec.cases.length} cases)\n`);
}

test('inventory-routes oracle', async () => {
  await main();
});

const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
