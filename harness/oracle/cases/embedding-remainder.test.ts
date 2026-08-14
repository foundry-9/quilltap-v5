/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the embedding-family remainder (P4.6BM) — v4's REAL
 * `handleConversationRender` (`lib/background-jobs/handlers/conversation-render.ts`)
 * driven over the committed two-database fixture, with NO seam mocked at all:
 * neither the render nor the reconcile nor the reindex calls a model, so the
 * whole path is v4's own code against the real encrypted DBs.
 *
 * The ONE thing stubbed is `ensureProcessorRunning`. Every enqueue calls it, and
 * in-process it FORKS v4's job child, which then claims the very jobs this
 * oracle is measuring and never lets the process exit. v4's own kill switch
 * (`QUILLTAP_JOB_CHILD=1`) is not usable here: it also puts the SQLite client
 * into the child's read-only mode, so the handlers could not write at all.
 * A jest run (rather than tsx) is what makes the stub possible.
 *
 * The one non-determinism is the wall clock — the render's header carries a
 * `Current time:` line and every mint stamps `new Date()` — so `globalThis.Date`
 * is frozen to the corpus `renderNowIso` before any handler is imported. The
 * Rust side injects the same value.
 *
 * Phases, in order (state accumulates across all of them, exactly as it would in
 * a live instance):
 *
 *   1. RECONCILE — v4's `reconcileConversationRendering()` over the PRISTINE
 *      fixture, recorded as `{kind:'reconcile', ...counters}`. It runs first
 *      because that is where boot runs it, and because both later phases mutate
 *      exactly what its scan reads. Since P4.D25 those counters include
 *      `skippedStale`, and the scan excludes chunks already FAILED for the
 *      resolved default profile — both read the frozen clock, so the corpus's
 *      pre-P4.D25 chats are all stale by construction and the four chats added
 *      for this round are the ones that get through.
 *   2. RENDER — each corpus `renderCases` entry through `handleConversationRender`
 *      with a synthesized job row, recorded as `{kind:'render', name, outcome,
 *      error}`.
 *   2b. DIMENSION RECONCILE (P4.d27) — v4's REAL `reconcileEmbeddingDimensions()`
 *      once per corpus `dimensionReconcileCases` entry, recorded as
 *      `{kind:'dimReconcile', name, ...result}`. It runs HERE — after the renders,
 *      before the reindexes — for a reason the first attempt found the hard way:
 *      run after the reindex phase, its DELETE and meta-snap arms both report
 *      zero, because full-scope reindex has already emptied `vector_entries` and
 *      `vector_indices`. Two arms would have been silently uncovered.
 *      Each case's prelude rewrites which profile is `isDefault` (one UPDATE of
 *      harness scaffolding, identical on both sides); the LAST case restores the
 *      corpus's BUILTIN default so the phases after it see the state they were
 *      written against. The cases walk the BUILTIN skip, the no-fixed-dim skip,
 *      the full enforce-and-enqueue arm, the dedupe, the no-profile skip, and the
 *      restore.
 *   3. REINDEX — each corpus `reindexCases` entry through
 *      `handleEmbeddingReindexAll`, same shape. `process.chdir`s into the
 *      COMMITTED help tree (`fixtures/help-sync/`, shared with the P4.1b
 *      family) BEFORE importing the handler, because its `syncHelpDocs` captures
 *      `HELP_DIR = join(process.cwd(), 'help')` at module load; the Rust side
 *      walks the same tree through the production host walker.
 *   4. QUEUE-MEMORIES — each corpus `queueMemoriesCases` entry through v4's REAL
 *      `handleQueueMemories` action, called directly with a synthesized
 *      `AuthenticatedContext` (so a case can name the SECOND user, whose missing
 *      chat-settings row is the resolver's `null` → 400 arm). Recorded as
 *      `{kind:'queueMemories', name, status, body}`.
 *
 * Then every diffed table is dumped, sorted by a NATURAL key (not by id), because
 * both the render's new chunks and its embed enqueues mint fresh UUIDs on each
 * side. The Rust test applies the identical sort and blanks any id absent from
 * the corpus's pinned-id set.
 *
 * `TZ=UTC` is load-bearing (the renderer formats through `toLocale*String('en-US')`
 * with no explicit zone).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<the v5 worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-er-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-er-mount.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-embedding-remainder-fixture.ts
 *   mkdir -p /tmp/qt-er-oracle/cases /tmp/qt-er-oracle/fixtures
 *   cp $V5/harness/oracle/cases/embedding-remainder.test.ts /tmp/qt-er-oracle/cases/
 *   cp $V5/harness/oracle/fixtures/embedding-remainder.json  /tmp/qt-er-oracle/fixtures/
 *   cp -R $V5/harness/oracle/fixtures/help-sync              /tmp/qt-er-oracle/fixtures/
 *   TZ=UTC QT_FIXTURE_ER_MAIN=$V5/crates/quilltap-web/tests/fixtures/embedding-remainder-main.db \
 *   QT_FIXTURE_ER_MOUNT=$V5/crates/quilltap-web/tests/fixtures/embedding-remainder-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-embedding-remainder.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots /tmp/qt-er-oracle/cases -- embedding-remainder
 */

import * as fs from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface RenderCase {
  name: string;
  chatId: string;
  fullReembed: boolean;
}
interface ReindexCase {
  name: string;
  profileId: string;
  scope: string | null;
}
interface QueueMemoriesCase {
  name: string;
  chatId: string;
  userId: string;
}
/** P4.d27 — one boot dimension-reconcile pass, with its default-profile prelude. */
interface DimensionReconcileCase {
  name: string;
  /** `UPDATE embedding_profiles SET isDefault = (id = ?)` when set. */
  setDefaultProfileId: string | null;
  /** `UPDATE embedding_profiles SET isDefault = 0` — reaches the no-profile arm. */
  clearDefault: boolean;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  renderNowIso: string;
  renderCases: RenderCase[];
  reindexCases: ReindexCase[];
  queueMemoriesCases: QueueMemoriesCase[];
  dimensionReconcileCases: DimensionReconcileCase[];
}

/** The natural sort key per diffed table — id-free, so minted rows still align. */
const SORT_KEYS: Record<string, string[]> = {
  chats: ['id'],
  conversation_chunks: ['chatId', 'interchangeIndex'],
  background_jobs: ['type', 'status', 'priority', 'payload', 'createdAt'],
  embedding_status: ['entityType', 'entityId', 'profileId'],
  memories: ['characterId', 'summary'],
  help_docs: ['path'],
  // P4.D77 — the full-scope reindex now clears section vectors too.
  help_doc_chunks: ['docId', 'chunkIndex'],
  vector_entries: ['characterId', 'id'],
  vector_indices: ['characterId'],
};

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  if (typeof v === 'bigint') return Number(v);
  return v;
}

