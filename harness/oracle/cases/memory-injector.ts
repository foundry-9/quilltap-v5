/**
 * Oracle case: memory-injector context formatters (chat-orchestration wave 1.3).
 *
 * Drives the REAL pure functions from v4's lib/chat/context/memory-injector.ts —
 * formatMemoryMetadataTag, formatCurrentSceneState, formatMemoriesForContext,
 * formatInterCharacterMemoriesForContext, formatFrozenMemoryArchive,
 * formatDynamicMemoryHead, formatSummaryForContext — over a fixed deterministic
 * corpus, printing results as NDJSON. The Rust port feeds the SAME corpus and
 * asserts byte-equal output (tier-1 exact).
 *
 * IMPORTANT — imports the actual app code, does not reimplement it. Run from the
 * server checkout so `@/` aliases resolve:
 *
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/memory-injector.ts \
 *     > /private/tmp/.../scratchpad/oracle-memory-injector.ndjson
 *
 * DETERMINISM: the injector functions call `new Date()` internally (scene state
 * `nowIso`, and `now` for formatRelativeAge). We pin the no-arg `Date`
 * constructor to a fixed instant BEFORE importing the module, so every `new
 * Date()` returns FIXED_NOW while `new Date(string)` still parses normally. The
 * memory timestamps are pinned in the corpus, so the relative-age math and all
 * derived weights are reproducible.
 */

// ---- Pin `new Date()` (no-arg) to a fixed instant --------------------------
const FIXED_NOW_ISO = '2026-06-27T12:00:00.000Z';
const FIXED_NOW_MS = Date.parse(FIXED_NOW_ISO);
const RealDate = Date;
// Replace the global Date so no-arg construction is deterministic. Preserve
// every other behaviour (parsing a string/number, Date.now, static methods).
class PinnedDate extends RealDate {
  constructor(...args: unknown[]) {
    if (args.length === 0) {
      super(FIXED_NOW_MS);
    } else {
      // @ts-expect-error forward all args to the real constructor
      super(...args);
    }
  }
  static now(): number {
    return FIXED_NOW_MS;
  }
}
// eslint-disable-next-line @typescript-eslint/no-explicit-any
;(globalThis as any).Date = PinnedDate;

import {
  formatMemoryMetadataTag,
  formatCurrentSceneState,
  formatMemoriesForContext,
  formatInterCharacterMemoriesForContext,
  formatFrozenMemoryArchive,
  formatDynamicMemoryHead,
  formatSummaryForContext,
  type SceneStateEmissionEntry,
} from '@/lib/chat/context/memory-injector';
import type { Memory } from '@/lib/schemas/types';
import type { SceneState } from '@/lib/schemas/chat.types';
import type { SemanticSearchResult } from '@/lib/memory/memory-service';

const rows: unknown[] = [];
function emit(row: unknown) {
  rows.push(row);
}

// ---- Memory factory --------------------------------------------------------
// A full-ish Memory; we set only the fields the formatters read. Cast through
// unknown so we don't have to satisfy the entire interface.
function mem(fields: Record<string, unknown>): Memory {
  return {
    keywords: [],
    importance: 0.5,
    ...fields,
  } as unknown as Memory;
}

// Timestamps in the corpus (ISO) and their epoch-ms (for the Rust side).
function toMs(iso: string | null | undefined): number | null {
  return iso == null ? null : Date.parse(iso);
}

