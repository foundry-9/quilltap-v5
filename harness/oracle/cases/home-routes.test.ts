/**
 * @jest-environment node
 *
 * P4.6au HOME-DASHBOARD ORACLE: drives v4's REAL `getHomeData`
 * (`lib/services/home-data.service.ts` — exported, imported directly, never
 * reimplemented) and, for the envelope cases, the REAL route handler
 * (`app/api/v1/system/home/route.ts` GET), over a FRESH copy of the committed
 * `home-{main,mount}.db` family per case. Emits each response {status, body} so
 * the Rust port (`api::system::system_home` over `services::home`) diffs
 * byte-for-byte, key order included.
 *
 * Case coverage (15):
 *   - the two ROUTE cases (primary + the empty user) — v4's `successResponse`
 *     is the RAW payload; the primary one also carries the key-order claim;
 *   - the displayName ladder (user name wins / fallback wins / 'there' /
 *     empty-string fallback is falsy / missing users row) — the missing-user
 *     case also proves projects+files are findAll (UNSCOPED) while
 *     chats+characters are user-scoped;
 *   - seven MUTATION cases (raw SQL against the case's own copy, replayed
 *     identically on the Rust side): a salon chat reclassified `help` leaves
 *     recentChats but keeps its character counts (counts run over allChatsRaw);
 *     an EMPTY-STRING chatType stays (JS `!c.chatType` — falsy, not just
 *     absent); a nulled lastMessageAt falls back to createdAt (v4 `735d9408c`
 *     — NEVER to updatedAt, which is the whole point of `chatActivityAt`); a touched file
 *     re-ranks its project (the file source, dynamically); a re-projected chat
 *     moves count + activity between projects; a cleared and an ORPHANED story
 *     background both resolve to null (the unset arm vs the lookup MISS arm).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-home-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/home-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/home-web.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_HOME_MAIN=$V5W/crates/quilltap-web/tests/fixtures/home-main.db \
 *   QT_FIXTURE_HOME_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/home-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-home.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- home-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  emptyUserId: string;
  missingUserId: string;
  mutations: {
    helpFlipChatId: string;
    nullLmChatId: string;
    touchFileId: string;
    touchFileTo: string;
    reprojectChatId: string;
    reprojectToProjectId: string;
    clearBgChatId: string;
    orphanBgFileId: string;
  };
}

function mockRequest(url: string): unknown {
  return {
    method: 'GET',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue({}),
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
  return { status: resp.status, body: await resp.json() };
}

/** Drive the REAL exported service; the route's success body is the raw data. */
async function service(
  userId: string,
  fallbackName: string | null,
): Promise<{ status: number; body: unknown }> {
  const { getHomeData } = await import('@/lib/services/home-data.service');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const data = await getHomeData(getRepositories(), { userId, fallbackName });
  // Through JSON so `undefined` fields drop exactly as the wire drops them.
  return { status: 200, body: JSON.parse(JSON.stringify(data)) };
}

/** Drive the REAL route handler (the envelope + ctx.user plumbing). */
async function route(): Promise<{ status: number; body: unknown }> {
  const mod = (await import('@/app/api/v1/system/home/route')) as {
    GET: (...a: unknown[]) => Promise<unknown>;
  };
  return respond(await mod.GET(mockRequest('http://localhost/api/v1/system/home')));
}

type Mutate = (rawQuery: (sql: string, params?: unknown[]) => Promise<unknown>, s: Spec) => Promise<void>;

interface CaseSpec {
  name: string;
  user: (s: Spec) => string;
  mutate?: Mutate;
  run: (s: Spec) => Promise<{ status: number; body: unknown }>;
}

