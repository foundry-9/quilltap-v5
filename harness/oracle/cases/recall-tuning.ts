/**
 * Oracle case: recall tuning (P4.D267 — v4 `f7d8064be`'s recall multiplier
 * retuning, pinned at `01a83539d`).
 *
 * Drives the REAL pure functions of v4's lib/memory/recall-tuning.ts —
 * `DEFAULT_RECALL_TUNING`, `resolveRecallTuning`, `selectSpecificAnchors`,
 * `describeRecallTuning` — and BOTH of its Zod schemas' `safeParse` (wrapped
 * in `.optional()` exactly as the recall-replay route calls them) over a fixed
 * corpus, emitting NDJSON. Every row carries its INPUT, so the Rust side
 * (`recall_tuning_equivalence`) recomputes from the same bytes and compares.
 *
 * `safeParse` rows carry `data` / `issues` as JSON STRINGS so key order is a
 * compared byte (serde_json's Map equality is order-blind).
 *
 * IMPORTANT — imports the actual app code. Run from inside the v4 checkout (or
 * a worktree pinned at the baseline) so `@/` resolves:
 *
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/recall-tuning.ts \
 *     > /tmp/oracle-recall-tuning.ndjson
 */

import {
  DEFAULT_RECALL_TUNING,
  RecallReplaySignalsSchema,
  RecallTuningInputSchema,
  describeRecallTuning,
  resolveRecallTuning,
  selectSpecificAnchors,
  type RecallTuningInput,
} from '@/lib/memory/recall/tuning';

const out: unknown[] = [];

out.push({ kind: 'default', tuning: DEFAULT_RECALL_TUNING });

// ---- resolveRecallTuning + describeRecallTuning ------------------------------
// Each input is validated first (as the route does) — the resolver only ever
// sees `tuningParse.data`.
const LEGACY = {
  boostGateAbs: 0,
  boostGateMargin: 0,
  boostCap: 4,
  multipliers: { freshEvent24h: 1.6, freshEvent48h: 1.35 },
  specificAnchors: false,
};
const resolveCases: Array<[string, unknown]> = [
  ['undefined', undefined],
  ['empty', {}],
  ['abs-only', { boostGateAbs: 0.5 }],
  ['abs-zero', { boostGateAbs: 0 }],
  ['abs-null', { boostGateAbs: null }],
  ['margin-only', { boostGateMargin: 0.1 }],
  ['margin-zero', { boostGateMargin: 0 }],
  ['margin-null', { boostGateMargin: null }],
  ['both-zero', { boostGateAbs: 0, boostGateMargin: 0 }],
  ['both-null', { boostGateAbs: null, boostGateMargin: null }],
  ['zero-and-null', { boostGateAbs: 0, boostGateMargin: null, boostGateRamp: 0.1 }],
  ['abs-zero-margin-set', { boostGateAbs: 0, boostGateMargin: 0.15 }],
  ['ramp-zero', { boostGateAbs: 0.45, boostGateMargin: 0, boostGateRamp: 0 }],
  ['ramp-null', { boostGateRamp: null }],
  ['ramp-set', { boostGateRamp: 0.25 }],
  ['cap', { boostCap: 1.6 }],
  ['cap-int', { boostCap: 4 }],
  ['fresh-bypass', { freshBypassesGate: true }],
  ['window-bypass', { windowBypassesGate: true }],
  ['both-bypass-false', { freshBypassesGate: false, windowBypassesGate: false }],
  ['one-multiplier', { multipliers: { freshEvent24h: 1.45 } }],
  [
    'all-multipliers',
    {
      multipliers: {
        freshEvent48h: 1.01,
        freshEvent24h: 1.02,
        occurredWithinWindow: 1.03,
        temporalMomentRetrospective: 1.04,
        temporalPastRetrospective: 1.05,
        recentlyWhispered: 0.5,
        participantPresent: 1.06,
        contextMatch: 1.07,
        temporalMoment: 0.6,
        temporalPast: 0.7,
        scopeNarrowCrossProjectDownWeight: 0.2,
        scopeNarrowSameProject: 1.08,
      },
    },
  ],
  ['multiplier-equal-default', { multipliers: { contextMatch: 1.1 } }],
  ['empty-multipliers', { multipliers: {} }],
  ['legacy', LEGACY],
  ['anchors-off', { specificAnchors: false }],
  ['anchors-off-with-knobs', { specificAnchors: false, anchorMinHits: 3, anchorOrder: 'distiller' }],
  ['anchors-minhits', { anchorMinHits: 2 }],
  ['anchors-distiller', { anchorOrder: 'distiller' }],
  ['anchors-on-explicit', { specificAnchors: true }],
  ['background', { backgroundReserve: 0.34 }],
  ['background-zero', { backgroundReserve: 0 }],
  ['jest-describe', { boostGateMargin: 0.1, boostCap: 1.6, multipliers: { freshEvent24h: 1.45 } }],
  [
    'full-candidate',
    {
      boostGateAbs: 0.45,
      boostGateMargin: 0.15,
      boostGateRamp: 0.1,
      boostCap: 1.6,
      multipliers: { freshEvent24h: 1.3, freshEvent48h: 1.15 },
      specificAnchors: true,
      backgroundReserve: 0.33,
    },
  ],
  [
    'every-knob',
    {
      boostGateAbs: 0.3,
      boostGateMargin: null,
      boostGateRamp: 0.05,
      boostCap: 2.5,
      freshBypassesGate: true,
      windowBypassesGate: true,
      multipliers: { participantPresent: 1.5 },
      specificAnchors: true,
      anchorMinHits: 4,
      anchorOrder: 'distiller',
      backgroundReserve: 0.5,
    },
  ],
  ['small-number', { boostGateAbs: 0.0000001 }],
];
for (const [id, raw] of resolveCases) {
  const parsed = RecallTuningInputSchema.optional().safeParse(raw);
  if (!parsed.success) throw new Error(`resolve case ${id} does not validate`);
  const resolved = resolveRecallTuning(parsed.data as RecallTuningInput | undefined);
  const row: Record<string, unknown> = { kind: 'resolve', id };
  if (raw !== undefined) row.input = raw;
  row.resolved = resolved;
  row.described = describeRecallTuning(resolved);
  out.push(row);
}