// Serialise a Memory into the wire shape the Rust corpus consumes.
function wireMem(m: Memory): Record<string, unknown> {
  const mm = m as unknown as Record<string, unknown>;
  return {
    id: mm.id ?? '',
    content: mm.content ?? null,
    summary: mm.summary ?? '',
    importance: mm.importance ?? 0.5,
    keywords: mm.keywords ?? [],
    aboutCharacterId: mm.aboutCharacterId ?? null,
    reinforcedImportance: mm.reinforcedImportance ?? null,
    createdAtMs: toMs(mm.createdAt as string | null | undefined) ?? 0,
    lastReinforcedAtMs: toMs(mm.lastReinforcedAt as string | null | undefined),
    lastAccessedAtMs: toMs(mm.lastAccessedAt as string | null | undefined),
    reinforcementCount: mm.reinforcementCount ?? null,
    graphDegree: Array.isArray(mm.relatedMemoryIds) ? mm.relatedMemoryIds.length : 0,
    // Episodic spine (v4 8bf3cb5f): the declared kind, the pre-parsed event
    // clock (null when absent/empty/unparsable — mirrors eventReferenceTimeMs's
    // fallback contract), and the verbatim in-story time.
    kindEpisodic: mm.kind === 'episodic',
    occurredAtMs:
      typeof mm.occurredAt === 'string' && mm.occurredAt !== '' &&
      Number.isFinite(Date.parse(mm.occurredAt as string))
        ? Date.parse(mm.occurredAt as string)
        : null,
    narrativeTime: mm.narrativeTime ?? null,
  };
}

function wireResult(r: SemanticSearchResult): Record<string, unknown> {
  return {
    memory: wireMem(r.memory),
    score: r.score,
    effectiveWeight: r.effectiveWeight ?? null,
    rawWeight: r.rawWeight ?? null,
    recallAdjustment: r.recallAdjustment
      ? {
          multiplier: r.recallAdjustment.multiplier ?? null,
          fired: r.recallAdjustment.fired ?? [],
          blendedBefore: r.recallAdjustment.blendedBefore ?? null,
          blendedAfter: r.recallAdjustment.blendedAfter ?? null,
        }
      : null,
  };
}

// ===========================================================================
// formatMemoryMetadataTag
// ===========================================================================
const tagCases: Array<[string, Parameters<typeof formatMemoryMetadataTag>[0]]> = [
  ['tag-empty', {}],
  ['tag-importance-only', { importance: 0.98 }],
  ['tag-all', { importance: 0.98, relevance: 0.87, weight: 0.92, keywords: ['a', 'b'] }],
  ['tag-rounding', { importance: 0.125, relevance: 0.005, weight: 2.675 }],
  ['tag-neg-round', { importance: -0.004 }],
  ['tag-null-fields', { importance: null, relevance: null, weight: null, keywords: null }],
  ['tag-nan', { importance: NaN, relevance: Infinity, weight: -Infinity }],
  ['tag-keywords-only', { keywords: ['one', 'two', 'three'] }],
  ['tag-keywords-trim', { keywords: ['  spaced  ', '', '   ', 'kept'] }],
  ['tag-keywords-all-blank', { keywords: ['', '   '] }],
  ['tag-adjustments', { adjustments: ['narrow✓', 'past↓'] }],
  ['tag-adjustments-trim', { adjustments: ['  narrow✓ ', '', 'past↓'] }],
  ['tag-adjustments-all-blank', { adjustments: ['', '  '] }],
  ['tag-adj-and-kw', { relevance: 0.5, adjustments: ['wide↑'], keywords: ['k1', 'k2'] }],
  ['tag-astral', { keywords: ['🎩steampunk', 'café'] }],
  ['tag-zero', { importance: 0, relevance: 0, weight: 0 }],
  ['tag-one', { importance: 1, relevance: 1, weight: 1 }],
];
for (const [id, opts] of tagCases) {
  emit({ kind: 'tag', id, opts, out: formatMemoryMetadataTag(opts) });
}