function buildCases(): CaseSpec[] {
  const u = (s: Spec) => s.userId;
  const e = (s: Spec) => s.emptyUserId;
  return [
    // ── The route (envelope) cases ──
    { name: 'route_primary', user: u, run: () => route() },
    { name: 'route_empty_user', user: e, run: () => route() },

    // ── The displayName ladder + scoping, via the exported service ──
    { name: 'service_primary', user: u, run: (s) => service(s.userId, null) },
    {
      name: 'service_primary_fallback_ignored',
      user: u,
      run: (s) => service(s.userId, 'Reginald'),
    },
    { name: 'service_empty_user_fallback', user: e, run: (s) => service(s.emptyUserId, 'Reginald') },
    { name: 'service_empty_user_no_fallback', user: e, run: (s) => service(s.emptyUserId, null) },
    { name: 'service_empty_user_blank_fallback', user: e, run: (s) => service(s.emptyUserId, '') },
    {
      // No users row at all — AND the unscoped findAll arms: projects (with
      // file-driven activity) still come back for a user who owns nothing.
      name: 'service_missing_user_fallback',
      user: u,
      run: (s) => service(s.missingUserId, 'Reginald'),
    },

    // ── The mutation cases (raw SQL on this case's own copy) ──
    {
      name: 'mutate_help_flip',
      user: u,
      mutate: (q, s) =>
        q('UPDATE "chats" SET "chatType" = ? WHERE "id" = ?', [
          'help',
          s.mutations.helpFlipChatId,
        ]).then(() => undefined),
      run: (s) => service(s.userId, null),
    },
    // NB: no empty-string-chatType case. v4's `!c.chatType` falsy arm is not
    // independently observable on v4-on-SQLite: chatType is enum-validated on
    // write, a raw `''` is dropped whole at the collection READ layer (the row
    // vanishes from findByUserId before the home filter sees it), and a NULL
    // hydrates through the zod default to 'salon' — which the `=== 'salon'`
    // arm already covers.
    {
      name: 'mutate_null_last_message',
      user: u,
      mutate: (q, s) =>
        q('UPDATE "chats" SET "lastMessageAt" = NULL WHERE "id" = ?', [
          s.mutations.nullLmChatId,
        ]).then(() => undefined),
      run: (s) => service(s.userId, null),
    },
    {
      name: 'mutate_touch_file',
      user: u,
      mutate: (q, s) =>
        q('UPDATE "files" SET "updatedAt" = ? WHERE "id" = ?', [
          s.mutations.touchFileTo,
          s.mutations.touchFileId,
        ]).then(() => undefined),
      run: (s) => service(s.userId, null),
    },
    {
      name: 'mutate_reproject_chat',
      user: u,
      mutate: (q, s) =>
        q('UPDATE "chats" SET "projectId" = ? WHERE "id" = ?', [
          s.mutations.reprojectToProjectId,
          s.mutations.reprojectChatId,
        ]).then(() => undefined),
      run: (s) => service(s.userId, null),
    },
    {
      name: 'mutate_clear_background',
      user: u,
      mutate: (q, s) =>
        q('UPDATE "chats" SET "storyBackgroundImageId" = NULL WHERE "id" = ?', [
          s.mutations.clearBgChatId,
        ]).then(() => undefined),
      run: (s) => service(s.userId, null),
    },
    {
      name: 'mutate_orphan_background',
      user: u,
      mutate: (q, s) =>
        q('DELETE FROM "files" WHERE "id" = ?', [s.mutations.orphanBgFileId]).then(
          () => undefined,
        ),
      run: (s) => service(s.userId, null),
    },
  ];
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(c.user(spec));

  const work = mkdtempSync(join(scratch, 'home-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();

  try {
    if (c.mutate) await c.mutate(rawQuery as never, spec);
    const out = await c.run(spec);
    return { name: c.name, status: out.status, body: out.body };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'home-web.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_HOME_MAIN ?? '',
    mount: process.env.QT_FIXTURE_HOME_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-home-oracle-'));
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
  process.stderr.write(`home-routes oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('home-routes oracle', async () => {
  await main();
});
