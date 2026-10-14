/**
 * Tier-1 ORACLE for v4's scoring module (P4.D289, pin `83334b9a5`) — the ONE
 * reinforced-importance formula (bug 216), salience + `sortBySalience`, the
 * `protection` wrapper, the importance constants / filter / bands, and the
 * ONE lexical-scale predicate (`lib/embedding/cosine-scale.ts`, `4ae0c4ca6`).
 *
 * Drives v4's REAL modules, imported by FILE path (the `scoring/index.ts`
 * barrel pulls `rank.ts` → `recall/*`):
 *   lib/memory/scoring/{reinforced-importance,salience,protection,relevance,importance}.ts
 *   lib/embedding/cosine-scale.ts
 *
 * Non-finite numbers ride as strings (`"NaN"`, `"Infinity"`, `"-Infinity"`)
 * since JSON has no spelling for them; the Rust side reads both shapes.
 *
 * Run from inside a v4 checkout (or the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/memory-scoring.ts" > /tmp/oracle-memory-scoring.ndjson
 */

import {
  reinforcedImportance,
  REINFORCED_IMPORTANCE_STEP,
} from '@/lib/memory/scoring/reinforced-importance';
import {
  salience,
  sortBySalience,
  referenceTimeMs,
  SALIENCE_HALF_LIFE_DAYS,
} from '@/lib/memory/scoring/salience';
import { protection, PROTECTION_THRESHOLD, DEFAULT_PROTECTION_CONFIG } from '@/lib/memory/scoring/protection';
import {
  RANKING_RELEVANCE_WEIGHT,
  RANKING_PRIORITY_WEIGHT,
  DEFAULT_MIN_COSINE_NEURAL,
  DEFAULT_MIN_COSINE_TFIDF,
  relevance,
} from '@/lib/memory/scoring/relevance';
import {
  RECALL_MIN_IMPORTANCE,
  TOOL_MIN_IMPORTANCE,
  IMPORTANCE_HIGH,
  IMPORTANCE_LOW,
  filterImportance,
  importanceBand,
} from '@/lib/memory/scoring/importance';
import { isLexicalEmbeddingProvider } from '@/lib/embedding/cosine-scale';

const NOW_MS = Date.parse('2026-06-27T12:00:00.000Z');

const num = (x: number): number | string => (Number.isFinite(x) ? x : String(x));
const emit = (row: unknown) => process.stdout.write(JSON.stringify(row) + '\n');

// ---- the formula grid (bug 216) ------------------------------------------
const COUNTS: Array<[string, number]> = [
  ['NaN', NaN], ['-1', -1], ['0', 0], ['0.5', 0.5], ['1', 1], ['2', 2], ['3', 3],
  ['7', 7], ['15', 15], ['64', 64], ['100', 100], ['Infinity', Infinity],
];
const IMPORTANCES = [0, 0.3, 0.5, 0.9, 0.97, 1];
for (const [token, n] of COUNTS) {
  for (const i of IMPORTANCES) {
    emit({ kind: 'formula', i, n: token, out: num(reinforcedImportance(i, n)) });
  }
}

