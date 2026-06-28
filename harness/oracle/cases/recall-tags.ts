/**
 * Oracle case #3: recall-side targeting-tag multipliers.
 *
 * Drives the REAL pure functions from the v4 server's
 * lib/memory/recall-tags.ts over fixed corpora and emits NDJSON. This is the
 * "recall-tag / anti-repetition multipliers" Phase-1 target: a self-contained,
 * I/O-free module (its own header certifies "Pure + I/O-free — no logging, no
 * DB, no LLM") of seven functions —
 *   parseTargetingTags, scopeProjectMultiplier, temporalMultiplier,
 *   contextMultiplier, participantMultiplier, recentlyWhisperedMultiplier,
 *   combineRecallMultipliers.
 *
 * It exercises all three equivalence shapes the harness proves: float
 * multipliers (1e-12), exact unicode strings (the `fired` debug labels like
 * `narrow✓` / `past↓`), and small enum/bool structs (TargetingTags, exclude).
 *
 * IMPORTANT — this imports the actual app code, it does not reimplement it.
 * Run it from inside the server checkout so `@/` path aliases resolve:
 *
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/recall-tags.ts \
 *     > /tmp/oracle-recall-tags.ndjson
 *
 * The corpus is fixed in code (no randomness, no Date.now()), so the Rust side
 * hardcodes the identical inputs and compares field-for-field by `id`.
 */

import {
  parseTargetingTags,
  scopeProjectMultiplier,
  temporalMultiplier,
  contextMultiplier,
  participantMultiplier,
  recentlyWhisperedMultiplier,
  combineRecallMultipliers,
  type TargetingTags,
  type TemporalTag,
  type ScopeTag,
  type ContextTag,
  type ScopePolicy,
  type RecallContext,
} from '@/lib/memory/recall-tags';

// Each row is tagged by `kind` so the Rust side dispatches to the right fn.
// `mult` rows carry the RecallMultiplier shape ({ multiplier, fired, exclude? });
// `parse` rows carry the parsed TargetingTags; `combine` rows carry the
// CombinedRecallAdjustment ({ multiplier, fired, exclude }).
type Row =
  | { kind: 'parse'; id: string; temporal: TemporalTag; scope: ScopeTag; context: ContextTag }
  | { kind: 'mult'; fn: string; id: string; multiplier: number; fired: string[]; exclude?: boolean }
  | { kind: 'combine'; id: string; multiplier: number; fired: string[]; exclude: boolean };

const rows: Row[] = [];

function tags(temporal: TemporalTag, scope: ScopeTag, context: ContextTag): TargetingTags {
  return { temporal, scope, context };
}

// ---------------------------------------------------------------------------
// parseTargetingTags — branch + last-match-wins + normalization coverage.
// ---------------------------------------------------------------------------
const parseCases: Array<[string, string[]]> = [
  ['empty', []],
  ['temporal-past', ['past']],
  ['temporal-future', ['future']],
  ['scope-narrow', ['scope: narrow']],
  ['scope-narrow-nospace', ['scope:narrow']],
  ['scope-wide', ['scope: wide']],
  ['context-history', ['history']],
  ['context-banter', ['banter']],
  ['all-three', ['past', 'scope: narrow', 'philosophy']],
  ['case-insensitive', ['PAST', 'SCOPE: NARROW', 'Philosophy']],
  ['whitespace', ['  moment  ', ' scope:  wide ']],
  ['last-wins-temporal', ['past', 'present']],
  ['last-wins-scope', ['scope: narrow', 'scope: wide']],
  ['unknown-scope', ['scope: sideways']],
  ['unknown-word', ['banana']],
  ['free-then-real-context', ['information', 'philosophy']],
];
for (const [id, keywords] of parseCases) {
  const t = parseTargetingTags(keywords);
  rows.push({ kind: 'parse', id, temporal: t.temporal, scope: t.scope, context: t.context });
}

// ---------------------------------------------------------------------------
// scopeProjectMultiplier(tags, memoryProjectId, currentProjectId, policy).
// ---------------------------------------------------------------------------
const W = tags('present', 'wide', 'information');
const N = tags('present', 'narrow', 'information');
const scopeCases: Array<[string, TargetingTags, string | null, string | null, ScopePolicy]> = [
  ['wide-passthrough', W, 'P1', 'P1', 'down-weight'],
  ['narrow-no-memproj', N, null, 'P1', 'down-weight'],
  ['narrow-same-project', N, 'P1', 'P1', 'down-weight'],
  ['narrow-cross-downweight', N, 'P1', 'P2', 'down-weight'],
  ['narrow-cross-exclude', N, 'P1', 'P2', 'exclude'],
  ['narrow-memproj-no-current', N, 'P1', null, 'down-weight'],
  ['narrow-memproj-no-current-exclude', N, 'P1', null, 'exclude'],
];
for (const [id, t, memProj, curProj, policy] of scopeCases) {
  const r = scopeProjectMultiplier(t, memProj, curProj, policy);
  rows.push({ kind: 'mult', fn: 'scope', id, multiplier: r.multiplier, fired: r.fired, exclude: r.exclude });
}

