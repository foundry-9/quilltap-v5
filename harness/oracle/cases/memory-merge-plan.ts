/**
 * Oracle case (P4.D266 U1; RE-AIMED at P4.D289, pin `83334b9a5`): the merge
 * corpus through v4's ONE COMBINE + the footnote cap.
 *
 * v4 `8af764514` (memory refactor Phase C) DELETED `lib/memory/memory-merge.ts`
 * (`planMemoryMerge`) and moved the footnote helpers out of
 * `lib/memory/memory-gate.ts` (deleted). The same corpus now drives v4's REAL:
 *   foldInto, FOLD_CAPS                               (lib/memory/admission/fold.ts)
 *   appendCappedFootnotes, countReinforcementFootnotes,
 *   MAX_REINFORCEMENT_FOOTNOTES                       (lib/memory/reinforcement-footnotes.ts)
 * with each loser passed as a whole `Memory` row (the maintenance fold's shape,
 * `fold-near-duplicates.ts:183`). The plan rows' expectation is now v4's fold
 * result — the rows that measured the deleted merge's WRITE semantics (an
 * absolute `reinforcementCount` + recomputed `reinforcedImportance` in the
 * patch, the whole-array `relatedMemoryIds` rewrite, `lastReinforcedAt` in the
 * patch, `contentChanged`) are retired BY MEASUREMENT: the fold carries a
 * count DELTA, an `addLinks` delta that never removes, `reinforcedAt`, and
 * `reembed` instead. The `links-excluded` row (the merge's `exclude` batch) is
 * dropped: a fold has no exclude set — scrubbing links to the rows retired in
 * the same pass is `retireMemories`' job in v4's maintenance (P4.D297).
 *
 * The plan's `patch` is emitted as v4 builds it (`patchJson` — its
 * `JSON.stringify` BYTES — and `patchKeys`), so key order is part of the
 * comparand.
 *
 * Run from inside a v4 checkout (or the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/memory-merge-plan.ts" > /tmp/oracle-memory-merge-plan.ndjson
 */

import { foldInto, FOLD_CAPS } from '@/lib/memory/admission/fold';
import {
  appendCappedFootnotes,
  countReinforcementFootnotes,
  MAX_REINFORCEMENT_FOOTNOTES,
} from '@/lib/memory/reinforcement-footnotes';
import type { Memory } from '@/lib/schemas/types';

const rows: unknown[] = [];

rows.push({ kind: 'constant', id: 'max-footnotes', out: MAX_REINFORCEMENT_FOOTNOTES });

// ---- footnote helpers -----------------------------------------------------
const footnotes = (n: number): string =>
  Array.from({ length: n }, (_, i) => `\n[+] Detail${i}`).join('');
const DETAILS: Record<string, string[]> = {
  none: [],
  one: ['Lisbon'],
  three: ['Lisbon', 'Porto', 'Faro'],
};
for (const existing of [0, 7, 8, 9]) {
  for (const [dname, details] of Object.entries(DETAILS)) {
    const content = `Mara sails.${footnotes(existing)}`;
    rows.push({
      kind: 'append',
      id: `footnotes-${existing}-details-${dname}`,
      content,
      details,
      count: countReinforcementFootnotes(content),
      out: appendCappedFootnotes(content, details),
    });
  }
}
const oddContents: Array<[string, string]> = [
  ['crlf', 'Mara sails.\r\n[+] One\r\n[+] Two\r\n'],
  ['no-space', 'Mara sails.\n[+]x\n[+] y'],
  ['leading-footnote', '[+] first\nbody'],
  ['empty', ''],
  ['indented', 'body\n  [+] indented'],
];
for (const [id, content] of oddContents) {
  rows.push({
    kind: 'append',
    id: `odd-${id}`,
    content,
    details: ['Lisbon'],
    count: countReinforcementFootnotes(content),
    out: appendCappedFootnotes(content, ['Lisbon']),
  });
}

// ---- foldInto over the merge corpus ----------------------------------------
// The fold reads id / content / occurredAt / narrativeTime / entities /
// relatedMemoryIds / lastReinforcedAt (survivor) and, per loser, those plus
// reinforcementCount / createdAt; the casts keep the rows minimal.
const m = (o: Record<string, unknown>): Memory => o as unknown as Memory;
const base = (id: string, extra: Record<string, unknown> = {}): Memory =>
  m({ id, content: 'Mara sails the harbour.', importance: 0.5, ...extra });

