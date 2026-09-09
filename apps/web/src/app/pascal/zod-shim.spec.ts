import { describe, expect, it } from 'vitest';

import { parseFiniteNumber } from './zod-shim';

/**
 * The one `custom-tool-types.ts` sentence the shared corpus cannot carry: a
 * JSON document holding `1e999` parses to `Infinity` under `JSON.parse` (a
 * browser exactly as v4's Node) but `serde_json` refuses it outright, so no
 * committed corpus row can pose it on both sides. Measured against Zod 4.5.4
 * (`zod/v4/core/schemas.js:586`): the TYPE check names the value.
 */
describe('zod-shim parseFiniteNumber', () => {
  it('names Infinity and NaN in `received`, as Zod 4.5.4 does', () => {
    expect(parseFiniteNumber(Number.POSITIVE_INFINITY).issues.map((i) => i.message)).toEqual([
      'Invalid input: expected number, received Infinity',
    ]);
    expect(parseFiniteNumber(Number.NEGATIVE_INFINITY).issues.map((i) => i.message)).toEqual([
      'Invalid input: expected number, received -Infinity',
    ]);
    expect(parseFiniteNumber(Number.NaN).issues.map((i) => i.message)).toEqual([
      'Invalid input: expected number, received NaN',
    ]);
  });

  it('still renders the generic type sentence for a non-number', () => {
    expect(parseFiniteNumber('7').issues.map((i) => i.message)).toEqual([
      'Invalid input: expected number, received string',
    ]);
    expect(parseFiniteNumber(7).value).toBe(7);
  });
});
