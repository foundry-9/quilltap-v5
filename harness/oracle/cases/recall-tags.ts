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

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
