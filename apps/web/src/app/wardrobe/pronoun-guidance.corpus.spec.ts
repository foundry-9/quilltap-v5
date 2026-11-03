import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/pronoun-guidance.oracle.ndjson';
import { standardPronounLabel, standardPronounSubjectOf } from '../characters/pronoun-presets';
import {
  filterByPronounGuidance,
  fitsPronounGuidance,
  formatPronounGuidance,
  normalizePronounGuidance,
} from './pronoun-guidance';

/**
 * The tier-1 corpus differential for v4 `lib/wardrobe/pronoun-guidance.ts` +
 * `lib/characters/pronoun-presets.ts` at the pin `236c04e35` (`eb90d9ed8` —
 * P4.D359 item 15). Recorded by `apps/web/tooling/record-pronoun-guidance.mts`
 * against v4's REAL modules (see that file's header for the pinned-worktree
 * invocation); every row replays through the TS twins and byte-compares the
 * `JSON.stringify`'d result.
 */

interface Row {
  kind: 'normalize' | 'subjectOf' | 'label' | 'fits' | 'filter' | 'format';
  args: unknown[];
  expected: string;
}

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

const decode = (v: unknown): unknown =>
  v === '__undefined__'
    ? undefined
    : v && typeof v === 'object' && '$set' in v
      ? new Set((v as { $set: string[] }).$set)
      : v;

/* eslint-disable @typescript-eslint/no-explicit-any */
const RUN: Record<Row['kind'], (...a: any[]) => unknown> = {
  normalize: (raw) => normalizePronounGuidance(raw),
  subjectOf: (p) => standardPronounSubjectOf(p),
  label: (s) => standardPronounLabel(s),
  fits: (item, p) => fitsPronounGuidance(item, p),
  filter: (items, p, keep) => filterByPronounGuidance(items, p, keep),
  format: (g) => formatPronounGuidance(g),
};
/* eslint-enable @typescript-eslint/no-explicit-any */

const of = (kind: Row['kind']) => ROWS.filter((r) => r.kind === kind);

describe('pronoun guidance agrees with v4 row for row (236c04e35)', () => {
  it('carries the whole recorded corpus', () => {
    // A truncated fixture would make every `it.each` below vacuously green.
    expect(ROWS).toHaveLength(131);
    expect(of('normalize')).toHaveLength(22);
    expect(of('subjectOf')).toHaveLength(13);
    expect(of('label')).toHaveLength(6);
    expect(of('fits')).toHaveLength(70);
    expect(of('filter')).toHaveLength(11);
    expect(of('format')).toHaveLength(9);
  });

  it.each(ROWS.map((row, i) => [i, row.kind, row] as const))('row %i — %s', (_i, kind, row) => {
    expect(JSON.stringify(RUN[kind](...row.args.map(decode)))).toBe(row.expected);
  });
});
