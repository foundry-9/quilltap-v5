/**
 * @jest-environment node
 *
 * P4.6ar LLM-LOGS route-surface ORACLE: drives v4's REAL route handlers over a
 * FRESH copy of the committed `inspector-*` fixture family per case, and emits
 * each response {status, body} — so the Rust ports (`api::llm_logs::*`, over the
 * `db::llm_logs` reads) diff byte-for-byte.
 *
 * ── THE THREE-PARTITION MACHINERY ────────────────────────────────────────────
 * The logs live in v4's dedicated llm-logs database, so each case copies THREE
 * fixture files and points `SQLITE_PATH` / `SQLITE_MOUNT_INDEX_PATH` /
 * `SQLITE_LLM_LOGS_PATH` at the copies. `closeDatabase()` closes the llm-logs
 * client too (backend.ts calls closeLLMLogsSQLiteClient), so one close flushes
 * everything.
 *
 * Case coverage (24): every list branch (message / chat / chat+includeMessages /
 * character / standalone / type / recent) including branch PRECEDENCE, both
 * includeMessages defaults, the clamp above cap, a garbage limit (the NaN
 * no-slice quirk + `limit: null` on the wire), a zero limit, a negative limit
 * (JS `slice(0, -n)` drops from the TAIL — reachable and faithful), offset,
 * offset+limit, the chat/character notFound arms, unknown message/type ids, the
 * item GET found/missing, the item DELETE found/missing, and the CROSS-USER item
 * read+delete that prove the item routes have no ownership check.
 *
 * `list_standalone` is expected to come back EMPTY — v4's `$eq: null` lowers to
 * `col = NULL`, which matches nothing. That is the assertion, not a bug in the
 * fixture: the corpus carries five genuinely standalone rows.
 *
 * The mutating DELETE cases re-GET the same id afterwards and emit the resulting
 * status as `extra.afterStatus`, so the diff sees the DB effect and not just the
 * ack.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-llmlogs-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/llm-logs-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/inspector-web.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_INSP_MAIN=$V5W/crates/quilltap-web/tests/fixtures/inspector-main.db \
 *   QT_FIXTURE_INSP_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/inspector-mount.db \
 *   QT_FIXTURE_INSP_LLM=$V5W/crates/quilltap-web/tests/fixtures/inspector-llm.db \
 *   QT_ORACLE_OUT=/tmp/oracle-llm-logs.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- llm-logs-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  otherUserId: string;
  characterId: string;
  chatId: string;
  missingId: string;
  missingChatId: string;
  missingCharacterId: string;
  missingMessageId: string;
  messages: Array<{ id: string }>;
  logs: Array<{ id: string; type: string; user: string }>;
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

async function loadRoute(
  path: string,
): Promise<Record<string, (...a: unknown[]) => Promise<unknown>>> {
  return (await import(path)) as never;
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  if (resp.status === 204) return { status: 204, body: null };
  return { status: resp.status, body: await resp.json() };
}

const LIST_ROUTE = '@/app/api/v1/llm-logs/route';
const ITEM_ROUTE = '@/app/api/v1/llm-logs/[id]/route';
const B = 'http://localhost/api/v1';

const list = async (query: string) =>
  respond(await (await loadRoute(LIST_ROUTE)).GET(mockRequest(`${B}/llm-logs${query}`)));
const itemGet = async (id: string) =>
  respond(
    await (await loadRoute(ITEM_ROUTE)).GET(mockRequest(`${B}/llm-logs/${id}`), {
      params: Promise.resolve({ id }),
    }),
  );
const itemDelete = async (id: string) =>
  respond(
    await (await loadRoute(ITEM_ROUTE)).DELETE(mockRequest(`${B}/llm-logs/${id}`), {
      params: Promise.resolve({ id }),
    }),
  );

interface CaseSpec {
  name: string;
  /** Which fixture user signs in (defaults to the primary). */
  user?: (s: Spec) => string;
  blank?: string[];
  run: (s: Spec) => Promise<{ status: number; body: unknown; extra?: unknown }>;
}