// ===========================================================================
// formatCurrentSceneState
// ===========================================================================
type SceneCharWire = {
  characterId: string;
  characterName: string;
  action: string;
  clothing: string | null;
};
function sceneCase(
  id: string,
  scene: SceneState | null | undefined,
  time: string | null | undefined,
  liveClothing: Array<[string, string]>,
  priorEmission: Array<[string, SceneStateEmissionEntry]>,
) {
  const liveMap = new Map(liveClothing);
  const priorMap = new Map(priorEmission);
  const res = formatCurrentSceneState(scene, time, undefined, liveMap, priorMap);
  emit({
    kind: 'scene',
    id,
    scene: scene
      ? {
          location: scene.location,
          characters: (scene.characters ?? []).map((c) => ({
            characterId: c.characterId,
            characterName: c.characterName,
            action: c.action,
            clothing: c.clothing,
          })) as SceneCharWire[],
        }
      : null,
    time: time ?? null,
    nowIso: FIXED_NOW_ISO,
    liveClothing,
    priorEmission,
    out: {
      content: res.content,
      tokenCount: res.tokenCount,
      emittedByCharacter: Array.from(res.emittedByCharacter.entries()).map(([k, v]) => ({
        characterId: k,
        actionHash: v.actionHash,
        clothingHash: v.clothingHash,
        emittedAt: v.emittedAt,
      })),
    },
  });
}
function sc(
  characterId: string,
  characterName: string,
  action: string,
  clothing: string | null,
): SceneState['characters'][number] {
  return { characterId, characterName, action, clothing, appearance: null } as SceneState['characters'][number];
}
function scene(location: string, characters: SceneState['characters']): SceneState {
  return { location, characters, updatedAt: FIXED_NOW_ISO, updatedAtMessageCount: 0 } as unknown as SceneState;
}

sceneCase('scene-null', null, null, [], []);
sceneCase('scene-empty', scene('', []), null, [], []);
sceneCase(
  'scene-basic',
  scene('The Study', [sc('c1', 'Ada', 'reading a book', 'a tweed jacket')]),
  '3:00 PM',
  [],
  [],
);
sceneCase(
  'scene-no-time',
  scene('The Study', [sc('c1', 'Ada', 'reading', 'tweed')]),
  null,
  [],
  [],
);
sceneCase(
  'scene-blank-time',
  scene('The Study', [sc('c1', 'Ada', 'reading', 'tweed')]),
  '   ',
  [],
  [],
);
sceneCase(
  'scene-trim-time',
  scene('The Study', [sc('c1', 'Ada', 'reading', 'tweed')]),
  '  midnight  ',
  [],
  [],
);
sceneCase(
  'scene-unspecified',
  scene('Parlour', [sc('c1', 'Ada', '   ', null)]),
  null,
  [],
  [],
);
sceneCase(
  'scene-multi',
  scene('The Ballroom', [
    sc('c1', 'Ada', 'dancing', 'a gown'),
    sc('c2', 'Bertie', 'watching', null),
    sc('', 'NoId', 'lurking', 'a cloak'),
    sc('c4', '', 'ignored (no name)', 'invisible'),
  ]),
  'evening',
  [],
  [],
);
sceneCase(
  'scene-live-clothing',
  scene('The Study', [sc('c1', 'Ada', 'reading', 'tweed')]),
  null,
  [['c1', '  a freshly-pressed waistcoat  ']],
  [],
);
sceneCase(
  'scene-live-clothing-blank',
  scene('The Study', [sc('c1', 'Ada', 'reading', 'tweed')]),
  null,
  [['c1', '   ']],
  [],
);
// Unchanged path: prior hashes match current (action+clothing).
{
  const action = 'reading';
  const clothing = 'tweed';
  // Compute the hashes the same way the module does: sha256 hex slice(0,16).
  // We reuse the module by first emitting a full case, then feeding its hashes.
  const first = formatCurrentSceneState(
    scene('The Study', [sc('c1', 'Ada', action, clothing)]),
    null,
    undefined,
    new Map(),
    new Map(),
  );
  const prior = first.emittedByCharacter.get('c1')!;
  // Pin a distinct prior emittedAt so we can prove it's preserved.
  const priorPinned: SceneStateEmissionEntry = { ...prior, emittedAt: '2020-01-01T00:00:00.000Z' };
  sceneCase(
    'scene-unchanged',
    scene('The Study', [sc('c1', 'Ada', action, clothing)]),
    null,
    [],
    [['c1', priorPinned]],
  );
  // Changed action → full section, new emittedAt.
  sceneCase(
    'scene-changed',
    scene('The Study', [sc('c1', 'Ada', 'now writing', clothing)]),
    null,
    [],
    [['c1', priorPinned]],
  );
}
sceneCase(
  'scene-astral',
  scene('🏰 Castle', [sc('c1', 'Zöe', 'casting a 🔮 spell', 'robes of midnight')]),
  '𝟏𝟐 o’clock',
  [],
  [],
);

