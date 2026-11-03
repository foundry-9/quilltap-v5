/**
 * Record the pronoun-guidance tier-1 corpus from v4's REAL modules
 * (`lib/wardrobe/pronoun-guidance.ts` + `lib/characters/pronoun-presets.ts`,
 * v4 `eb90d9ed8`) — P4.D359 item 15. `pronoun-guidance.corpus.spec.ts` replays
 * every row through the SPA's TS twins and byte-compares (`JSON.stringify`).
 *
 * Regenerate — run tsx FROM the pinned v4 worktree (its cwd resolves the `@/`
 * tsconfig aliases the modules import), with `QT_V4_ROOT` naming the same tree:
 *   cd <pin> && QT_V4_ROOT=$PWD PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
 *     npx tsx <v5>/apps/web/tooling/record-pronoun-guidance.mts \
 *     > <v5>/apps/web/src/testing/fixtures/pronoun-guidance.oracle.ndjson
 *
 * Row shape: `{kind, args, expected}`. `args` is the call's argument list as
 * JSON, with `"__undefined__"` standing for an `undefined` argument (JSON has
 * none) and `{"$set": [...]}` for a `Set`; `expected` is
 * `JSON.stringify(result)`.
 */

import { homedir } from 'node:os';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const V4_ROOT = process.env['QT_V4_ROOT'] ?? resolve(homedir(), 'source/quilltap-server');
const load = async <T,>(rel: string): Promise<T> =>
  (await import(pathToFileURL(resolve(V4_ROOT, rel)).href)) as T;

type Pronouns = { subject: string; object: string; possessive: string };
const guidance = await load<{
  normalizePronounGuidance(raw: unknown): string[];
  fitsPronounGuidance(item: unknown, pronouns: unknown): boolean;
  filterByPronounGuidance(items: unknown[], pronouns: unknown, keep?: Set<string>): unknown;
  formatPronounGuidance(g: unknown): string;
}>('lib/wardrobe/pronoun-guidance.ts');
const presets = await load<{
  standardPronounSubjectOf(p: unknown): string | null;
  standardPronounLabel(s: string): string;
}>('lib/characters/pronoun-presets.ts');

const U = '__undefined__';
const decode = (v: unknown): unknown =>
  v === U
    ? undefined
    : v && typeof v === 'object' && '$set' in (v as object)
      ? new Set((v as { $set: string[] }).$set)
      : v;
const rows: string[] = [];
const emit = (kind: string, args: unknown[], fn: (...a: unknown[]) => unknown): void => {
  rows.push(JSON.stringify({ kind, args, expected: JSON.stringify(fn(...args.map(decode))) }));
};

const HE: Pronouns = { subject: 'he', object: 'him', possessive: 'his' };
const SHE: Pronouns = { subject: 'she', object: 'her', possessive: 'her' };
const THEY: Pronouns = { subject: 'they', object: 'them', possessive: 'their' };
const IT: Pronouns = { subject: 'it', object: 'it', possessive: 'its' };
const XE: Pronouns = { subject: 'xe', object: 'xem', possessive: 'xyr' };

// normalize
for (const raw of [
  ['he'],
  ['she', 'he'],
  ['it', 'they', 'she', 'he'],
  'she',
  'He/Him/His',
  ['She/Her/Her', 'they/them/their'],
  ['  HE  ', 'She'],
  [' they / them '],
  ['xe', 'he'],
  ['he', 'he', 'HE'],
  [],
  '',
  ['', ' '],
  [1, 'it', null, {}],
  null,
  U,
  42,
  true,
  { he: true },
  ['he/', '/he'],
  ['h e'],
  ['They/Them/Their', 'It/It/Its', 'He/Him/His'],
])
  emit('normalize', [raw], (r) => guidance.normalizePronounGuidance(r));

// subjectOf
for (const p of [
  HE,
  SHE,
  THEY,
  IT,
  XE,
  { subject: 'HE', object: 'Him', possessive: 'HIS' },
  { subject: ' she ', object: ' her', possessive: 'her ' },
  { subject: 'she', object: 'her', possessive: 'hers' },
  { subject: 'he', object: 'him', possessive: 'her' },
  { subject: 'they', object: 'them', possessive: 'theirs' },
  { subject: '', object: '', possessive: '' },
  null,
  U,
])
  emit('subjectOf', [p], (x) => presets.standardPronounSubjectOf(x));

// label
for (const s of ['he', 'she', 'they', 'it', 'xe', ''])
  emit('label', [s], (x) => presets.standardPronounLabel(x as string));

// fits
const GUIDANCES: unknown[] = [
  U,
  null,
  [],
  ['he'],
  ['she'],
  ['they'],
  ['it'],
  ['he', 'she'],
  ['she', 'they'],
  ['he', 'they', 'it'],
];
for (const g of GUIDANCES) {
  for (const p of [HE, SHE, THEY, IT, XE, null, U]) {
    emit('fits', [g === U ? {} : { pronounGuidance: g }, p], (i, q) =>
      guidance.fitsPronounGuidance(i, q),
    );
  }
}

// filter
const ITEMS = [
  { id: 'gown', pronounGuidance: ['she'] },
  { id: 'waistcoat', pronounGuidance: ['he'] },
  { id: 'cloak' },
  { id: 'kilt', pronounGuidance: ['he', 'they'] },
  { id: 'lantern', pronounGuidance: [] },
  { id: 'chassis', pronounGuidance: ['it'] },
];
for (const p of [HE, SHE, THEY, IT, XE, null, U])
  emit('filter', [ITEMS, p], (i, q) => guidance.filterByPronounGuidance(i as unknown[], q));
emit('filter', [ITEMS, HE, { $set: ['gown'] }], (i, q, k) =>
  guidance.filterByPronounGuidance(i as unknown[], q, k as Set<string>),
);
emit('filter', [ITEMS, SHE, { $set: ['waistcoat', 'chassis', 'nope'] }], (i, q, k) =>
  guidance.filterByPronounGuidance(i as unknown[], q, k as Set<string>),
);
emit('filter', [ITEMS, XE, { $set: ['gown'] }], (i, q, k) =>
  guidance.filterByPronounGuidance(i as unknown[], q, k as Set<string>),
);
emit('filter', [[], HE], (i, q) => guidance.filterByPronounGuidance(i as unknown[], q));

// format
for (const g of [
  ['she', 'they'],
  ['they', 'he'],
  ['it'],
  [],
  null,
  U,
  ['xe', 'she'],
  'he',
  ['He/Him/His', 'SHE'],
]) {
  emit('format', [g], (x) => guidance.formatPronounGuidance(x));
}

process.stdout.write(rows.join('\n') + '\n');
