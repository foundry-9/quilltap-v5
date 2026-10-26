import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/inventory-display.oracle.ndjson';
import { containerOfOrigin, inventoryOriginLabel } from './inventory-container';
import {
  inventoryDisplayName,
  inventoryLabel,
  isEquippable,
  isKin,
  kitState,
} from './inventory-display';

/**
 * The corpus differential for the Inventory's client-safe pure helpers at the
 * pin `322a5488e` (P4.D330, Tier 1 item 13): v4's `inventoryDisplayName`,
 * `inventoryLabel`, `isKin`, `isEquippable`, `kitState`
 * (`lib/inventory/resolve.ts`) and `inventoryOriginLabel`, `containerOfOrigin`
 * (`lib/inventory/container.ts`), recorded by
 * `apps/web/oracle/inventory-display.recorder.ts` against v4's REAL functions.
 */

interface Row {
  fn:
    | 'inventoryDisplayName'
    | 'inventoryLabel'
    | 'isKin'
    | 'isEquippable'
    | 'kitState'
    | 'inventoryOriginLabel'
    | 'containerOfOrigin';
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
  inventoryDisplayName,
  inventoryLabel,
  isKin,
  isEquippable,
  kitState: (kit, equipped, pool: Record<string, unknown>) =>
    kitState(kit, equipped, (id: string) => pool[id] as never),
  inventoryOriginLabel,
  containerOfOrigin,
};

describe('the Inventory display helpers agree with v4 row for row', () => {
  it('carries the whole recorded corpus', () => {
    expect(ROWS).toHaveLength(67);
    expect(ROWS.filter((r) => r.fn === 'kitState')).toHaveLength(10);
  });

  it.each(ROWS)('$fn — $id', (row) => {
    expect(JSON.stringify(FNS[row.fn](...row.args)) ?? null).toBe(row.out);
  });
});