// ===========================================================================
// Shared memory corpus for the list formatters
// ===========================================================================
function m(
  id: string,
  fields: Record<string, unknown>,
): Memory {
  return mem({ id, createdAt: '2026-06-01T00:00:00.000Z', summary: `sum-${id}`, ...fields });
}
function res(
  m: Memory,
  score: number,
  extra: Partial<SemanticSearchResult> = {},
): SemanticSearchResult {
  return { memory: m, score, usedEmbedding: true, ...extra } as SemanticSearchResult;
}

// ===========================================================================
// formatMemoriesForContext
// ===========================================================================
function memoriesCase(id: string, memories: SemanticSearchResult[], maxTokens: number) {
  const out = formatMemoriesForContext(memories, maxTokens, undefined as never);
  emit({
    kind: 'memories',
    id,
    memories: memories.map(wireResult),
    maxTokens,
    nowMs: FIXED_NOW_MS,
    out: {
      content: out.content,
      tokenCount: out.tokenCount,
      memoriesUsed: out.memoriesUsed,
      debugMemories: out.debugMemories,
    },
  });
}

memoriesCase('mem-empty', [], 1000);
memoriesCase(
  'mem-single',
  [res(m('a', { content: 'A memory about tea.', importance: 0.9, keywords: ['tea'] }), 0.8)],
  1000,
);
memoriesCase(
  'mem-sort-weight',
  [
    res(m('low', { importance: 0.2, createdAt: '2026-06-25T00:00:00.000Z' }), 0.9),
    res(m('high', { importance: 0.95, createdAt: '2026-06-26T00:00:00.000Z' }), 0.5),
  ],
  1000,
);
memoriesCase(
  'mem-tiebreak-score',
  [
    // Same importance & date → weight tie within 0.05 → score tiebreak.
    res(m('s1', { importance: 0.8 }), 0.6),
    res(m('s2', { importance: 0.8 }), 0.9),
  ],
  1000,
);
memoriesCase(
  'mem-content-fallback',
  [res(m('cf', { content: '   ', summary: 'fallback summary' }), 0.7)],
  1000,
);
memoriesCase(
  'mem-effective-weight-override',
  [res(m('ew', { importance: 0.3 }), 0.5, { effectiveWeight: 0.99 })],
  1000,
);
memoriesCase(
  'mem-age-variety',
  [
    res(m('today', { createdAt: '2026-06-27T06:00:00.000Z', importance: 0.9 }), 0.9),
    res(m('yesterday', { createdAt: '2026-06-26T00:00:00.000Z', importance: 0.85 }), 0.9),
    res(m('daysago', { createdAt: '2026-06-23T00:00:00.000Z', importance: 0.8 }), 0.9),
    res(m('lastweek', { createdAt: '2026-06-15T00:00:00.000Z', importance: 0.75 }), 0.9),
    res(m('weeksago', { createdAt: '2026-06-05T00:00:00.000Z', importance: 0.72 }), 0.9),
    res(m('lastmonth', { createdAt: '2026-05-20T00:00:00.000Z', importance: 0.71 }), 0.9),
    res(m('monthsago', { createdAt: '2026-01-01T00:00:00.000Z', importance: 0.7 }), 0.9),
    res(m('yearsago', { createdAt: '2023-01-01T00:00:00.000Z', importance: 0.7 }), 0.9),
  ],
  10000,
);
// Budget cut exactly at threshold: first line fits, second overflows.
memoriesCase(
  'mem-budget-cut',
  [
    res(m('b1', { content: 'x'.repeat(50), importance: 0.9 }), 0.9),
    res(m('b2', { content: 'y'.repeat(200), importance: 0.85 }), 0.9),
  ],
  30,
);
// Budget so small nothing fits → empty.
memoriesCase(
  'mem-budget-none',
  [res(m('n1', { content: 'z'.repeat(200), importance: 0.9 }), 0.9)],
  5,
);
memoriesCase(
  'mem-astral',
  [res(m('ast', { content: '🎩 A steampunk soirée with Zöe.', importance: 0.88, keywords: ['🎩', 'café'] }), 0.77)],
  1000,
);

