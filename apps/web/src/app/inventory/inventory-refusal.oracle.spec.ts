import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/inventory-refusal.oracle.ndjson';
import { CoreDispatchError } from '../core/core-contract';
import { describeInventoryRefusal } from './inventory-refusal';

/**
 * The corpus differential for `describeInventoryRefusal` at the pin
 * `322a5488e` (P4.D330, Tier 1 item 13; v4 `useInventoryItemActions.ts:41-61`
 * with `7263105c6`'s title-lint join), recorded by
 * `apps/web/oracle/inventory-refusal.recorder.ts` through v4's REAL function
 * and `InventoryRequestError`. A `request` row is rebuilt as the
 * `CoreDispatchError` the dispatch transports throw — `code` and `details`
 * as R-L carries them.
 */

type Input =
  | { kind: 'request'; message: string; code: string | null; details: Record<string, unknown> }
  | { kind: 'error'; message: string }
  | { kind: 'other'; value: unknown };

interface Row {
  id: string;
  input: Input;
  fallback: string;
  out: string;
}

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

function thrown(input: Input): unknown {
  if (input.kind === 'request') {
    return new CoreDispatchError({
      kind: 'bad-request',
      message: input.message,
      ...(input.code !== null ? { code: input.code } : {}),
      details: input.details,
    });
  }
  return input.kind === 'error' ? new Error(input.message) : input.value;
}

describe('describeInventoryRefusal agrees with v4 row for row', () => {
  it('carries the whole recorded corpus', () => {
    expect(ROWS).toHaveLength(24);
  });

  it.each(ROWS)('$id', (row) => {
    expect(describeInventoryRefusal(thrown(row.input), row.fallback)).toBe(row.out);
  });
});
