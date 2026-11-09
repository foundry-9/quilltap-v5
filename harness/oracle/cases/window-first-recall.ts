/**
 * Tier-2 oracle — P4.D375 (v4 4.12.1 `6121f59b9`, the `624d0d25c` pin): the
 * window-first dated search through v4's REAL code over a fixture v4's own
 * code builds.
 *
 * Drives: `repos.memories.findIdsInEventWindow` (both `hotOnly`, the
 * string-compared window carried VERBATIM — R-3), the REAL `gather` (vector:
 * the index re-searched over the window's ids; text: the period's newest rows
 * through the `Date.parse` comparator) and the REAL `searchRecall` on the TOOL
 * path (no recall context: a ≤ 14-day window → `floorRelaxed`, a 31-day one →
 * not, an undated query → no `window` trace keys) and on the PROMPT path (a
 * recall context: never relaxed). `searchRecall` is what `searchMemories`
 * calls once it has embedded the text; the query is built here with its
 * vector in hand so no embedding service runs (tsx — no jest mocks).
 *
 * The case seeds `memories` through `MemoriesRepository.create` (each vector
 * on the row), plants the event-time shapes v4's schema would refuse at
 * create time by raw `UPDATE`, closes, copies the file to
 * `$QT_FIXTURE_WFR_DIR/base.db` (the Rust side's start — minted per run, not
 * committed), re-opens and runs the op list, recording each op's result, its
 * trace and the lines it logged. Every op READS; the last op drops
 * `occurredAt` to drive the read's fallback arm.
 *
 * Run (Node 24, from the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   rm -rf /tmp/qt-window-first-recall /tmp/oracle-window-first-recall.ndjson
 *   cd ~/source/quilltap-server
 *   TZ=UTC QT_FIXTURE_WFR_DIR=/tmp/qt-window-first-recall \
 *     $N/npx tsx $V5W/harness/oracle/cases/window-first-recall.ts \
 *     > /tmp/oracle-window-first-recall.ndjson
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
const CHAR = 'd3750000-0000-4000-8000-0000000000c1';
const mid = (n: number) => `d3750000-0000-4000-8000-${String(n).padStart(12, '0')}`;
const T0 = '2026-01-01T00:00:00.000Z';
const NOW_MS = Date.parse('2026-11-04T14:00:00.000Z');

/** A unit vector whose cosine against [1, 0, 0] is `c`. */
function cosVec(c: number): Float32Array {
  return new Float32Array([c, Math.sqrt(Math.max(0, 1 - c * c)), 0]);
}

/** One row of the corpus: id number, cosine, event time (raw), tier, content. */
interface Row {
  n: number;
  cos: number | null;
  occurredAt: string | null;
  createdAt: string;
  tier: 'hot' | 'cold';
  content: string;
}

function corpus(): Row[] {
  const rows: Row[] = [];
  // Months of better-matching rows outside the window (the Friday shape).
  for (let i = 0; i < 20; i++) {
    const month = i < 10 ? '09' : '10';
    const day = String((i % 10) + 1).padStart(2, '0');
    rows.push({
      n: 100 + i,
      cos: 0.6 + (i % 10) * 0.02,
      occurredAt: `2026-${month}-${day}T18:00:00.000Z`,
      createdAt: `2026-${month}-${day}T18:00:00.000Z`,
      tier: 'hot',
      content: `A long swim in the pool, number ${i}.`,
    });
  }
  // The window's own rows: loosely matching, on Nov 3 (UTC).
  [0.28, 0.29, 0.31, 0.33].forEach((c, i) =>
    rows.push({
      n: 200 + i,
      cos: c,
      occurredAt: `2026-11-03T22:${String(40 + i * 4).padStart(2, '0')}:00.000Z`,
      createdAt: '2026-11-04T01:00:00.000Z',
      tier: 'hot',
      content: `Dinner by the water, course ${i}.`,
    }),
  );
  // occurredAt NULL → the COALESCE reads createdAt (in the window).
  rows.push({ n: 210, cos: 0.2, occurredAt: null, createdAt: '2026-11-03T10:00:00.000Z', tier: 'hot', content: 'A note about the pool filter.' });
  // A cold row in the window.
  rows.push({ n: 211, cos: 0.35, occurredAt: '2026-11-03T12:00:00.000Z', createdAt: T0, tier: 'cold', content: 'An archived pool memory.' });
  // The excluded row in the window.
  rows.push({ n: 212, cos: 0.32, occurredAt: '2026-11-03T13:00:00.000Z', createdAt: T0, tier: 'hot', content: 'An excluded pool memory.' });
  // R-3 shapes, planted raw (v4's schema refuses some at create time).
  rows.push({ n: 300, cos: 0.3, occurredAt: '2026-11-03', createdAt: T0, tier: 'hot', content: 'Date-only pool day.' });
  rows.push({ n: 301, cos: 0.3, occurredAt: '2026-11-03T01:00:00+05:00', createdAt: T0, tier: 'hot', content: 'Offset pool time.' });
  rows.push({ n: 302, cos: 0.3, occurredAt: '2026-11-03T23:59:59Z', createdAt: T0, tier: 'hot', content: 'No-millis pool midnight.' });
  rows.push({ n: 303, cos: 0.3, occurredAt: 'soon', createdAt: T0, tier: 'hot', content: 'Unparseable pool moment.' });
  rows.push({ n: 304, cos: 0.3, occurredAt: '2026-11-03T12:61:00.000Z', createdAt: T0, tier: 'hot', content: 'Invalid-minute pool moment.' });
  // In the window with no vector (the vector arm cannot score it; the text arm can).
  rows.push({ n: 305, cos: null, occurredAt: '2026-11-03T15:00:00.000Z', createdAt: T0, tier: 'hot', content: 'An unembedded pool moment.' });
  return rows;
}

