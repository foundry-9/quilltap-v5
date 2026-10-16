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
 *   cp "$V5W/harness/oracle/fixtures/apply-memory-tiers-migration.ts" "$TMPO/fixtures/"
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

// C1 §13 (P4.D265): v4's REAL `add-memory-tiers-v1` on the per-case copy —
// staged beside this case (`$TMPO/fixtures/`, see the header).
import { applyMemoryTiersMigration } from '../fixtures/apply-memory-tiers-migration';

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
  /** P4.D298: the embedding service REJECTS for this case (re-registered as
   *  the real module by `applyMocks` for every other case — memory note
   *  `jest-domock-survives-resetmodules`). */
  embeddingFails?: boolean;
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
  // === P4.D298: the job host held off, the markdown stub, the log capture (append-only) ===
  // Hold v4's job host off BEFORE any import (a runner spawned mid-case claims
  // queued jobs — memory note `v4-oracle-job-host-and-log-capture`).
  (globalThis as Record<string, unknown>).__quilltapJobHost = {
    child: null,
    spawning: false,
    shuttingDown: true,
    childCrashed: false,
    restartTimestamps: [],
    signalHandlersInstalled: false,
    invalidationListeners: new Set(),
  };
  // The messages / chats routes transitively import the npm-native markdown
  // renderer (ESM-only `unified`); stub it so jest can compile (the
  // salon-mutations idiom).
  jest.doMock('@/lib/services/markdown-renderer.service', () => ({
    __esModule: true,
    renderMarkdownToHtml: async () => null,
    canPreRenderMessage: () => false,
  }));
  if (c.embeddingFails) {
    jest.doMock('@/lib/embedding/embedding-service', () => ({
      ...jest.requireActual('@/lib/embedding/embedding-service'),
      generateEmbeddingForUser: async () => {
        throw new Error('planted embedding failure');
      },
    }));
  }
  const logs = await installLogCapture();
  // === end ===
  // Register the provider plugins (incl. the BUILTIN embeddings provider) so the
  // semantic-search arm resolves the builtin embedding at query time.
  const { initializePlugins } = await import('@/lib/startup/plugin-initialization');
  await initializePlugins();

  const work = mkdtempSync(join(scratch, 'mem-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  // P4.D265 (C1 §13): the memory-tiers columns on the ENCRYPTED per-case copy,
  // before the case's own v4 connection opens it — the twin of the Rust side's
  // `ensure_memory_tiers_columns`. Idempotent on the widened committed pair
  // (§R.3); the belt for plants and future pairs. P4.D266 / P4.D269 inherit
  // this hunk and never edit it.
  await applyMemoryTiersMigration(mainWork, { pepperBase64: spec.testPepperBase64 });
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  // The `83334b9a5` unification: v4's memory index cache is a PROCESS singleton
  // (`processSingleton('embedding.vectorStoreManager')`, `vector-store.ts:482`)
  // that survives `jest.resetModules()`, while every case runs on a FRESH copy
  // of the fixture pair — so the `delete` case's removal of `MEM_REL_B` and the
  // update cases' re-embeddings leaked into the `search*` cases through the
  // cache (one row missing, one row scored on a stale vector), pinning rows no
  // single v4 process ever answers. Drop every cached store so each case's
  // first search reloads from ITS copy, as a v4 process does from its own rows.
  {
    const { memoryIndex } = await import('@/lib/memory/index-store');
    memoryIndex.reloadAll();
  }

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();
  try {
    const out = await c.run({});
    // P4.D298: let an un-awaited tail land before the lines are read.
    await new Promise((r) => setImmediate(r));
    return {
      name: c.name,
      status: out.status,
      body: out.body,
      ...(out.tables !== undefined ? { tables: out.tables } : {}),
      logs: logs.own,
      siblingMessages: [...new Set(logs.sibling)],
    };
  } finally {
    // Let any fire-and-forget write (the item-GET access-time bump; the search
    // access bumps) settle before teardown, so it can't leave the single-writer
    // child / connection in a state that corrupts the NEXT case's user read.
    await new Promise((r) => setTimeout(r, 350));
    await closeDatabase();
    closeMountIndexSQLiteClient();
    jest.useRealTimers();
    rmSync(work, { recursive: true, force: true });
  }
}

// === P4.D298: the log capture + plant helpers (append-only) ===
interface LogLine {
  level: string;
  message: string;
  fields: Record<string, unknown>;
}

/** Splice v4's logger transports (children share the array) so every line
 *  lands here, at debug. `own` = the route's `[Memories API…]` / `[Messages
 *  API v1]` lines (`{level, message, fields}`, `service` / `environment`
 *  dropped, `error.message` folded in as `error`, an `undefined` field
 *  OMITTED by the JSON round-trip — §R.5); `sibling` = the bare messages of
 *  the chokepoints' lines (`memory:retire` / `memory:admission` /
 *  `memory:update-memory`), compared by PRESENCE only. */
async function installLogCapture(): Promise<{ own: LogLine[]; sibling: string[] }> {
  process.env.LOG_LEVEL = 'debug';
  const { logger } = await import('@/lib/logger');
  const own: LogLine[] = [];
  const sibling: string[] = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as {
        level: string;
        message: string;
        context?: Record<string, unknown>;
        error?: { message: string };
      };
      const { service: _s, environment: _e, ...fields } = (data.context ?? {}) as Record<
        string,
        unknown
      >;
      if (data.error) fields.error = data.error.message;
      if (/^\[(Memories API|Messages API v1|Chats v1)/.test(data.message)) {
        own.push(JSON.parse(JSON.stringify({ level: data.level, message: data.message, fields })));
      } else if (
        typeof fields.module === 'string' &&
        ['memory:retire', 'memory:admission', 'memory:update-memory'].includes(fields.module)
      ) {
        sibling.push(`${fields.module}|${data.message}`);
      }
    },
  });
  return { own, sibling };
}

