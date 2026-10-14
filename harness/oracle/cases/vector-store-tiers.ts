/**
 * Tier-2 oracle — v4's REAL `CharacterVectorStore` over memory ROWS (P4.D288,
 * rewritten for the `83334b9a5` pin: `lib/embedding/vector-store.ts` after
 * `af48f6cc2`, memory refactor Phase B — the mirror is gone, a store loads
 * `id, embedding, tier` from `memories` through
 * `MemoriesRepository.findIndexRowsByCharacterId`).
 *
 * Builds its own fixture at `QT_FIXTURE_VECTOR_STORE_TIERS` through v4's REAL
 * code (`MemoriesRepository.create` with each embedding — v4 stores the
 * QUANTIZED blob — and `rawQuery` for the rows a writer never produces), then
 * records, in fresh stores:
 *
 *   - `load()` — its `Read memory index rows` DEBUG, the `Skipped memory
 *     embeddings that could not be decoded` WARN (a garbage TEXT cell and an
 *     EMPTY blob are counted and skipped; a JSON-array TEXT cell decodes), and
 *     `Loaded vector index from memory rows {context, characterId, entries,
 *     cold, dimensions}`;
 *   - a MIXED corpus (3-d majority, a 2-d minority): `getDimensions()` the
 *     dominant length, `countWithDimensions`, a 2-d query scoring only the 2-d
 *     entries, a 4-d query → the `Query vector dimension mismatch` WARN and
 *     `[]`;
 *   - a second character whose two lengths TIE (2-d inserted first) — the
 *     first length to reach the count wins;
 *   - `search(q, k, isHotVector)` vs `search(q, k)` (a cold best match filtered
 *     before the top-K), each hit's cold TAG;
 *   - `setTier(ids, tier)` — the changed count (an absent id, a no-op
 *     re-stamp) and its `Re-stamped vector tiers` DEBUG, then a search that
 *     sees it.
 *
 * Scores are compared within 1e-12 on the Rust side, ids and tiers exactly.
 * Output: one NDJSON line `{case, characterId, ops: [{op, …, lines}]}`.
 *
 * Run (Node 24, from the round's pin):
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
const CHAR_TIE = 'e2650000-0000-4000-8000-0000000000c2';
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
    'Read memory index rows',
    'Skipped memory embeddings that could not be decoded',
    'Loaded vector index from memory rows',
    'Error loading vector index from memory rows',
    'Query vector dimension mismatch — search embedding profile likely differs from index',
    'Re-stamped vector tiers',
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

  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { MemoriesRepository } = await import('@/lib/database/repositories/memories.repository');
  const { MemorySchema } = await import('@/lib/schemas/types');
  const { CharacterVectorStore, isHotVector } = await import('@/lib/embedding/vector-store');

  await initializeDatabase();
  await ensureCollection('memories', MemorySchema);
  const repo = new MemoriesRepository();
  const TS = '2026-01-01T00:00:00.000Z';
  const rows: Array<[number, 'hot' | 'cold', number[] | null, string]> = [
    [1, 'cold', [1, 0, 0], CHAR],
    [2, 'hot', [0.9, 0.1, 0], CHAR],
    [3, 'hot', [0.5, 0.5, 0], CHAR],
    [4, 'cold', [0.95, 0.05, 0], CHAR],
    [5, 'hot', [0, 1, 0], CHAR],
    [6, 'cold', null, CHAR],
    [7, 'hot', [1, 0], CHAR],
    [8, 'cold', [0.6, 0.8], CHAR],
    [21, 'hot', [1, 0], CHAR_TIE],
    [22, 'hot', [1, 0, 0], CHAR_TIE],
    [23, 'hot', [0, 1], CHAR_TIE],
    [24, 'hot', [0, 1, 0], CHAR_TIE],
  ];
  for (const [n, tier, v, character] of rows) {
    await repo.create(
      {
        characterId: character,
        content: `v${n}`,
        summary: `v${n}`,
        tier,
        ...(v ? { embedding: unit(v) } : {}),
      } as never,
      { id: mid(n), createdAt: TS, updatedAt: TS },
    );
  }
  // Rows no writer produces: undecodable TEXT, an EMPTY blob, a JSON-array TEXT.
  for (const [n, embedding] of [
    [9, 'garbage'],
    [10, Buffer.alloc(0)],
    [11, '[0, 0, 1]'],
  ] as Array<[number, unknown]>) {
    await repo.create(
      { characterId: CHAR, content: `v${n}`, summary: `v${n}`, tier: 'hot' } as never,
      { id: mid(n), createdAt: TS, updatedAt: TS },
    );
    await rawQuery('UPDATE memories SET embedding = ? WHERE id = ?', [embedding, mid(n)]);
  }

  const ops: Array<Record<string, unknown>> = [];
  const hits = (r: Array<{ id: string; score: number; metadata: { tier?: string } }>) =>
    r.map((x) => ({ id: x.id, score: x.score, cold: x.metadata.tier === 'cold' }));
  const q3 = unit([1, 0, 0]);

  // The tie: a fresh store for the second character.
  {
    const tie = new CharacterVectorStore(CHAR_TIE);
    captured = [];
    await tie.load();
    ops.push({
      op: 'load',
      characterId: CHAR_TIE,
      size: tie.size,
      dimensions: tie.getDimensions(),
      count2: tie.countWithDimensions(2),
      count3: tie.countWithDimensions(3),
      lines: captured,
    });
  }

  const store = new CharacterVectorStore(CHAR);
  captured = [];
  await store.load();
  ops.push({
    op: 'load',
    characterId: CHAR,
    size: store.size,
    dimensions: store.getDimensions(),
    count2: store.countWithDimensions(2),
    count3: store.countWithDimensions(3),
    lines: captured,
  });

  const search = (label: string, q: Float32Array, k: number, hotOnly: boolean) => {
    captured = [];
    const r = store.search(q, k, hotOnly ? isHotVector : undefined);
    ops.push({ op: 'search', label, q: Array.from(q), k, hotOnly, hits: hits(r as never), lines: captured });
  };
  const setTier = (ids: string[], tier: 'hot' | 'cold') => {
    captured = [];
    store.setTier(ids, tier);
    ops.push({ op: 'setTier', ids, tier, lines: captured });
  };

  search('hot top-2: the cold best match is filtered before top-K', q3, 2, true);
  search('both tiers top-2 (cold tags)', q3, 2, false);
  search('hot all', q3, 10, true);
  setTier([mid(2), mid(99)], 'cold');
  search('hot all after demoting m2', q3, 10, true);
  setTier([mid(2)], 'cold');
  setTier([mid(1)], 'hot');
  search('hot top-2 after promoting m1', q3, 2, true);
  search('a 2-d query scores only the 2-d entries', unit([1, 0]), 10, false);
  search('a 4-d query matches no entry: the WARN and []', unit([1, 0, 0, 0]), 10, false);

  await closeDatabase();
  process.stdout.write(JSON.stringify({ case: 'vector-store-tiers', characterId: CHAR, ops }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`vector-store-tiers oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
