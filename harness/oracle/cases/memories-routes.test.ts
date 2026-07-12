/**
 * @jest-environment node
 *
 * P4.6s MEMORIES route-surface ORACLE: drives v4's REAL memories route handlers
 * over a FRESH copy of the committed memories-{main,mount}.db fixture per case,
 * and emits each response body (+ status) so the Rust ports (`api::memories::*`)
 * can be diffed byte-for-byte.
 *
 * Only the seams the Rust harness also neutralizes are mocked: the auth session
 * and the startup gate. The DB stack + the provider registry (for the builtin
 * embedding search) are doMocked to the REAL modules (past jest.setup) + the real
 * cipher binding.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-mem-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/memories-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/memories-web.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MEM_MAIN=$V5W/crates/quilltap-web/tests/fixtures/memories-main.db \
 *   QT_FIXTURE_MEM_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/memories-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-memories-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- memories-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

const MNEMO = 'b1000000-0000-4000-8000-000000000001';
const ORLA = 'b1000000-0000-4000-8000-000000000002';
const PIP = 'b1000000-0000-4000-8000-000000000003';
const CHAT_SALON = 'c1000000-0000-4000-8000-000000000001';
const MSG_S1 = 'd1000000-0000-4000-8000-000000000001';
const MSG_S2A = 'd1000000-0000-4000-8000-000000000002';
const MEM_TAGGED_1 = 'b2000000-0000-4000-8000-0000000000a0';
const MEM_TAGGED_2 = 'b2000000-0000-4000-8000-0000000000a1';
const MEM_REL_A = 'b2000000-0000-4000-8000-0000000000e0';
const MEM_REL_B = 'b2000000-0000-4000-8000-0000000000e1';
const MEM_NEARDUP = 'b2000000-0000-4000-8000-0000000000d0';
const MISSING = '00000000-0000-4000-8000-0000000000ff';

// A fixed wall-clock the memory-search recency weighting is pinned to (v4's
// route uses `new Date()`; the Rust differential passes the same now_ms). After
// the fixture memory dates (2026-02/03) so recency is positive.
const FIXED_NOW = 1783000000000;

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'GET',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

function applyMocks(spec: Spec, pinClock: boolean): void {
  // Pin `new Date()` / `Date.now()` so the search recency weighting is
  // deterministic, but leave the real timers alone (the per-case settle delay
  // relies on a real setTimeout). Only the search (read) cases pin the clock —
  // a frozen `Date.now` breaks v4's single-writer IPC deadline logic, so the
  // write cases run on the real clock (their minted timestamps are blanked).
  if (pinClock) {
    jest.useFakeTimers({
      now: FIXED_NOW,
      doNotFake: ['setTimeout', 'setInterval', 'setImmediate', 'clearTimeout', 'clearInterval'],
    });
  }
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
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store'),
  );
  // jest.setup stubs generateEmbeddingForUser to a 3-dim `[0.1,0.2,0.3]` vector;
  // un-mock it so the gate/search run the REAL builtin TF-IDF (286-dim, matching
  // the fixture's baked vectors).
  jest.doMock('@/lib/embedding/embedding-service', () =>
    jest.requireActual('@/lib/embedding/embedding-service'),
  );
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
}

interface CaseSpec {
  name: string;
  /** Pin `new Date()`/`Date.now()` to FIXED_NOW (search recency determinism). */
  clock?: boolean;
  run: (mods: Record<string, unknown>) => Promise<{ status: number; body: unknown; tables?: unknown }>;
}

async function loadRoute(path: string): Promise<Record<string, (...a: unknown[]) => Promise<unknown>>> {
  return (await import(path)) as never;
}

/** Dump the reinforcement/link state of specific memory rows (baked ids → no
 *  remap), for the create-reinforce + delete-unlink structural checks. */
