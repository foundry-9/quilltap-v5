import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/wear-display.ndjson';
import type { WardrobeWearSummary } from '../core/core-contract';
import { formatWearLine, formatWornRelative, formatWornWhen } from './wear-display';

/**
 * The corpus differential for v4 `lib/wardrobe/wear-display.ts` at the pin
 * `01a83539d` (bug 183, `ed8b15b50` — P4.D277): `formatWornRelative`,
 * `formatWornWhen` and `formatWearLine` over the whole day ladder, every rung
 * edge and the empty / unparseable inputs. Recorded by
 * `apps/web/oracle/wear-display.recorder.ts` against v4's REAL module (which
 * imports the REAL `lib/format-time.ts` ladder) — see that file's header for
 * the pinned-worktree invocation.
 */

interface Row {
  fn: 'formatWornRelative' | 'formatWornWhen' | 'formatWearLine';
  id: string;
  args: unknown[];
  out: string;
}

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

/** The recorder's tag for an `undefined` argument (JSON cannot carry one). */
function revive(arg: unknown): unknown {
  return arg !== null && typeof arg === 'object' && '$undefined' in arg ? undefined : arg;
}

function replay(row: Row): string {
  const [first, nowMs] = [revive(row.args[0]), row.args[1] as number];
  switch (row.fn) {
    case 'formatWornRelative':
      return formatWornRelative(first as number, nowMs);
    case 'formatWornWhen':
      return formatWornWhen(first as string | null | undefined, nowMs);
    case 'formatWearLine':
      return formatWearLine(first as WardrobeWearSummary | null | undefined, nowMs);
  }
}

describe("wear-display agrees with v4's lib/wardrobe/wear-display.ts row for row", () => {
  it('carries the whole recorded corpus', () => {
    // A truncated fixture would make the it.each below vacuously green.
    expect(ROWS).toHaveLength(112);
    expect(ROWS.filter((r) => r.fn === 'formatWornRelative')).toHaveLength(32);
    expect(ROWS.filter((r) => r.fn === 'formatWornWhen')).toHaveLength(38);
    expect(ROWS.filter((r) => r.fn === 'formatWearLine')).toHaveLength(42);
  });

  it.each(ROWS)('$fn $id', (row) => {
    expect(replay(row)).toBe(row.out);
  });
});
