/**
 * Tier-2 oracle — v4's REAL `CharacterVectorStore` tier stamping (P4.D265, v4
 * `d58548051`, `lib/embedding/vector-store.ts`).
 *
 * Builds its own fixture at `QT_FIXTURE_VECTOR_STORE_TIERS` through v4's REAL
 * code (memories through `MemoriesRepository.create` — two of them `cold`, one
 * cold memory with NO vector so the load's `cold` count is the cold-id SET's
 * size, not the stamped entries'; the vectors through a v4 store's
 * `addVector` + `save`), then — in a fresh store — records:
 *
 *   - `load()` and its DEBUG `Loaded vector index {context, characterId,
 *     entries, cold}`;
 *   - `search(q, k, isHotVector)` vs `search(q, k)` — a cold entry that would
 *     rank FIRST is excluded before the top-K and `k` hot results still come
 *     back; the unfiltered search keeps both tiers;
 *   - `setTier(ids, tier)` — the changed count (an absent id, a no-op
 *     re-stamp) and its DEBUG `Re-stamped vector tiers {context, characterId,
 *     tier, requested, changed}`, then a search that sees the re-stamp.
 *
 * Every vector is unit-length, so v4's cosine and v5's agree to the last
 * float; scores are compared within 1e-12 on the Rust side, ids exactly.
 * Output: one NDJSON line `{case, characterId, ops: [{op, …, lines}]}`.
 *
 * Run (Node 24, from the v4 checkout or the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   rm -f /tmp/oracle-vector-store-tiers.ndjson /tmp/qt-vector-store-tiers.db
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_VECTOR_STORE_TIERS=/tmp/qt-vector-store-tiers.db \
 *     $N/npx tsx $V5W/harness/oracle/cases/vector-store-tiers.ts \
 *     > /tmp/oracle-vector-store-tiers.ndjson
 */

import { existsSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
const CHAR = 'e2650000-0000-4000-8000-0000000000c1';
const mid = (n: number) => `e2650000-0000-4000-8000-${String(n).padStart(12, '0')}`;

function unit(v: number[]): Float32Array {
  const norm = Math.sqrt(v.reduce((a, x) => a + x * x, 0));
  return new Float32Array(v.map((x) => x / norm));
}

async function main(): Promise<void> {
  const out = process.env.QT_FIXTURE_VECTOR_STORE_TIERS;
  if (!out) throw new Error('QT_FIXTURE_VECTOR_STORE_TIERS must name the fixture .db to write');
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    if (existsSync(out + suffix)) rmSync(out + suffix);
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-vector-store-tiers-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'debug';

  const CAPTURED = new Set([
    'Loaded vector index',
    'Re-stamped vector tiers',
    'Could not read cold memory ids; treating every vector as hot',
  ]);
  const { logger } = await import('@/lib/logger');
  let captured: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown> };
      if (!CAPTURED.has(data.message)) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      captured.push({ level: data.level, message: data.message, fields });
    },
  });

  const { initializeDatabase, ensureCollection, closeDatabase } = await import('@/lib/database/manager');
  const { MemoriesRepository } = await import('@/lib/database/repositories/memories.repository');
  const { MemorySchema } = await import('@/lib/schemas/types');
  const { CharacterVectorStore, isHotVector } = await import('@/lib/embedding/vector-store');

  await initializeDatabase();
  await ensureCollection('memories', MemorySchema);
  const repo = new MemoriesRepository();
  const TS = '2026-01-01T00:00:00.000Z';
  const tiers: Array<[number, 'hot' | 'cold']> = [
    [1, 'cold'],
    [2, 'hot'],
    [3, 'hot'],
    [4, 'cold'],
    [5, 'hot'],
    [6, 'cold'],
  ];
  for (const [n, tier] of tiers) {
    await repo.create(
      { characterId: CHAR, content: `v${n}`, summary: `v${n}`, tier } as never,
      { id: mid(n), createdAt: TS, updatedAt: TS },
    );
  }
  const vectors: Array<[number, number[]]> = [
    [1, [1, 0, 0]],
    [2, [0.9, 0.1, 0]],
    [3, [0.5, 0.5, 0]],
    [4, [0.95, 0.05, 0]],
    [5, [0, 1, 0]],
  ];
  const builder = new CharacterVectorStore(CHAR);
  await builder.load();
  for (const [n, v] of vectors) {
    await builder.addVector(mid(n), unit(v), { memoryId: mid(n), characterId: CHAR });
  }
  await builder.save();

  // A fresh store: the load under test.
  const store = new CharacterVectorStore(CHAR);
  const ops: Array<Record<string, unknown>> = [];
  const hits = (r: Array<{ id: string; score: number }>) => r.map((x) => ({ id: x.id, score: x.score }));
  const q = unit([1, 0, 0]);

  captured = [];
  await store.load();
  ops.push({ op: 'load', size: store.size, lines: captured });

  const search = (label: string, k: number, hotOnly: boolean) => {
    captured = [];
    const r = store.search(q, k, hotOnly ? isHotVector : undefined);
    ops.push({ op: 'search', label, k, hotOnly, hits: hits(r), lines: captured });
  };
  const setTier = (ids: string[], tier: 'hot' | 'cold') => {
    captured = [];
    store.setTier(ids, tier);
    ops.push({ op: 'setTier', ids, tier, lines: captured });
  };

  search('hot top-2: the cold best match is filtered before top-K', 2, true);
  search('both tiers top-2', 2, false);
  search('hot all', 10, true);
  setTier([mid(2), mid(99)], 'cold');
  search('hot all after demoting m2', 10, true);
  setTier([mid(2)], 'cold');
  setTier([mid(1)], 'hot');
  search('hot top-2 after promoting m1', 2, true);

  await closeDatabase();
  process.stdout.write(JSON.stringify({ case: 'vector-store-tiers', characterId: CHAR, ops }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`vector-store-tiers oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
