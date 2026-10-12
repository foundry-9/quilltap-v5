/**
 * Oracle case (P4.D266 U1): the pure memory-merge plan + the footnote cap.
 *
 * Drives v4's REAL pure helpers (pin `01a83539d`, `3f7320138` F3 / F9):
 *   planMemoryMerge                                   (lib/memory/memory-merge.ts)
 *   appendCappedFootnotes, countReinforcementFootnotes,
 *   MAX_REINFORCEMENT_FOOTNOTES                       (lib/memory/memory-gate.ts)
 * No impurity; no injection. The plan's `patch` is emitted as v4 builds it, so
 * its KEY ORDER is part of the comparand (the Rust side byte-compares it).
 *
 * Run from inside the server checkout (a pin worktree when v4 HEAD has moved):
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/memory-merge-plan.ts \
 *     > /tmp/oracle-memory-merge-plan.ndjson
 */

import { planMemoryMerge } from '@/lib/memory/memory-merge';
import {
  appendCappedFootnotes,
  countReinforcementFootnotes,
  MAX_REINFORCEMENT_FOOTNOTES,
} from '@/lib/memory/memory-gate';
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

// ---- planMemoryMerge ------------------------------------------------------
// The plan reads only id / content / importance / reinforcementCount /
// relatedMemoryIds / occurredAt / lastReinforcedAt; the casts keep the rows
// minimal (a row's other keys never reach it).
const m = (o: Record<string, unknown>): Memory => o as unknown as Memory;
const base = (id: string, extra: Record<string, unknown> = {}): Memory =>
  m({ id, content: 'Mara sails the harbour.', importance: 0.5, ...extra });