const W_DAY = { from: '2026-11-03T00:00:00.000Z', to: '2026-11-03T23:59:59.999Z' };
const W_14 = { from: '2026-10-20T23:59:59.999Z', to: '2026-11-03T23:59:59.999Z' };
const W_14_1MS = { from: '2026-10-20T23:59:59.998Z', to: '2026-11-03T23:59:59.999Z' };
const W_31 = { from: '2026-10-04T00:00:00.000Z', to: '2026-11-03T23:59:59.999Z' };
const W_WIDE = { from: '2026-09-01T00:00:00.000Z', to: '2026-11-30T23:59:59.999Z' };

async function main(): Promise<void> {
  const dir = process.env.QT_FIXTURE_WFR_DIR;
  if (!dir) throw new Error('QT_FIXTURE_WFR_DIR must name the dir for base.db');
  mkdirSync(dir, { recursive: true });
  const work = join(dir, 'work.db');
  for (const p of [work, join(dir, 'base.db')]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) if (existsSync(p + suffix)) rmSync(p + suffix);
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-window-first-recall-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'debug';

  const CAPTURED = new Set([
    'Found memories in event window',
    'Error finding memories in event window',
    '[Memory] Window-first gather',
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
      const { service: _s, environment: _e, module: _m, ...fields } = data.context ?? {};
      if (data.error?.message !== undefined) fields.error = data.error.message;
      captured.push({ level: data.level, message: data.message, fields });
    },
  });

  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { MemorySchema } = await import('@/lib/schemas/types');
  const { MemoriesRepository } = await import('@/lib/database/repositories/memories.repository');
  const ensure = async () => {
    await initializeDatabase();
    await ensureCollection('memories', MemorySchema);
  };

  // ---- seed, plant the raw shapes, then hand the Rust side the same file ----
  await ensure();
  const rows = corpus();
  {
    const repo = new MemoriesRepository();
    for (const r of rows) {
      await repo.create(
        {
          characterId: CHAR,
          content: r.content,
          summary: r.content,
          importance: 0.5,
          reinforcedImportance: 0.5,
          tier: r.tier,
          occurredAt: null,
          ...(r.cos !== null ? { embedding: cosVec(r.cos) } : {}),
        } as never,
        { id: mid(r.n), createdAt: r.createdAt, updatedAt: r.createdAt },
      );
    }
    for (const r of rows) {
      if (r.occurredAt !== null) {
        await rawQuery('UPDATE memories SET occurredAt = ? WHERE id = ?', [r.occurredAt, mid(r.n)]);
      }
    }
  }
  await closeDatabase();
  copyFileSync(work, join(dir, 'base.db'));
  await ensure();

  const { memoryIndex } = await import('@/lib/memory/index-store');
  memoryIndex.reloadAll();
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { gather } = await import('@/lib/memory/recall/gather');
  const { searchRecall } = await import('@/lib/memory/recall/search');
  const repos = getRepositories();

  const ops: Array<Record<string, unknown>> = [];
  const run = async (op: string, input: unknown, f: (trace: unknown[]) => Promise<unknown>) => {
    captured = [];
    const trace: unknown[] = [];
    const result = await f(trace);
    ops.push({ op, input, result, trace, lines: captured });
  };
  const q = new Float32Array([1, 0, 0]);
  const queryOf = (window: { from: string; to: string } | null, embedded: boolean) => ({
    text: 'pool yesterday',
    embedding: embedded ? { embedding: q, provider: 'OPENAI', model: 'probe', dimensions: 3 } : null,
    probes: [],
    signals: null,
    anchors: [],
    window,
    retrospective: false,
  });
  const EXCLUDED = [mid(212)];

  // ---- findIdsInEventWindow (sorted: the read has no ORDER BY) ----
  for (const [window, hotOnly] of [
    [W_DAY, false],
    [W_DAY, true],
    [W_WIDE, true],
  ] as const) {
    await run('findIds', { window, hotOnly }, async () =>
      [...(await repos.memories.findIdsInEventWindow(CHAR, window, { hotOnly }))].sort(),
    );
  }

  // ---- gather, directly ----
  type GatherIn = {
    window: { from: string; to: string } | null;
    embedded: boolean;
    limit: number;
    includeCold: boolean;
    exclude: boolean;
    forceText: boolean;
  };
  const gathers: GatherIn[] = [
    { window: W_DAY, embedded: true, limit: 10, includeCold: true, exclude: true, forceText: false },
    { window: W_DAY, embedded: true, limit: 10, includeCold: false, exclude: false, forceText: false },
    { window: null, embedded: true, limit: 10, includeCold: true, exclude: false, forceText: false },
    { window: W_DAY, embedded: true, limit: 1, includeCold: false, exclude: true, forceText: true },
    { window: W_DAY, embedded: false, limit: 10, includeCold: true, exclude: false, forceText: false },
    { window: W_DAY, embedded: false, limit: 10, includeCold: false, exclude: true, forceText: false },
    { window: null, embedded: false, limit: 10, includeCold: true, exclude: false, forceText: false },
  ];
  for (const g of gathers) {
    await run('gather', g, async (trace) => {
      const out = await gather(
        CHAR,
        queryOf(g.window, g.embedded) as never,
        {
          limit: g.limit,
          includeCold: g.includeCold,
          excludeIds: g.exclude ? new Set(EXCLUDED) : undefined,
          forceText: g.forceText,
        },
        trace as never,
      );
      return { strategy: out.strategy, hits: out.hits.map((h) => ({ id: h.id, score: h.score, via: h.via })) };
    });
  }

  // ---- searchRecall: the tool path and the prompt path ----
  type SearchIn = {
    window: { from: string; to: string } | null;
    embedded: boolean;
    limit: number;
    includeCold: boolean;
    forceText: boolean;
    recallContext: boolean;
  };
  const searches: SearchIn[] = [
    { window: W_DAY, embedded: true, limit: 10, includeCold: true, forceText: false, recallContext: false },
    { window: W_14, embedded: true, limit: 10, includeCold: true, forceText: false, recallContext: false },
    { window: W_14_1MS, embedded: true, limit: 10, includeCold: true, forceText: false, recallContext: false },
    { window: W_31, embedded: true, limit: 10, includeCold: true, forceText: false, recallContext: false },
    { window: null, embedded: true, limit: 10, includeCold: true, forceText: false, recallContext: false },
    { window: W_DAY, embedded: true, limit: 10, includeCold: false, forceText: false, recallContext: true },
    { window: W_DAY, embedded: true, limit: 10, includeCold: true, forceText: true, recallContext: false },
    { window: W_DAY, embedded: false, limit: 3, includeCold: false, forceText: false, recallContext: false },
  ];
  for (const s of searches) {
    await run('search', s, async (trace) => {
      const out = await searchRecall(
        CHAR,
        queryOf(s.window, s.embedded) as never,
        {
          limit: s.limit,
          includeCold: s.includeCold,
          literalPhrase: true,
          forceText: s.forceText,
          nowMs: NOW_MS,
          ...(s.recallContext ? { recallContext: { currentProjectId: null, scopePolicy: 'down-weight' } as never } : {}),
        },
        trace as never,
      );
      return {
        strategy: out.strategy,
        candidates: out.candidates.map((c) => ({ id: c.memory.id, score: c.score, salience: c.salience })),
      };
    });
  }

  // ---- the read's fallback arm: a column it needs is gone ----
  captured = [];
  await rawQuery('ALTER TABLE memories DROP COLUMN occurredAt');
  ops.push({ op: 'plant', input: 'ALTER TABLE memories DROP COLUMN occurredAt', result: null, trace: [], lines: captured });
  await run('findIds', { window: W_DAY, hotOnly: true }, async () =>
    [...(await repos.memories.findIdsInEventWindow(CHAR, W_DAY, { hotOnly: true }))].sort(),
  );

  await closeDatabase();
  process.stdout.write(JSON.stringify({ case: 'window-first-recall', characterId: CHAR, excluded: EXCLUDED, ops }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`window-first-recall oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
