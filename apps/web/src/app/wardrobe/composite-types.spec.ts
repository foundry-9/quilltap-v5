import { describe, expect, it } from 'vitest';

import type { WardrobeSlotType } from '../core/core-contract';
import { buildCompositeTypes, isComposite } from './composite-types';

/**
 * P4.D286 — v4 `lib/wardrobe/composite-types.ts` `buildCompositeTypes`
 * (`9ecebf986`, read at the pin `4a38222e6`). No v4 jsdom test names it; the
 * five vectors are the survey's (§2.1): union only, designated adds,
 * designated already in the union, canonical reorder, both empty.
 */
const c = (...types: WardrobeSlotType[]) => ({ types });

describe('buildCompositeTypes', () => {
  it('is the components’ union when nothing is designated', () => {
    expect(buildCompositeTypes([c('bottom'), c('top')])).toEqual(['top', 'bottom']);
  });

  it('widens by designated slots the components do not cover', () => {
    expect(buildCompositeTypes([c('accessories')], ['top', 'bottom'])).toEqual([
      'top',
      'bottom',
      'accessories',
    ]);
  });

  it('does not repeat a designated slot already in the union', () => {
    expect(buildCompositeTypes([c('top', 'footwear')], ['top'])).toEqual(['top', 'footwear']);
  });

  it('answers in canonical slot order whatever order it is given', () => {
    expect(buildCompositeTypes([c('hair'), c('footwear')], ['accessories', 'top'])).toEqual([
      'top',
      'footwear',
      'accessories',
      'hair',
    ]);
  });

  it('is empty when there are no components and nothing designated', () => {
    expect(buildCompositeTypes([])).toEqual([]);
    expect(buildCompositeTypes([], [])).toEqual([]);
  });
});

describe('isComposite', () => {
  it('is true only for an item with components', () => {
    expect(isComposite({ componentItemIds: ['a'] })).toBe(true);
    expect(isComposite({ componentItemIds: [] })).toBe(false);
    expect(isComposite({ componentItemIds: null })).toBe(false);
    expect(isComposite({})).toBe(false);
  });
});