const plans: Array<[string, Memory, Memory[], string[]]> = [
  ['no-losers', base('s'), [], []],
  ['identical-loser', base('s'), [base('l1')], []],
  [
    'novel-details',
    base('s'),
    [m({ id: 'l1', content: 'Mara sails to Lisbon with Joao.', importance: 0.9 })],
    [],
  ],
  [
    'shared-detail-appended-once',
    base('s'),
    [
      m({ id: 'l1', content: 'Mara met Lisbon traders.' }),
      m({ id: 'l2', content: 'Lisbon was loud; Porto quiet.' }),
    ],
    [],
  ],
  [
    'loser-details-all-exist',
    base('s', { content: 'Mara sails to Lisbon.' }),
    [m({ id: 'l1', content: 'The Lisbon run again.' })],
    [],
  ],
  [
    'cap-seven-plus-two-losers',
    base('s', { content: `Mara sails.${footnotes(7)}` }),
    [m({ id: 'l1', content: 'She met Joao and Ines.' }), m({ id: 'l2', content: 'Then Porto and Faro.' })],
    [],
  ],
  [
    'cap-already-full',
    base('s', { content: `Mara sails.${footnotes(8)}` }),
    [m({ id: 'l1', content: 'She met Joao.' })],
    [],
  ],
  // ---- counts: absent / null / 0 / 3 ----
  ['count-absent-absent', base('s'), [base('l1', { content: 'x' })], []],
  ['count-null', base('s', { reinforcementCount: null }), [base('l1', { reinforcementCount: null })], []],
  ['count-zero', base('s', { reinforcementCount: 0 }), [base('l1', { reinforcementCount: 0 })], []],
  ['count-zero-survivor', base('s', { reinforcementCount: 0 }), [base('l1', { reinforcementCount: 3 })], []],
  ['count-sum', base('s', { reinforcementCount: 2 }), [base('l1', { reinforcementCount: 3 }), base('l2')], []],
  ['count-importance-cap', base('s', { reinforcementCount: 40, importance: 0.95 }), [base('l1', { reinforcementCount: 40 })], []],
  // ---- links ----
  [
    'links-union-minus-group',
    base('s', { relatedMemoryIds: ['x', 'l1'] }),
    [base('l1', { relatedMemoryIds: ['s', 'y', 'x'] }), base('l2', { relatedMemoryIds: ['z', 'l1'] })],
    [],
  ],
  ['links-survivor-duplicates', base('s', { relatedMemoryIds: ['x', 'x'] }), [base('l1')], []],
  ['links-order-only', base('s', { relatedMemoryIds: ['b', 'a'] }), [base('l1', { relatedMemoryIds: ['a'] })], []],
  ['links-loser-to-survivor', base('s'), [base('l1', { relatedMemoryIds: ['s'] })], []],
  ['links-excluded', base('s', { relatedMemoryIds: ['x', 'gone'] }), [base('l1', { relatedMemoryIds: ['gone2', 'y'] })], ['gone', 'gone2']],
  ['links-null-arrays', base('s', { relatedMemoryIds: null }), [base('l1', { relatedMemoryIds: null })], []],
  // ---- occurredAt ----
  [
    'occurred-earliest-loser',
    base('s', { occurredAt: '2026-05-02T00:00:00.000Z' }),
    [base('l1', { occurredAt: '2026-05-01T00:00:00.000Z' })],
    [],
  ],
  ['occurred-survivor-null', base('s', { occurredAt: null }), [base('l1', { occurredAt: '2026-05-01T00:00:00.000Z' })], []],
  ['occurred-unparseable', base('s', { occurredAt: 'not a date' }), [base('l1', { occurredAt: 'garbage' })], []],
  ['occurred-unparseable-vs-real', base('s', { occurredAt: 'not a date' }), [base('l1', { occurredAt: '2026-05-01T00:00:00.000Z' })], []],
  ['occurred-empty', base('s', { occurredAt: '' }), [base('l1', { occurredAt: '' })], []],
  ['occurred-date-only', base('s', { occurredAt: '2026-05-02T00:00:00.000Z' }), [base('l1', { occurredAt: '2026-05-01' })], []],
  [
    'occurred-offset',
    base('s', { occurredAt: '2026-05-01T00:30:00.000Z' }),
    [base('l1', { occurredAt: '2026-05-01T01:00:00+02:00' })],
    [],
  ],
  [
    'occurred-tie-first-wins',
    base('s', { occurredAt: '2026-05-01T02:00:00+02:00' }),
    [base('l1', { occurredAt: '2026-05-01T00:00:00.000Z' })],
    [],
  ],
  [
    'occurred-tie-loser-first',
    base('s', { occurredAt: '2026-06-01T00:00:00.000Z' }),
    [base('l1', { occurredAt: '2026-05-01T00:00:00Z' }), base('l2', { occurredAt: '2026-05-01T00:00:00.000Z' })],
    [],
  ],
  // ---- lastReinforcedAt: the string max ----
  [
    'lra-string-vs-date',
    base('s', { lastReinforcedAt: '2026-09-01T00:00:00.000Z' }),
    [base('l1', { lastReinforcedAt: '2026-1' })],
    [],
  ],
  [
    'lra-z-vs-offset',
    base('s', { lastReinforcedAt: '2026-09-01T00:00:00Z' }),
    [base('l1', { lastReinforcedAt: '2026-09-01T00:00:00+00:00' })],
    [],
  ],
  ['lra-astral-vs-bmp', base('s', { lastReinforcedAt: '�' }), [base('l1', { lastReinforcedAt: '\u{1F600}' })], []],
  ['lra-bmp-vs-astral', base('s', { lastReinforcedAt: '\u{1F600}' }), [base('l1', { lastReinforcedAt: '�' })], []],
  ['lra-survivor-null', base('s', { lastReinforcedAt: null }), [base('l1', { lastReinforcedAt: '2026-01-01T00:00:00.000Z' })], []],
  ['lra-all-empty', base('s', { lastReinforcedAt: '' }), [base('l1', { lastReinforcedAt: '' })], []],
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
    ['x'],
  ],
];

for (const [id, survivor, losers, exclude] of plans) {
  rows.push({
    kind: 'plan',
    id,
    survivor,
    losers,
    exclude,
    out: planMemoryMerge(survivor, losers, exclude),
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