// ---- salience -------------------------------------------------------------
type Mem = Record<string, unknown>;
const SALIENCE: Array<[string, Mem]> = [
  ['fresh', { importance: 0.9, createdAt: '2026-06-27T00:00:00.000Z' }],
  ['month-old', { importance: 0.6, createdAt: '2026-05-28T12:00:00.000Z' }],
  ['future', { importance: 0.7, createdAt: '2026-07-10T00:00:00.000Z' }],
  ['unparseable-created', { importance: 0.5, createdAt: 'not-a-date' }],
  ['reinforced-absent', { importance: 0.4, createdAt: '2026-06-01T00:00:00.000Z' }],
  ['reinforced-present', { importance: 0.4, reinforcedImportance: 0.75, createdAt: '2026-06-01T00:00:00.000Z' }],
  ['reinforced-zero', { importance: 0.4, reinforcedImportance: 0, createdAt: '2026-06-01T00:00:00.000Z' }],
  ['reinforced-null', { importance: 0.4, reinforcedImportance: null, createdAt: '2026-06-01T00:00:00.000Z' }],
  ['last-reinforced-later', { importance: 0.5, createdAt: '2026-01-01T00:00:00.000Z', lastReinforcedAt: '2026-06-20T00:00:00.000Z' }],
  ['last-reinforced-earlier', { importance: 0.5, createdAt: '2026-06-20T00:00:00.000Z', lastReinforcedAt: '2026-01-01T00:00:00.000Z' }],
  ['last-reinforced-empty', { importance: 0.5, createdAt: '2026-06-20T00:00:00.000Z', lastReinforcedAt: '' }],
  ['last-reinforced-unparseable', { importance: 0.5, createdAt: '2026-06-20T00:00:00.000Z', lastReinforcedAt: 'soon' }],
];
for (const [id, m] of SALIENCE) {
  const s = salience(m as any, { nowMs: NOW_MS });
  emit({
    kind: 'salience', id, input: m,
    referenceTimeMs: num(referenceTimeMs(m as any)),
    out: { salience: num(s.salience), base: num(s.base), decay: num(s.decay), daysSince: num(s.daysSince) },
  });
}

// ---- sortBySalience (stable ties; a NaN key) -------------------------------
const SORTS: Array<[string, Array<Mem & { id: string }>]> = [
  ['ties-keep-input-order', [
    { id: 'a', importance: 0.5, createdAt: '2026-06-01T00:00:00.000Z' },
    { id: 'b', importance: 0.5, createdAt: '2026-06-01T00:00:00.000Z' },
    { id: 'c', importance: 0.9, createdAt: '2026-06-01T00:00:00.000Z' },
    { id: 'd', importance: 0.5, createdAt: '2026-06-01T00:00:00.000Z' },
  ]],
  ['mixed', [
    { id: 'old-high', importance: 0.9, createdAt: '2025-06-27T00:00:00.000Z' },
    { id: 'new-low', importance: 0.2, createdAt: '2026-06-27T00:00:00.000Z' },
    { id: 'mid', importance: 0.6, createdAt: '2026-06-10T00:00:00.000Z' },
    { id: 'reinforced', importance: 0.3, reinforcedImportance: 0.95, createdAt: '2026-06-26T00:00:00.000Z' },
  ]],
  ['nan-key', [
    { id: 'p', importance: 0.4, createdAt: '2026-06-01T00:00:00.000Z' },
    { id: 'nan', importance: 0.9, createdAt: 'not-a-date' },
    { id: 'q', importance: 0.8, createdAt: '2026-06-01T00:00:00.000Z' },
    { id: 'r', importance: 0.1, createdAt: '2026-06-01T00:00:00.000Z' },
  ]],
  ['nan-key-first', [
    { id: 'nan', importance: 0.9, createdAt: 'nope' },
    { id: 'x', importance: 0.2, createdAt: '2026-06-01T00:00:00.000Z' },
    { id: 'y', importance: 0.7, createdAt: '2026-06-01T00:00:00.000Z' },
  ]],
  ['empty', []],
];
for (const [id, items] of SORTS) {
  emit({ kind: 'sort', id, items, out: sortBySalience(items as any, { nowMs: NOW_MS }).map((m: any) => m.id) });
}