function sortKey(table: string, row: Record<string, unknown>): string {
  return (SORT_KEYS[table] ?? ['id']).map((c) => String(canonValue(row[c]) ?? '')).join(' ');
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'embedding-remainder.json'), 'utf8')
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_ER_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_ER_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_ER_MAIN / QT_FIXTURE_ER_MOUNT must point at the committed fixtures');
  }

  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  // The COMMITTED help tree both sides sync from (shared with the P4.1b family).
  const helpTreeRoot = join(here, '..', 'fixtures', 'help-sync');
  if (!existsSync(join(helpTreeRoot, 'help'))) {
    throw new Error(`help tree missing at ${helpTreeRoot}/help`);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-er-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'er-main.db');
  const workMount = join(scratch, 'er-mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // Freeze the clock BEFORE importing any handler: the render's header carries a
  // `Current time:` line and every repo mint stamps `new Date()`.
  const FIXED_MS = Date.parse(spec.renderNowIso);
  const RealDate = Date;
  class FrozenDate extends RealDate {
    constructor(...args: unknown[]) {
      if (args.length === 0) {
        super(FIXED_MS);
      } else {
        // @ts-expect-error — forwarding the real Date overloads verbatim.
        super(...args);
      }
    }
    static now(): number {
      return FIXED_MS;
    }
  }
  (globalThis as unknown as { Date: unknown }).Date = FrozenDate;

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  // Past jest.setup's global mocks — this family needs the REAL data layer
  // (`jest-real-db-oracle`).
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories')
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store')
  );
  // ⚠ REQUIRED, and silent when missing: jest.setup mocks this module globally,
  // so `EMBEDDING_MAX_CHARS` imports as `undefined`. The reconcile binds it into
  // `LENGTH(content) BETWEEN 1 AND ?`, and an undefined bind is SQL NULL — the
  // comparison is then never true, so arm (B) matches NOTHING and the oracle
  // reports a plausible-looking, wrong result.
  jest.doMock('@/lib/embedding/embedding-service', () =>
    jest.requireActual('@/lib/embedding/embedding-service')
  );
  // The one stub (see the header): keep every enqueue from forking the job child.
  jest.doMock('@/lib/background-jobs/processor', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/background-jobs/processor'),
    ensureProcessorRunning: () => {},
  }));

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  // `HELP_DIR` is captured at module load from `process.cwd()` — chdir FIRST
  // (the cipher-driver path above was resolved before this, deliberately).
  process.chdir(helpTreeRoot);

  const { handleConversationRender } = await import(
    '@/lib/background-jobs/handlers/conversation-render'
  );
  const { handleEmbeddingReindexAll } = await import(
    '@/lib/background-jobs/handlers/embedding-reindex'
  );
  const { handleQueueMemories } = await import(
    '@/app/api/v1/chats/[id]/actions/memories'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { reconcileConversationRendering } = await import(
    '@/lib/startup/reconcile-conversation-rendering'
  );

  await initializeDatabase();

  const lines: string[] = [];

  // ── Phase 1: the startup reconcile, over the pristine fixture.
  const reconcile = await reconcileConversationRendering();
  lines.push(JSON.stringify({ kind: 'reconcile', ...reconcile }));

  // ── Phase 2: the render handler, corpus order, state accumulating.
  for (const [i, rc] of spec.renderCases.entries()) {
    const job = {
      id: `00000000-0000-4000-8000-00000000ff${String(i).padStart(2, '0')}`,
      userId: spec.userId,
      type: 'CONVERSATION_RENDER',
      status: 'PROCESSING',
      payload: { chatId: rc.chatId, fullReembed: rc.fullReembed },
    };
    let error: string | null = null;
    try {
      await handleConversationRender(job as never);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
    lines.push(
      JSON.stringify({
        kind: 'render',
        name: rc.name,
        outcome: error === null ? 'ok' : 'error',
        error,
      })
    );
  }

  // ── Phase 2b: the boot DIMENSION reconcile (P4.d27 / v4 `7391404e`), LAST
  // because each case's prelude rewrites which embedding profile is the default
  // — state the earlier phases depend on. The prelude itself is harness
  // scaffolding (one UPDATE, identical on both sides), not ported code; what is
  // measured is v4's REAL `reconcileEmbeddingDimensions()` and the rows it
  // leaves behind.
  const { reconcileEmbeddingDimensions } = await import(
    '@/lib/startup/reconcile-embedding-dimensions'
  );
  for (const dc of spec.dimensionReconcileCases) {
    if (dc.clearDefault) {
      await rawQuery('UPDATE embedding_profiles SET isDefault = 0');
    } else if (dc.setDefaultProfileId) {
      await rawQuery('UPDATE embedding_profiles SET isDefault = 0');
      await rawQuery('UPDATE embedding_profiles SET isDefault = 1 WHERE id = ?', [
        dc.setDefaultProfileId,
      ]);
    }
    const result = await reconcileEmbeddingDimensions();
    lines.push(JSON.stringify({ kind: 'dimReconcile', name: dc.name, ...result }));
  }

  // ── Phase 3: the reindex handler, corpus order.
  for (const [i, rc] of spec.reindexCases.entries()) {
    const job = {
      id: `00000000-0000-4000-8000-00000000ee${String(i).padStart(2, '0')}`,
      userId: spec.userId,
      type: 'EMBEDDING_REINDEX_ALL',
      status: 'PROCESSING',
      payload: rc.scope === null
        ? { profileId: rc.profileId }
        : { profileId: rc.profileId, scope: rc.scope },
    };
    let error: string | null = null;
    try {
      await handleEmbeddingReindexAll(job as never);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
    lines.push(
      JSON.stringify({
        kind: 'reindex',
        name: rc.name,
        outcome: error === null ? 'ok' : 'error',
        error,
      })
    );
  }

  // ── Phase 4: the queue-memories action, corpus order.
  const repos = getRepositories();
  for (const qc of spec.queueMemoriesCases) {
    const chat = await repos.chats.findById(qc.chatId);
    if (!chat) throw new Error(`queue-memories case ${qc.name}: chat not found`);
    const ctx = { user: { id: qc.userId }, repos } as never;
    const res = await handleQueueMemories({} as never, qc.chatId, chat as never, ctx);
    const body = await (res as Response).json();
    lines.push(
      JSON.stringify({
        kind: 'queueMemories',
        name: qc.name,
        status: (res as Response).status,
        body,
      })
    );
  }

  const dumpTable = async (table: string) => {
    const columns = (
      (await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>
    ).map((c) => c.name);
    const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
    const rows = rawRows
      .map((r) => {
        const out: Record<string, unknown> = {};
        for (const col of columns) out[col] = canonValue(r[col]);
        return out;
      })
      .sort((a, b) => {
        const av = sortKey(table, a);
        const bv = sortKey(table, b);
        return av < bv ? -1 : av > bv ? 1 : 0;
      });
    return { table, columns, rows };
  };

  for (const table of [
    'chats',
    'conversation_chunks',
    'background_jobs',
    'embedding_status',
    'memories',
    'help_docs',
    'help_doc_chunks',
    'vector_entries',
    'vector_indices',
  ]) {
    lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable(table)) }));
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`embedding-remainder oracle wrote ${outPath}\n`);
}

test('embedding-remainder tier-3 oracle', async () => {
  await main();
});
