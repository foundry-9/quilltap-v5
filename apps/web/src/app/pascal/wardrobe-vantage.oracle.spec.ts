import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/wardrobe-vantage.oracle.ndjson';
import type { CustomToolDestinations } from '../core/core-contract';
import { wardrobeReachLabel } from '../wardrobe/wardrobe-container';
import { defaultWardrobeVantage } from './tool-draft';

/**
 * The corpus differential for the Workbench's wardrobe lens at the pin
 * `e8e17b2d9` (`5cb0e0190` — P4.D320): v4's `defaultWardrobeVantage`
 * (`lib/pascal/tool-draft.ts`) over v4's own test destinations plus a set whose
 * stores collide across the tiers, and `wardrobeReachLabel`
 * (`lib/wardrobe/wardrobe-container.ts`) over its four scopes and every
 * blank / padded / absent argument. Recorded by
 * `apps/web/oracle/wardrobe-vantage.recorder.ts` against v4's REAL functions.
 */

interface VantageRow {
  fn: 'defaultWardrobeVantage';
  id: string;
  mountPointId: string | null;
  fixture: string;
  destinations: CustomToolDestinations;
  out: string;
}
interface ReachRow {
  fn: 'wardrobeReachLabel';
  id: string;
  /** The argument LIST as passed — an absent argument is not in it. */
  args: unknown[];
  out: string | null;
}
type Row = VantageRow | ReachRow;

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

const vantageRows = ROWS.filter((r): r is VantageRow => r.fn === 'defaultWardrobeVantage');
const reachRows = ROWS.filter((r): r is ReachRow => r.fn === 'wardrobeReachLabel');

describe('the wardrobe lens agrees with v4 row for row', () => {
  it('carries the whole recorded corpus', () => {
    expect(ROWS).toHaveLength(40);
    expect(vantageRows).toHaveLength(20);
    expect(reachRows).toHaveLength(20);
  });

  it.each(vantageRows)('defaultWardrobeVantage — $id', (row) => {
    expect(JSON.stringify(defaultWardrobeVantage(row.mountPointId, row.destinations))).toBe(row.out);
  });

  it.each(reachRows)('wardrobeReachLabel — $id', (row) => {
    const label = (wardrobeReachLabel as (...args: unknown[]) => string | null)(...row.args);
    expect(label).toBe(row.out);
  });
});
