/**
 * @jest-environment node
 *
 * P4.D297 tier-1 ORACLE — the memory maintenance job's pure planners and the
 * ledger's decisions (v4 `lib/memory/maintenance/` at the `83334b9a5` pin,
 * `104e986c7`), driven through v4's REAL exports:
 *
 *   - `resolvePhases` + `exemptPhasesFor` (run.ts) over the whole grid:
 *     reason × requested phases × exemptions × the maintenance switch ×
 *     the consolidation switch × the per-phase toggles × `coldRetentionDays`;
 *   - `withImpliedPhases` (types.ts);
 *   - `summarizeMaintenance` (run.ts) — key orders, byte for byte;
 *   - `planDemotion` / `planRetention` (demote.ts) — ties, an unparseable
 *     `createdAt`, excess 0, a fractional cap, MANUAL / digest / cold rows,
 *     `supersededAt` vs the `updatedAt` fallback, non-finite dates;
 *   - `planNearDuplicateFolds` (fold-near-duplicates.ts) — non-transitive
 *     chains, the date guard, dimension / kind / subject buckets, the
 *     empty-string `aboutCharacterId`, the 2000-row bucket cap, both threshold
 *     sets (`admissionThresholdsForProvider`);
 *   - `bucketCandidates` (consolidate.ts) — bug 211's demoted candidates, the
 *     2000-row cap order;
 *   - `resolveClusterThreshold` (settings-defaults.ts, `4ae0c4ca6`);
 *   - the ledger (ledger.ts): `shouldEnqueue`, `recordMaintenanceOutcome`,
 *     `takeWatermarkCheck`, `noteEnqueued`, `getMaintenanceLedgerStatus`
 *     (which exposes the private `lastDemotionIneffective` as `effective`) —
 *     scripted sequences, the seed planted through a mocked
 *     `backgroundJobs.findRecentByType` answering as the real one does
 *     (`updatedAt DESC`; a failed read is the safeQuery's `[]`).
 *
 * Every row carries its INPUT and v4's OUTPUT; the Rust side replays the
 * input. Memory rows are emitted as JSON (a `Float32Array` stringifies to its
 * `{"0": v, …}` object — v5's row shape). Ledger lines (`module:
 * 'memory:maintenance:ledger'`) are captured per step.
 *
 * Run (Node 24, from the v4 checkout or the round's pin — a /tmp mirror,
 * jest ignores .claude/ paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; M=${M:-/tmp/qt-maintenance-planning-oracle}
 *   rm -rf $M && mkdir -p $M && cp -R $V5W/harness/oracle/* $M/
 *   cd ~/source/quilltap-server
 *   rm -f ${QT_OUT:-/tmp/qt-maintenance-planning.ndjson}
 *   QT_ORACLE_OUT=${QT_OUT:-/tmp/qt-maintenance-planning.ndjson} \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$M/cases" -- memory-maintenance-planning
 */

import * as fs from 'fs';

type Row = Record<string, unknown>;

const NOW_MS = Date.parse('2026-10-08T12:00:00.000Z');
const HOLDER = 'a0000000-0000-4000-8000-000000000001';
const LAURA = 'a0000000-0000-4000-8000-000000000002';
const DAY = 86_400_000;

function iso(ms: number): string {
  return new Date(ms).toISOString();
}

/** A deterministic PRNG (mulberry32). */
function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function uuid(prefix: string, n: number): string {
  const tail = n.toString(16).padStart(12, '0');
  return `${prefix}-0000-4000-8000-${tail}`;
}

/** A memory row as v4's repository hands it (the fields the planners read). */
function mem(id: string, o: Row = {}): Row {
  return {
    id,
    characterId: HOLDER,
    aboutCharacterId: LAURA,
    content: `content ${id}`,
    summary: `summary ${id}`,
    keywords: [],
    tags: [],
    importance: 0.5,
    source: 'AUTO',
    entities: [],
    kind: 'semantic',
    reinforcementCount: 1,
    relatedMemoryIds: [],
    reinforcedImportance: 0.5,
    tier: 'hot',
    consolidatedFrom: [],
    createdAt: iso(NOW_MS - 30 * DAY),
    updatedAt: iso(NOW_MS - 30 * DAY),
    ...o,
  };
}

function vec(values: number[]): Float32Array {
  return new Float32Array(values);
}

function unit(values: number[]): Float32Array {
  const n = Math.sqrt(values.reduce((s, v) => s + v * v, 0));
  return vec(values.map((v) => v / n));
}

/** JSON of a row (a Float32Array → its index object, v5's shape). */
function rowJson(r: Row): unknown {
  return JSON.parse(JSON.stringify(r));
}

