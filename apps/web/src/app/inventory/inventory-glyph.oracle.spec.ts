import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/inventory-abbreviation.oracle.ndjson';
import { inventoryAbbreviation } from './inventory-glyph';

/**
 * The corpus differential for v4's `inventoryAbbreviation`
 * (`components/inventory/inventory-glyph.tsx:23-32`, `27fb3ec77`) — the
 * two-letter glyph an Inventory object with neither SVG nor emoji shows under
 * a Salon avatar. Recorded by `apps/web/oracle/inventory-abbreviation.recorder.ts`
 * importing v4's REAL module at the pin.
 */

interface Row {
  id: string;
  kind: 'abbr';
  label: string;
  out: string;
}

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

describe('inventoryAbbreviation agrees with v4 row for row', () => {
  it('carries the whole recorded corpus', () => {
    expect(ROWS).toHaveLength(38);
  });

  it.each(ROWS)('$id', (row) => {
    expect(inventoryAbbreviation(row.label)).toBe(row.out);
  });
});
