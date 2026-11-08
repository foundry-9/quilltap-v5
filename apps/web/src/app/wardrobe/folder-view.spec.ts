import { describe, expect, it } from 'vitest';

import { OWN_TIER } from '../item-browser/folder-view';
import { viewTierOf } from './folder-view';
import type { WardrobeOrigin } from './wardrobe-container';

/**
 * v4 `lib/wardrobe/folder-view.ts` at the pin `624d0d25c` keeps ONLY
 * `viewTierOf` (33 lines; `b2b783b47` moved the view model to
 * `lib/folder-browser/view.ts`, whose rows are `item-browser/
 * folder-view.spec.ts`, and `600c2906f` deleted the re-exports).
 */

const PROJECT: WardrobeOrigin = { scope: 'project', id: 'p1', name: 'Blackwood' };
const GROUP: WardrobeOrigin = { scope: 'group', id: 'g1', name: 'The Sisters' };

interface Row {
  id: string;
  characterId?: string | null;
  archivedAt?: string | null;
  folder?: string | null;
  origin?: WardrobeOrigin | null;
}

const item = (id: string, extra: Partial<Row> = {}): Row => ({ id, characterId: 'c1', ...extra });

describe('viewTierOf (v4 lib/wardrobe/folder-view.ts)', () => {
  it('outside a character view every item is in the own tier, manageable or not', () => {
    expect(viewTierOf(item('a', { origin: PROJECT }), false, false)).toBe(OWN_TIER);
    expect(viewTierOf(item('a'), true, false)).toBe(OWN_TIER);
  });

  it('in a character view a manageable item is own; a borrowed one is keyed by its origin container', () => {
    expect(viewTierOf(item('a', { origin: PROJECT }), true, true)).toBe(OWN_TIER);
    expect(viewTierOf(item('a', { characterId: null, origin: PROJECT }), false, true)).toBe('project:p1');
    expect(viewTierOf(item('a', { characterId: null, origin: GROUP }), false, true)).toBe('group:g1');
    // An origin-less borrowed item is taken for a General archetype.
    expect(viewTierOf(item('a', { characterId: null }), false, true)).toBe('general:');
  });
});
