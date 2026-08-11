/**
 * @jest-environment node
 *
 * P4.9P UI-SEARCH route ORACLE: drives v4's REAL `GET /api/v1/ui/search`
 * handler over a fresh copy of the /tmp-built ui-search fixture per case, and
 * emits each response {status, body} — so the Rust port (`api::ui_search`) can
 * diff byte-for-byte (including key ORDER: `countsByType`'s insertion order and
 * every result object's literal field order).
 *
 * Case coverage (23): the all-types fan-out (five types + the sort + the
 * broken-`participant.id` characterNames quirk in BOTH directions + the
 * Untitled-Chat message enrichment), each single-type filter, unknown-type
 * filtering + the all-unknown fallback-to-all, the three validation arms
 * (missing q / whitespace q / 1-char q), the 50 cap over a 66-match corpus,
 * offset paging + hasMore both ways, the garbage limit (NaN empties the page,
 * hasMore stays true) and the garbage offset (NaN poisons hasMore), a negative
 * limit (slice-from-the-tail), a negative offset (clamped to 0), an
 * empty-string limit (JS-falsy → default 20), the exact-match priority-0 arm,
 * an uppercase query, the trimmed-query echo, and an over-long query (the
 * repo's MAX_SEARCH_QUERY_LENGTH guard empties messages+memories while the
 * in-handler filters still run).
 *
 * Run (Node 24 — cp to a /tmp mirror; jest ignores .claude/ paths). The
 * fixture is /tmp-built, never committed, so the build stage is part of this
 * recipe — it used to say "the fixture must already exist", which made the
 * family dead the first time /tmp was cleaned (P4.45):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-ui-search-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/ui-search.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/ui-search.json" "$TMPO/fixtures/"
 *   mkdir -p /tmp/qt-ui-search-fixture
 *   cd ~/source/quilltap-server   # or the pinned detached worktree on drift
 *   QT_FIXTURE_UI_SEARCH_MAIN=/tmp/qt-ui-search-fixture/main.db \
 *   QT_FIXTURE_UI_SEARCH_MOUNT=/tmp/qt-ui-search-fixture/mount.db \
 *     $N/node --import tsx "$V5W/harness/oracle/fixtures/build-ui-search-fixture.ts"
 *   QT_FIXTURE_UI_SEARCH_MAIN=/tmp/qt-ui-search-fixture/main.db \
 *   QT_FIXTURE_UI_SEARCH_MOUNT=/tmp/qt-ui-search-fixture/mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-ui-search.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- ui-search
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
  if (resp.status === 204) return { status: 204, body: null };
  return { status: resp.status, body: await resp.json() };
}

const ROUTE = '@/app/api/v1/ui/search/route';
const B = 'http://localhost/api/v1';

const search = async (query: string) => {
  const mod = (await import(ROUTE)) as { GET: (...a: unknown[]) => Promise<unknown> };
  return respond(await mod.GET(mockRequest(`${B}/ui/search${query}`)));
};

interface CaseSpec {
  name: string;
  run: () => Promise<{ status: number; body: unknown }>;
}

function buildCases(): CaseSpec[] {
  return [
    // ── The all-types fan-out ──
    // Pins: five sections' insertion order under the stable sort, the
    // characterNames quirk both directions (CH1 empty / CH2 resolving), the
    // Untitled-Chat message enrichment, the SYSTEM-role exclusion, the
    // description-only match (matchedField), the title||null falsy arm, the
    // importance||5 falsy arm, the content-only memory match (matchedField
    // stays 'summary'; snippet takes the no-match branch), snippet centering +
    // truncation, and the user-scoping of all five types.
    { name: 'all_types_default', run: () => search('?q=airship') },
    // ── Single-type filters ──
    { name: 'characters_only', run: () => search('?q=airship&types=characters') },
    { name: 'chats_and_messages', run: () => search('?q=airship&types=chats,messages') },
    // chats are still FETCHED for the message enrichment when only messages
    // are requested — chatTitle must resolve.
    { name: 'messages_only', run: () => search('?q=airship&types=messages') },
    { name: 'memories_only', run: () => search('?q=airship&types=memories') },
    { name: 'tags_only', run: () => search('?q=airship&types=tags') },
    // ── The types filter arms ──
    { name: 'types_unknown_filtered', run: () => search('?q=airship&types=bogus,characters') },
    // An ALL-unknown list parses to [], and `parsedTypes.length > 0` keeps the
    // all-types default.
    { name: 'types_all_unknown', run: () => search('?q=airship&types=bogus,junk') },
    // ── Validation ──
    { name: 'missing_q', run: () => search('') },
    { name: 'whitespace_q', run: () => search('?q=%20%20') },
    { name: 'short_q', run: () => search('?q=a') },
    // ── Pagination over the 66-match brass corpus ──
    // 61 messages (60 generated + CH1's long message, whose "brass fittings"
    // also match) + 1 character (description) + 1 chat title + 1 memory
    // (content) + 2 tags = 66. limit=999 caps at 50; countsByType carries the
    // PRE-pagination counts.
    { name: 'limit_cap_50', run: () => search('?q=brass&limit=999') },
    { name: 'offset_past_cap', run: () => search('?q=brass&limit=999&offset=50') },
    { name: 'offset_mid_page', run: () => search('?q=brass&limit=10&offset=5') },
    // ── The NaN quirks ──
    // parseInt('abc') → NaN → slice(offset, NaN) is EMPTY while hasMore stays
    // `0 + 0 < total` → true.
    { name: 'garbage_limit', run: () => search('?q=airship&limit=abc') },
    // Math.max(NaN, 0) → NaN → slice(NaN, NaN+20) is empty AND
    // `NaN + 0 < total` → false.
    { name: 'garbage_offset', run: () => search('?q=airship&offset=abc') },
    // Math.min(-5, 50) → -5 → slice(0, -5) drops the last five of the 66.
    { name: 'negative_limit', run: () => search('?q=brass&limit=-5') },
    { name: 'negative_offset', run: () => search('?q=airship&offset=-3') },
    // `limitParam ?` — the empty string is JS-falsy → default 20.
    { name: 'empty_limit_param', run: () => search('?q=airship&limit=') },
    // ── Priority + echo arms ──
    { name: 'exact_name_priority', run: () => search('?q=ottoline') },
    { name: 'uppercase_q', run: () => search('?q=AIRSHIP') },
    { name: 'trimmed_q_echo', run: () => search('?q=%20%20airship%20%20') },
    // ── The repo length guard ──
    // >MAX_SEARCH_QUERY_LENGTH (1000): searchMessagesGlobal + searchByContent
    // return [] while the in-handler substring filters still run (and match
    // nothing).
    { name: 'overlong_q', run: () => search(`?q=${'x'.repeat(1001)}`) },
  ];
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec.userId);

  const work = mkdtempSync(join(scratch, 'uisearch-'));
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

  try {
    const out = await c.run();
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
    fs.readFileSync(join(here, '..', 'fixtures', 'ui-search.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_UI_SEARCH_MAIN ?? '',
    mount: process.env.QT_FIXTURE_UI_SEARCH_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ui-search-oracle-'));
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
  process.stderr.write(`ui-search oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('ui-search oracle', async () => {
  await main();
});