// ---- selectSpecificAnchors ------------------------------------------------------
type Cand = { phrase: string; count: number };
const c = (pairs: Array<[string, number]>): Cand[] => pairs.map(([phrase, count]) => ({ phrase, count }));
const selectCases: Array<[string, Cand[], string[], number | undefined, { minHits?: number; order?: 'rarest' | 'distiller' }]> = [
  ['jest-rarest', c([['Amy', 40], ['Steinway', 0], ['Tessarium', 1], ['Prospero', 3]]), ['Amy'], 3, {}],
  ['jest-ties', c([['A', 2], ['B', 2], ['C', 1], ['D', 2]]), [], 3, {}],
  ['present-case-whitespace', c([['  amy ', 2], ['AMY', 1], ['Bob', 3], ['Cid', 1]]), [' Amy  ', '', '   ', 'bob'], 3, {}],
  ['zero-hit-only', c([['X', 0], ['Y', 0]]), [], 3, {}],
  ['minhits-2', c([['A', 1], ['B', 2], ['C', 5], ['D', 3]]), [], 3, { minHits: 2 }],
  ['minhits-0-floors-to-1', c([['A', 0], ['B', 1]]), [], 3, { minHits: 0 }],
  ['minhits-3-jest', c([['Mochi', 3], ['Feline Duchy', 2], ['Ariel Stars', 7]]), [], 3, { minHits: 3 }],
  ['distiller', c([['Mochi', 3], ['Feline Duchy', 2], ['Ariel Stars', 7]]), [], 3, { order: 'distiller' }],
  ['distiller-drops', c([['Amy', 9], ['Tessarium', 1], ['Nobody', 0], ['Prospero', 4]]), ['Amy'], 3, { order: 'distiller' }],
  ['max-cut', c([['A', 5], ['B', 4], ['C', 3], ['D', 2], ['E', 1]]), [], 2, {}],
  ['max-default', c([['A', 5], ['B', 4], ['C', 3], ['D', 2], ['E', 1]]), [], undefined, {}],
  ['duplicates', c([['Quay', 2], ['Quay', 2], ['Lamp', 1]]), [], 3, {}],
  ['empty', [], ['Amy'], 3, {}],
];
for (const [id, cands, present, max, opts] of selectCases) {
  const chosen = max === undefined ? selectSpecificAnchors(cands, present) : selectSpecificAnchors(cands, present, max, opts);
  out.push({ kind: 'select', id, candidates: cands, present, max: max ?? null, minHits: opts.minHits ?? null, order: opts.order ?? null, chosen });
}

