import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/breadcrumb-density.oracle.ndjson';
import {
  BREADCRUMB_DENSITIES,
  MINOR_WORDS,
  TITLE_MEASURE_MAX_PX,
  abbreviateName,
  chooseDensity,
  type BreadcrumbDensity,
} from './breadcrumb-density';

/**
 * The corpus differential for v4 `components/chat/ChatHeaderBreadcrumb.tsx`'s two
 * pure helpers (`abbreviateName`, `chooseDensity`; v4 `3c41cf3da`, pinned at
 * `624d0d25c`). Recorded by `apps/web/oracle/breadcrumb-density.recorder.ts`
 * against v4's REAL module (see that file's header for the pinned-worktree
 * invocation); every row is compared exactly.
 */

/** A recorded number: finite as itself, `NaN` / `±Infinity` spelled as strings. */
type Enc = number | string;

interface AbbreviateRow {
  id: string;
  kind: 'abbreviate';
  name: string;
  out: string;
}

interface DensityRow {
  id: string;
  kind: 'density';
  available: Enc;
  widths: Record<BreadcrumbDensity, Enc>;
  out: BreadcrumbDensity;
}

const ALL: Array<AbbreviateRow | DensityRow> = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as AbbreviateRow | DensityRow);

const ABBREVIATE = ALL.filter((r): r is AbbreviateRow => r.kind === 'abbreviate');
const DENSITY = ALL.filter((r): r is DensityRow => r.kind === 'density');

const dec = (n: Enc): number => Number(n);

describe("the breadcrumb helpers agree with v4's ChatHeaderBreadcrumb.tsx row for row", () => {
  it('carries the whole recorded corpus (52 abbreviate + 26 density)', () => {
    // A truncated fixture would make the it.each below vacuously green.
    expect(ALL).toHaveLength(78);
    expect(ABBREVIATE).toHaveLength(52);
    expect(DENSITY).toHaveLength(26);
    expect(new Set(ALL.map((r) => r.id)).size).toBe(ALL.length);
  });

  it.each(ABBREVIATE.map((r) => [r.id, r] as const))('abbreviateName — %s', (_id, row) => {
    expect(abbreviateName(row.name)).toBe(row.out);
  });

  it.each(DENSITY.map((r) => [r.id, r] as const))('chooseDensity — %s', (_id, row) => {
    const widths = {
      full: dec(row.widths.full),
      abbrev: dec(row.widths.abbrev),
      avatar: dec(row.widths.avatar),
    };
    expect(chooseDensity(dec(row.available), widths)).toBe(row.out);
  });

  it("keeps v4's constants (the density order, the title cap, the ten minor words)", () => {
    expect(BREADCRUMB_DENSITIES).toEqual(['full', 'abbrev', 'avatar']);
    expect(TITLE_MEASURE_MAX_PX).toBe(256);
    expect([...MINOR_WORDS]).toEqual(['a', 'an', 'and', 'the', 'of', 'in', 'on', 'at', 'to', 'for']);
  });
});