async function dumpMemoryRows(ids: string[]): Promise<unknown> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const db = getRawDatabase() as unknown as {
    prepare: (s: string) => { get: (...a: unknown[]) => unknown };
  };
  const rows = ids.map((id) => {
    const r = db
      .prepare(
        'SELECT id, reinforcementCount, reinforcedImportance, relatedMemoryIds FROM memories WHERE id = ?',
      )
      .get(id) as
      | { id: string; reinforcementCount: number; reinforcedImportance: number; relatedMemoryIds: string }
      | undefined;
    if (!r) return { id, present: false };
    return {
      id: r.id,
      present: true,
      reinforcementCount: r.reinforcementCount,
      relatedMemoryIds: JSON.parse(r.relatedMemoryIds ?? '[]'),
    };
  });
  return { rows };
}

/** Count memories in a chat (deleteByChat structural check). */
async function countMemoriesInChat(chatId: string): Promise<unknown> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const db = getRawDatabase() as unknown as {
    prepare: (s: string) => { get: (...a: unknown[]) => unknown };
  };
  const row = db.prepare('SELECT COUNT(*) AS c FROM memories WHERE chatId = ?').get(chatId) as {
    c: number;
  };
  return { remaining: row.c };
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec, c.clock === true);
  // Register the provider plugins (incl. the BUILTIN embeddings provider) so the
  // semantic-search arm resolves the builtin embedding at query time.
  const { initializePlugins } = await import('@/lib/startup/plugin-initialization');
  await initializePlugins();

  const work = mkdtempSync(join(scratch, 'mem-'));
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
    const out = await c.run({});
    return {
      name: c.name,
      status: out.status,
      body: out.body,
      ...(out.tables !== undefined ? { tables: out.tables } : {}),
    };
  } finally {
    // Let any fire-and-forget write (the item-GET access-time bump; the search
    // access bumps) settle before teardown, so it can't leave the single-writer
    // child / connection in a state that corrupts the NEXT case's user read.
    await new Promise((r) => setTimeout(r, 150));
    await closeDatabase();
    closeMountIndexSQLiteClient();
    jest.useRealTimers();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'memories-web.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_MEM_MAIN ?? '',
    mount: process.env.QT_FIXTURE_MEM_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-mem-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const B = 'http://localhost/api/v1/memories';
  const listGet = async (qs: string) =>
    respond(await (await loadRoute('@/app/api/v1/memories/route')).GET(mockRequest(`${B}?${qs}`)));
  const itemGet = async (id: string) =>
    respond(
      await (await loadRoute('@/app/api/v1/memories/[id]/route')).GET(mockRequest(`${B}/${id}`), {
        params: Promise.resolve({ id }),
      }),
    );

  const cases: CaseSpec[] = [
    // --- List (paginated) ---
    { name: 'list_paginated', run: () => listGet(`characterId=${MNEMO}&limit=20&offset=0`) },
    {
      name: 'list_paginated_p2',
      run: () => listGet(`characterId=${MNEMO}&limit=20&offset=20`),
    },
    {
      name: 'list_paginated_importance_asc',
      run: () => listGet(`characterId=${MNEMO}&limit=15&sortBy=importance&sortOrder=asc`),
    },
    {
      name: 'list_paginated_search',
      run: () => listGet(`characterId=${MNEMO}&limit=50&search=smuggler`),
    },
    {
      name: 'list_paginated_minImportance',
      run: () => listGet(`characterId=${MNEMO}&limit=50&minImportance=0.7`),
    },
    {
      name: 'list_paginated_source_manual',
      run: () => listGet(`characterId=${MNEMO}&limit=50&source=MANUAL`),
    },
    // --- List (legacy, no limit) ---
    { name: 'list_legacy', run: () => listGet(`characterId=${ORLA}`) },
    { name: 'list_legacy_search', run: () => listGet(`characterId=${MNEMO}&search=barometer`) },
    // --- List errors ---
    { name: 'list_missing_character', run: () => listGet(`characterId=${MISSING}`) },
    // --- Item GET ---
    { name: 'get', run: () => itemGet(MEM_TAGGED_2) },
    { name: 'get_missing', run: () => itemGet(MISSING) },
    // --- Count by chat ---
    { name: 'count_by_chat', run: () => listGet(`chatId=${CHAT_SALON}`) },
    { name: 'count_by_chat_missing', run: () => listGet(`chatId=${MISSING}`) },
    // --- By message ---
    { name: 'by_message_swipe', run: () => listGet(`messageId=${MSG_S2A}`) },
    { name: 'by_message_single', run: () => listGet(`messageId=${MSG_S1}`) },
    { name: 'by_message_missing', run: () => listGet(`messageId=${MISSING}`) },
    // --- Character memory counts ---
    { name: 'character_counts', run: () => listGet(`action=character-memory-counts`) },
    // --- Create (gate) ---
    {
      name: 'create_insert',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/memories/route')).POST(
            mockRequest(B, {
              characterId: MNEMO,
              content: 'The clocktower chimes thirteen times on the winter solstice.',
              summary: 'The clocktower chimes thirteen at solstice.',
              keywords: ['clocktower', 'solstice'],
              importance: 0.55,
              source: 'MANUAL',
            }),
          ),
        ),
    },
    {
      // Identical content to the near-dup anchor → cosine 1.0 → the gate absorbs
      // it (SKIP_NEAR_DUPLICATE): no new row, the anchor is returned unchanged.
      name: 'create_near_duplicate',
      run: async () => {
        const r = await (await loadRoute('@/app/api/v1/memories/route')).POST(
          mockRequest(B, {
            characterId: MNEMO,
            content: 'Mnemo always double-checks the barometer before hoisting the mainsail.',
            summary: 'Mnemo checks the barometer before the mainsail.',
            source: 'AUTO',
          }),
        );
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpMemoryRows([MEM_NEARDUP]) };
      },
    },
    // --- Update (no re-embed) ---
    {
      name: 'update',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/memories/[id]/route')).PUT(
            mockRequest(`${B}/${MEM_TAGGED_1}`, { importance: 0.95, summary: 'Alden, the stern mentor.' }),
            { params: Promise.resolve({ id: MEM_TAGGED_1 }) },
          ),
        ),
    },
    // --- Delete (+ unlink scrub) ---
    {
      name: 'delete',
      run: async () => {
        const r = await (await loadRoute('@/app/api/v1/memories/[id]/route')).DELETE(
          mockRequest(`${B}/${MEM_REL_B}`),
          { params: Promise.resolve({ id: MEM_REL_B }) },
        );
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpMemoryRows([MEM_REL_A, MEM_REL_B]) };
      },
    },
    // --- Delete by chat ---
    {
      name: 'delete_by_chat',
      run: async () => {
        const r = await (await loadRoute('@/app/api/v1/memories/route')).DELETE(
          mockRequest(`${B}?chatId=${CHAT_SALON}`),
        );
        const { status, body } = await respond(r);
        return { status, body, tables: await countMemoriesInChat(CHAT_SALON) };
      },
    },
    // --- Search (builtin TF-IDF; pinned clock) ---
    {
      name: 'search',
      clock: true,
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/memories/route')).POST(
            mockRequest(`${B}?action=search`, {
              characterId: MNEMO,
              query: 'smuggler cove hidden by the reef',
              limit: 5,
            }),
          ),
        ),
    },
    {
      name: 'search_min_importance',
      clock: true,
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/memories/route')).POST(
            mockRequest(`${B}?action=search`, {
              characterId: MNEMO,
              query: 'lighthouse keeper storm beacon',
              limit: 10,
              minImportance: 0.7,
            }),
          ),
        ),
    },
  ];

  const lines: string[] = [];
  for (const c of cases) {
    const rec = await runCase(spec, c, scratch, fixtures);
    lines.push(JSON.stringify(rec));
  }
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(`wrote ${cases.length} memories-routes oracle records to ${outPath}\n`);
}

// Jest wrapper: one test that runs the whole corpus.
test('memories-routes oracle', async () => {
  await main();
}, 120000);
