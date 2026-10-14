/**
 * Tier-1 ORACLE for v4's ONE COMBINE (P4.D289 unit 7, pin `83334b9a5`) —
 * `foldInto` + `FOLD_CAPS` from `lib/memory/admission/fold.ts` (`8af764514`,
 * Phase C of the memory refactor; bugs 204 / 226 from `c2795733f`).
 *
 * Drives v4's REAL module, imported by FILE path (the `admission/index.ts`
 * barrel pulls `admit.ts` → repositories). Rows (`kind: 'fold'`): the nine
 * `foldInto` rows of `__tests__/unit/lib/memory/admission-fold.test.ts`
 * (`applyFold`'s seven are P4.D290's), then the growing-content judge over
 * three sources, the footnote cap with and without an anchor change,
 * entities case-insensitive survivor-first ≤ 12 and grown past the anchor
 * line's four (O13 — re-embeds, faithfully), blank / non-string entities,
 * `narrativeTime` fill (falsy survivor, blank source), `latest` over
 * `lastReinforcedAt ?? createdAt` incl. a non-ISO lexicographic case, an
 * empty `lastReinforcedAt` that does NOT fall back (`??`), astral vs BMP, an
 * identical source → count only, fractional / zero / negative / NaN /
 * infinite / null / string `reinforcementCount`, no sources, the link union,
 * `occurredAt` edges, custom caps. Plus the `FOLD_CAPS` constant.
 *
 * The plan's `patch` is emitted as v4 builds it — `patchJson` (its
 * `JSON.stringify` BYTES, so key order is part of the comparand) and
 * `patchKeys` (`Object.keys`). Non-finite numbers ride as strings (`"NaN"`,
 * `"Infinity"`), in sources and in `reinforcementDelta` alike.
 *
 * Run from inside a v4 checkout (or the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/admission-fold-plan.ts" > /tmp/oracle-admission-fold-plan.ndjson
 */

import { foldInto, FOLD_CAPS, type FoldCaps, type FoldSource } from '@/lib/memory/admission/fold';
import { MAX_REINFORCEMENT_FOOTNOTES } from '@/lib/memory/reinforcement-footnotes';

const rows: unknown[] = [];
const enc = (_k: string, v: unknown) => (typeof v === 'number' && !Number.isFinite(v) ? String(v) : v);

rows.push({ kind: 'constant', id: 'fold-caps', out: FOLD_CAPS });

const CAPS: FoldCaps = { footnoteCap: MAX_REINFORCEMENT_FOOTNOTES, entityCap: 12 };
const footnotes = (n: number) => Array.from({ length: n }, (_, i) => `[+] Detail${i}`).join('\n');

// `admission-fold.test.ts`'s `makeMemory`.
const makeMemory = (overrides: Record<string, unknown> = {}): any => ({
  id: 'keep',
  characterId: 'char-1',
  content: 'Friday keeps the ledger.',
  summary: 'Friday keeps the ledger.',
  keywords: [],
  tags: [],
  importance: 0.6,
  source: 'AUTO',
  reinforcementCount: 1,
  lastReinforcedAt: null,
  relatedMemoryIds: [],
  reinforcedImportance: 0.6,
  entities: [],
  occurredAt: null,
  narrativeTime: null,
  tier: 'hot',
  createdAt: '2026-01-01T00:00:00.000Z',
  updatedAt: '2026-01-01T00:00:00.000Z',
  ...overrides,
});

