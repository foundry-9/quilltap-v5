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
  occurredWithinMultiplier,
  freshEventMultiplier,
  combineRecallMultipliers,
  boostGateThreshold,
  boostGateStrength,
  type BoostGate,
  type TargetingTags,
  type TemporalTag,
  type ScopeTag,
  type ContextTag,
  type ScopePolicy,
  type RecallContext,
} from '@/lib/memory/recall-tags';
import { RecallTuningInputSchema, resolveRecallTuning } from '@/lib/memory/recall-tuning';

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
// temporalMultiplier(tags[, retrospective]) — episodic overhaul: the
// retrospective flip. The bare-call rows keep their original ids (the default
// arg is itself a behavior to pin); the explicit-arg rows are new.
// ---------------------------------------------------------------------------
for (const temporal of ['past', 'moment', 'present', 'future'] as TemporalTag[]) {
  const r = temporalMultiplier(tags(temporal, 'wide', 'information'));
  rows.push({ kind: 'mult', fn: 'temporal', id: temporal, multiplier: r.multiplier, fired: r.fired });
}
for (const temporal of ['past', 'moment', 'present', 'future'] as TemporalTag[]) {
  for (const retro of [false, true]) {
    const r = temporalMultiplier(tags(temporal, 'wide', 'information'), retro);
    rows.push({
      kind: 'mult',
      fn: 'temporal-retro',
      id: `${temporal}-${retro}`,
      multiplier: r.multiplier,
      fired: r.fired,
    });
  }
}

// ---------------------------------------------------------------------------
// occurredWithinMultiplier(memory, window) — the soft time-window boost.
// occurredAt ?? createdAt (nullish — an EMPTY occurredAt is used and parses
// NaN), Date.parse finiteness on all three, inclusive bounds.
// ---------------------------------------------------------------------------
const WINDOW = { from: '2026-07-13T00:00:00.000Z', to: '2026-07-19T23:59:59.999Z' };
const windowCases: Array<[
  string,
  { occurredAt?: string | null; createdAt?: string },
  { from: string; to: string } | null | undefined,
]> = [
  ['inside', { occurredAt: '2026-07-14T00:00:00.000Z', createdAt: '2026-07-01T00:00:00.000Z' }, WINDOW],
  ['outside', { occurredAt: '2026-01-01T00:00:00.000Z', createdAt: '2026-01-01T00:00:00.000Z' }, WINDOW],
  ['edge-from', { occurredAt: '2026-07-13T00:00:00.000Z' }, WINDOW],
  ['edge-to', { occurredAt: '2026-07-19T23:59:59.999Z' }, WINDOW],
  ['created-fallback', { occurredAt: null, createdAt: '2026-07-14T12:00:00.000Z' }, WINDOW],
  ['empty-occurred-not-nullish', { occurredAt: '', createdAt: '2026-07-14T12:00:00.000Z' }, WINDOW],
  ['unparsable-event', { occurredAt: 'the third night at sea', createdAt: '2026-07-14T12:00:00.000Z' }, WINDOW],
  ['no-event-time', {}, WINDOW],
  ['unparsable-window-from', { occurredAt: '2026-07-14T00:00:00.000Z' }, { from: 'whenever', to: WINDOW.to }],
  ['unparsable-window-to', { occurredAt: '2026-07-14T00:00:00.000Z' }, { from: WINDOW.from, to: 'whenever' }],
  ['null-window', { occurredAt: '2026-07-14T00:00:00.000Z' }, null],
  ['undefined-window', { occurredAt: '2026-07-14T00:00:00.000Z' }, undefined],
];
for (const [id, mem, window] of windowCases) {
  const r = occurredWithinMultiplier(mem, window);
  rows.push({ kind: 'mult', fn: 'window', id, multiplier: r.multiplier, fired: r.fired });
}