// ---------------------------------------------------------------------------
// temporalMultiplier(tags).
// ---------------------------------------------------------------------------
for (const temporal of ['past', 'moment', 'present', 'future'] as TemporalTag[]) {
  const r = temporalMultiplier(tags(temporal, 'wide', 'information'));
  rows.push({ kind: 'mult', fn: 'temporal', id: temporal, multiplier: r.multiplier, fired: r.fired });
}

// ---------------------------------------------------------------------------
// contextMultiplier(tags, turnContext).
// ---------------------------------------------------------------------------
const ctxCases: Array<[string, ContextTag, ContextTag | null]> = [
  ['match', 'philosophy', 'philosophy'],
  ['no-match', 'philosophy', 'banter'],
  ['null-turn', 'philosophy', null],
];
for (const [id, memCtx, turnCtx] of ctxCases) {
  const r = contextMultiplier(tags('present', 'wide', memCtx), turnCtx);
  rows.push({ kind: 'mult', fn: 'context', id, multiplier: r.multiplier, fired: r.fired });
}

// ---------------------------------------------------------------------------
// participantMultiplier(memory, presentAboutCharacterIds).
// ---------------------------------------------------------------------------
const partCases: Array<[string, string | null, string[]]> = [
  ['present', 'C1', ['C1', 'C2']],
  ['absent', 'C3', ['C1']],
  ['no-about', null, ['C1']],
  ['empty-present', 'C1', []],
];
for (const [id, aboutCharacterId, present] of partCases) {
  const r = participantMultiplier({ aboutCharacterId }, present);
  rows.push({ kind: 'mult', fn: 'participant', id, multiplier: r.multiplier, fired: r.fired });
}

// ---------------------------------------------------------------------------
// recentlyWhisperedMultiplier(memory, recentlyWhisperedIds).
// ---------------------------------------------------------------------------
const recentCases: Array<[string, string | null, string[]]> = [
  ['whispered', 'M1', ['M1']],
  ['not', 'M2', ['M1']],
  ['no-id', null, ['M1']],
  ['empty-set', 'M1', []],
];
for (const [id, memId, recent] of recentCases) {
  const r = recentlyWhisperedMultiplier({ id: memId ?? undefined }, new Set(recent));
  rows.push({ kind: 'mult', fn: 'recent', id, multiplier: r.multiplier, fired: r.fired });
}

// ---------------------------------------------------------------------------
// combineRecallMultipliers(memory, ctx) — the integrator: product → clamp,
// exclude short-circuit, fired-label concatenation order.
// ---------------------------------------------------------------------------
interface MemoryTagView {
  id?: string;
  projectId?: string | null;
  keywords?: readonly string[] | null;
  aboutCharacterId?: string | null;
}
const combineCases: Array<[string, MemoryTagView, RecallContext]> = [
  [
    'plain',
    { id: 'M1', projectId: null, keywords: [], aboutCharacterId: null },
    { currentProjectId: null, scopePolicy: 'down-weight' },
  ],
  [
    'exclude-shortcircuit',
    { id: 'M1', projectId: 'P1', keywords: ['scope: narrow'], aboutCharacterId: null },
    { currentProjectId: 'P2', scopePolicy: 'exclude' },
  ],
  [
    'stacked-boosts',
    { id: 'M1', projectId: 'P1', keywords: ['scope: narrow', 'philosophy'], aboutCharacterId: 'C1' },
    {
      currentProjectId: 'P1',
      scopePolicy: 'down-weight',
      turnContext: 'philosophy',
      presentAboutCharacterIds: ['C1'],
    },
  ],
  [
    'stacked-penalties',
    { id: 'M1', projectId: 'P1', keywords: ['scope: narrow', 'past'], aboutCharacterId: 'C9' },
    {
      currentProjectId: 'P2',
      scopePolicy: 'down-weight',
      recentlyWhisperedIds: new Set(['M1']),
    },
  ],
  [
    'mixed',
    { id: 'M2', projectId: null, keywords: ['moment'], aboutCharacterId: 'C1' },
    {
      currentProjectId: 'P1',
      scopePolicy: 'down-weight',
      turnContext: 'banter',
      presentAboutCharacterIds: ['C1'],
      recentlyWhisperedIds: new Set<string>(),
    },
  ],
];
for (const [id, memory, ctx] of combineCases) {
  const r = combineRecallMultipliers(memory, ctx);
  rows.push({ kind: 'combine', id, multiplier: r.multiplier, fired: r.fired, exclude: r.exclude });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