type FoldCase = [string, any, FoldSource[], FoldCaps];
const cases: FoldCase[] = [
  // ---- admission-fold.test.ts — foldInto (9) ----
  [
    'jest-folds-details-counts-links-earliest',
    makeMemory({ reinforcementCount: 2, relatedMemoryIds: ['n1', 'loser'], occurredAt: '2026-03-10T00:00:00.000Z' }),
    [
      makeMemory({
        id: 'loser',
        content: 'Friday keeps the ledger for Marguerite.',
        reinforcementCount: 3,
        relatedMemoryIds: ['keep', 'n1', 'n2'],
        occurredAt: '2026-03-08T00:00:00.000Z',
      }),
    ],
    CAPS,
  ],
  [
    'jest-growing-content-judge',
    makeMemory(),
    [{ content: 'Seen with Marguerite.' }, { content: 'Again with Marguerite and Valparaiso.' }],
    CAPS,
  ],
  [
    'jest-up-to-the-cap',
    makeMemory({ content: `Body\n${footnotes(MAX_REINFORCEMENT_FOOTNOTES - 2)}` }),
    [{ content: 'Body with Alpha, Beta and Gamma.' }],
    CAPS,
  ],
  [
    'jest-past-cap-entities-reembed',
    makeMemory({ content: `Friday keeps the ledger.\n${footnotes(MAX_REINFORCEMENT_FOOTNOTES)}`, entities: ['Friday'] }),
    [{ content: 'Friday keeps the ledger with Marguerite in Valparaiso.', entities: ['Marguerite'] }],
    CAPS,
  ],
  [
    'jest-past-cap-no-anchor-change',
    makeMemory({ content: `Body\n${footnotes(MAX_REINFORCEMENT_FOOTNOTES)}` }),
    [{ content: 'Body with Marguerite.' }],
    CAPS,
  ],
  [
    'jest-entities-case-insensitive-cap',
    makeMemory({ entities: ['Friday', 'Lighthouse'] }),
    [{ content: 'x', entities: ['friday', ...Array.from({ length: 20 }, (_, i) => `Place${i}`)] }],
    CAPS,
  ],
  ['jest-fills-narrative-time', makeMemory(), [{ content: 'x', narrativeTime: 'the third bell' }], CAPS],
  ['jest-never-overwrites-narrative-time', makeMemory({ narrativeTime: 'dawn' }), [{ content: 'x', narrativeTime: 'dusk' }], CAPS],
  [
    'jest-latest-observation-from-creation',
    makeMemory({ lastReinforcedAt: '2026-02-01T00:00:00.000Z' }),
    [makeMemory({ id: 'l', createdAt: '2026-05-01T00:00:00.000Z' })],
    CAPS,
  ],
  ['jest-identical-source-count-only', makeMemory({ content: 'Same words.' }), [{ content: 'Same words.' }], CAPS],

  // ---- the growing-content judge ----
  [
    'growing-three-sources-overlap',
    makeMemory(),
    [
      { content: 'With Marguerite in Valparaiso.' },
      { content: 'Valparaiso again, and Tessarium.' },
      { content: 'Marguerite, Tessarium and Odessa.' },
    ],
    CAPS,
  ],
  [
    'growing-detail-already-in-survivor',
    makeMemory({ content: 'Friday keeps the ledger.\n[+] Marguerite' }),
    [{ content: 'Seen with Marguerite and Odessa.' }],
    CAPS,
  ],
  // ---- the footnote cap with and without an anchor change ----
  [
    'cap-full-occurred-at-change-reembeds',
    makeMemory({ content: `Body\n${footnotes(8)}`, occurredAt: '2026-05-02T00:00:00.000Z' }),
    [{ content: 'Body with Marguerite.', occurredAt: '2026-05-01T00:00:00.000Z' }],
    CAPS,
  ],
  [
    'cap-full-narrative-fill-reembeds',
    makeMemory({ content: `Body\n${footnotes(8)}` }),
    [{ content: 'Body with Marguerite.', narrativeTime: 'second bell' }],
    CAPS,
  ],
  [
    'cap-full-later-occurred-at-no-change',
    makeMemory({ content: `Body\n${footnotes(8)}`, occurredAt: '2026-05-01T00:00:00.000Z' }),
    [{ content: 'Body.', occurredAt: '2026-05-09T00:00:00.000Z' }],
    CAPS,
  ],
  [
    'cap-over-full',
    makeMemory({ content: `Body\n${footnotes(10)}` }),
    [{ content: 'Body with Marguerite.' }],
    CAPS,
  ],
  [
    'cap-one-room-two-sources',
    makeMemory({ content: `Body\n${footnotes(7)}` }),
    [{ content: 'With Marguerite.' }, { content: 'With Odessa.' }],
    CAPS,
  ],
  // ---- entities ----
  [
    'entities-grown-past-four-reembeds',
    makeMemory({ entities: ['Alpha', 'Beta', 'Gamma', 'Delta'] }),
    [{ content: 'Friday keeps the ledger.', entities: ['Epsilon'] }],
    CAPS,
  ],
  [
    'entities-blank-and-non-string-skipped',
    makeMemory({ entities: ['Friday'] }),
    [{ content: 'x', entities: ['', '   ', 5 as any, null as any, 'Odessa', ' ', 'odessa', 'FRIDAY'] }],
    CAPS,
  ],
  [
    'entities-survivor-at-cap',
    makeMemory({ entities: Array.from({ length: 12 }, (_, i) => `Keep${i}`) }),
    [{ content: 'x', entities: ['New'] }],
    CAPS,
  ],
  [
    'entities-across-sources-first-spelling-wins',
    makeMemory({ entities: null }),
    [{ content: 'x', entities: ['Place'] }, { content: 'y', entities: ['PLACE', 'Harbour'] }, { content: 'z', entities: null }],
    CAPS,
  ],
  [
    'entities-cap-three',
    makeMemory({ entities: ['A1'] }),
    [{ content: 'x', entities: ['B1', 'C1', 'D1', 'E1'] }],
    { footnoteCap: 8, entityCap: 3 },
  ],
  [
    'entities-no-growth-no-patch',
    makeMemory({ entities: ['Friday'] }),
    [{ content: 'x', entities: ['friday', 'FRIDAY'] }],
    CAPS,
  ],
  // ---- narrativeTime ----
  ['narrative-survivor-empty-string-filled', makeMemory({ narrativeTime: '' }), [{ content: 'x', narrativeTime: 'noon' }], CAPS],
  [
    'narrative-blank-source-skipped',
    makeMemory(),
    [{ content: 'x', narrativeTime: '   ' }, { content: 'y', narrativeTime: null }, { content: 'z', narrativeTime: 'dusk' }],
    CAPS,
  ],
  ['narrative-none-anywhere', makeMemory(), [{ content: 'x' }], CAPS],
  // ---- latest observation ----
  [
    'latest-non-iso-lexicographic',
    makeMemory({ lastReinforcedAt: '2026-09-01T00:00:00.000Z' }),
    [{ content: 'x', createdAt: '2026-1' }],
    CAPS,
  ],
  [
    'latest-empty-last-reinforced-no-fallback',
    makeMemory({ lastReinforcedAt: '2026-02-01T00:00:00.000Z' }),
    [{ content: 'x', lastReinforcedAt: '', createdAt: '2026-12-01T00:00:00.000Z' }],
    CAPS,
  ],
  [
    'latest-null-last-reinforced-falls-back',
    makeMemory({ lastReinforcedAt: '2026-02-01T00:00:00.000Z' }),
    [{ content: 'x', lastReinforcedAt: null, createdAt: '2026-12-01T00:00:00.000Z' }],
    CAPS,
  ],
  ['latest-astral-vs-bmp', makeMemory({ lastReinforcedAt: '�' }), [{ content: 'x', lastReinforcedAt: '\u{1F600}' }], CAPS],
  ['latest-all-absent', makeMemory(), [{ content: 'x' }], CAPS],
  [
    'latest-z-vs-offset',
    makeMemory({ lastReinforcedAt: '2026-09-01T00:00:00Z' }),
    [{ content: 'x', lastReinforcedAt: '2026-09-01T00:00:00+00:00' }],
    CAPS,
  ],
  // ---- counts (the delta) ----
  ['count-fractional', makeMemory(), [{ content: 'x', reinforcementCount: 2.5 }], CAPS],
  ['count-zero-and-negative', makeMemory(), [{ content: 'x', reinforcementCount: 0 }, { content: 'y', reinforcementCount: -3 }], CAPS],
  ['count-nan', makeMemory(), [{ content: 'x', reinforcementCount: 2 }, { content: 'y', reinforcementCount: NaN }], CAPS],
  ['count-infinity', makeMemory(), [{ content: 'x', reinforcementCount: Infinity }], CAPS],
  ['count-null-and-absent', makeMemory(), [{ content: 'x', reinforcementCount: null }, { content: 'y' }], CAPS],
  ['count-numeric-string', makeMemory(), [{ content: 'x', reinforcementCount: '3' as any }], CAPS],
  ['count-half', makeMemory(), [{ content: 'x', reinforcementCount: 0.5 }, { content: 'y', reinforcementCount: 1.25 }], CAPS],
  ['no-sources', makeMemory({ lastReinforcedAt: '2026-03-01T00:00:00.000Z' }), [], CAPS],
  // ---- links ----
  [
    'links-union-minus-group-and-existing',
    makeMemory({ relatedMemoryIds: ['x', 'l1'] }),
    [
      { id: 'l1', content: 'a', relatedMemoryIds: ['keep', 'y', 'x', 'l2'] },
      { id: 'l2', content: 'b', relatedMemoryIds: ['z', 'l1', 'y', 'w'] },
    ],
    CAPS,
  ],
  [
    'links-survivor-null-source-empty-id',
    makeMemory({ relatedMemoryIds: null }),
    [{ id: '', content: 'a', relatedMemoryIds: ['', 'p'] }, { content: 'b', relatedMemoryIds: null }],
    CAPS,
  ],
  [
    'links-never-removes',
    makeMemory({ relatedMemoryIds: ['gone', 'l1', 'keep'] }),
    [{ id: 'l1', content: 'a', relatedMemoryIds: [] }],
    CAPS,
  ],
  // ---- occurredAt ----
  [
    'occurred-unparseable-survivor',
    makeMemory({ occurredAt: 'last spring' }),
    [{ content: 'x', occurredAt: '2026-05-01T00:00:00.000Z' }],
    CAPS,
  ],
  [
    'occurred-tie-survivor-first',
    makeMemory({ occurredAt: '2026-05-01T02:00:00+02:00' }),
    [{ content: 'x', occurredAt: '2026-05-01T00:00:00.000Z' }],
    CAPS,
  ],
  [
    'occurred-date-only-earlier',
    makeMemory({ occurredAt: '2026-05-02T00:00:00.000Z' }),
    [{ content: 'x', occurredAt: '2026-05-01' }],
    CAPS,
  ],
  [
    'occurred-empty-and-garbage-sources',
    makeMemory(),
    [{ content: 'x', occurredAt: '' }, { content: 'y', occurredAt: 'nope' }, { content: 'z', occurredAt: '2026-04-01T00:00:00.000Z' }],
    CAPS,
  ],
  // ---- custom caps ----
  [
    'caps-footnote-two',
    makeMemory({ content: 'Body\n[+] One' }),
    [{ content: 'With Marguerite, Odessa and Tessarium.' }],
    { footnoteCap: 2, entityCap: 12 },
  ],
  [
    'caps-footnote-zero',
    makeMemory({ content: 'Body' }),
    [{ content: 'With Marguerite.' }],
    { footnoteCap: 0, entityCap: 12 },
  ],
  // ---- everything at once ----
  [
    'all-fields',
    makeMemory({
      content: `Mara sails.\n${footnotes(6)}`,
      reinforcementCount: 2,
      relatedMemoryIds: ['x'],
      occurredAt: '2026-05-03T00:00:00.000Z',
      lastReinforcedAt: '2026-05-03T00:00:00.000Z',
      entities: ['Mara'],
    }),
    [
      {
        id: 'l1',
        content: 'Mara reached Lisbon and Porto and Faro.',
        reinforcementCount: 3,
        relatedMemoryIds: ['y', 'keep'],
        occurredAt: '2026-05-01T00:00:00.000Z',
        narrativeTime: 'the fourth watch',
        entities: ['Lisbon', 'mara', 'Porto'],
        lastReinforcedAt: '2026-05-04T00:00:00.000Z',
      },
    ],
    CAPS,
  ],
];

for (const [id, survivor, sources, caps] of cases) {
  const plan = foldInto(survivor, sources, caps);
  rows.push({
    kind: 'fold',
    id,
    survivor,
    sources,
    caps,
    out: {
      survivorId: plan.survivorId,
      footnotes: plan.footnotes,
      content: plan.content,
      addLinks: plan.addLinks,
      patchJson: JSON.stringify(plan.patch),
      patchKeys: Object.keys(plan.patch),
      reinforcementDelta: plan.reinforcementDelta,
      reinforcedAt: plan.reinforcedAt,
      atCap: plan.atCap,
      reembed: plan.reembed,
    },
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r, enc) + '\n');