/** Plant SQL on the case's own copy through a DEDICATED keyed connection, then
 *  re-open v4's connection (memory note `jest-oracle-in-case-plants`). */
async function plantSql(statements: string[]): Promise<void> {
  const Driver = require(
    require('node:path').join(process.cwd(), 'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'),
  );
  const db = new Driver(process.env.SQLITE_PATH);
  const hex = Buffer.from(process.env.ENCRYPTION_MASTER_PEPPER ?? '', 'base64').toString('hex');
  db.pragma(`key = "x'${hex}'"`);
  try {
    for (const sql of statements) db.exec(sql);
  } finally {
    db.close();
  }
  const { closeDatabase, initializeDatabase } = await import('@/lib/database/manager');
  await closeDatabase();
  await initializeDatabase();
}

/** Every memory row's identity + the columns the retire / wipe rows move,
 *  ordered by the BAKED id (never a minted one). */
async function dumpAllMemories(): Promise<unknown> {
  const Driver = require(
    require('node:path').join(process.cwd(), 'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'),
  );
  const db = new Driver(process.env.SQLITE_PATH, { readonly: true });
  const hex = Buffer.from(process.env.ENCRYPTION_MASTER_PEPPER ?? '', 'base64').toString('hex');
  db.pragma(`key = "x'${hex}'"`);
  try {
    return db
      .prepare(
        'SELECT id, characterId, chatId, source, tier, relatedMemoryIds, consolidatedFrom, lastAccessedAt FROM memories ORDER BY id',
      )
      .all();
  } finally {
    db.close();
  }
}

const P4D298_SPARE_PLANT = [
  // MANUAL in the chat — spared.
  "UPDATE memories SET chatId = 'c1000000-0000-4000-8000-000000000001' WHERE id = 'b2000000-0000-4000-8000-100000000011'",
  // cold in the chat — spared.
  "UPDATE memories SET tier = 'cold' WHERE id = 'b2000000-0000-4000-8000-0000000000f1'",
  // a digest in the chat whose member (outside the chat) lives — spared.
  "UPDATE memories SET source = 'CONSOLIDATED', chatId = 'c1000000-0000-4000-8000-000000000001', consolidatedFrom = '[\"b2000000-0000-4000-8000-0000000000b1\"]' WHERE id = 'b2000000-0000-4000-8000-0000000000b0'",
  // …a member "still names it" (`supersededById`) — v4's living-member rule.
  "UPDATE memories SET tier = 'cold', supersededById = 'b2000000-0000-4000-8000-0000000000b0' WHERE id = 'b2000000-0000-4000-8000-0000000000b1'",
  // (f0 stays a plain AUTO row in the chat — retired.)
];