// ---- the protection wrapper ------------------------------------------------
const PROTECT: Array<[string, Mem]> = [
  ['auto-low', { importance: 0.2, createdAt: '2025-09-01T00:00:00.000Z', source: 'AUTO' }],
  ['auto-high', { importance: 1, createdAt: '2026-06-27T00:00:00.000Z', reinforcementCount: 64, relatedMemoryIds: ['a', 'b', 'c', 'd'], lastAccessedAt: '2026-06-26T00:00:00.000Z', source: 'AUTO' }],
  ['manual-low', { importance: 0.2, createdAt: '2025-09-01T00:00:00.000Z', source: 'MANUAL' }],
  ['consolidated-low', { importance: 0.1, createdAt: '2025-01-01T00:00:00.000Z', source: 'CONSOLIDATED' }],
  ['absent-source-low', { importance: 0.2, createdAt: '2025-09-01T00:00:00.000Z' }],
  ['absent-source-high', { importance: 1, createdAt: '2026-06-27T00:00:00.000Z', reinforcementCount: 64, relatedMemoryIds: ['a', 'b', 'c', 'd'], lastAccessedAt: '2026-06-26T00:00:00.000Z' }],
  ['lowercase-manual', { importance: 0.2, createdAt: '2025-09-01T00:00:00.000Z', source: 'manual' }],
  ['episodic-threshold', { importance: 0.3, createdAt: '2026-06-20T00:00:00.000Z', reinforcementCount: 3, kind: 'episodic' }],
  ['negative-count', { importance: 0.5, createdAt: '2026-06-20T00:00:00.000Z', reinforcementCount: -3 }],
  ['access-unparseable', { importance: 0.5, createdAt: '2026-06-20T00:00:00.000Z', lastAccessedAt: 'later' }],
];
for (const [id, m] of PROTECT) {
  const p = protection(m as any, { nowMs: NOW_MS });
  emit({
    kind: 'protection', id, input: m,
    out: {
      score: num(p.score), contentComponent: num(p.contentComponent), reinforcementBonus: num(p.reinforcementBonus),
      graphDegreeBonus: num(p.graphDegreeBonus), recentAccessBonus: num(p.recentAccessBonus),
      episodicBonus: num(p.episodicBonus), daysSinceRefTime: num(p.daysSinceRefTime),
      protected: p.protected, protectedBySource: p.protectedBySource,
    },
  });
}

// ---- filterImportance / importanceBand --------------------------------------
const FILTERS: Array<[string, Mem]> = [
  ['absent', { importance: 0.4 }],
  ['null', { importance: 0.4, reinforcedImportance: null }],
  ['zero', { importance: 0.4, reinforcedImportance: 0 }],
  ['present', { importance: 0.4, reinforcedImportance: 0.8 }],
];
for (const [id, m] of FILTERS) emit({ kind: 'filter', id, input: m, out: filterImportance(m as any) });

for (const [token, v] of [
  ['0', 0], ['0.29999', 0.29999], ['0.3', 0.3], ['0.5', 0.5], ['0.69999', 0.69999], ['0.7', 0.7], ['1', 1], ['NaN', NaN],
] as Array<[string, number]>) {
  emit({ kind: 'band', value: token, out: importanceBand(v) });
}

// ---- isLexicalEmbeddingProvider ----------------------------------------------
for (const [id, p] of [
  ['BUILTIN', 'BUILTIN'], ['builtin', 'builtin'], ['empty', ''], ['null', null], ['undefined', undefined], ['OPENAI', 'OPENAI'],
] as Array<[string, string | null | undefined]>) {
  emit({ kind: 'lexical', id, provider: p ?? null, out: isLexicalEmbeddingProvider(p) });
}

// ---- the constants --------------------------------------------------------------
emit({
  kind: 'constants',
  REINFORCED_IMPORTANCE_STEP, SALIENCE_HALF_LIFE_DAYS, PROTECTION_THRESHOLD,
  RANKING_RELEVANCE_WEIGHT, RANKING_PRIORITY_WEIGHT, DEFAULT_MIN_COSINE_NEURAL, DEFAULT_MIN_COSINE_TFIDF,
  RECALL_MIN_IMPORTANCE, TOOL_MIN_IMPORTANCE, IMPORTANCE_HIGH, IMPORTANCE_LOW,
  DEFAULT_PROTECTION_CONFIG, relevanceOfHalf: relevance(0.5),
});
