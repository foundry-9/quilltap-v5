/**
 * Tier-2 oracle — P4.D288 (C1-3): v4's REAL `memoryIndex` (the index half of
 * `lib/memory/index-store/`, memory refactor Phase B, at the `83334b9a5` pin)
 * and the three new reads, over a fixture v4's own code builds.
 *
 * The case seeds `memories` (through `MemoriesRepository.create`, each with
 * its embedding — v4 stores the quantized blob), closes, copies the file to
 * `$QT_FIXTURE_MEMORY_INDEX_DIR/base.db` (the Rust side's start), re-opens and
 * runs the op list through v4's real modules, recording each op's result and
 * the lines it logged. **MEMORY_INDEX_ALWAYS_LOADED (L-2):** the first op is
 * `memoryIndex.shape` — it LOADS the character's cache, so every later op
 * sees v4's loaded state (v5 keeps no cache and loads on every read: its
 * per-op load lines are the recorded difference, both ways).
 *
 * Ops: `shape`; `search` under each `MemoryIndexSearchOptions` combination
 * (both tiers / hot only / an exclude / hot only + exclude); `upsert` (an
 * `alongside` edit in ONE update, a vanished row, another character's row);
 * `setTier` per `supersededById` state (set → `supersededAt` stamped, `null`
 * → cleared, absent → untouched) and a search that sees it; `remove`; the
 * three reads (`findActiveByType`, `findInstanceDefault`, `countCorpusSince`);
 * `enqueueMissingEmbeddings` — no profile, then `ok` with one memory DEDUPED
 * against an active job, then every entity already queued, then the
 * all-characters scope; `indexInserted` LAST (it writes nothing and would
 * leave v4's cache ahead of the rows). After the ops, the `memories` and
 * `background_jobs` rows are dumped (minted ids / timestamps masked on the
 * Rust side).
 *
 * The job host is held off (`__quilltapJobHost.shuttingDown`) so no job is
 * claimed mid-case.
 *
 * Run (Node 24, from the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   rm -rf /tmp/qt-memory-index-tier2 /tmp/oracle-memory-index-tier2.ndjson
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MEMORY_INDEX_DIR=/tmp/qt-memory-index-tier2 \
 *     $N/npx tsx $V5W/harness/oracle/cases/memory-index-tier2.ts \
 *     > /tmp/oracle-memory-index-tier2.ndjson
 */

