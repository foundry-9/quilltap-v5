import { describe, expect, it } from 'vitest';

import { codePointLength, zodLenEqOk, zodLenMaxOk, zodLenMinOk } from './zod-length';

// Zod 4.5.4's code-point window rules (v4 `6e1a64ea6`), measured against
// `node_modules/zod/v4/core/checks.js` at the `d883a5ee1` unification; the
// same table as `jsstr::zod_length_tests` on the server.
describe('zod-length (Zod 4.5.4 string length rules)', () => {
  const hats = (n: number) => '🎩'.repeat(n); // 2 UTF-16 units, 1 code point

  it('counts code points only inside each check\'s window', () => {
    expect(codePointLength(hats(3))).toBe(3);
    expect(hats(3).length).toBe(6);

    expect(zodLenMaxOk(hats(101), 200)).toBe(true);
    expect(zodLenMaxOk(hats(201), 200)).toBe(false);
    expect(zodLenMaxOk('a'.repeat(200), 200)).toBe(true);
    expect(zodLenMaxOk('a'.repeat(201), 200)).toBe(false);

    expect(zodLenMinOk(hats(3), 5)).toBe(false);
    expect(zodLenMinOk(hats(5), 5)).toBe(true);
    expect(zodLenMinOk('a'.repeat(5), 5)).toBe(true);
    expect(zodLenMinOk('a'.repeat(4), 5)).toBe(false);
    expect(zodLenMinOk(hats(1), 1)).toBe(true);

    expect(zodLenEqOk(hats(64), 64)).toBe(true);
    expect(zodLenEqOk(hats(32), 64)).toBe(false);
    expect(zodLenEqOk('a'.repeat(64), 64)).toBe(true);
  });
});
