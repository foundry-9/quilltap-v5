import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/inventory-action-summary.oracle.ndjson';
import {
  buildInventoryActionSummary,
  inventoryRowLabel,
  withArticle,
} from './inventory-action-summary';

/**
 * The corpus differential for the Salon tool row's Inventory notice at the
 * pin `322a5488e` (P4.D330, Tier 1 item 14): v4's module-private
 * `withArticle`, `inventoryRowLabel` and `buildInventoryActionSummary`
 * (`components/chat/ToolMessage.tsx:175-245`), recorded by
 * `apps/web/oracle/inventory-action-summary.recorder.ts` through an exporting
 * copy of v4's file (one appended `export` line). The `unicorn` rows pin the
 * CLIENT rule — `an unicorn horn` — which the server's Pascal line spells
 * with `a` (R-6, the recorded pair).
 */

interface Row {
  fn: 'withArticle' | 'inventoryRowLabel' | 'buildInventoryActionSummary';
  id: string;
  args: unknown[];
  out: string;
}

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const FNS: Record<Row['fn'], (...args: any[]) => unknown> = {
  withArticle,
  inventoryRowLabel,
  buildInventoryActionSummary,
};

describe('the Inventory tool notice agrees with v4 row for row', () => {
  it('carries the whole recorded corpus', () => {
    expect(ROWS).toHaveLength(58);
    expect(ROWS.filter((r) => r.out.includes('an unicorn horn'))).toHaveLength(2);
  });

  it.each(ROWS)('$fn — $id', (row) => {
    expect(JSON.stringify(FNS[row.fn](...row.args)) ?? null).toBe(row.out);
  });
});
