/**
 * The one lookup rule for a wearable reference (v4 `lib/wardrobe/wearable-ref.ts`):
 * id first, then case-insensitive title, first match in pool order, and a
 * templated reference is never resolved. v4 `5cb0e0190`
 * `__tests__/unit/lib/wardrobe/wearable-ref.test.ts`, transcribed name for name;
 * the row-for-row differential is `wearable-ref.oracle.spec.ts`.
 */

import { describe, expect, it } from 'vitest';

import { isTemplatedWearableRef, matchWearableTitle, resolveWearableRef } from './wearable-ref';

const pool = [
  { id: 'a', title: 'Cloak of Night' },
  { id: 'b', title: 'cloak of night' },
  { id: 'c', title: 'b' },
  { id: 'd', title: 'Linen Shirt' },
];

describe('resolveWearableRef', () => {
  it('prefers an id over a title that happens to match', () => {
    // "b" is item c's title and item b's id: the id wins.
    expect(resolveWearableRef(pool, 'b')?.id).toBe('b');
  });

  it('matches a title case-insensitively, trimmed', () => {
    expect(resolveWearableRef(pool, '  LINEN shirt ')?.id).toBe('d');
  });

  it('takes the first title match in pool order', () => {
    expect(resolveWearableRef(pool, 'CLOAK OF NIGHT')?.id).toBe('a');
  });

  it('never resolves a templated reference', () => {
    expect(resolveWearableRef(pool, '{{params.garment}}')).toBeUndefined();
    expect(
      resolveWearableRef([{ id: 'x', title: '{{params.garment}}' }], '{{params.garment}}'),
    ).toBeUndefined();
  });

  it('resolves nothing for a blank or unknown reference', () => {
    expect(resolveWearableRef(pool, '   ')).toBeUndefined();
    expect(resolveWearableRef(pool, 'Bowler Hat')).toBeUndefined();
  });
});

describe('matchWearableTitle', () => {
  it('ignores ids', () => {
    expect(matchWearableTitle(pool, 'a')).toBeUndefined();
  });
});

describe('isTemplatedWearableRef', () => {
  it('spots a placeholder anywhere in the string', () => {
    expect(isTemplatedWearableRef('{{params.garment}}')).toBe(true);
    expect(isTemplatedWearableRef('The {{metadata.hat}} hat')).toBe(true);
    expect(isTemplatedWearableRef('Cloak of Night')).toBe(false);
    expect(isTemplatedWearableRef('{ not a template }')).toBe(false);
  });
});
