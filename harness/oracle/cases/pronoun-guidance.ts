/**
 * Tier-1 oracle — wardrobe pronoun guidance (v4 `eb90d9ed8`).
 *
 * Drives v4's REAL pure modules `lib/characters/pronoun-presets` and
 * `lib/wardrobe/pronoun-guidance` over one fixed corpus: every input runs
 * through `normalizePronounGuidance` and `formatPronounGuidance`; every
 * (guidance, pronouns) pair through `standardPronounSubjectOf` and
 * `fitsPronounGuidance`; and a fixed three-item list through
 * `filterByPronounGuidance` per pronouns set (with and without `keepIds`).
 * The corpus carries the twelve pure `it`s of v4's `pronoun-guidance.test.ts`
 * plus the edge rows P4.D349 names.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/pronoun-guidance.ts \
 *     > /tmp/oracle-pronoun-guidance.ndjson
 */

import {
  STANDARD_PRONOUNS,
  STANDARD_PRONOUN_SUBJECTS,
  standardPronounLabel,
  standardPronounSubjectOf,
} from '@/lib/characters/pronoun-presets';
import {
  filterByPronounGuidance,
  fitsPronounGuidance,
  formatPronounGuidance,
  hasNoPronounGuidance,
  normalizePronounGuidance,
} from '@/lib/wardrobe/pronoun-guidance';
import type { Pronouns } from '@/lib/schemas/character.types';

const out = (row: unknown) => process.stdout.write(JSON.stringify(row) + '\n');

// ── the constants ────────────────────────────────────────────────────────
out({
  kind: 'presets',
  id: 'presets',
  presets: STANDARD_PRONOUNS.map((p) => ({ label: p.label, ...p.value })),
  subjects: STANDARD_PRONOUN_SUBJECTS,
  labels: ['he', 'she', 'they', 'it', 'xe', ''].map((s) => standardPronounLabel(s as never)),
});

// ── normalize / format over raw inputs ───────────────────────────────────
// `undefined` cannot cross JSON; the `absent` flag carries it.
const raws: Array<{ id: string; raw?: unknown; absent?: true }> = [
  { id: 'n-mixed-test', raw: ['They', 'he', 'he', 'xe', 7] },
  { id: 'n-single-string', raw: 'she' },
  { id: 'n-preset-label-list', raw: ['He/Him/His'] },
  { id: 'n-undefined', absent: true },
  { id: 'n-object', raw: { he: true } },
  { id: 'n-order', raw: ['they', 'she'] },
  { id: 'n-label-string', raw: 'He/Him/His' },
  { id: 'n-padded-label', raw: ' SHE / her ' },
  { id: 'n-dedup', raw: ['it', 'he', 'he'] },
  { id: 'n-unknown', raw: ['xe'] },
  { id: 'n-null', raw: null },
  { id: 'n-number', raw: 5 },
  { id: 'n-mixed-types', raw: [5, true, 'she'] },
  { id: 'n-they-them', raw: 'they/them' },
  { id: 'n-empty-string', raw: '' },
  { id: 'n-empty-list', raw: [] },
  { id: 'n-hand-edited', raw: ['She', 'xe'] },
  { id: 'n-slash-first', raw: '/he' },
  { id: 'n-nested', raw: [['he']] },
];
for (const r of raws) {
  const raw = r.absent ? undefined : r.raw;
  out({
    kind: 'normalize',
    id: r.id,
    absent: r.absent ?? false,
    raw: r.absent ? null : r.raw,
    normalize: normalizePronounGuidance(raw),
    format: formatPronounGuidance(raw as string[] | undefined),
  });
}

// ── subjectOf / fits over (guidance, pronouns) ───────────────────────────
const HE = { subject: 'he', object: 'him', possessive: 'his' };
const pronounSets: Array<{ id: string; pronouns: Pronouns | null }> = [
  { id: 'he', pronouns: HE as Pronouns },
  { id: 'she', pronouns: { subject: 'she', object: 'her', possessive: 'her' } as Pronouns },
  { id: 'He-case', pronouns: { subject: 'He', object: 'Him', possessive: 'His' } as Pronouns },
  { id: 'She-case', pronouns: { subject: 'She', object: 'Her', possessive: 'Her' } as Pronouns },
  { id: 'padded-it', pronouns: { subject: ' it ', object: 'IT', possessive: ' its' } as Pronouns },
  { id: 'partial-he-her', pronouns: { subject: 'he', object: 'her', possessive: 'his' } as Pronouns },
  { id: 'partial-they-his', pronouns: { subject: 'they', object: 'them', possessive: 'his' } as Pronouns },
  { id: 'xe', pronouns: { subject: 'xe', object: 'xem', possessive: 'xyr' } as Pronouns },
  { id: 'ze', pronouns: { subject: 'ze', object: 'zir', possessive: 'zir' } as Pronouns },
  { id: 'unset', pronouns: null },
];
const guidances: Array<{ id: string; guidance: string[] | null }> = [
  { id: 'none', guidance: null },
  { id: 'empty', guidance: [] },
  { id: 'gown', guidance: ['she', 'they'] },
  { id: 'he-only', guidance: ['he'] },
  { id: 'it-only', guidance: ['it'] },
];
for (const p of pronounSets) {
  out({
    kind: 'subjectOf',
    id: `s-${p.id}`,
    pronouns: p.pronouns,
    subjectOf: standardPronounSubjectOf(p.pronouns),
  });
  for (const g of guidances) {
    const item = g.guidance === null ? {} : { pronounGuidance: g.guidance };
    out({
      kind: 'fits',
      id: `f-${g.id}-${p.id}`,
      pronouns: p.pronouns,
      guidance: g.guidance,
      hasNone: hasNoPronounGuidance(item),
      fits: fitsPronounGuidance(item, p.pronouns),
    });
  }
}

// ── filter over a fixed item list ────────────────────────────────────────
const items = [
  { id: 'gown', pronounGuidance: ['she'] },
  { id: 'suit', pronounGuidance: ['he'] },
  { id: 'boots' },
  { id: 'cloak', pronounGuidance: ['they', 'it'] },
];
for (const p of pronounSets) {
  for (const keep of [null, ['gown'], ['suit', 'cloak']] as const) {
    const res = filterByPronounGuidance(items, p.pronouns, keep ? new Set(keep) : undefined);
    out({
      kind: 'filter',
      id: `x-${p.id}-${keep ? keep.join('+') : 'nokeep'}`,
      pronouns: p.pronouns,
      keepIds: keep,
      kept: res.items.map((i) => i.id),
      hiddenCount: res.hiddenCount,
    });
  }
}
