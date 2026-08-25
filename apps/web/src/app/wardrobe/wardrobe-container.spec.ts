import { describe, expect, it } from 'vitest';

import {
  GENERAL_CONTAINER,
  decodeWardrobeContainer,
  encodeWardrobeContainer,
  sameWardrobeContainer,
  type WardrobeContainer,
} from './wardrobe-container';

/**
 * The transcription parity pins for v4 `lib/wardrobe/wardrobe-container.ts`
 * (`d7263f39`). This module is the one place the scope spellings live, and the
 * transfers wire (`source.scope`) shares them — a drift here is a wire drift,
 * so every arm of v4's three functions is pinned, not just the happy path.
 */
describe('encodeWardrobeContainer (v4 :27-29)', () => {
  it('joins scope and id with a colon, the null id collapsing to empty', () => {
    expect(encodeWardrobeContainer({ scope: 'character', id: 'c1' })).toBe('character:c1');
    expect(encodeWardrobeContainer({ scope: 'project', id: 'p1' })).toBe('project:p1');
    expect(encodeWardrobeContainer({ scope: 'group', id: 'g1' })).toBe('group:g1');
    expect(encodeWardrobeContainer(GENERAL_CONTAINER)).toBe('general:');
  });

  it('GENERAL_CONTAINER is the singleton null-id general scope (v4 :24)', () => {
    expect(GENERAL_CONTAINER).toEqual({ scope: 'general', id: null });
  });
});

describe('decodeWardrobeContainer (v4 :32-44)', () => {
  it('round-trips every scope', () => {
    const cases: WardrobeContainer[] = [
      { scope: 'character', id: 'c1' },
      { scope: 'project', id: 'p1' },
      { scope: 'group', id: 'g1' },
      GENERAL_CONTAINER,
    ];
    for (const c of cases) {
      expect(decodeWardrobeContainer(encodeWardrobeContainer(c))).toEqual(c);
    }
  });

  it('rejects an unknown scope (v4 :34-41)', () => {
    expect(decodeWardrobeContainer('bogus:x')).toBeNull();
    expect(decodeWardrobeContainer('')).toBeNull();
    expect(decodeWardrobeContainer('characters:c1')).toBeNull();
  });

  it('a non-general scope MUST carry an id (v4 :43)', () => {
    expect(decodeWardrobeContainer('character:')).toBeNull();
    expect(decodeWardrobeContainer('project:')).toBeNull();
    expect(decodeWardrobeContainer('group:')).toBeNull();
  });

  it('general needs no id, and an id on general is honoured as written (v4 :42-44)', () => {
    expect(decodeWardrobeContainer('general:')).toEqual({ scope: 'general', id: null });
    // v4 does not special-case a general id — split/assign is unconditional.
    expect(decodeWardrobeContainer('general:x')).toEqual({ scope: 'general', id: 'x' });
  });

  it("split(':', 2) truncates at the second colon, exactly as v4's does", () => {
    expect(decodeWardrobeContainer('project:p1:extra')).toEqual({ scope: 'project', id: 'p1' });
  });
});

describe('sameWardrobeContainer (v4 :47-53)', () => {
  it('is false when either side is missing (v4 :51)', () => {
    expect(sameWardrobeContainer(null, GENERAL_CONTAINER)).toBe(false);
    expect(sameWardrobeContainer(GENERAL_CONTAINER, null)).toBe(false);
    expect(sameWardrobeContainer(undefined, undefined)).toBe(false);
  });

  it('compares scope and id, treating undefined id as null (v4 :52)', () => {
    expect(sameWardrobeContainer({ scope: 'project', id: 'p1' }, { scope: 'project', id: 'p1' })).toBe(
      true,
    );
    expect(sameWardrobeContainer({ scope: 'project', id: 'p1' }, { scope: 'project', id: 'p2' })).toBe(
      false,
    );
    expect(sameWardrobeContainer({ scope: 'project', id: 'p1' }, { scope: 'group', id: 'p1' })).toBe(
      false,
    );
    expect(
      sameWardrobeContainer(GENERAL_CONTAINER, { scope: 'general' } as WardrobeContainer),
    ).toBe(true);
  });
});
