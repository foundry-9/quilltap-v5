import { describe, expect, it } from 'vitest';

import { coreErrorMessage } from './core-client';
import { CoreDispatchError } from './core-contract';

/**
 * v5's analog of v4 `apiErrorMessage()` in `lib/query/fetcher.ts` (`0246c6c8`).
 */

describe('coreErrorMessage (v4 apiErrorMessage @ 0246c6c8)', () => {
  it('reads a dispatch failure’s sentence — already unwrapped by the transport', () => {
    // v4's helper also has to dig `{ error }` out of an ApiFetchError's parsed
    // body; v5 has no analog of that half, because CoreDispatchError is built
    // FROM the error envelope.
    const err = new CoreDispatchError({
      kind: 'not_found',
      message: 'No such character.',
    } as never);

    expect(coreErrorMessage(err, 'unused')).toBe('No such character.');
  });

  it('reads a plain Error’s own message', () => {
    expect(coreErrorMessage(new Error('the socket gave out'), 'unused')).toBe(
      'the socket gave out',
    );
  });

  it('falls back only for a thrown value that is not an Error', () => {
    expect(coreErrorMessage('a bare string', 'The letter could not be posted.')).toBe(
      'The letter could not be posted.',
    );
    expect(coreErrorMessage(undefined, 'The bench could not oblige.')).toBe(
      'The bench could not oblige.',
    );
  });

  it('returns an Error’s empty message rather than the fallback (v4’s semantics)', () => {
    // Deliberate: v4's helper returns `err.message` unconditionally for an
    // Error. Two v5 sites had drifted to `(err instanceof Error && err.message)
    // || fallback`, which substitutes the fallback here; adopting the shared
    // helper converges them back onto v4.
    expect(coreErrorMessage(new Error(''), 'the fallback')).toBe('');
  });
});