// Episodic spine (v4 8bf3cb5f): the event-clock age + the narrativeTime rider.
memoriesCase(
  'ctx-episodic-event-clock',
  [
    res(
      mem({
        id: 'ep1', summary: 'Lighthouse visit', content: 'We visited Lighthouse Point.',
        importance: 0.8, createdAt: '2026-06-25T00:00:00.000Z',
        occurredAt: '2026-05-20T00:00:00.000Z', kind: 'episodic',
      }),
      0.9,
    ),
  ],
  2000,
);
memoriesCase(
  'ctx-narrative-time',
  [
    res(
      mem({
        id: 'ep2', summary: 'Night watch', content: 'Stood the night watch together.',
        importance: 0.7, createdAt: '2026-06-25T00:00:00.000Z',
        occurredAt: '2026-06-20T00:00:00.000Z', narrativeTime: 'the third night at sea',
        kind: 'episodic',
      }),
      0.8,
    ),
    res(
      mem({
        id: 'ep3', summary: 'Blank narrative trims away', content: 'Rigging lesson.',
        importance: 0.6, createdAt: '2026-06-25T00:00:00.000Z',
        narrativeTime: '   ',
      }),
      0.7,
    ),
  ],
  2000,
);

// ===========================================================================
// formatInterCharacterMemoriesForContext
// ===========================================================================
function interCase(
  id: string,
  importanceMemories: Memory[],
  names: Array<[string, string]>,
  maxTokens: number,
  relevanceResults: SemanticSearchResult[],
) {
  const nameMap = new Map(names);
  const out = formatInterCharacterMemoriesForContext(
    importanceMemories,
    nameMap,
    maxTokens,
    undefined as never,
    relevanceResults,
  );
  emit({
    kind: 'inter',
    id,
    importanceMemories: importanceMemories.map(wireMem),
    names,
    maxTokens,
    relevanceResults: relevanceResults.map(wireResult),
    nowMs: FIXED_NOW_MS,
    out: {
      content: out.content,
      tokenCount: out.tokenCount,
      memoriesUsed: out.memoriesUsed,
      debugMemories: out.debugMemories,
    },
  });
}

