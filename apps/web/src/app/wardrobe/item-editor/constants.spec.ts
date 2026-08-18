import { describe, expect, it } from 'vitest';

import { getCandidateGroup, GROUP_LABEL, GROUP_ORDER } from './constants';
import type { CandidateItem } from './types';

const candidate = (over: Partial<CandidateItem> & { id: string }): CandidateItem => ({
  title: over.id,
  types: ['top'],
  componentItemIds: [],
  isShared: false,
  ...over,
});

/**
 * The component picker's grouping constants (v4
 * `components/wardrobe/wardrobe-item-editor/constants.ts`). Since `4423ad10`
 * both tables derive from the slot registry; these pin the derived values
 * literally, so a registry mistake cannot agree with a derived expectation.
 */
describe('item-editor grouping constants (v4 constants.ts)', () => {
  it('labels every slot group from the registry, Hair included', () => {
    expect(GROUP_LABEL).toEqual({
      top: 'Tops',
      bottom: 'Bottoms',
      footwear: 'Footwear',
      accessories: 'Accessories',
      hair: 'Hair',
      multi: 'Multi-slot',
    });
  });

  it('orders the groups canonically with the multi-slot catch-all LAST (v4 `:18`)', () => {
    expect(GROUP_ORDER).toEqual(['top', 'bottom', 'footwear', 'accessories', 'hair', 'multi']);
  });

  it('groups a single-slot hairdo under hair and a multi-slot one under multi (v4 `:26-29`)', () => {
    expect(getCandidateGroup(candidate({ id: 'waves', types: ['hair'] }))).toBe('hair');
    expect(getCandidateGroup(candidate({ id: 'wig-and-hat', types: ['hair', 'accessories'] }))).toBe(
      'multi',
    );
  });
});