const P4D298_COUNTS_PLANT = [
  "UPDATE memories SET source = 'MANUAL' WHERE characterId = 'b1000000-0000-4000-8000-000000000002'",
  "UPDATE memories SET characterId = 'b1000000-0000-4000-8000-000000000003' WHERE id IN ('b2000000-0000-4000-8000-100000000011','b2000000-0000-4000-8000-100000000013','b2000000-0000-4000-8000-100000000015','b2000000-0000-4000-8000-100000000017','b2000000-0000-4000-8000-100000000019','b2000000-0000-4000-8000-100000000021')",
  "UPDATE memories SET tier = NULL WHERE id = 'b2000000-0000-4000-8000-100000000011'",
  "UPDATE memories SET tier = 'cold' WHERE id = 'b2000000-0000-4000-8000-100000000013'",
  "UPDATE memories SET tier = 'cold' WHERE id = 'b2000000-0000-4000-8000-0000000000f1'",
  "UPDATE memories SET tier = NULL WHERE id = 'b2000000-0000-4000-8000-0000000000b3'",
];

/** `reinforcedImportance` moved apart from `importance` (both ways), so the
 *  list's `filterImportance` read decides which row a `minImportance` keeps. */
const P4D298_REINFORCED_PLANT = [
  "UPDATE memories SET reinforcedImportance = 0.9 WHERE id = 'b2000000-0000-4000-8000-100000000019'",
  "UPDATE memories SET reinforcedImportance = 0.1 WHERE id = 'b2000000-0000-4000-8000-100000000027'",
];

/** The search's only hit loses its `reinforcedImportance` (a vector kept). */
const P4D298_NULL_REINFORCED_PLANT = [
  "UPDATE memories SET reinforcedImportance = NULL WHERE id = 'b2000000-0000-4000-8000-0000000000e0'",
];

/** A retire that THROWS (every memory delete aborts) — the two
 *  re-attribution catches' ERRORs. */
const P4D298_RETIRE_FAILS_PLANT = [
  "CREATE TRIGGER qt_p4d298_retire_fails BEFORE DELETE ON memories BEGIN SELECT RAISE(ABORT, 'planted retire failure'); END",
];

/** The chat the derived-project create names carries a project id. */
const P4D298_CHAT_PROJECT_PLANT = [
  "UPDATE chats SET projectId = 'b7000000-0000-4000-8000-000000000001' WHERE id = 'c1000000-0000-4000-8000-000000000001'",
];

/** `z.iso.datetime({ offset: true })` — the tier-1 rows (L-6). */
const P4D298_OFFSET_DATETIMES = [
  '2026-03-01T10:00:00Z',
  '2026-03-01T10:00:00+05:30',
  '2026-03-01T10:00:00-08:00',
  '2026-03-01T10:00:00-0800',
  '2026-03-01T10:00:00+05',
  '2026-03-01T10:00Z',
  '2026-03-01T10:00:00.123456789+00:00',
  '2026-03-01T10:00:00.Z',
  '2024-02-29T00:00:00Z',
  '2023-02-29T00:00:00Z',
  '2026-03-01T24:00:00Z',
  '2026-03-01T10:00:00z',
  '2026-03-01 10:00:00Z',
  '2026-03-01T10:00:00+24:00',
  '+002026-03-01T10:00:00Z',
];

/** A delete the trigger swallows: the row survives, so `retired === 0`. */
const P4D298_DELETE_RACE_PLANT = [
  "CREATE TRIGGER qt_p4d298_keep_row BEFORE DELETE ON memories WHEN old.id = 'b2000000-0000-4000-8000-0000000000e1' BEGIN SELECT RAISE(IGNORE); END",
];
// === end ===

// === P4.D269: list/search rows (append-only) ===
/** The plant every P4.D269 row runs on its copy (the Rust twin is
 *  `P4D269_PLANT` in memories_routes_equivalence.rs): two cold rows (one
 *  superseded by a hot row), a CONSOLIDATED hot digest, a NULL-tier row. */