interCase('inter-empty', [], [], 1000, []);
interCase(
  'inter-importance-only',
  [
    m('i1', { aboutCharacterId: 'cA', content: 'A is brave.', importance: 0.9 }),
    m('i2', { aboutCharacterId: 'cA', content: 'A likes tea.', importance: 0.7 }),
  ],
  [['cA', 'Ada']],
  2000,
  [],
);
interCase(
  'inter-relevance-only',
  [],
  [['cA', 'Ada']],
  2000,
  [res(m('r1', { aboutCharacterId: 'cA', content: 'A dueled.', importance: 0.8 }), 0.85)],
);
interCase(
  'inter-merged-dedup',
  [
    // Same id present in both halves → dedup keeps relevance copy.
    m('shared', { aboutCharacterId: 'cA', content: 'Shared memory.', importance: 0.8 }),
    m('imp-only', { aboutCharacterId: 'cA', content: 'Importance-only.', importance: 0.75 }),
  ],
  [['cA', 'Ada']],
  2000,
  [
    res(m('shared', { aboutCharacterId: 'cA', content: 'Shared memory.', importance: 0.8 }), 0.9),
    res(m('rel-only', { aboutCharacterId: 'cA', content: 'Relevance-only.', importance: 0.7 }), 0.6),
  ],
);
interCase(
  'inter-multi-char',
  [
    m('a1', { aboutCharacterId: 'cA', content: 'About A.', importance: 0.8 }),
    m('b1', { aboutCharacterId: 'cB', content: 'About B.', importance: 0.9 }),
    m('noabout', { content: 'No aboutCharacterId — skipped.', importance: 0.9 }),
  ],
  [['cA', 'Ada'], ['cB', 'Bertie']],
  4000,
  [res(m('b2', { aboutCharacterId: 'cB', content: 'B relevant.', importance: 0.85 }), 0.88)],
);
interCase(
  'inter-unknown-name',
  [m('u1', { aboutCharacterId: 'cZ', content: 'Mystery character.', importance: 0.8 })],
  [],
  2000,
  [],
);
interCase(
  'inter-budget-cut',
  [
    m('bc1', { aboutCharacterId: 'cA', content: 'p'.repeat(40), importance: 0.9 }),
    m('bc2', { aboutCharacterId: 'cA', content: 'q'.repeat(200), importance: 0.85 }),
  ],
  [['cA', 'Ada']],
  35,
  [],
);
interCase(
  'inter-effweight-override',
  [],
  [['cA', 'Ada']],
  2000,
  [res(m('ov', { aboutCharacterId: 'cA', importance: 0.3 }), 0.5, { effectiveWeight: 0.88 })],
);

// ===========================================================================
// formatFrozenMemoryArchive
// ===========================================================================
function frozenCase(id: string, memories: Memory[], maxTokens: number) {
  const out = formatFrozenMemoryArchive(memories, maxTokens, undefined as never);
  emit({
    kind: 'frozen',
    id,
    memories: memories.map(wireMem),
    maxTokens,
    out: {
      content: out.content,
      tokenCount: out.tokenCount,
      memoriesUsed: out.memoriesUsed,
      debugMemories: out.debugMemories,
    },
  });
}

frozenCase('frozen-empty', [], 1000);
frozenCase(
  'frozen-basic',
  [
    m('f1', { summary: 'The pact was sealed.', importance: 0.95, keywords: ['pact'] }),
    m('f2', { summary: 'A rivalry began.', importance: 0.8 }),
  ],
  1000,
);
frozenCase(
  'frozen-summary-fallback',
  [m('f3', { summary: '   ', content: 'content fallback here', importance: 0.7 })],
  1000,
);
frozenCase(
  'frozen-skip-empty',
  [
    m('f4', { summary: '   ', content: '   ', importance: 0.7 }),
    m('f5', { summary: 'kept', importance: 0.6 }),
  ],
  1000,
);
frozenCase(
  'frozen-budget-cut',
  [
    m('fb1', { summary: 'a'.repeat(40), importance: 0.9 }),
    m('fb2', { summary: 'b'.repeat(200), importance: 0.8 }),
  ],
  25,
);
frozenCase(
  'frozen-order-verbatim',
  [
    m('z', { summary: 'Z first (no re-sort).', importance: 0.1 }),
    m('a', { summary: 'A second.', importance: 0.99 }),
  ],
  1000,
);

// ===========================================================================
// formatDynamicMemoryHead
// ===========================================================================
function headCase(
  id: string,
  memories: SemanticSearchResult[],
  options: { maxTokens?: number; maxEntries?: number },
) {
  const out = formatDynamicMemoryHead(memories, undefined as never, options);
  emit({
    kind: 'head',
    id,
    memories: memories.map(wireResult),
    options: { maxTokens: options.maxTokens ?? null, maxEntries: options.maxEntries ?? null },
    nowMs: FIXED_NOW_MS,
    out: {
      content: out.content,
      tokenCount: out.tokenCount,
      memoriesUsed: out.memoriesUsed,
      debugMemories: out.debugMemories,
    },
  });
}