// ---- the two schemas, as the route calls them -----------------------------------
const parseRow = (kind: string, id: string, schema: { safeParse: (v: unknown) => any }, input: unknown) => {
  const r = schema.safeParse(input);
  const row: Record<string, unknown> = { kind, id };
  if (input !== undefined) row.input = input;
  row.success = r.success;
  if (r.success) row.data = JSON.stringify(r.data ?? null);
  else row.issues = JSON.stringify(r.error.issues);
  out.push(row);
};
const T = RecallTuningInputSchema.optional();
const tuningParseCases: Array<[string, unknown]> = [
  ['undefined', undefined],
  ['full-candidate', resolveCases.find(([id]) => id === 'full-candidate')![1]],
  ['scrambled', { backgroundReserve: 0.1, boostGateAbs: 0.5, multipliers: { freshEvent48h: 1, scopeNarrowSameProject: 2 }, anchorOrder: 'distiller' }],
  ['gate-nulls', { boostGateRamp: null, boostGateAbs: null }],
  ['unknown-one', { boostGateMarginX: 0.1 }],
  ['unknown-two', { zz: 1, aa: 2 }],
  ['unknown-nested', { multipliers: { freshEvent12h: 2 } }],
  ['cap-low', { boostCap: 0.5 }],
  ['cap-high', { boostCap: 11 }],
  ['cap-string', { boostCap: 'x' }],
  ['cap-null', { boostCap: null }],
  ['abs-high', { boostGateAbs: 1.5 }],
  ['abs-negative', { boostGateAbs: -0.1 }],
  ['abs-string', { boostGateAbs: 'x' }],
  ['abs-bool', { boostGateMargin: true }],
  ['minhits-fraction', { anchorMinHits: 1.5 }],
  ['minhits-zero', { anchorMinHits: 0 }],
  ['minhits-high', { anchorMinHits: 101 }],
  ['minhits-unsafe', { anchorMinHits: 1e300 }],
  ['minhits-unsafe-negative', { anchorMinHits: -1e300 }],
  ['minhits-string', { anchorMinHits: '2' }],
  ['order-bad', { anchorOrder: 'x' }],
  ['order-null', { anchorOrder: null }],
  ['order-number', { anchorOrder: 1 }],
  ['background-high', { backgroundReserve: 0.6 }],
  ['fresh-bypass-string', { freshBypassesGate: 'yes' }],
  ['window-bypass-null', { windowBypassesGate: null }],
  ['anchors-number', { specificAnchors: 1 }],
  ['multipliers-array', { multipliers: [] }],
  ['multipliers-null', { multipliers: null }],
  ['multipliers-high', { multipliers: { freshEvent24h: 11 } }],
  ['multipliers-mixed', { multipliers: { contextMatch: -1, temporalPast: 'x' } }],
  ['field-plus-unknown', { boostCap: 0.5, zz: 1, anchorOrder: 'x' }],
  ['nested-plus-top-unknown', { zz: 1, multipliers: { freshEvent24h: 11, q: 1 } }],
  ['root-null', null],
  ['root-number', 5],
  ['root-array', []],
  ['root-string', 'x'],
  ['root-bool', true],
];
for (const [id, input] of tuningParseCases) parseRow('tuningParse', id, T, input);

const S = RecallReplaySignalsSchema.optional();
const signalsParseCases: Array<[string, unknown]> = [
  ['undefined', undefined],
  ['minimal', { keywords: [] }],
  [
    'full',
    {
      keywords: ['Gullwing Quay', 'dawn'],
      temporal: 'past',
      context: 'history',
      paraphrase: 'They remember the quay.',
      retrospective: true,
      timeRange: { from: '2026-06-01', to: '2026-06-07' },
      entities: ['Gullwing Quay'],
    },
  ],
  ['scrambled', { entities: ['a'], keywords: ['k'], retrospective: false, paraphrase: '', temporal: 'moment' }],
  ['extra-top-key', { keywords: ['k'], zz: 1, nested: { a: 1 } }],
  ['extra-timerange-key', { keywords: [], timeRange: { to: 'b', from: 'a', x: 1 } }],
  ['timerange-null', { keywords: ['a'], timeRange: null, paraphrase: '' }],
  ['missing-keywords', {}],
  ['keywords-string', { keywords: 'fair' }],
  ['keywords-number-element', { keywords: [1] }],
  ['keywords-mixed', { keywords: ['a', 2, 'b', 3] }],
  ['keywords-null', { keywords: null }],
  ['bad-temporal-context', { keywords: [], temporal: 'x', context: 'y' }],
  ['temporal-null', { keywords: [], temporal: null }],
  ['timerange-from-only', { keywords: [], timeRange: { from: 'a' } }],
  ['timerange-empty', { keywords: [], timeRange: {} }],
  ['timerange-array', { keywords: [], timeRange: [] }],
  ['timerange-numbers', { keywords: [], timeRange: { from: 1, to: 2 } }],
  ['entities-number', { keywords: [], entities: [1] }],
  ['entities-null', { keywords: [], entities: null }],
  ['retrospective-string', { keywords: [], retrospective: 'true' }],
  ['paraphrase-number', { keywords: [], paraphrase: 5 }],
  ['everything-wrong', { keywords: 'x', temporal: 1, context: 2, paraphrase: 3, retrospective: 4, timeRange: 5, entities: 6 }],
  ['root-null', null],
  ['root-array', []],
  ['root-number', 7],
];
for (const [id, input] of signalsParseCases) parseRow('signalsParse', id, S, input);

for (const r of out) process.stdout.write(JSON.stringify(r) + '\n');