const P4D269_PLANT = [
  "UPDATE memories SET tier = 'cold', supersededById = 'b2000000-0000-4000-8000-0000000000e0' WHERE id = 'b2000000-0000-4000-8000-0000000000e1'",
  "UPDATE memories SET tier = 'cold' WHERE id = 'b2000000-0000-4000-8000-100000000016'",
  "UPDATE memories SET source = 'CONSOLIDATED', consolidatedFrom = '[\"b2000000-0000-4000-8000-0000000000f0\",\"b2000000-0000-4000-8000-0000000000f1\"]', consolidatedAt = '2026-03-02T00:00:00.000Z' WHERE id = 'b2000000-0000-4000-8000-0000000000a1'",
  "UPDATE memories SET tier = NULL WHERE id = 'b2000000-0000-4000-8000-0000000000b3'",
];

async function p4d269Plant(): Promise<void> {
  // A DEDICATED connection on the case's own copy (`SQLITE_PATH`, keyed with
  // the test pepper): the raw client's handle is a cross-case singleton that
  // can still point at the PREVIOUS case's copy here (measured), and
  // `manager.rawQuery` saw the same stale file.
  const Driver = require(
    require('node:path').join(process.cwd(), 'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'),
  );
  const db = new Driver(process.env.SQLITE_PATH);
  const hex = Buffer.from(process.env.ENCRYPTION_MASTER_PEPPER ?? '', 'base64').toString('hex');
  db.pragma(`key = "x'${hex}'"`);
  try {
    for (const sql of P4D269_PLANT) db.prepare(sql).run();
  } finally {
    db.close();
  }
  // Re-open v4's own connection so it reads the planted file fresh (without
  // this, a later user read intermittently missed — measured).
  const { closeDatabase, initializeDatabase } = await import('@/lib/database/manager');
  await closeDatabase();
  await initializeDatabase();
}

