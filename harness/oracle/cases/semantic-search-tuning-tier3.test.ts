/**
 * @jest-environment node
 *
 * Tier-3 ORACLE (P4.D267; REWRITTEN by P4.D291 onto Phase E): v4's REAL
 * `searchMemories` (`lib/memory/recall/search.ts`, `584d28164`, pinned at
 * `83334b9a5`) — `buildRecallQuery` then `searchRecall`'s gather / hydrate /
 * score / select — the recall retuning's R1 gate (and its gate-off for a
 * lexical provider), R4 specific anchors, R6 background reservation, the
 * embedding memo, the knobs (`excludeIds`, `nowMs`, `headSize`), hot-only
 * search (`includeCold`), the hybrid literal step, and bug 214's text
 * fallback honouring every option (window, exclusions, multipliers, the
 * salience clock, probes + anchors) — over a fresh copy of the committed
 * `semantic-search-tuning-main.db` per case. Plus the pure
 * `reserveBackgroundSlots` (`lib/memory/scoring/rank.ts`) over the jest
 * vectors.
 *
 * ONLY `generateEmbeddingForUser` is mocked: each case scripts text →
 * `{embedding, model: 'canned', dimensions, provider}` (or a thrown
 * `EmbeddingError`), and every call is counted (the memo comparand). The
 * index store, the repositories and the cosine are REAL. `Date` is frozen to
 * the corpus `$nowMs` (the decay seam; the Rust side passes the same value as
 * `now_ms`). The search's `[Memory]` / `[Recall]` lines are captured by
 * splicing a transport into the singleton logger; `lastAccessedAt` is dumped
 * before and after each case (a search stamps nothing — `deliver` does).
 *
 * Real-DB-under-jest (the memory-gate-tier3 recipe): resetModules + doMock
 * past jest.setup's global DB mocks + better-sqlite3 →
 * better-sqlite3-multiple-ciphers. Stage the case OUTSIDE any `.claude/`
 * path (v4's jest ignores it).
 *
 * Run (Node 24, from the v4 checkout pinned at the baseline):
 *   see semantic_search_tuning_tier3_equivalence.rs's header for the recipe.
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

type EmbedSpec = { vector?: number[]; provider?: string; throw?: boolean };
interface CallSpec {
  characterId: string;
  query: string;
  options: Record<string, any>;
  memo?: boolean;
}
interface CaseSpec {
  name: string;
  embed: Record<string, EmbedSpec>;
  calls: CallSpec[];
}
interface ReserveSpec {
  name: string;
  ranked: Array<[string, number]>;
  background: Array<[string, number]>;
  headSize: number;
  fraction: number;
}
interface Spec {
  $nowMs: number;
  userId: string;
  cases: CaseSpec[];
  reserve: ReserveSpec[];
}

/** The search lines this family pins (v4's exact messages). */
const CAPTURED = new Set([
  '[Memory] Specific entity anchors chosen',
  '[Memory] Semantic search failed, falling back to text search',
  '[Recall] Query embedding failed; recall falls back to text search',
  '[Recall] Probe embedding failed; skipping probe',
  '[Memory] Embedding dimension mismatch — recall is degraded to TEXT search (embeddings ignored). Reindex this character or restore the matching embedding profile.',
]);

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'semantic-search-tuning-cases.json'), 'utf8'),
  ) as Spec;
  const fixtureSpec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'semantic-search-tuning.json'), 'utf8'),
  ) as { testPepperBase64: string };

  const mainFixture = process.env.QT_FIXTURE_SST_MAIN;
  if (!mainFixture || !existsSync(mainFixture)) {
    throw new Error('QT_FIXTURE_SST_MAIN must point at the committed fixture');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );

  const RealDate = Date;
  const FakeDate = class extends RealDate {
    constructor(...args: unknown[]) {
      if (args.length === 0) {
        super(spec.$nowMs);
      } else {
        // @ts-expect-error variadic forwarding
        super(...args);
      }
    }
    static now(): number {
      return spec.$nowMs;
    }
  } as DateConstructor;
  (global as { Date: DateConstructor }).Date = FakeDate;

  // The job host never runs here (no enqueue on this path) — held off anyway.
  (globalThis as Record<string, unknown>).__quilltapJobHost = {
    child: null,
    spawning: false,
    shuttingDown: true,
    childCrashed: false,
    restartTimestamps: [],
    signalHandlersInstalled: false,
    invalidationListeners: new Set(),
  };

  const lines: string[] = [];

  for (const c of spec.cases) {
    const scratch = mkdtempSync(join(tmpdir(), 'qt-sst-oracle-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const mainWork = join(scratch, 'main-work.db');
    copyFileSync(mainFixture, mainWork);

    process.env.ENCRYPTION_MASTER_PEPPER = fixtureSpec.testPepperBase64;
    process.env.SQLITE_PATH = mainWork;
    process.env.SQLITE_MOUNT_INDEX_PATH = join(scratch, 'mount-work.db');
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'debug';

    const embedTexts: string[] = [];
    jest.resetModules();
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/database/repositories', () =>
      jest.requireActual('@/lib/database/repositories'),
    );
    jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
    jest.doMock('@/lib/embedding/vector-store', () =>
      jest.requireActual('@/lib/embedding/vector-store'),
    );
    // The ONE model mock: the embedding boundary, scripted by exact text.
    jest.doMock('@/lib/embedding/embedding-service', () => {
      const actual = jest.requireActual('@/lib/embedding/embedding-service');
      return {
        ...actual,
        generateEmbeddingForUser: jest.fn(async (text: string) => {
          embedTexts.push(text);
          const s = c.embed[text];
          if (!s || s.throw) {
            throw new actual.EmbeddingError('scripted embedding failure');
          }
          return {
            embedding: new Float32Array(s.vector!),
            model: 'canned',
            dimensions: s.vector!.length,
            provider: s.provider,
          };
        }),
      };
    });

    const { logger } = await import('@/lib/logger');
    const captured: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
    const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
    internal.transports.splice(0, internal.transports.length, {
      write: (d: unknown) => {
        const data = d as {
          level: string;
          message: string;
          context?: Record<string, unknown>;
          error?: { message: string };
        };
        if (!CAPTURED.has(data.message)) return;
        const { service: _s, environment: _e, ...fields } = data.context ?? {};
        if (data.error) fields.error = data.error.message;
        captured.push({ level: data.level, message: data.message, fields });
      },
    });

    const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
    await initializeDatabase();
    const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
    const { searchMemories } = await import('@/lib/memory/recall/search');
    const { RecallTuningInputSchema, resolveRecallTuning } = await import('@/lib/memory/recall/tuning');
    const db = getRawDatabase()!;
    const access = () =>
      db.prepare('SELECT "id", "lastAccessedAt" FROM "memories" ORDER BY "id"').all();
    const accessBefore = access();

    const memo = new Map();
    const calls: unknown[] = [];
    for (const call of c.calls) {
      const o = { ...call.options, userId: spec.userId } as Record<string, any>;
      if (o.recallContext) {
        const rc = { ...o.recallContext };
        if (Array.isArray(rc.recentlyWhisperedIds)) rc.recentlyWhisperedIds = new Set(rc.recentlyWhisperedIds);
        if ('tuningInput' in rc) {
          rc.tuning = resolveRecallTuning(RecallTuningInputSchema.parse(rc.tuningInput));
          delete rc.tuningInput;
        }
        o.recallContext = rc;
      }
      if (Array.isArray(o.excludeIds)) o.excludeIds = new Set(o.excludeIds);
      if (call.memo) o.embeddingMemo = memo;
      const results = await searchMemories(call.characterId, call.query, o as never);
      calls.push(
        results.map((r) => ({
          id: r.memory.id,
          score: r.score,
          usedEmbedding: r.usedEmbedding,
          salience: r.salience,
          recallAdjustment: r.recallAdjustment ?? null,
        })),
      );
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
    const accessAfter = access();
    await closeDatabase();
    rmSync(scratch, { recursive: true, force: true });

    lines.push(
      JSON.stringify({ kind: 'case', name: c.name, calls, embedTexts, lines: captured, accessBefore, accessAfter }),
    );
  }

  // The pure R6 slotting, v4's exported fn.
  const { reserveBackgroundSlots } = await import('@/lib/memory/scoring/rank');
  for (const r of spec.reserve) {
    const row = (id: string, after: number) =>
      ({ memory: { id }, score: 0, recallAdjustment: { multiplier: 1, fired: [], blendedBefore: after, blendedAfter: after } }) as never;
    const out = reserveBackgroundSlots(
      r.ranked.map(([id, a]) => row(id, a)),
      r.background.map(([id, a]) => row(id, a)),
      r.headSize,
      r.fraction,
    ) as Array<{ memory: { id: string } }>;
    lines.push(JSON.stringify({ kind: 'reserve', name: r.name, ids: out.map((x) => x.memory.id) }));
  }

  (global as { Date: DateConstructor }).Date = RealDate;
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`semantic-search-tuning oracle wrote ${outPath} (${lines.length} rows)\n`);
}

test('semantic-search-tuning oracle', async () => {
  await main();
}, 240000);