headCase('head-empty', [], {});
headCase(
  'head-basic',
  [
    res(m('h1abcd', { summary: 'First anchor.', importance: 0.9, keywords: ['k'] }), 0.8),
    res(m('h2efgh', { summary: 'Second anchor.', importance: 0.85 }), 0.7),
  ],
  {},
);
headCase(
  'head-maxentries',
  [
    res(m('e1xxxx', { summary: 'e1', importance: 0.9 }), 0.9),
    res(m('e2xxxx', { summary: 'e2', importance: 0.85 }), 0.9),
    res(m('e3xxxx', { summary: 'e3', importance: 0.8 }), 0.9),
  ],
  { maxEntries: 2 },
);
headCase(
  'head-recall-sort',
  [
    // With recallAdjustment.blendedAfter present on both → sort by it.
    res(m('ra1', { summary: 'lower blend', importance: 0.9 }), 0.9, {
      recallAdjustment: { multiplier: 1.0, fired: ['narrow✓'], blendedBefore: 0.5, blendedAfter: 0.3 },
    }),
    res(m('ra2', { summary: 'higher blend', importance: 0.5 }), 0.5, {
      recallAdjustment: { multiplier: 1.2, fired: ['wide↑', 'past↓'], blendedBefore: 0.6, blendedAfter: 0.9 },
    }),
  ],
  {},
);
headCase(
  'head-summary-fallback',
  [res(m('sfxxxx', { summary: '   ', content: 'content used', importance: 0.8 }), 0.6)],
  {},
);
headCase(
  'head-skip-empty',
  [
    res(m('skip1x', { summary: '   ', content: '   ', importance: 0.8 }), 0.6),
    res(m('keep1x', { summary: 'kept', importance: 0.7 }), 0.5),
  ],
  {},
);
headCase(
  'head-budget-cut',
  [
    res(m('bc1xxx', { summary: 'a'.repeat(30), importance: 0.9 }), 0.9),
    res(m('bc2xxx', { summary: 'b'.repeat(400), importance: 0.85 }), 0.9),
  ],
  { maxTokens: 20 },
);
headCase(
  'head-short-id',
  [res(m('ab', { summary: 'short id', importance: 0.8 }), 0.6)],
  {},
);
headCase(
  'head-raw-weight',
  [res(m('rwxxxx', { summary: 'raw', importance: 0.8 }), 0.6, { rawWeight: 0.42 })],
  {},
);

// Episodic spine (v4 8bf3cb5f): every head entry is dated; narrativeTime rides.
headCase(
  'head-episodic-dated',
  [
    res(
      mem({
        id: 'hd1aaaa', summary: 'Dated head entry.', importance: 0.9,
        createdAt: '2026-06-25T00:00:00.000Z',
        occurredAt: '2026-05-20T00:00:00.000Z', kind: 'episodic',
      }),
      0.9,
    ),
    res(
      mem({
        id: 'hd2bbbb', summary: 'Narrative head entry.', importance: 0.8,
        createdAt: '2026-06-20T00:00:00.000Z',
        occurredAt: '2026-06-24T00:00:00.000Z',
        narrativeTime: 'midwinter', kind: 'episodic',
      }),
      0.8,
    ),
    res(
      mem({
        id: 'hd3cccc', summary: 'Semantic write-clock entry.', importance: 0.7,
        createdAt: '2026-06-26T00:00:00.000Z',
      }),
      0.7,
    ),
  ],
  {},
);

// ===========================================================================
// formatSummaryForContext
// ===========================================================================
function summaryCase(id: string, summary: string, maxTokens: number) {
  const out = formatSummaryForContext(summary, maxTokens, undefined as never);
  emit({ kind: 'summary', id, summary, maxTokens, out });
}

summaryCase('sum-empty', '', 1000);
summaryCase('sum-blank', '   ', 1000);
summaryCase('sum-fits', 'A short recap of the tale so far.', 1000);
summaryCase('sum-truncate', 'word '.repeat(200).trim(), 30);
summaryCase('sum-truncate-tiny', 'word '.repeat(200).trim(), 12);
summaryCase('sum-astral', '🎩 A soirée with Zöe and a café.', 1000);

// ---- emit ------------------------------------------------------------------
for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