function P4D269_LIST_SEARCH_CASES(
  listGet: (qs: string) => Promise<{ status: number; body: unknown }>,
): CaseSpec[] {
  const B = 'http://localhost/api/v1/memories';
  const M = 'b1000000-0000-4000-8000-000000000001';
  const list = (name: string, qs: string): CaseSpec => ({
    name,
    run: async () => {
      await p4d269Plant();
      return listGet(qs);
    },
  });
  const search = (name: string, body: unknown): CaseSpec => ({
    name,
    clock: true,
    run: async () => {
      await p4d269Plant();
      return respond(
        await (await loadRoute('@/app/api/v1/memories/route')).POST(
          mockRequest(`${B}?action=search`, body),
        ),
      );
    },
  });
  return [
    list('list_paginated_tier_hot', `characterId=${M}&limit=50&tier=hot`),
    list('list_paginated_tier_cold', `characterId=${M}&limit=50&tier=cold`),
    list('list_paginated_tier_bogus', `characterId=${M}&limit=50&tier=warm`),
    list('list_paginated_source_consolidated', `characterId=${M}&limit=50&source=CONSOLIDATED`),
    list('list_paginated_tier_cold_search', `characterId=${M}&limit=50&search=reef&tier=cold`),
    list('list_paginated_null_tier', `characterId=${M}&limit=50&search=Unindexed&tier=hot`),
    list('list_legacy_tier_hot', `characterId=${M}&tier=hot`),
    list('list_legacy_tier_cold', `characterId=${M}&tier=cold`),
    list('list_legacy_source_consolidated', `characterId=${M}&source=CONSOLIDATED`),
    list('list_legacy_null_tier', `characterId=${M}&search=Unindexed&tier=hot`),
    search('search_include_cold', { characterId: M, query: 'smuggler cove hidden by the reef', limit: 5 }),
    search('search_source_consolidated', {
      characterId: M,
      query: 'Alden trusts the harbor master',
      source: 'CONSOLIDATED',
    }),
    search('search_source_bogus', { characterId: M, query: 'reef', source: 'BOGUS' }),
    search('search_query_empty', { characterId: M, query: '' }),
    search('search_bad_uuid', { characterId: 'nope', query: 'reef' }),
    search('search_limit_101', { characterId: M, query: 'reef', limit: 101 }),
    search('search_many_issues', { characterId: 5, query: 5, limit: '5', minScore: -1, source: null }),
  ];
}
// === end ===

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
    // === P4.D298: item GET / DELETE, the chat wipe, the counts (append-only) ===
    {
      // Bug 219: viewing is not an access — `lastAccessedAt` stays as it was.
      name: 'get_does_not_stamp',
      run: async () => {
        const out = await itemGet(MEM_TAGGED_2);
        await new Promise((r) => setTimeout(r, 350));
        const rows = (await dumpAllMemories()) as Array<{ id: string; lastAccessedAt: unknown }>;
        const row = rows.find((r) => r.id === MEM_TAGGED_2);
        return { ...out, tables: { present: row !== undefined, lastAccessedAt: row?.lastAccessedAt ?? null } };
      },
    },
    {
      // The retire found nothing to delete (`retired === 0`) → 404.
      name: 'delete_race_retired_zero',
      run: async () => {
        await plantSql(P4D298_DELETE_RACE_PLANT);
        const r = await (await loadRoute('@/app/api/v1/memories/[id]/route')).DELETE(
          mockRequest(`${B}/${MEM_REL_B}`),
          { params: Promise.resolve({ id: MEM_REL_B }) },
        );
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpMemoryRows([MEM_REL_A, MEM_REL_B]) };
      },
    },
    {
      // Bug 201: manual, cold and living-digest rows survive the wipe.
      name: 'delete_by_chat_spares_manual_cold_digest',
      run: async () => {
        await plantSql(P4D298_SPARE_PLANT);
        const r = await (await loadRoute('@/app/api/v1/memories/route')).DELETE(
          mockRequest(`${B}?chatId=${CHAT_SALON}`),
        );
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpAllMemories() };
      },
    },
    {
      name: 'delete_by_chat_missing',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/memories/route')).DELETE(mockRequest(`${B}?chatId=${MISSING}`)),
        ),
    },
    {
      // Bug 210: the four cap keys; both sort keys decide an order.
      name: 'character_counts_breakdown',
      run: async () => {
        await plantSql(P4D298_COUNTS_PLANT);
        return listGet(`action=character-memory-counts`);
      },
    },
    {
      name: 'list_min_importance_reinforced_legacy',
      run: async () => {
        await plantSql(P4D298_REINFORCED_PLANT);
        return listGet(`characterId=${MNEMO}&minImportance=0.7&sortBy=importance&sortOrder=asc`);
      },
    },
    {
      name: 'list_min_importance_reinforced_paginated',
      run: async () => {
        await plantSql(P4D298_REINFORCED_PLANT);
        return listGet(`characterId=${MNEMO}&limit=50&minImportance=0.7&sortBy=importance&sortOrder=asc`);
      },
    },
    {
      // `minImportance ?? TOOL_MIN_IMPORTANCE`: the 0 default filters the
      // `reinforcedImportance` COLUMN, so a NULL row drops out.
      name: 'search_min_importance_default',
      clock: true,
      run: async () => {
        await plantSql(P4D298_NULL_REINFORCED_PLANT);
        return respond(
          await (await loadRoute('@/app/api/v1/memories/route')).POST(
            mockRequest(`${B}?action=search`, {
              characterId: MNEMO,
              query: 'smuggler cove hidden by the reef',
              limit: 5,
            }),
          ),
        );
      },
    },
    // --- The manual create through admission (bug 218 / 236, R-I, L-1, L-6, L-7).
    ...((): CaseSpec[] => {
      const create = (name: string, body: unknown, extra: Partial<CaseSpec> = {}, plant?: string[]): CaseSpec => ({
        name,
        ...extra,
        run: async () => {
          if (plant) await plantSql(plant);
          const before = ((await dumpAllMemories()) as unknown[]).length;
          const out = await respond(
            await (await loadRoute('@/app/api/v1/memories/route')).POST(mockRequest(B, body)),
          );
          const after = ((await dumpAllMemories()) as unknown[]).length;
          return { ...out, tables: { rowsBefore: before, rowsAfter: after } };
        },
      });
      const base = {
        characterId: MNEMO,
        content: 'The tide clock above the chandlery runs nine minutes fast.',
        summary: 'The chandlery tide clock runs fast.',
      };
      return [
        create('create_manual_trims', {
          characterId: MNEMO,
          content: '\u00a0  The lamp oil sits under the third stair. \n',
          // U+FEFF is JS whitespace (trimmed); U+0085 is not (kept) — L-7.
          summary: '\ufeffLamp oil under the third stair.\u0085',
          keywords: ['lamp', 'oil'],
        }),
        create('create_manual_trims_no_keywords', {
          characterId: MNEMO,
          content: '\u00a0  The lamp oil sits under the third stair. \n',
          summary: '\ufeffLamp oil under the third stair.\u0085',
        }),
        // MEASURED divergence witness (admission / the BUILTIN vector — not this
        // lane's code): the one vocabulary term this text shares with `f1` is
        // "kept"; v4's admission scores every neighbour 0 and inserts, v5's
        // links `f1`. The Rust side pins it as `ADMISSION_SINGLE_TERM_LINK`.
        create('create_plain_lamp_text', {
          characterId: MNEMO,
          content: 'The lamp oil is kept under the third stair.',
          summary: 'Lamp oil under the third stair.',
        }),
        create('create_blank_after_trim', { characterId: MNEMO, content: '   \t', summary: 'x' }),
        create(
          'create_project_derived_from_chat',
          { ...base, chatId: CHAT_SALON },
          {},
          P4D298_CHAT_PROJECT_PLANT,
        ),
        create('create_missing_project_404', { ...base, projectId: MISSING }),
        create('create_missing_chat_404', { ...base, chatId: MISSING }),
        create('create_missing_character_first_404', { ...base, characterId: MISSING, chatId: MISSING }),
        create('create_kind_occurred_entities', {
          ...base,
          kind: 'episodic',
          occurredAt: '2026-03-01T10:00:00.000Z',
          entities: ['Alden', 'harbor'],
        }),
        // MEASURED: the route's `z.iso.datetime({ offset: true })` admits an
        // offset, but the memory ROW schema's `TimestampSchema` (`Z` only)
        // throws inside the create — the middleware's 400.
        create('create_offset_occurred_at_row_refusal', {
          ...base,
          occurredAt: '2026-03-01T10:00:00+05:30',
        }),
        create('create_bad_offset_400', { ...base, occurredAt: '2026-03-01T10:00:00-0800' }),
        create('create_source_skipgate_stripped', { ...base, source: 'AUTO', skipGate: true }),
        create('create_source_null_logged', { ...base, source: null }),
        create('create_source_manual_silent', { ...base, source: 'MANUAL' }),
        // A MANUAL twin of the fixture's MANUAL `a1` — reinforced, 200.
        create('create_reinforce_200', {
          characterId: MNEMO,
          content: 'Alden the lighthouse keeper trusts only the harbor master.',
          summary: 'Alden trusts the harbor master.',
        }),
        create('create_embedding_failed_502', base, { embeddingFails: true }),
        create('create_entities_over_cap_400', {
          ...base,
          entities: Array.from({ length: 13 }, (_, i) => `entity-${i}`),
        }),
      ];
    })(),
    // --- The item PUT through the edit chokepoint (bug 203 / 236).
    ...((): CaseSpec[] => {
      const put = (name: string, id: string, body: unknown, plant?: string[]): CaseSpec => ({
        name,
        run: async () => {
          if (plant) await plantSql(plant);
          const out = await respond(
            await (await loadRoute('@/app/api/v1/memories/[id]/route')).PUT(
              mockRequest(`${B}/${id}`, body),
              { params: Promise.resolve({ id }) },
            ),
          );
          const rows = (await dumpAllMemories()) as Array<Record<string, unknown>>;
          return { ...out, tables: rows.filter((r) => r.id === id || r.id === MEM_TAGGED_2) };
        },
      });
      const NO_PROFILE = ['DELETE FROM embedding_profiles'];
      return [
        put('update_text_reembeds', MEM_TAGGED_1, {
          content: 'The old captain Alden mentored every apprentice at the harbor.',
        }),
        put('update_importance_only_no_profile', MEM_TAGGED_1, { importance: 0.35 }, NO_PROFILE),
        put('update_text_no_profile_502', MEM_TAGGED_1, { summary: 'Alden mentored them all.' }, NO_PROFILE),
        put('update_invalid_related_400', MEM_TAGGED_1, { relatedMemoryIds: [MISSING] }),
        put('update_related_links_both_ways', MEM_TAGGED_1, { relatedMemoryIds: [MEM_TAGGED_2] }),
        put('update_blank_after_trim_400', MEM_TAGGED_1, { summary: ' \u00a0 ' }),
        put('update_bad_body_missing_memory_404', MISSING, { importance: 'very' }),
        put('update_null_about_character', MEM_TAGGED_1, { aboutCharacterId: null, keywords: ['alden'] }),
      ];
    })(),
    // The three re-attribution / cascade lines (af48f6cc2) over this fixture's
    // swipe pair `d…02` / `d…03`, which sourced `f0` / `f1`.
    {
      name: 'message_delete_cascade_line',
      run: async () => {
        // P4.D288's L-2 (`MEMORY_INDEX_ALWAYS_LOADED`): v4's `vectorsRemoved`
        // counts only an index its process has LOADED; v5's is always loaded.
        const { memoryIndex } = await import('@/lib/memory/index-store');
        await memoryIndex.shape(MNEMO, 3);
        const r = await (await loadRoute('@/app/api/v1/messages/[id]/route')).DELETE(
          mockRequest(`http://localhost/api/v1/messages/${MSG_S2A}?memoryAction=DELETE_MEMORIES`),
          { params: Promise.resolve({ id: MSG_S2A }) },
        );
        const { status } = await respond(r);
        return { status, body: null, tables: await dumpAllMemories() };
      },
    },
    ...([
      ['reattribute_line_silent', false],
      ['reattribute_retire_fails_line', true],
    ] as const).map(
      ([name, fails]): CaseSpec => ({
        name,
        run: async () => {
          if (fails) await plantSql(P4D298_RETIRE_FAILS_PLANT);
          const r = await (await loadRoute('@/app/api/v1/messages/[id]/route')).POST(
            mockRequest(`http://localhost/api/v1/messages/${MSG_S2A}?action=reattribute`, {
              newParticipantId: 'e1000000-0000-4000-8000-000000000002',
            }),
            { params: Promise.resolve({ id: MSG_S2A }) },
          );
          const { status } = await respond(r);
          return { status, body: null, tables: await dumpAllMemories() };
        },
      }),
    ),
    ...([
      ['bulk_reattribute_line_silent', false],
      ['bulk_reattribute_retire_fails_line', true],
    ] as const).map(
      ([name, fails]): CaseSpec => ({
        name,
        run: async () => {
          if (fails) await plantSql(P4D298_RETIRE_FAILS_PLANT);
          const r = await (await loadRoute('@/app/api/v1/chats/[id]/route')).POST(
            mockRequest(`http://localhost/api/v1/chats/${CHAT_SALON}?action=bulk-reattribute`, {
              sourceParticipantId: 'e1000000-0000-4000-8000-000000000001',
              targetParticipantId: 'e1000000-0000-4000-8000-000000000002',
            }),
            { params: Promise.resolve({ id: CHAT_SALON }) },
          );
          const { status } = await respond(r);
          return { status, body: null, tables: await dumpAllMemories() };
        },
      }),
    ),
    // === end ===
    // === P4.D269: list/search rows (append-only) ===
    // v4 `d58548051`: the list's `?tier=` (TWO code paths with different
    // NULL-tier rules) + `source=CONSOLIDATED`; the search's `includeCold:
    // true` + the CONSOLIDATED enum + its Zod refusals (R-E). Every row plants
    // P4D269_PLANT on its copy first (after the shared tier migration).
    ...P4D269_LIST_SEARCH_CASES(listGet),
    // === end ===
  ];

  // === P4.D298: the tier-1 offset-datetime rows (L-6) — v4's zod at the pin ===
  // The pin's OWN zod (resolved from the v4 checkout — this case is staged in a
  // /tmp mirror outside v4's node_modules resolution).
  const { z } = require(require('node:path').join(process.cwd(), 'node_modules/zod')) as typeof import('zod');
  const offsetSchema = z.iso.datetime({ offset: true });
  const offsetRows = P4D298_OFFSET_DATETIMES.map((input) => ({
    input,
    ok: offsetSchema.safeParse(input).success,
  }));
  // === end ===
  const lines: string[] = [JSON.stringify({ name: 'iso_datetime_offset_rows', rows: offsetRows })];
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