async function main(): Promise<void> {
  const out: string[] = [];
  const emit = (o: unknown) => out.push(JSON.stringify(o));

  // The ledger's seed source, scriptable per step. It answers as v4's REAL
  // `findRecentByType` does: `ORDER BY updatedAt DESC` (SQLite's binary text
  // order; ties keep insertion order), and a failed read is a fallback
  // `safeQuery` — `[]`, never a throw (so the seed's own WARN is unreachable).
  const seedState: { rows: Array<Record<string, unknown>>; fails: boolean } = { rows: [], fails: false };
  jest.resetModules();
  jest.doMock('@/lib/repositories/factory', () => ({
    __esModule: true,
    getRepositories: () => ({
      backgroundJobs: {
        findRecentByType: async () => {
          if (seedState.fails) return [];
          return [...seedState.rows].sort((a, b) => {
            const x = String(a.updatedAt);
            const y = String(b.updatedAt);
            return x < y ? 1 : x > y ? -1 : 0;
          });
        },
      },
    }),
  }));

  const { Logger } = await import('@/lib/logger');
  let sink: Array<{ level: string; message: string; context: unknown }> | null = null;
  for (const level of ['error', 'warn', 'info', 'debug'] as const) {
    const proto = Logger.prototype as unknown as Record<string, (...a: unknown[]) => void>;
    const original = proto[level];
    proto[level] = function (this: { context?: Row }, message: string, context?: Row, ...rest: unknown[]) {
      const { service: _s, environment: _e, ...own } = this.context ?? {};
      const merged = { ...own, ...(context ?? {}) };
      if (sink && typeof message === 'string' && merged.module === 'memory:maintenance:ledger') {
        sink.push({ level, message, context: JSON.parse(JSON.stringify(merged)) });
      }
      return original.call(this, message, context, ...rest);
    };
  }

  const run = await import('@/lib/memory/maintenance/run');
  const types = await import('@/lib/memory/maintenance/types');
  const demote = await import('@/lib/memory/maintenance/demote');
  const fold = await import('@/lib/memory/maintenance/fold-near-duplicates');
  const consolidate = await import('@/lib/memory/maintenance/consolidate');
  const ledger = await import('@/lib/memory/maintenance/ledger');
  const defaults = await import('@/lib/memory/settings-defaults');

  // ── resolvePhases / exemptPhasesFor ─────────────────────────────────────
  const reasons = ['scheduled', 'watermark', 'manual'] as const;
  const phaseLists: Array<string[] | undefined> = [
    undefined,
    [],
    ['consolidate'],
    ['fold-near-duplicates', 'retire'],
    ['mirror'],
    ['bogus'],
    ['demote', 'demote'],
  ];
  const exemptLists: Array<string[] | undefined> = [undefined, [], ['retire'], ['consolidate', 'fold-near-duplicates']];
  const toggleSets = [
    { foldNearDuplicates: true, demote: true, retire: true },
    { foldNearDuplicates: false, demote: true, retire: true },
    { foldNearDuplicates: true, demote: false, retire: false },
  ];
  for (const reason of reasons) {
    for (const phases of phaseLists) {
      for (const exemptPhases of exemptLists) {
        for (const maintenanceEnabled of [true, false]) {
          for (const consolidationEnabled of [true, false]) {
            for (const toggles of toggleSets) {
              for (const coldRetentionDays of [null, 0, 30]) {
                const request: Row = { reason };
                if (phases !== undefined) request.phases = phases;
                if (exemptPhases !== undefined) request.exemptPhases = exemptPhases;
                const settings = {
                  maintenance: {
                    enabled: maintenanceEnabled,
                    phases: toggles,
                    consolidation: { enabled: consolidationEnabled },
                    coldRetentionDays,
                  },
                };
                const plan = run.resolvePhases(request as never, settings as never);
                emit({
                  kind: 'resolvePhases',
                  request,
                  settings: { maintenanceEnabled, consolidationEnabled, toggles, coldRetentionDays },
                  out: Array.from(plan.entries()),
                  exempt: Array.from(run.exemptPhasesFor(request as never)),
                });
              }
            }
          }
        }
      }
    }
  }

  // ── withImpliedPhases ───────────────────────────────────────────────────
  for (const phases of [
    [],
    ['consolidate'],
    ['mirror', 'consolidate'],
    ['retire', 'fold-near-duplicates', 'retire'],
    ['demote'],
    ['mirror'],
    ['retire', 'consolidate', 'demote', 'fold-near-duplicates'],
  ]) {
    emit({ kind: 'withImpliedPhases', phases, out: types.withImpliedPhases(phases as never) });
  }

  // ── summarizeMaintenance (synthetic reports; key orders) ────────────────
  const outcome = (phase: string, status: string, skippedBecause?: string, durationMs = 3) =>
    skippedBecause ? { phase, status, skippedBecause, durationMs } : { phase, status, durationMs };
  const stats = (o: Row = {}) => ({
    rowsLoaded: 5,
    candidates: 3,
    demotedCandidates: 1,
    digestsLoaded: 0,
    immatureSkipped: 0,
    noEmbeddingSkipped: 0,
    bucketsScanned: 1,
    bucketRowsCapped: 0,
    clustersFound: 1,
    clustersQualified: 1,
    clustersSelected: 1,
    clustersDeferred: 0,
    clustersBelowMin: 0,
    clustersStale: 0,
    clustersAttempted: 1,
    clustersSucceeded: 1,
    clustersFailed: 0,
    clustersLostToTimeout: 0,
    digestsCreated: 1,
    digestsUpdated: 0,
    membersSuperseded: 3,
    contradictionsApplied: 0,
    rowsMarkedConsidered: 3,
    linksRewired: 0,
    budgetExhausted: false,
    durationMs: 9,
    ...o,
  });
  const base = {
    characterId: HOLDER,
    characterName: 'Friday',
    reason: 'manual',
    dryRun: false,
    startedAt: '2026-10-08T12:00:00.000Z',
    finishedAt: '2026-10-08T12:00:01.500Z',
    durationMs: 1500,
  };
  const reports: Array<[string, Row]> = [
    [
      'skip_not_found',
      {
        ...base,
        characterName: null,
        reason: 'scheduled',
        phases: ['fold-near-duplicates', 'consolidate', 'demote', 'retire', 'mirror'].map((p) => outcome(p, 'skipped', undefined, 0)),
        skippedReason: 'character-not-found',
      },
    ],
    [
      'full',
      {
        ...base,
        phases: [
          outcome('fold-near-duplicates', 'ran'),
          outcome('consolidate', 'ran'),
          outcome('demote', 'ran'),
          outcome('retire', 'ran'),
          outcome('mirror', 'ran'),
        ],
        fold: { eligible: 9, deferred: 0, groups: [{ survivorId: 's1', absorbedIds: ['a1', 'a2'], minCosine: 0.93 }], folded: 2, groupsFailed: 0, groupsDeferred: 0 },
        consolidation: { characterId: HOLDER, stats: stats(), skippedReason: undefined },
        demote: { cap: 2000, hotTotal: 2105, demotable: 2100, protected: 5, excess: 100, demoted: 100, demotedIds: ['x'], highestScoreDemoted: 0.41234567 },
        retire: { retentionDays: 30, retired: 2, retiredIds: ['r1', 'r2'] },
        mirror: { subjects: [LAURA, HOLDER], filesWritten: 0, deferredToCommit: true },
      },
    ],
    [
      'partial_no_llm',
      {
        ...base,
        dryRun: true,
        reason: 'watermark',
        phases: [
          outcome('fold-near-duplicates', 'skipped', 'not-requested', 0),
          outcome('consolidate', 'ran'),
          outcome('demote', 'skipped', 'maintenance-disabled', 0),
          outcome('retire', 'skipped', 'no-retention-window', 0),
          outcome('mirror', 'skipped', 'nothing-touched', 0),
        ],
        consolidation: { characterId: HOLDER, stats: stats({ digestsCreated: 0, membersSuperseded: 0, clustersFailed: 2, clustersDeferred: 1 }), skippedReason: 'no-llm' },
      },
    ],
    [
      'demote_only_null_score',
      {
        ...base,
        phases: [outcome('demote', 'ran')],
        demote: { cap: 100, hotTotal: 7, demotable: 3, protected: 4, excess: 0, demoted: 0, demotedIds: [], highestScoreDemoted: null },
      },
    ],
  ];
  for (const [name, report] of reports) {
    emit({ kind: 'summarize', name, report: JSON.parse(JSON.stringify(report)), out: run.summarizeMaintenance(report as never) });
  }

  // ── planDemotion ────────────────────────────────────────────────────────
  const demoteRows: Row[] = [
    mem(uuid('d0000000', 1), { importance: 0.2, reinforcedImportance: 0.2, createdAt: iso(NOW_MS - 50 * DAY) }),
    mem(uuid('d0000000', 2), { importance: 0.2, reinforcedImportance: 0.2, createdAt: iso(NOW_MS - 60 * DAY) }), // tie with 1, older
    mem(uuid('d0000000', 3), { importance: 0.9, reinforcedImportance: 0.9 }),
    mem(uuid('d0000000', 4), { importance: 0.2, reinforcedImportance: 0.2, createdAt: 'not a date' }),
    mem(uuid('d0000000', 5), { source: 'MANUAL', importance: 0.1, reinforcedImportance: 0.1 }),
    mem(uuid('d0000000', 6), { source: 'CONSOLIDATED', importance: 0.1, reinforcedImportance: 0.1 }),
    mem(uuid('d0000000', 7), { tier: 'cold', importance: 0.1, reinforcedImportance: 0.1 }),
    mem(uuid('d0000000', 8), { tier: null, importance: 0.3, reinforcedImportance: 0.3, reinforcementCount: 4 }),
    mem(uuid('d0000000', 9), { importance: 0.3, reinforcedImportance: null, relatedMemoryIds: ['x', 'y', 'z'] }),
    mem(uuid('d0000000', 10), { importance: 0.4, kind: 'episodic', lastAccessedAt: iso(NOW_MS - 2 * DAY) }),
    mem(uuid('d0000000', 11), { importance: 0.2, reinforcedImportance: 0.2, createdAt: iso(NOW_MS - 50 * DAY) }), // exact tie with 1
    mem(uuid('d0000000', 12), { importance: 0.5, lastReinforcedAt: iso(NOW_MS - 1 * DAY) }),
    // Floor-saturated ties: past ~100 days the content decay sits on its 0.10
    // floor, so two rows of equal fields but different ages score EXACTLY
    // alike — the order is the createdAt tie-break (oldest first).
    mem(uuid('d0000000', 13), { importance: 0.25, reinforcedImportance: 0.25, createdAt: iso(NOW_MS - 200 * DAY) }),
    mem(uuid('d0000000', 14), { importance: 0.25, reinforcedImportance: 0.25, createdAt: iso(NOW_MS - 300 * DAY) }),
    mem(uuid('d0000000', 15), { importance: 0.25, reinforcedImportance: 0.25, createdAt: iso(NOW_MS - 250 * DAY) }),
  ];
  for (const cap of [100, 8, 5, 3, 2.5, 0, 9, 12, 11]) {
    emit({ kind: 'planDemotion', rows: demoteRows.map(rowJson), cap, nowMs: NOW_MS, out: demote.planDemotion(demoteRows as never, cap, NOW_MS) });
  }

  // ── planRetention ───────────────────────────────────────────────────────
  const retireRows: Row[] = [
    mem(uuid('e0000000', 1), { tier: 'cold', supersededById: 'dg', supersededAt: iso(NOW_MS - 40 * DAY) }),
    mem(uuid('e0000000', 2), { tier: 'cold', supersededById: 'dg', supersededAt: iso(NOW_MS - 10 * DAY) }),
    mem(uuid('e0000000', 3), { tier: 'cold', supersededById: 'dg', supersededAt: null, updatedAt: iso(NOW_MS - 40 * DAY) }),
    mem(uuid('e0000000', 4), { tier: 'cold', supersededById: 'dg', updatedAt: iso(NOW_MS - 40 * DAY) }),
    mem(uuid('e0000000', 5), { tier: 'cold', supersededById: 'dg', supersededAt: 'garbage', updatedAt: iso(NOW_MS - 40 * DAY) }),
    mem(uuid('e0000000', 6), { tier: 'cold', supersededById: null, supersededAt: iso(NOW_MS - 40 * DAY) }),
    mem(uuid('e0000000', 7), { tier: 'cold', supersededById: '', supersededAt: iso(NOW_MS - 40 * DAY) }),
    mem(uuid('e0000000', 8), { tier: 'cold', supersededById: 'dg', source: 'MANUAL', supersededAt: iso(NOW_MS - 40 * DAY) }),
    mem(uuid('e0000000', 9), { tier: 'hot', supersededById: 'dg', supersededAt: iso(NOW_MS - 40 * DAY) }),
    mem(uuid('e0000000', 10), { tier: 'cold', supersededById: 'dg', supersededAt: iso(NOW_MS - 30 * DAY) }),
    mem(uuid('e0000000', 11), { tier: 'cold', supersededById: 'dg', supersededAt: '', updatedAt: iso(NOW_MS - 40 * DAY) }),
  ];
  for (const retentionDays of [30, 1, 365]) {
    emit({ kind: 'planRetention', rows: retireRows.map(rowJson), retentionDays, nowMs: NOW_MS, out: demote.planRetention(retireRows as never, retentionDays, NOW_MS) });
  }

  // ── planNearDuplicateFolds ──────────────────────────────────────────────
  const r = rng(297);
  const near = (base: number[], eps: number) => unit(base.map((v) => v + (r() - 0.5) * eps));
  const A = [1, 0, 0, 0];
  const foldRows: Row[] = [
    // A chain: s≈b (0.95), b≈c (0.95) but s and c further apart — non-transitive.
    mem(uuid('f0000000', 1), { embedding: unit([1, 0, 0, 0]), importance: 0.9, reinforcedImportance: 0.9 }),
    mem(uuid('f0000000', 2), { embedding: unit([1, 0.33, 0, 0]) }),
    mem(uuid('f0000000', 3), { embedding: unit([1, 0.7, 0, 0]) }),
    // Same vectors, a different occasion (date guard), then the same occasion.
    mem(uuid('f0000000', 4), { embedding: unit([0, 1, 0, 0]), occurredAt: '2026-09-01T10:00:00.000Z' }),
    mem(uuid('f0000000', 5), { embedding: unit([0, 1, 0.01, 0]), occurredAt: '2026-09-20T10:00:00.000Z' }),
    mem(uuid('f0000000', 6), { embedding: unit([0, 1, 0.02, 0]), occurredAt: '2026-09-03T10:00:00.000Z' }),
    // A different kind and a different subject (empty-string about = holder).
    mem(uuid('f0000000', 7), { embedding: unit([0, 0, 1, 0]), kind: 'episodic' }),
    mem(uuid('f0000000', 8), { embedding: unit([0, 0, 1, 0.01]), kind: 'episodic' }),
    mem(uuid('f0000000', 9), { embedding: unit([0, 0, 0, 1]), aboutCharacterId: '' }),
    mem(uuid('f0000000', 10), { embedding: unit([0, 0, 0.02, 1]), aboutCharacterId: HOLDER }),
    mem(uuid('f0000000', 11), { embedding: unit([0, 0, 0.5, 1]), aboutCharacterId: null }),
    // Ineligible: MANUAL, digest, cold, no vector, empty vector, other dims.
    mem(uuid('f0000000', 12), { embedding: unit([1, 0, 0, 0]), source: 'MANUAL' }),
    mem(uuid('f0000000', 13), { embedding: unit([1, 0, 0, 0]), source: 'CONSOLIDATED' }),
    mem(uuid('f0000000', 14), { embedding: unit([1, 0, 0, 0]), tier: 'cold' }),
    mem(uuid('f0000000', 15), {}),
    mem(uuid('f0000000', 16), { embedding: new Float32Array(0) }),
    mem(uuid('f0000000', 17), { embedding: unit([1, 0, 0, 0, 0, 0, 0, 0]) }),
    mem(uuid('f0000000', 18), { embedding: unit([1, 0.01, 0, 0, 0, 0, 0, 0]) }),
    // A tie in protection resolved by the oldest.
    mem(uuid('f0000000', 19), { embedding: near(A, 0.0), createdAt: iso(NOW_MS - 90 * DAY), kind: 'semantic', aboutCharacterId: 'c0000000-0000-4000-8000-000000000009' }),
    mem(uuid('f0000000', 20), { embedding: near(A, 0.0), createdAt: iso(NOW_MS - 95 * DAY), kind: 'semantic', aboutCharacterId: 'c0000000-0000-4000-8000-000000000009' }),
  ];
  for (const provider of [null, 'BUILTIN', 'OPENAI']) {
    const thresholds = defaults.admissionThresholdsForProvider(provider);
    emit({
      kind: 'planFolds',
      name: `mixed:${provider}`,
      provider,
      rows: foldRows.map(rowJson),
      nowMs: NOW_MS,
      eligible: foldRows.map((m) => fold.isFoldEligible(m as never)),
      out: await fold.planNearDuplicateFolds(foldRows as never, thresholds, NOW_MS),
    });
  }
  // The 2000-row bucket bound: 2003 rows in one bucket, mostly far apart.
  const capRows: Row[] = [];
  for (let i = 0; i < 2003; i++) {
    const v = [r(), r(), r(), r(), r(), r(), r(), r()].map((x) => x - 0.5);
    capRows.push(
      mem(uuid('fc000000', i + 1), {
        embedding: unit(i % 500 === 7 ? [1, 1, 1, 1, 1, 1, 1, 1] : v),
        createdAt: iso(NOW_MS - (40 + (i % 97)) * DAY - i * 1000),
        importance: 0.3 + (i % 7) * 0.05,
        reinforcedImportance: null,
      }),
    );
  }
  emit({
    kind: 'planFolds',
    name: 'bucket_cap',
    provider: null,
    rows: capRows.map(rowJson),
    nowMs: NOW_MS,
    out: await fold.planNearDuplicateFolds(capRows as never, defaults.admissionThresholdsForProvider(null), NOW_MS),
  });

  // ── bucketCandidates ────────────────────────────────────────────────────
  const matureCutoff = NOW_MS - 7 * DAY;
  const candRows: Row[] = [
    mem(uuid('b0000000', 1), { embedding: unit([1, 0, 0, 0]) }),
    mem(uuid('b0000000', 2), { embedding: unit([1, 0, 0, 0]), aboutCharacterId: HOLDER }),
    mem(uuid('b0000000', 3), { embedding: unit([1, 0, 0, 0]), aboutCharacterId: null }),
    mem(uuid('b0000000', 4), { embedding: unit([1, 0, 0, 0]), aboutCharacterId: '' }),
    mem(uuid('b0000000', 5), { embedding: unit([1, 0, 0, 0]), tier: 'cold' }), // demoted → candidate (bug 211)
    mem(uuid('b0000000', 6), { embedding: unit([1, 0, 0, 0]), tier: 'cold', supersededById: 'dg' }),
    mem(uuid('b0000000', 7), { embedding: unit([1, 0, 0, 0]), source: 'MANUAL' }),
    mem(uuid('b0000000', 8), { embedding: unit([1, 0, 0, 0]), source: 'CONSOLIDATED' }),
    mem(uuid('b0000000', 9), { source: 'CONSOLIDATED' }),
    mem(uuid('b0000000', 10), { embedding: unit([1, 0, 0, 0]), source: 'CONSOLIDATED', tier: 'cold' }),
    mem(uuid('b0000000', 11), { embedding: unit([1, 0, 0, 0]), createdAt: iso(NOW_MS - 1 * DAY) }),
    mem(uuid('b0000000', 12), { embedding: unit([1, 0, 0, 0]), createdAt: 'nope' }),
    mem(uuid('b0000000', 13), {}),
    mem(uuid('b0000000', 14), { tier: 'cold' }), // demoted, no embedding
    mem(uuid('b0000000', 15), { embedding: unit([1, 0, 0, 0]), tier: 'cold', createdAt: iso(NOW_MS - 1 * DAY) }),
    mem(uuid('b0000000', 16), { embedding: unit([1, 0, 0, 0]), relatedMemoryIds: ['x', 'y'], aboutCharacterId: 'c0000000-0000-4000-8000-000000000003' }),
  ];
  const describeBuckets = (b: ReturnType<typeof consolidate.bucketCandidates>) => ({
    rowIndex: Array.from(b.rowIndex.entries()),
    buckets: Array.from(b.buckets.values()).map((k) => ({
      key: k.key,
      ref: k.ref,
      aboutCharacterId: k.aboutCharacterId,
      rows: k.rows.map((m) => m.id),
      digests: k.digests.map((m) => m.id),
    })),
    stats: b.stats,
  });
  emit({
    kind: 'bucketCandidates',
    name: 'mixed',
    rows: candRows.map(rowJson),
    holderId: HOLDER,
    matureCutoffMs: matureCutoff,
    out: describeBuckets(consolidate.bucketCandidates(candRows as never, HOLDER, matureCutoff)),
  });
  const bigBucket: Row[] = [];
  for (let i = 0; i < 2004; i++) {
    bigBucket.push(
      mem(uuid('bb000000', i + 1), {
        embedding: unit([1, 0, 0, 0]),
        consolidatedAt: i % 3 === 0 ? iso(NOW_MS - 5 * DAY) : null,
        reinforcedImportance: i % 11 === 0 ? null : 0.1 + (i % 17) * 0.05,
      }),
    );
  }
  emit({
    kind: 'bucketCandidates',
    name: 'cap',
    rows: bigBucket.map(rowJson),
    holderId: HOLDER,
    matureCutoffMs: matureCutoff,
    out: describeBuckets(consolidate.bucketCandidates(bigBucket as never, HOLDER, matureCutoff)),
  });

  // ── resolveClusterThreshold ─────────────────────────────────────────────
  for (const provider of [null, undefined, 'BUILTIN', 'OPENAI', 'builtin', '']) {
    for (const override of [undefined, 0.55, 0]) {
      const settings = { clusterThreshold: 0.72, tfidfClusterThreshold: 0.3 };
      emit({
        kind: 'resolveClusterThreshold',
        provider: provider ?? null,
        providerUndefined: provider === undefined,
        override: override ?? null,
        out: defaults.resolveClusterThreshold(settings as never, provider, override),
      });
    }
  }

  // ── the ledger ──────────────────────────────────────────────────────────
  // Each scenario is a script of steps over a reset ledger; every step's
  // output and the ledger lines it logged are recorded.
  const summary = (o: Row = {}) => ({
    characterId: HOLDER,
    reason: 'scheduled',
    dryRun: false,
    finishedAt: iso(NOW_MS - 10 * 60_000),
    durationMs: 12,
    phases: [
      { phase: 'fold-near-duplicates', status: 'ran', durationMs: 1 },
      { phase: 'consolidate', status: 'skipped', skippedBecause: 'consolidation-disabled', durationMs: 0 },
      { phase: 'demote', status: 'ran', durationMs: 2 },
      { phase: 'retire', status: 'skipped', skippedBecause: 'no-retention-window', durationMs: 0 },
      { phase: 'mirror', status: 'skipped', skippedBecause: 'nothing-touched', durationMs: 0 },
    ],
    fold: { eligible: 3, groups: 0, folded: 0, groupsFailed: 0, groupsDeferred: 0 },
    demote: { cap: 2000, hotTotal: 2200, demotable: 2150, protected: 50, excess: 150, demoted: 150, highestScoreDemoted: 0.2 },
    ...o,
  });
  const reading = (o: Row = {}) => ({
    demotable: 1900,
    cap: 2000,
    unconsidered: 10,
    consolidationWatermark: 150,
    maintenanceEnabled: true,
    consolidationEnabled: true,
    ...o,
  });
  const job = (id: string, o: Row = {}) => ({
    id,
    status: 'COMPLETED',
    updatedAt: iso(NOW_MS - 3 * 60 * 60_000),
    payload: { characterId: HOLDER, reason: 'scheduled' },
    ...o,
  });
  type Step = Row & { op: string };
  const scenarios: Array<{ name: string; seed?: Array<Record<string, unknown>>; seedFails?: boolean; steps: Step[] }> = [
    {
      name: 'non_watermark_and_no_reading',
      steps: [
        { op: 'should', characterId: HOLDER, reason: 'scheduled', nowMs: NOW_MS },
        { op: 'should', characterId: HOLDER, reason: 'manual', nowMs: NOW_MS },
        { op: 'should', characterId: HOLDER, reason: 'watermark', nowMs: NOW_MS },
        { op: 'status' },
      ],
    },
    {
      name: 'cap_watermark_arms',
      steps: [
        { op: 'should', characterId: HOLDER, reason: 'watermark', reading: reading({ demotable: 1799 }), nowMs: NOW_MS },
        { op: 'should', characterId: HOLDER, reason: 'watermark', reading: reading({ demotable: 1800 }), nowMs: NOW_MS },
        { op: 'should', characterId: HOLDER, reason: 'watermark', reading: reading({ cap: 1001, demotable: 900 }), nowMs: NOW_MS },
        { op: 'should', characterId: HOLDER, reason: 'watermark', reading: reading({ maintenanceEnabled: false }), nowMs: NOW_MS },
        { op: 'note', characterId: HOLDER, phases: ['fold-near-duplicates', 'demote', 'retire'], nowMs: NOW_MS },
        { op: 'should', characterId: HOLDER, reason: 'watermark', reading: reading(), nowMs: NOW_MS + 14 * 60_000 },
        { op: 'should', characterId: HOLDER, reason: 'watermark', reading: reading(), nowMs: NOW_MS + 15 * 60_000 },
        { op: 'status' },
      ],
    },
    {
      name: 'ineffective_backoff',
      steps: [
        { op: 'record', summary: summary({ finishedAt: iso(NOW_MS - 20 * 60_000), demote: { cap: 2000, hotTotal: 2010, demotable: 2005, protected: 5, excess: 5, demoted: 5, highestScoreDemoted: 0.1 } }) },
        { op: 'should', characterId: HOLDER, reason: 'watermark', reading: reading({ unconsidered: 0 }), nowMs: NOW_MS },
        { op: 'should', characterId: HOLDER, reason: 'watermark', reading: reading({ unconsidered: 0 }), nowMs: NOW_MS + 41 * 60_000 },
        { op: 'record', summary: summary({ characterId: LAURA, finishedAt: iso(NOW_MS - 20 * 60_000), demote: { cap: 2000, hotTotal: 3500, demotable: 3400, protected: 100, excess: 1400, demoted: 12, highestScoreDemoted: 0.3 } }) },
        { op: 'should', characterId: LAURA, reason: 'watermark', reading: reading({ unconsidered: 0 }), nowMs: NOW_MS },
        { op: 'record', summary: summary({ characterId: 'c0000000-0000-4000-8000-000000000005', finishedAt: iso(NOW_MS - 20 * 60_000), demote: { cap: 2000, hotTotal: 1900, demotable: 1900, protected: 0, excess: 0, demoted: 0, highestScoreDemoted: null } }) },
        { op: 'should', characterId: 'c0000000-0000-4000-8000-000000000005', reason: 'watermark', reading: reading({ unconsidered: 0 }), nowMs: NOW_MS },
        { op: 'status' },
        { op: 'status', characterId: LAURA },
        { op: 'status', characterId: 'c0000000-0000-4000-8000-0000000000aa' },
      ],
    },
    {
      name: 'consolidation_watermark',
      steps: [
        { op: 'should', characterId: HOLDER, reason: 'watermark', reading: reading({ demotable: 0, unconsidered: 150 }), nowMs: NOW_MS },
        { op: 'should', characterId: HOLDER, reason: 'watermark', reading: reading({ demotable: 0, unconsidered: 151 }), nowMs: NOW_MS },
        { op: 'should', characterId: HOLDER, reason: 'watermark', reading: reading({ demotable: 0, unconsidered: 151, consolidationEnabled: false }), nowMs: NOW_MS },
        { op: 'record', summary: summary({ phases: [{ phase: 'consolidate', status: 'ran', durationMs: 4 }, { phase: 'mirror', status: 'ran', durationMs: 1 }], fold: undefined, demote: undefined, finishedAt: iso(NOW_MS - 60 * 60_000) }) },
        { op: 'should', characterId: HOLDER, reason: 'watermark', reading: reading({ unconsidered: 400 }), nowMs: NOW_MS },
        { op: 'should', characterId: HOLDER, reason: 'watermark', reading: reading({ unconsidered: 400 }), nowMs: NOW_MS + 5 * 60 * 60_000 + 1 },
        { op: 'status' },
      ],
    },
    {
      name: 'record_ignores_dry_and_skipped',
      steps: [
        { op: 'record', summary: summary({ dryRun: true }) },
        { op: 'record', summary: summary({ skippedReason: 'archived' }) },
        { op: 'status' },
        { op: 'record', summary: summary({ finishedAt: 'not a date', reason: 'manual' }) },
        { op: 'statusShape' },
      ],
    },
    {
      name: 'seed_from_jobs',
      seed: [
        job('j1'),
        job('j2', { updatedAt: iso(NOW_MS - 1 * 60 * 60_000), payload: { characterId: HOLDER, reason: 'watermark', phases: ['consolidate', 'mirror'] } }),
        job('j3', { status: 'FAILED' }),
        job('j4', { status: 'PROCESSING', payload: { characterId: LAURA } }),
        job('j5', { payload: { characterId: '' } }),
        job('j6', { payload: { reason: 'scheduled' } }),
        job('j7', { updatedAt: 'whenever' }),
        job('j8', { payload: { characterId: LAURA, reason: 'manual', phases: ['demote'] }, updatedAt: iso(NOW_MS - 10 * 60 * 60_000) }),
        job('j9', { payload: null }),
      ],
      steps: [
        { op: 'should', characterId: HOLDER, reason: 'watermark', reading: reading({ unconsidered: 400 }), nowMs: NOW_MS },
        { op: 'should', characterId: LAURA, reason: 'watermark', reading: reading({ unconsidered: 400 }), nowMs: NOW_MS },
        { op: 'status' },
      ],
    },
    {
      name: 'seed_failure_then_no_retry',
      seedFails: true,
      steps: [
        { op: 'should', characterId: HOLDER, reason: 'watermark', reading: reading(), nowMs: NOW_MS },
        { op: 'unthrow', seed: [job('k1')] },
        { op: 'should', characterId: HOLDER, reason: 'watermark', reading: reading(), nowMs: NOW_MS },
        { op: 'status' },
      ],
    },
    {
      name: 'watermark_debounce',
      steps: [
        { op: 'take', characterId: HOLDER, nowMs: NOW_MS },
        { op: 'take', characterId: HOLDER, nowMs: NOW_MS + 119_999 },
        { op: 'take', characterId: HOLDER, nowMs: NOW_MS + 120_000 },
        { op: 'take', characterId: LAURA, nowMs: NOW_MS },
        { op: 'status' },
      ],
    },
  ];
  for (const scenario of scenarios) {
    ledger.resetMaintenanceLedgerForTests();
    seedState.rows = scenario.seed ?? [];
    seedState.fails = scenario.seedFails ?? false;
    const results: unknown[] = [];
    for (const step of scenario.steps) {
      sink = [];
      let result: unknown = null;
      switch (step.op) {
        case 'should':
          result = await ledger.shouldEnqueue(step.characterId as string, step.reason as never, step.reading as never, step.nowMs as number);
          break;
        case 'record':
          ledger.recordMaintenanceOutcome(step.summary as never);
          break;
        case 'take':
          result = ledger.takeWatermarkCheck(step.characterId as string, step.nowMs as number);
          break;
        case 'note':
          ledger.noteEnqueued(step.characterId as string, step.phases as never, step.nowMs as number);
          break;
        case 'status':
          result = await ledger.getMaintenanceLedgerStatus(step.characterId as string | undefined);
          break;
        case 'statusShape': {
          // `at` falls back to Date.now() for an unparseable finishedAt: record the shape only.
          const status = await ledger.getMaintenanceLedgerStatus();
          result = status.map((s) => ({ characterId: s.characterId, phases: Object.keys(s.lastRun), reasons: Object.values(s.lastRun).map((x) => (x as { reason: string }).reason), hasSummary: s.lastSummary !== null, lastDemotion: s.lastDemotion ? { demoted: s.lastDemotion.demoted, excess: s.lastDemotion.excess, effective: s.lastDemotion.effective } : null }));
          break;
        }
        case 'unthrow':
          seedState.fails = false;
          seedState.rows = step.seed as Array<Record<string, unknown>>;
          break;
        default:
          throw new Error(`unknown op ${step.op}`);
      }
      results.push({ step, result, lines: sink });
      sink = null;
    }
    emit({ kind: 'ledger', name: scenario.name, seed: scenario.seed ?? [], seedFails: scenario.seedFails ?? false, results });
  }

  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  fs.writeFileSync(outPath, out.join('\n') + '\n');
  process.stderr.write(`wrote ${out.length} maintenance-planning records to ${outPath}\n`);
}

test('memory maintenance planning oracle', async () => {
  await main();
}, 180000);