function buildCases(): CaseSpec[] {
  return [
    // ── The default (recent) branch + the pagination arithmetic ──
    { name: 'list_recent_default', run: () => list('') },
    { name: 'list_recent_limit_2', run: () => list('?limit=2') },
    {
      // Math.min(999, 100) → 100; the corpus is smaller, so nothing is sliced,
      // but the echoed `limit` proves the clamp.
      name: 'list_recent_limit_above_cap',
      run: () => list('?limit=999'),
    },
    {
      // THE NaN QUIRK: parseInt('garbage') → NaN; Math.min(NaN, 100) → NaN;
      // `logs.length > NaN` → false, so NOTHING is sliced and the wire carries
      // `"limit": null` (JSON.stringify(NaN)).
      name: 'list_recent_garbage_limit',
      run: () => list('?limit=garbage'),
    },
    {
      // limit=0 → the repo emits no LIMIT (translatePagination guards `> 0`),
      // and the route's `length > 0` slice(0,0) empties the page: count 0 with
      // a non-zero total.
      name: 'list_recent_zero_limit',
      run: () => list('?limit=0'),
    },
    {
      // limit=-3 → no repo LIMIT; `length > -3` is true → slice(0, -3) drops the
      // last THREE rows. Reachable, faithful, and worth pinning.
      name: 'list_recent_negative_limit',
      run: () => list('?limit=-3'),
    },
    { name: 'list_recent_offset', run: () => list('?offset=3') },
    { name: 'list_recent_offset_and_limit', run: () => list('?offset=2&limit=3') },
    {
      // A garbage offset is NaN too: `offset > 0` is false, so no slice, and the
      // echoed `offset` is null.
      name: 'list_recent_garbage_offset',
      run: () => list('?offset=nope'),
    },

    // ── The entity branches ──
    { name: 'list_by_message', run: (s) => list(`?messageId=${s.messages[1].id}`) },
    { name: 'list_by_message_unknown', run: (s) => list(`?messageId=${s.missingMessageId}`) },
    { name: 'list_by_chat', run: (s) => list(`?chatId=${s.chatId}`) },
    {
      // The union arm: chat-linked logs OR logs linked via any of the chat's
      // message ids. The default limit becomes 500 and the cap 500.
      name: 'list_by_chat_include_messages',
      run: (s) => list(`?chatId=${s.chatId}&includeMessages=true`),
    },
    {
      // `includeMessages` is a strict `=== 'true'` compare — `TRUE` is NOT true,
      // so this must take the plain findByChatId branch AND the 50/100 defaults.
      name: 'list_by_chat_include_messages_not_exact',
      run: (s) => list(`?chatId=${s.chatId}&includeMessages=TRUE`),
    },
    { name: 'list_by_chat_missing', run: (s) => list(`?chatId=${s.missingChatId}`) },
    { name: 'list_by_character', run: (s) => list(`?characterId=${s.characterId}`) },
    {
      name: 'list_by_character_missing',
      run: (s) => list(`?characterId=${s.missingCharacterId}`),
    },
    {
      // Branch PRECEDENCE: messageId wins over chatId and characterId, and the
      // (missing) chat id never reaches its 404 check.
      name: 'list_branch_precedence_message_wins',
      run: (s) =>
        list(
          `?messageId=${s.messages[1].id}&chatId=${s.missingChatId}&characterId=${s.missingCharacterId}`,
        ),
    },

    // ── standalone / type ──
    {
      // ALWAYS EMPTY on v4-on-SQLite: the filter's `$eq: null` lowers to
      // `"messageId" = NULL`, which is UNKNOWN for every row. The corpus has five
      // genuinely standalone rows; none can be returned.
      name: 'list_standalone',
      run: () => list('?standalone=true'),
    },
    { name: 'list_by_type', run: () => list('?type=CHAT_MESSAGE') },
    {
      // The route CASTS `type` without validating it — an unknown string reaches
      // findByType and matches nothing (no 400).
      name: 'list_by_type_unknown',
      run: () => list('?type=NOT_A_REAL_TYPE'),
    },

    // ── The item routes ──
    { name: 'item_get_found', run: (s) => itemGet(s.logs[0].id) },
    { name: 'item_get_missing', run: (s) => itemGet(s.missingId) },
    {
      // NO OWNERSHIP CHECK: the primary user reads the OTHER user's log, and gets
      // a 200. (The log is `logs[13]`, the one seeded under otherUserId.)
      name: 'item_get_other_user',
      run: (s) => itemGet(s.logs[s.logs.length - 1].id),
    },
    {
      name: 'item_delete_found',
      run: async (s) => {
        const del = await itemDelete(s.logs[0].id);
        const after = await itemGet(s.logs[0].id);
        return { ...del, extra: { afterStatus: after.status } };
      },
    },
    { name: 'item_delete_missing', run: (s) => itemDelete(s.missingId) },
    {
      // The delete sibling of the no-ownership-check read.
      name: 'item_delete_other_user',
      run: async (s) => {
        const id = s.logs[s.logs.length - 1].id;
        const del = await itemDelete(id);
        const after = await itemGet(id);
        return { ...del, extra: { afterStatus: after.status } };
      },
    },
  ];
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string; llm: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(c.user ? c.user(spec) : spec.userId);

  const work = mkdtempSync(join(scratch, 'insp-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  const llmWork = join(work, 'llm.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  copyFileSync(fixtures.llm, llmWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.SQLITE_LLM_LOGS_PATH = llmWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();

  try {
    const out = await c.run(spec);
    return {
      name: c.name,
      status: out.status,
      body: out.body,
      ...(out.extra !== undefined ? { extra: out.extra } : {}),
      ...(c.blank ? { blank: c.blank } : {}),
    };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'inspector-web.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_INSP_MAIN ?? '',
    mount: process.env.QT_FIXTURE_INSP_MOUNT ?? '',
    llm: process.env.QT_FIXTURE_INSP_LLM ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-llmlogs-oracle-'));
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
  process.stderr.write(`llm-logs-routes oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('llm-logs-routes oracle', async () => {
  await main();
});