// ---------------------------------------------------------------------------
// freshEventMultiplier(memory, nowMs, currentChatId) — the unconditional
// fresh-event boost (v4 `505dcb1f`). The guard ladder IN ORDER: no/non-finite
// clock, the echo guard (TRUTHINESS on both chat ids), occurredAt ?? createdAt,
// Date.parse finiteness, negative age, then the two inclusive bands.
// ---------------------------------------------------------------------------
const FRESH_NOW = Date.parse('2026-07-29T02:44:00.000Z');
const FRESH_HOUR = 60 * 60 * 1000;
const FRESH_CHAT = 'chat-current';
const freshAt = (msAgo: number) => new Date(FRESH_NOW - msAgo).toISOString();
const freshCases: Array<[
  string,
  { occurredAt?: string | null; createdAt?: string; chatId?: string | null },
  number | null | undefined,
  string | null | undefined,
]> = [
  ['inside-24h', { occurredAt: freshAt(6 * FRESH_HOUR) }, FRESH_NOW, FRESH_CHAT],
  ['edge-24h-inclusive', { occurredAt: freshAt(24 * FRESH_HOUR) }, FRESH_NOW, FRESH_CHAT],
  ['just-past-24h', { occurredAt: freshAt(24 * FRESH_HOUR + 1) }, FRESH_NOW, FRESH_CHAT],
  ['inside-48h', { occurredAt: freshAt(30 * FRESH_HOUR) }, FRESH_NOW, FRESH_CHAT],
  ['edge-48h-inclusive', { occurredAt: freshAt(48 * FRESH_HOUR) }, FRESH_NOW, FRESH_CHAT],
  ['just-past-48h', { occurredAt: freshAt(48 * FRESH_HOUR + 1) }, FRESH_NOW, FRESH_CHAT],
  ['older-than-48h', { occurredAt: freshAt(49 * FRESH_HOUR) }, FRESH_NOW, FRESH_CHAT],
  ['created-fallback', { createdAt: freshAt(2 * FRESH_HOUR) }, FRESH_NOW, FRESH_CHAT],
  ['created-fallback-null-occurred', { occurredAt: null, createdAt: freshAt(2 * FRESH_HOUR) }, FRESH_NOW, FRESH_CHAT],
  ['empty-occurred-not-nullish', { occurredAt: '', createdAt: freshAt(2 * FRESH_HOUR) }, FRESH_NOW, FRESH_CHAT],
  ['no-clock-undefined', { occurredAt: freshAt(FRESH_HOUR) }, undefined, FRESH_CHAT],
  ['no-clock-null', { occurredAt: freshAt(FRESH_HOUR) }, null, FRESH_CHAT],
  ['no-clock-nan', { occurredAt: freshAt(FRESH_HOUR) }, NaN, FRESH_CHAT],
  ['no-event-time', {}, FRESH_NOW, FRESH_CHAT],
  ['unparsable-event', { occurredAt: 'not a date' }, FRESH_NOW, FRESH_CHAT],
  ['future-event', { occurredAt: freshAt(-FRESH_HOUR) }, FRESH_NOW, FRESH_CHAT],
  ['echo-same-chat', { occurredAt: freshAt(FRESH_HOUR), chatId: FRESH_CHAT }, FRESH_NOW, FRESH_CHAT],
  ['echo-other-chat', { occurredAt: freshAt(FRESH_HOUR), chatId: 'chat-other' }, FRESH_NOW, FRESH_CHAT],
  ['echo-null-chat', { occurredAt: freshAt(FRESH_HOUR), chatId: null }, FRESH_NOW, FRESH_CHAT],
  // Truthiness, not equality: an empty string on EITHER side disables the guard.
  ['echo-empty-memory-chat', { occurredAt: freshAt(FRESH_HOUR), chatId: '' }, FRESH_NOW, ''],
  ['echo-empty-current-chat', { occurredAt: freshAt(FRESH_HOUR), chatId: 'chat-other' }, FRESH_NOW, ''],
  ['echo-no-current-chat', { occurredAt: freshAt(FRESH_HOUR), chatId: FRESH_CHAT }, FRESH_NOW, null],
  ['echo-undefined-current-chat', { occurredAt: freshAt(FRESH_HOUR), chatId: FRESH_CHAT }, FRESH_NOW, undefined],
  // The echo guard precedes the clock guard in neither direction that matters:
  // no clock wins even for a foreign chat.
  ['no-clock-other-chat', { occurredAt: freshAt(FRESH_HOUR), chatId: 'chat-other' }, undefined, FRESH_CHAT],
];
for (const [id, mem, nowMs, currentChatId] of freshCases) {
  const r = freshEventMultiplier(mem, nowMs, currentChatId);
  rows.push({ kind: 'mult', fn: 'fresh', id, multiplier: r.multiplier, fired: r.fired });
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
// Episodic overhaul: the suspended (retrospective re-ask) arm.
for (const [id, memId, recent] of [
  ['whispered-suspended', 'M1', ['M1']],
  ['whispered-not-suspended', 'M1', ['M1']],
] as Array<[string, string, string[]]>) {
  const suspended = id === 'whispered-suspended';
  const r = recentlyWhisperedMultiplier({ id: memId }, new Set(recent), suspended);
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
  occurredAt?: string | null;
  createdAt?: string;
  /** The chat the memory was extracted from — the fresh-event echo guard. */
  chatId?: string | null;
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
  // ---- episodic overhaul arms (P4.d13) ----
  [
    'retro-flip-past',
    {
      id: 'M1',
      projectId: null,
      keywords: ['past', 'scope: wide', 'history'],
      aboutCharacterId: null,
      occurredAt: '2026-07-14T00:00:00.000Z',
      createdAt: '2026-07-14T01:00:00.000Z',
    },
    { currentProjectId: null, scopePolicy: 'down-weight', turnRetrospective: true },
  ],
  [
    'retro-flip-moment',
    { id: 'M1', projectId: null, keywords: ['moment'], aboutCharacterId: null },
    { currentProjectId: null, scopePolicy: 'down-weight', turnRetrospective: true },
  ],
  [
    'retro-suspends-whisper',
    {
      id: 'M1',
      projectId: null,
      keywords: ['past'],
      aboutCharacterId: null,
      occurredAt: '2026-07-14T00:00:00.000Z',
      createdAt: '2026-07-14T01:00:00.000Z',
    },
    {
      currentProjectId: null,
      scopePolicy: 'down-weight',
      turnRetrospective: true,
      recentlyWhisperedIds: new Set(['M1']),
      occurredWithin: { from: '2026-07-13T00:00:00.000Z', to: '2026-07-19T23:59:59.999Z' },
    },
  ],
  [
    'window-in',
    {
      id: 'M1',
      projectId: null,
      keywords: [],
      aboutCharacterId: null,
      occurredAt: '2026-07-14T00:00:00.000Z',
      createdAt: '2026-07-01T00:00:00.000Z',
    },
    {
      currentProjectId: null,
      scopePolicy: 'down-weight',
      occurredWithin: { from: '2026-07-13T00:00:00.000Z', to: '2026-07-19T23:59:59.999Z' },
    },
  ],
  [
    'window-out',
    {
      id: 'M1',
      projectId: null,
      keywords: [],
      aboutCharacterId: null,
      occurredAt: '2026-01-01T00:00:00.000Z',
      createdAt: '2026-01-01T00:00:00.000Z',
    },
    {
      currentProjectId: null,
      scopePolicy: 'down-weight',
      occurredWithin: { from: '2026-07-13T00:00:00.000Z', to: '2026-07-19T23:59:59.999Z' },
    },
  ],
  [
    'window-created-fallback-unparsable-occurred',
    {
      id: 'M1',
      projectId: null,
      keywords: [],
      aboutCharacterId: null,
      occurredAt: 'the third night at sea',
      createdAt: '2026-07-14T12:00:00.000Z',
    },
    {
      currentProjectId: null,
      scopePolicy: 'down-weight',
      occurredWithin: { from: '2026-07-13T00:00:00.000Z', to: '2026-07-19T23:59:59.999Z' },
    },
  ],
  [
    'retro-window-whisper-stack',
    {
      id: 'M1',
      projectId: 'P1',
      keywords: ['scope: narrow', 'past', 'history'],
      aboutCharacterId: 'C1',
      occurredAt: '2026-07-15T09:00:00.000Z',
      createdAt: '2026-07-15T10:00:00.000Z',
    },
    {
      currentProjectId: 'P1',
      scopePolicy: 'down-weight',
      turnContext: 'history',
      turnTemporal: 'past',
      turnRetrospective: true,
      presentAboutCharacterIds: ['C1'],
      recentlyWhisperedIds: new Set(['M1']),
      occurredWithin: { from: '2026-07-13T00:00:00.000Z', to: '2026-07-19T23:59:59.999Z' },
    },
  ],
  [
    'inert-context-unchanged',
    {
      id: 'M1',
      projectId: null,
      keywords: ['past'],
      aboutCharacterId: null,
      occurredAt: '2026-07-14T00:00:00.000Z',
      createdAt: '2026-07-14T01:00:00.000Z',
    },
    {
      currentProjectId: null,
      scopePolicy: 'down-weight',
      recentlyWhisperedIds: new Set(['M1']),
    },
  ],
  // ---- fresh-event boost arms (P4.d26) ----
  // The product order matters for f64 bit-equality: fresh is multiplied LAST
  // and its label concatenated LAST.
  [
    'fresh-with-moment-demotion',
    {
      id: 'M1',
      projectId: 'P1',
      chatId: 'chat-other',
      keywords: ['moment', 'scope: narrow', 'history'],
      occurredAt: freshAt(6 * FRESH_HOUR),
    },
    {
      currentProjectId: 'P1',
      scopePolicy: 'down-weight',
      currentChatId: FRESH_CHAT,
      nowMs: FRESH_NOW,
    },
  ],
  [
    'fresh-inert-without-clock',
    {
      id: 'M1',
      projectId: 'P1',
      chatId: 'chat-other',
      keywords: [],
      occurredAt: freshAt(6 * FRESH_HOUR),
    },
    { currentProjectId: 'P1', scopePolicy: 'down-weight' },
  ],
  [
    'fresh-suppressed-by-echo-guard',
    {
      id: 'M1',
      projectId: null,
      chatId: FRESH_CHAT,
      keywords: [],
      occurredAt: freshAt(FRESH_HOUR),
    },
    {
      currentProjectId: null,
      scopePolicy: 'down-weight',
      currentChatId: FRESH_CHAT,
      nowMs: FRESH_NOW,
    },
  ],
  [
    'fresh48-band-in-combine',
    {
      id: 'M1',
      projectId: null,
      chatId: 'chat-other',
      keywords: ['past'],
      occurredAt: freshAt(30 * FRESH_HOUR),
    },
    {
      currentProjectId: null,
      scopePolicy: 'down-weight',
      currentChatId: FRESH_CHAT,
      nowMs: FRESH_NOW,
    },
  ],
  [
    'maximal-fresh-window-retro-stack',
    {
      id: 'M1',
      projectId: 'P1',
      chatId: 'chat-other',
      aboutCharacterId: 'C1',
      keywords: ['past', 'scope: narrow', 'history'],
      occurredAt: freshAt(3 * FRESH_HOUR),
    },
    {
      currentProjectId: 'P1',
      scopePolicy: 'down-weight',
      currentChatId: FRESH_CHAT,
      nowMs: FRESH_NOW,
      turnRetrospective: true,
      turnContext: 'history',
      presentAboutCharacterIds: ['C1'],
      occurredWithin: {
        from: new Date(FRESH_NOW - 24 * FRESH_HOUR).toISOString(),
        to: new Date(FRESH_NOW).toISOString(),
      },
    },
  ],
  [
    'fresh-with-cross-project-penalty',
    {
      id: 'M1',
      projectId: 'P1',
      chatId: 'chat-other',
      keywords: ['scope: narrow'],
      occurredAt: freshAt(FRESH_HOUR),
    },
    {
      currentProjectId: 'P2',
      scopePolicy: 'down-weight',
      currentChatId: FRESH_CHAT,
      nowMs: FRESH_NOW,
    },
  ],
];
for (const [id, memory, ctx] of combineCases) {
  const r = combineRecallMultipliers(memory, ctx);
  rows.push({ kind: 'combine', id, multiplier: r.multiplier, fired: r.fired, exclude: r.exclude });
}


// ---------------------------------------------------------------------------
// P4.D267 — the recall multiplier retuning (v4 `f7d8064be`, pin `01a83539d`).
// These rows are DATA-DRIVEN: each carries its own inputs (memory, ctx, the
// tuning INPUT, the relevance) so the Rust side recomputes from the same bytes.
// The tuning input runs through v4's real schema + resolver on this side and
// through v5's validator + resolver on that side.
// ---------------------------------------------------------------------------
type Extra =
  | { kind: 'gate'; id: string; gate: BoostGate; best: number; cosine: number; threshold: number; strength: number }
  | { kind: 'multT'; id: string; fn: string; memory: unknown; ctx: unknown; tuningInput: unknown; multiplier: number; fired: string[]; exclude?: boolean }
  | { kind: 'combineT'; id: string; memory: unknown; ctx: unknown; tuningInput?: unknown; relevance?: unknown; multiplier: number; fired: string[]; exclude: boolean };
const extra: Extra[] = [];

// ---- boostGateThreshold / boostGateStrength grid ----
const gates: Array<[string, BoostGate]> = [
  ['abs-only', { abs: 0.45, margin: null, ramp: 0.1 }],
  ['margin-only', { abs: null, margin: 0.15, ramp: 0.1 }],
  ['both', { abs: 0.45, margin: 0.15, ramp: 0.1 }],
  ['both-ramp0', { abs: 0.45, margin: 0.15, ramp: 0 }],
  ['wide-ramp', { abs: 0.3, margin: 0.05, ramp: 0.25 }],
];
const probes: Array<[string, number, number]> = [
  ['above', 0.7, 0.72],
  ['at', 0.57, 0.72],
  ['inside', 0.52, 0.72],
  ['below-ramp', 0.3, 0.72],
  ['low-best-above-abs', 0.5, 0.511],
  ['low-best-at-abs', 0.45, 0.511],
  ['low-best-inside', 0.4, 0.511],
  ['low-best-edge', 0.35, 0.511],
  ['low-best-below', 0.2, 0.511],
  ['tie-125', 0.3625, 0.5],
  ['zero-best', 0.1, 0],
];
for (const [gid, gate] of gates) {
  for (const [pid, cosine, best] of probes) {
    extra.push({
      kind: 'gate',
      id: `${gid}/${pid}`,
      gate,
      best,
      cosine,
      threshold: boostGateThreshold(gate, best),
      strength: boostGateStrength(gate, cosine, best),
    });
  }
}

// ---- JSON → v4 shapes (sets ride as arrays) ----
type J = Record<string, unknown>;
const toCtx = (c: J, tuningInput?: unknown): RecallContext => {
  const ctx = { ...c } as J;
  if (Array.isArray(ctx.recentlyWhisperedIds)) ctx.recentlyWhisperedIds = new Set(ctx.recentlyWhisperedIds as string[]);
  if (tuningInput !== undefined) ctx.tuning = resolveRecallTuning(RecallTuningInputSchema.parse(tuningInput));
  return ctx as unknown as RecallContext;
};

const NOW = Date.parse('2026-10-08T12:00:00.000Z');
const HOUR = 60 * 60 * 1000;
const baseCtx: J = {
  currentProjectId: 'proj-a',
  scopePolicy: 'down-weight',
  turnContext: 'information',
  presentAboutCharacterIds: ['char-present'],
  currentChatId: 'chat-here',
  nowMs: NOW,
};
const boosted: J = {
  id: 'm-boosted',
  projectId: 'proj-a',
  keywords: ['scope: narrow', 'information', 'present'],
  aboutCharacterId: 'char-present',
  createdAt: new Date(NOW - 2 * HOUR).toISOString(),
  chatId: 'chat-elsewhere',
};
const past: J = { ...boosted, keywords: ['scope: narrow', 'information', 'past'] };
const freshOnly: J = { id: 'f', keywords: [], createdAt: boosted.createdAt, chatId: 'chat-elsewhere' };
const penaltyOnly: J = { id: 'p', keywords: ['moment'], createdAt: '2026-01-01T00:00:00.000Z', chatId: 'chat-elsewhere' };
const window3h = { from: new Date(NOW - 3 * HOUR).toISOString(), to: new Date(NOW).toISOString() };
const GATE = { boostGateAbs: 0.45, boostGateMargin: 0.15, boostGateRamp: 0.1 };
const LEGACY = { boostGateAbs: 0, boostGateMargin: 0, boostCap: 4, multipliers: { freshEvent24h: 1.6, freshEvent48h: 1.35 }, specificAnchors: false };
const OVERRIDE_ALL = {
  boostCap: 10,
  multipliers: {
    scopeNarrowSameProject: 1.25, scopeNarrowCrossProjectDownWeight: 0.25, temporalPast: 0.8, temporalMoment: 0.65,
    contextMatch: 1.15, participantPresent: 1.3, recentlyWhispered: 0.55, temporalPastRetrospective: 1.2,
    temporalMomentRetrospective: 1.05, occurredWithinWindow: 1.35, freshEvent24h: 1.45, freshEvent48h: 1.2,
  },
};

// ---- the seven fns under an overridden table ----
const multTCases: Array<[string, string, J, J]> = [
  ['scope-same', 'scope', { id: 'm', projectId: 'proj-a', keywords: ['scope: narrow'] }, baseCtx],
  ['scope-cross', 'scope', { id: 'm', projectId: 'proj-b', keywords: ['scope: narrow'] }, baseCtx],
  ['temporal-past', 'temporal', { id: 'm', keywords: ['past'] }, baseCtx],
  ['temporal-past-retro', 'temporal', { id: 'm', keywords: ['past'] }, { ...baseCtx, turnRetrospective: true }],
  ['temporal-moment', 'temporal', { id: 'm', keywords: ['moment'] }, baseCtx],
  ['temporal-moment-retro', 'temporal', { id: 'm', keywords: ['moment'] }, { ...baseCtx, turnRetrospective: true }],
  ['window-in', 'window', { id: 'm', keywords: [], createdAt: boosted.createdAt }, { ...baseCtx, occurredWithin: window3h }],
  ['fresh24', 'fresh', freshOnly, baseCtx],
  ['fresh48', 'fresh', { ...freshOnly, createdAt: new Date(NOW - 30 * HOUR).toISOString() }, baseCtx],
  ['context', 'context', { id: 'm', keywords: ['information'] }, baseCtx],
  ['participant', 'participant', { id: 'm', keywords: [], aboutCharacterId: 'char-present' }, baseCtx],
  ['recent', 'recent', { id: 'm', keywords: [] }, { ...baseCtx, recentlyWhisperedIds: ['m'] }],
];
for (const [id, fn, mem, c] of multTCases) {
  const ctx = toCtx(c, OVERRIDE_ALL);
  const memory = mem as never as Parameters<typeof combineRecallMultipliers>[0];
  const t = parseTargetingTags((mem.keywords as string[]) ?? []);
  const m = ctx.tuning!.multipliers;
  const retro = ctx.turnRetrospective === true;
  const r =
    fn === 'scope' ? scopeProjectMultiplier(t, mem.projectId as string, ctx.currentProjectId, ctx.scopePolicy, m)
    : fn === 'temporal' ? temporalMultiplier(t, retro, m)
    : fn === 'window' ? occurredWithinMultiplier(memory, ctx.occurredWithin, m)
    : fn === 'fresh' ? freshEventMultiplier(memory, ctx.nowMs, ctx.currentChatId, m)
    : fn === 'context' ? contextMultiplier(t, ctx.turnContext, m)
    : fn === 'participant' ? participantMultiplier(memory, ctx.presentAboutCharacterIds, m)
    : recentlyWhisperedMultiplier(memory, ctx.recentlyWhisperedIds, retro, m);
  extra.push({ kind: 'multT', id, fn, memory: mem, ctx: c, tuningInput: OVERRIDE_ALL, multiplier: r.multiplier, fired: r.fired, exclude: r.exclude });
}

// ---- combineRecallMultipliers under tuning / relevance ----
const combineTCases: Array<[string, J, J, unknown, { cosine: number; bestCosine: number } | undefined]> = [
  ['defaults-no-relevance-capped', boosted, baseCtx, undefined, undefined],
  ['defaults-below-gate', boosted, baseCtx, undefined, { cosine: 0.33, bestCosine: 0.511 }],
  ['defaults-above-gate-capped', boosted, baseCtx, undefined, { cosine: 0.6, bestCosine: 0.6 }],
  ['defaults-halfway', boosted, baseCtx, undefined, { cosine: 0.4, bestCosine: 0.511 }],
  ['tuned-strip', boosted, baseCtx, GATE, { cosine: 0.33, bestCosine: 0.511 }],
  ['tuned-halfway', boosted, baseCtx, GATE, { cosine: 0.4, bestCosine: 0.511 }],
  ['tuned-tie-125', boosted, baseCtx, GATE, { cosine: 0.3625, bestCosine: 0.5 }],
  ['tuned-tie-625', boosted, baseCtx, GATE, { cosine: 0.4125, bestCosine: 0.5 }],
  ['penalties-in-full', past, baseCtx, GATE, { cosine: 0.2, bestCosine: 0.6 }],
  ['penalty-only-below-gate', penaltyOnly, baseCtx, GATE, { cosine: 0.1, bestCosine: 0.6 }],
  ['cap-1.6', boosted, baseCtx, { boostCap: 1.6 }, undefined],
  ['cap-4-no-label', boosted, baseCtx, { boostCap: 4 }, undefined],
  ['cap-int-2', boosted, baseCtx, { boostCap: 2 }, undefined],
  ['overridden-multipliers', boosted, baseCtx, { boostCap: 4, multipliers: { freshEvent24h: 1.3 } }, undefined],
  ['fresh-bypass', boosted, baseCtx, { ...GATE, freshBypassesGate: true }, { cosine: 0.33, bestCosine: 0.69 }],
  ['both-bypass', boosted, { ...baseCtx, occurredWithin: window3h }, { boostGateAbs: 0.45, boostGateMargin: 0.15, boostCap: 4, freshBypassesGate: true, windowBypassesGate: true }, { cosine: 0.3, bestCosine: 0.69 }],
  ['window-bypass-only', boosted, { ...baseCtx, occurredWithin: window3h }, { ...GATE, boostCap: 4, windowBypassesGate: true }, { cosine: 0.3, bestCosine: 0.69 }],
  ['bypass-still-capped', boosted, baseCtx, { boostGateAbs: 0.45, boostCap: 1.4, freshBypassesGate: true }, { cosine: 0.6, bestCosine: 0.6 }],
  ['fresh-only-under-bypass', freshOnly, { ...baseCtx, turnContext: null }, { boostGateAbs: 0.45, freshBypassesGate: true }, { cosine: 0.2, bestCosine: 0.6 }],
  ['fresh-only-no-bypass', freshOnly, { ...baseCtx, turnContext: null }, { boostGateAbs: 0.45 }, { cosine: 0.2, bestCosine: 0.6 }],
  ['tuning-without-relevance', boosted, baseCtx, GATE, undefined],
  ['margin-only-gate', boosted, baseCtx, { boostGateAbs: 0, boostGateMargin: 0.1, boostGateRamp: 0.2, boostCap: 4 }, { cosine: 0.45, bestCosine: 0.6 }],
  ['hard-step', boosted, baseCtx, { boostGateAbs: 0.45, boostGateMargin: 0, boostGateRamp: 0, boostCap: 4 }, { cosine: 0.449, bestCosine: 0.6 }],
  ['legacy-relevance', boosted, baseCtx, LEGACY, { cosine: 0.1, bestCosine: 0.9 }],
  ['gate-off-capped', boosted, baseCtx, { boostGateAbs: 0, boostGateMargin: 0 }, { cosine: 0.1, bestCosine: 0.9 }],
  ['exclude-under-tuning', { ...boosted, projectId: 'proj-b' }, { ...baseCtx, scopePolicy: 'exclude' }, GATE, { cosine: 0.1, bestCosine: 0.9 }],
  ['override-all-relevance', boosted, baseCtx, { ...OVERRIDE_ALL, ...GATE }, { cosine: 0.42, bestCosine: 0.7 }],
  [
    'jest-maximal-stack',
    { id: 'M1', projectId: 'P1', chatId: 'chat-other', aboutCharacterId: 'C1', keywords: ['past', 'scope: narrow', 'history'], occurredAt: freshAt(3 * FRESH_HOUR) },
    { currentProjectId: 'P1', scopePolicy: 'down-weight', currentChatId: FRESH_CHAT, nowMs: FRESH_NOW, turnRetrospective: true, turnContext: 'history', presentAboutCharacterIds: ['C1'], occurredWithin: { from: freshAt(24 * FRESH_HOUR), to: freshAt(0) } },
    undefined,
    undefined,
  ],
  [
    'jest-maximal-stack-gated',
    { id: 'M1', projectId: 'P1', chatId: 'chat-other', aboutCharacterId: 'C1', keywords: ['past', 'scope: narrow', 'history'], occurredAt: freshAt(3 * FRESH_HOUR) },
    { currentProjectId: 'P1', scopePolicy: 'down-weight', currentChatId: FRESH_CHAT, nowMs: FRESH_NOW, turnRetrospective: true, turnContext: 'history', presentAboutCharacterIds: ['C1'], recentlyWhisperedIds: ['M1'], occurredWithin: { from: freshAt(24 * FRESH_HOUR), to: freshAt(0) } },
    undefined,
    { cosine: 0.41, bestCosine: 0.55 },
  ],
];
for (const [id, mem, c, tuningInput, relevance] of combineTCases) {
  const r = combineRecallMultipliers(mem as never, toCtx(c, tuningInput), relevance);
  const row: Extra = { kind: 'combineT', id, memory: mem, ctx: c, multiplier: r.multiplier, fired: r.fired, exclude: r.exclude };
  if (tuningInput !== undefined) row.tuningInput = tuningInput;
  if (relevance !== undefined) row.relevance = relevance;
  extra.push(row);
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
for (const r of extra) process.stdout.write(JSON.stringify(r) + '\n');