const plans: Array<[string, Memory, Memory[]]> = [
  ['no-losers', base('s'), []],
  ['identical-loser', base('s'), [base('l1')]],
  [
    'novel-details',
    base('s'),
    [m({ id: 'l1', content: 'Mara sails to Lisbon with Joao.', importance: 0.9 })],
  ],
  [
    'shared-detail-appended-once',
    base('s'),
    [
      m({ id: 'l1', content: 'Mara met Lisbon traders.' }),
      m({ id: 'l2', content: 'Lisbon was loud; Porto quiet.' }),
    ],
  ],
  [
    'loser-details-all-exist',
    base('s', { content: 'Mara sails to Lisbon.' }),
    [m({ id: 'l1', content: 'The Lisbon run again.' })],
  ],
  [
    'cap-seven-plus-two-losers',
    base('s', { content: `Mara sails.${footnotes(7)}` }),
    [m({ id: 'l1', content: 'She met Joao and Ines.' }), m({ id: 'l2', content: 'Then Porto and Faro.' })],
  ],
  [
    'cap-already-full',
    base('s', { content: `Mara sails.${footnotes(8)}` }),
    [m({ id: 'l1', content: 'She met Joao.' })],
  ],
  // ---- counts: absent / null / 0 / 3 ----
  ['count-absent-absent', base('s'), [base('l1', { content: 'x' })]],
  ['count-null', base('s', { reinforcementCount: null }), [base('l1', { reinforcementCount: null })]],
  ['count-zero', base('s', { reinforcementCount: 0 }), [base('l1', { reinforcementCount: 0 })]],
  ['count-zero-survivor', base('s', { reinforcementCount: 0 }), [base('l1', { reinforcementCount: 3 })]],
  ['count-sum', base('s', { reinforcementCount: 2 }), [base('l1', { reinforcementCount: 3 }), base('l2')]],
  ['count-importance-cap', base('s', { reinforcementCount: 40, importance: 0.95 }), [base('l1', { reinforcementCount: 40 })]],
  // ---- links ----
  [
    'links-union-minus-group',
    base('s', { relatedMemoryIds: ['x', 'l1'] }),
    [base('l1', { relatedMemoryIds: ['s', 'y', 'x'] }), base('l2', { relatedMemoryIds: ['z', 'l1'] })],
  ],
  ['links-survivor-duplicates', base('s', { relatedMemoryIds: ['x', 'x'] }), [base('l1')]],
  ['links-order-only', base('s', { relatedMemoryIds: ['b', 'a'] }), [base('l1', { relatedMemoryIds: ['a'] })]],
  ['links-loser-to-survivor', base('s'), [base('l1', { relatedMemoryIds: ['s'] })]],
  ['links-null-arrays', base('s', { relatedMemoryIds: null }), [base('l1', { relatedMemoryIds: null })]],
  // ---- occurredAt ----
  [
    'occurred-earliest-loser',
    base('s', { occurredAt: '2026-05-02T00:00:00.000Z' }),
    [base('l1', { occurredAt: '2026-05-01T00:00:00.000Z' })],
  ],
  ['occurred-survivor-null', base('s', { occurredAt: null }), [base('l1', { occurredAt: '2026-05-01T00:00:00.000Z' })]],
  ['occurred-unparseable', base('s', { occurredAt: 'not a date' }), [base('l1', { occurredAt: 'garbage' })]],
  ['occurred-unparseable-vs-real', base('s', { occurredAt: 'not a date' }), [base('l1', { occurredAt: '2026-05-01T00:00:00.000Z' })]],
  ['occurred-empty', base('s', { occurredAt: '' }), [base('l1', { occurredAt: '' })]],
  ['occurred-date-only', base('s', { occurredAt: '2026-05-02T00:00:00.000Z' }), [base('l1', { occurredAt: '2026-05-01' })]],
  [
    'occurred-offset',
    base('s', { occurredAt: '2026-05-01T00:30:00.000Z' }),
    [base('l1', { occurredAt: '2026-05-01T01:00:00+02:00' })],
  ],
  [
    'occurred-tie-first-wins',
    base('s', { occurredAt: '2026-05-01T02:00:00+02:00' }),
    [base('l1', { occurredAt: '2026-05-01T00:00:00.000Z' })],
  ],
  [
    'occurred-tie-loser-first',
    base('s', { occurredAt: '2026-06-01T00:00:00.000Z' }),
    [base('l1', { occurredAt: '2026-05-01T00:00:00Z' }), base('l2', { occurredAt: '2026-05-01T00:00:00.000Z' })],
  ],
  // ---- lastReinforcedAt: the string max ----
  [
    'lra-string-vs-date',
    base('s', { lastReinforcedAt: '2026-09-01T00:00:00.000Z' }),
    [base('l1', { lastReinforcedAt: '2026-1' })],
  ],
  [
    'lra-z-vs-offset',
    base('s', { lastReinforcedAt: '2026-09-01T00:00:00Z' }),
    [base('l1', { lastReinforcedAt: '2026-09-01T00:00:00+00:00' })],
  ],
  ['lra-astral-vs-bmp', base('s', { lastReinforcedAt: '�' }), [base('l1', { lastReinforcedAt: '\u{1F600}' })]],
  ['lra-bmp-vs-astral', base('s', { lastReinforcedAt: '\u{1F600}' }), [base('l1', { lastReinforcedAt: '�' })]],
  ['lra-survivor-null', base('s', { lastReinforcedAt: null }), [base('l1', { lastReinforcedAt: '2026-01-01T00:00:00.000Z' })]],
  ['lra-all-empty', base('s', { lastReinforcedAt: '' }), [base('l1', { lastReinforcedAt: '' })]],
  // ---- everything at once ----
  [
    'all-fields',
    base('s', {
      content: `Mara sails.${footnotes(6)}`,
      reinforcementCount: 2,
      relatedMemoryIds: ['x'],
      occurredAt: '2026-05-03T00:00:00.000Z',
      lastReinforcedAt: '2026-05-03T00:00:00.000Z',
    }),
    [
      m({
        id: 'l1',
        content: 'Mara reached Lisbon and Porto and Faro.',
        reinforcementCount: 3,
        relatedMemoryIds: ['y', 's'],
        occurredAt: '2026-05-01T00:00:00.000Z',
        lastReinforcedAt: '2026-05-04T00:00:00.000Z',
      }),
    ],
  ],
];

rows.push({ kind: 'constant', id: 'fold-caps', out: FOLD_CAPS });

const enc = (_k: string, v: unknown) => (typeof v === 'number' && !Number.isFinite(v) ? String(v) : v);
for (const [id, survivor, losers] of plans) {
  const plan = foldInto(survivor, losers, FOLD_CAPS);
  rows.push({
    kind: 'plan',
    id,
    survivor,
    losers,
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
