import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/wearable-ref.oracle.ndjson';
import { isTemplatedWearableRef, matchWearableTitle, resolveWearableRef } from './wearable-ref';

/**
 * The corpus differential for v4 `lib/wardrobe/wearable-ref.ts` at the pin
 * `e8e17b2d9` (`5cb0e0190` — P4.D320): the one lookup rule for a string that
 * names a garment or outfit. Recorded by
 * `apps/web/oracle/wearable-ref.recorder.ts` against v4's REAL module (which
 * imports v4's REAL `scanPlaceholders`) — see that file's header for the
 * pinned-worktree invocation. Every row replays through the twin, exact.
 */

interface Row {
  fn: 'resolveWearableRef' | 'matchWearableTitle' | 'isTemplatedWearableRef';
  id: string;
  pool: Array<{ id: string; title: string }>;
  arg: string;
  out: string | boolean | null;
}

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

const of = (fn: Row['fn']) => ROWS.filter((r) => r.fn === fn);

describe('wearable references agree with v4 row for row', () => {
  it('carries the whole recorded corpus', () => {
    // A truncated fixture would make every `it.each` below vacuously green.
    expect(ROWS).toHaveLength(53);
    expect(of('resolveWearableRef')).toHaveLength(31);
    expect(of('matchWearableTitle')).toHaveLength(10);
    expect(of('isTemplatedWearableRef')).toHaveLength(12);
  });

  it.each(of('resolveWearableRef'))('resolveWearableRef — $id', (row) => {
    expect(resolveWearableRef(row.pool, row.arg)?.id ?? null).toBe(row.out);
  });

  it.each(of('matchWearableTitle'))('matchWearableTitle — $id', (row) => {
    expect(matchWearableTitle(row.pool, row.arg)?.id ?? null).toBe(row.out);
  });

  it.each(of('isTemplatedWearableRef'))('isTemplatedWearableRef — $id', (row) => {
    expect(isTemplatedWearableRef(row.arg)).toBe(row.out);
  });
});