import { copyFileSync, existsSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

(globalThis as Record<string, unknown>).__quilltapJobHost = {
  child: null,
  spawning: false,
  shuttingDown: true,
  childCrashed: false,
  restartTimestamps: [],
  signalHandlersInstalled: false,
  invalidationListeners: new Set(),
};

const PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
const USER = 'e2880000-0000-4000-8000-0000000000aa';
const CHAR = 'e2880000-0000-4000-8000-0000000000c1';
const OTHER = 'e2880000-0000-4000-8000-0000000000c2';
const mid = (n: number) => `e2880000-0000-4000-8000-${String(n).padStart(12, '0')}`;
const T0 = '2026-01-01T00:00:00.000Z';

function unit(v: number[]): Float32Array {
  const norm = Math.sqrt(v.reduce((a, x) => a + x * x, 0));
  return new Float32Array(v.map((x) => x / norm));
}

async function main(): Promise<void> {
  const dir = process.env.QT_FIXTURE_MEMORY_INDEX_DIR;
  if (!dir) throw new Error('QT_FIXTURE_MEMORY_INDEX_DIR must name the dir for base.db');
  mkdirSync(dir, { recursive: true });
  const work = join(dir, 'work.db');
  for (const p of [work, join(dir, 'base.db')]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) if (existsSync(p + suffix)) rmSync(p + suffix);
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-memory-index-tier2-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'debug';

  const CAPTURED = new Set([
    'Read memory index rows',
    'Skipped memory embeddings that could not be decoded',
    'Loaded vector index from memory rows',
    'Re-stamped vector tiers',
    'Upsert found no row; vector not cached',
    'Upserted memory vectors',
    'Indexed newly inserted memories',
    'Moved memories between tiers',
    'Moved memories between tiers through the index',
    'Evicted retired memories from the index',
    'Memory not found for update',
    'Memory does not belong to character',
    'Embedding batch: every entity already has a job',
    'Embedding generate batch enqueued',
    '[Memory backfill] Embedding jobs enqueued',
    'Error finding entities by filter',
    'Error finding entity by filter',
    'Error counting entities',
    'Error reading memory index rows',
    'Error loading vector index from memory rows',
    'Raw query failed',
  ]);
  const { logger } = await import('@/lib/logger');
  let captured: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as {
        level: string;
        message: string;
        context?: Record<string, unknown>;
        error?: { message?: string };
      };
      if (!CAPTURED.has(data.message)) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      if (data.error?.message !== undefined) fields.error = data.error.message;
      captured.push({ level: data.level, message: data.message, fields });
    },
  });

  const { initializeDatabase, ensureCollection, closeDatabase } = await import('@/lib/database/manager');
  const { MemorySchema, BackgroundJobSchema, EmbeddingProfileSchema } = await import('@/lib/schemas/types');
  const { MemoriesRepository } = await import('@/lib/database/repositories/memories.repository');
  const ensure = async () => {
    await initializeDatabase();
    await ensureCollection('memories', MemorySchema);
    await ensureCollection('background_jobs', BackgroundJobSchema);
    await ensureCollection('embedding_profiles', EmbeddingProfileSchema);
  };

  // ---- seed, then hand the Rust side the same starting file ----
  await ensure();
  {
    const repo = new MemoriesRepository();
    const seed: Array<[number, string, 'hot' | 'cold', number[] | null]> = [
      [1, CHAR, 'hot', [1, 0, 0]],
      [2, CHAR, 'hot', [0.9, 0.1, 0]],
      [3, CHAR, 'cold', [0.95, 0.05, 0]],
      [4, CHAR, 'hot', null],
      [5, CHAR, 'hot', null],
      [9, OTHER, 'hot', [0, 1, 0]],
    ];
    for (const [n, character, tier, v] of seed) {
      await repo.create(
        {
          characterId: character,
          content: `content ${n}`,
          summary: `summary ${n}`,
          tier,
          ...(v ? { embedding: unit(v) } : {}),
        } as never,
        { id: mid(n), createdAt: T0, updatedAt: T0 },
      );
    }
  }
  await closeDatabase();
  copyFileSync(work, join(dir, 'base.db'));
  await ensure();

  const { memoryIndex, enqueueMissingEmbeddings } = await import('@/lib/memory/index-store');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const repos = getRepositories();
  const ops: Array<Record<string, unknown>> = [];
  const run = async (op: string, input: unknown, f: () => Promise<unknown> | unknown) => {
    captured = [];
    const result = await f();
    ops.push({ op, input, result, lines: captured });
  };
  const hits = (r: Array<{ id: string; score: number; metadata: { tier?: string } }>) =>
    r.map((x) => ({ id: x.id, score: x.score, cold: x.metadata.tier === 'cold' }));
  const q = unit([1, 0, 0]);

  await run('shape', { dims: 3 }, () => memoryIndex.shape(CHAR, 3));
  for (const opts of [
    { limit: 10, hotOnly: false },
    { limit: 10, hotOnly: true },
    { limit: 10, hotOnly: false, exclude: [mid(1)] },
    { limit: 1, hotOnly: true, exclude: [mid(1)] },
  ]) {
    await run('search', opts, async () =>
      hits(
        (await memoryIndex.search(CHAR, q, {
          limit: opts.limit,
          hotOnly: opts.hotOnly,
          exclude: opts.exclude ? new Set(opts.exclude) : undefined,
        })) as never,
      ),
    );
  }
  await run(
    'upsert',
    [
      { id: mid(1), v: [0.8, 0.6, 0], alongside: { content: 'edited content', summary: 'edited summary' } },
      { id: mid(77), v: [0, 0, 1] },
      { id: mid(9), v: [0, 0, 1] },
    ],
    async () =>
      (
        await memoryIndex.upsert(CHAR, [
          {
            id: mid(1),
            embedding: unit([0.8, 0.6, 0]),
            alongside: { content: 'edited content', summary: 'edited summary' },
          },
          { id: mid(77), embedding: unit([0, 0, 1]) },
          { id: mid(9), embedding: unit([0, 0, 1]) },
        ])
      ).map((r) => r !== null),
  );
  await run('setTier', { ids: [mid(1), mid(2)], tier: 'cold', extra: { supersededById: mid(3), consolidatedAt: T0 } }, () =>
    memoryIndex.setTier(CHAR, [mid(1), mid(2)], 'cold', { supersededById: mid(3), consolidatedAt: T0 }),
  );
  await run('search', { limit: 10, hotOnly: true }, async () =>
    hits((await memoryIndex.search(CHAR, q, { limit: 10, hotOnly: true })) as never),
  );
  await run('setTier', { ids: [mid(2)], tier: 'hot', extra: { supersededById: null } }, () =>
    memoryIndex.setTier(CHAR, [mid(2)], 'hot', { supersededById: null }),
  );
  await run('setTier', { ids: [mid(3)], tier: 'hot', extra: {} }, () => memoryIndex.setTier(CHAR, [mid(3)], 'hot'));
  await run('setTier', { ids: [], tier: 'cold', extra: {} }, () => memoryIndex.setTier(CHAR, [], 'cold'));
  await run('remove', { ids: [mid(2), mid(78)] }, () => memoryIndex.remove(CHAR, [mid(2), mid(78)]));
  await run('remove', { ids: [] }, () => memoryIndex.remove(CHAR, []));

  // ---- the three reads ----
  await run('countCorpusSince', { since: null }, () => repos.memories.countCorpusSince(null));
  await run('countCorpusSince', { since: '2026-01-01T00:00:00.000Z' }, () =>
    repos.memories.countCorpusSince('2026-01-01T00:00:00.000Z'),
  );
  await run('countCorpusSince', { since: '2027-01-01T00:00:00.000Z' }, () =>
    repos.memories.countCorpusSince('2027-01-01T00:00:00.000Z'),
  );
  await run('findInstanceDefault', {}, async () => (await repos.embeddingProfiles.findInstanceDefault())?.id ?? null);

  // ---- enqueueMissingEmbeddings: no profile → ok (+ dedupe) → all queued → all characters ----
  await run('enqueueMissingEmbeddings', { characterId: CHAR, limit: 10 }, () =>
    enqueueMissingEmbeddings({ userId: USER, characterId: CHAR, limit: 10 }),
  );
  await repos.embeddingProfiles.create(
    { userId: USER, name: 'Default', provider: 'OPENAI', modelName: 'text-embedding-3-small', isDefault: true } as never,
    { id: 'e2880000-0000-4000-8000-0000000000e1', createdAt: T0, updatedAt: T0 },
  );
  await repos.backgroundJobs.create(
    {
      userId: USER,
      type: 'EMBEDDING_GENERATE',
      status: 'PENDING',
      payload: { entityType: 'MEMORY', entityId: mid(5), characterId: CHAR, profileId: 'e2880000-0000-4000-8000-0000000000e1' },
      priority: 10,
      attempts: 0,
      maxAttempts: 3,
      lastError: null,
      scheduledAt: T0,
      startedAt: null,
      completedAt: null,
    } as never,
    { id: 'e2880000-0000-4000-8000-0000000000f1', createdAt: T0, updatedAt: T0 },
  );
  await run('findInstanceDefault', {}, async () => (await repos.embeddingProfiles.findInstanceDefault())?.id ?? null);
  await run('findActiveByType', { type: 'EMBEDDING_GENERATE' }, async () =>
    (await repos.backgroundJobs.findActiveByType('EMBEDDING_GENERATE')).map((j) => j.id),
  );
  await run('enqueueMissingEmbeddings', { characterId: CHAR, limit: 10 }, () =>
    enqueueMissingEmbeddings({ userId: USER, characterId: CHAR, limit: 10 }),
  );
  await run('enqueueMissingEmbeddings', { characterId: CHAR, limit: 10 }, () =>
    enqueueMissingEmbeddings({ userId: USER, characterId: CHAR, limit: 10 }),
  );
  await run('enqueueMissingEmbeddings', { characterId: null, limit: 10 }, () =>
    enqueueMissingEmbeddings({ userId: USER, limit: 10 }),
  );
  await run('findActiveByType', { type: 'EMBEDDING_GENERATE' }, async () =>
    (await repos.backgroundJobs.findActiveByType('EMBEDDING_GENERATE')).length,
  );

  // ---- indexInserted LAST: it writes nothing ----
  await run('indexInserted', { ids: [mid(2)] }, () => memoryIndex.indexInserted(CHAR, [{ id: mid(2), embedding: unit([0.9, 0.1, 0]) }]));
  await run('indexInserted', { ids: [] }, () => memoryIndex.indexInserted(CHAR, []));

  // ---- the end state (before the fallback arms drop columns) ----
  const { rawQuery } = await import('@/lib/database/manager');
  const memories = await rawQuery(
    `SELECT id, characterId, content, summary, tier, supersededById, supersededAt, consolidatedAt,
            quote(embedding) AS embedding, createdAt, updatedAt FROM memories ORDER BY id`,
  );
  const jobs = await rawQuery(
    `SELECT id, userId, type, status, payload, priority, attempts, maxAttempts, scheduledAt, createdAt, updatedAt
       FROM background_jobs ORDER BY createdAt, payload`,
  );

  // ---- the three reads' (and the row read's) FALLBACK arms: a column each
  // read needs is dropped (the same SQL on both sides), then the read ----
  const { rawQuery: plantQuery } = await import('@/lib/database/manager');
  const plant = async (sql: string) => {
    captured = [];
    for (const stmt of sql.split(';').map((x) => x.trim()).filter(Boolean)) await plantQuery(stmt);
    ops.push({ op: 'plant', input: sql, result: null, lines: captured });
  };
  await plant('ALTER TABLE background_jobs DROP COLUMN status');
  await run('findActiveByType', { type: 'EMBEDDING_GENERATE' }, async () =>
    (await repos.backgroundJobs.findActiveByType('EMBEDDING_GENERATE')).map((j) => j.id),
  );
  await plant('ALTER TABLE embedding_profiles DROP COLUMN isDefault');
  await run('findInstanceDefault', {}, async () => (await repos.embeddingProfiles.findInstanceDefault())?.id ?? null);
  await plant('DROP INDEX "idx_memories_createdAt"; ALTER TABLE memories DROP COLUMN createdAt');
  await run('countCorpusSince', { since: '2026-01-01T00:00:00.000Z' }, () =>
    repos.memories.countCorpusSince('2026-01-01T00:00:00.000Z'),
  );
  await plant('ALTER TABLE memories DROP COLUMN tier');
  await run('shape', { dims: 3 }, async () => {
    memoryIndex.reload(CHAR);
    return memoryIndex.shape(CHAR, 3);
  });

  await closeDatabase();
  process.stdout.write(JSON.stringify({ case: 'memory-index-tier2', ops, memories, jobs }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memory-index-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
