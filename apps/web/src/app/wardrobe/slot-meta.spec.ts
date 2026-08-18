import { describe, expect, it } from 'vitest';

import type { EquippedSlots } from '../core/core-contract';
import {
  CLOTHING_SLOT_TYPES,
  cloneSlots,
  freshSlots,
  isSlotReportedWhenEmpty,
  UNREPORTED_IF_BLANK_SLOT_TYPES,
  WARDROBE_SLOT_META,
  WARDROBE_SLOT_TYPES,
} from './slot-meta';

/**
 * Parity for the wardrobe slot registry (v4 `lib/schemas/wardrobe.types.ts`
 * as of `4423ad10`). The rows are compared LITERALLY against v4's table — a
 * derived expectation would agree with any mistake the registry makes.
 */
describe('the wardrobe slot registry (v4 WARDROBE_SLOT_TYPES + WARDROBE_SLOT_META)', () => {
  it('lists five slots in canonical order, hair appended LAST (v4 `wardrobe.types.ts:29`)', () => {
    expect(WARDROBE_SLOT_TYPES).toEqual(['top', 'bottom', 'footwear', 'accessories', 'hair']);
  });

  it('carries v4’s meta row for every slot, verbatim (v4 `:77-83`)', () => {
    expect(WARDROBE_SLOT_META).toEqual({
      top: {
        label: 'Top',
        groupLabel: 'Tops',
        badgeClass: 'qt-badge-wardrobe-top',
        isClothing: true,
        reportWhenEmpty: true,
        emptyFallback: 'topless',
      },
      bottom: {
        label: 'Bottom',
        groupLabel: 'Bottoms',
        badgeClass: 'qt-badge-wardrobe-bottom',
        isClothing: true,
        reportWhenEmpty: true,
        emptyFallback: 'bottomless',
      },
      footwear: {
        label: 'Footwear',
        groupLabel: 'Footwear',
        badgeClass: 'qt-badge-wardrobe-footwear',
        isClothing: true,
        reportWhenEmpty: true,
        emptyFallback: 'barefoot',
      },
      accessories: {
        label: 'Accessories',
        groupLabel: 'Accessories',
        badgeClass: 'qt-badge-wardrobe-accessories',
        isClothing: true,
        reportWhenEmpty: true,
        emptyFallback: 'no accessories',
      },
      hair: {
        label: 'Hair',
        groupLabel: 'Hair',
        badgeClass: 'qt-badge-wardrobe-hair',
        isClothing: false,
        reportWhenEmpty: false,
        emptyFallback: null,
      },
    });
  });

  it('hair is styling, not clothing — nudity semantics skip it (v4 CLOTHING_SLOT_TYPES)', () => {
    expect(CLOTHING_SLOT_TYPES).toEqual(['top', 'bottom', 'footwear', 'accessories']);
    expect(CLOTHING_SLOT_TYPES).not.toContain('hair');
  });

  it('hair is the one unreported-if-blank slot (v4 `:96-104`)', () => {
    expect(UNREPORTED_IF_BLANK_SLOT_TYPES).toEqual(['hair']);
    expect(isSlotReportedWhenEmpty('hair')).toBe(false);
    for (const slot of CLOTHING_SLOT_TYPES) expect(isSlotReportedWhenEmpty(slot)).toBe(true);
  });

  it('every reported slot has a fallback phrase and hair has none (v4 `emptyFallback`)', () => {
    for (const slot of WARDROBE_SLOT_TYPES) {
      const meta = WARDROBE_SLOT_META[slot];
      expect(meta.emptyFallback === null).toBe(!meta.reportWhenEmpty);
    }
  });
});

describe('freshSlots / cloneSlots (v4 makeEmptyEquippedSlots / cloneEquippedSlots)', () => {
  it('freshSlots mints one INDEPENDENT array per slot, hair included', () => {
    const a = freshSlots();
    expect(Object.keys(a)).toEqual([...WARDROBE_SLOT_TYPES]);
    a.hair.push('braids');
    expect(freshSlots().hair).toEqual([]);
  });

  it('cloneSlots deep-copies every slot (mutating the copy leaves the source alone)', () => {
    const src: EquippedSlots = {
      top: ['shirt'],
      bottom: [],
      footwear: [],
      accessories: [],
      hair: ['updo'],
    };
    const copy = cloneSlots(src);
    expect(copy).toEqual(src);
    copy.hair.push('ribbon');
    copy.top.push('coat');
    expect(src.hair).toEqual(['updo']);
    expect(src.top).toEqual(['shirt']);
  });

  it('cloneSlots reads a MISSING slot key as [] — v4’s forward-compat tolerance', () => {
    // An `equippedOutfit` bag written before the hair slot existed. The cast is
    // the point: raw JSON off the wire is not type-checked.
    const legacy = { top: ['shirt'], bottom: [], footwear: [], accessories: [] } as unknown as EquippedSlots;
    expect(cloneSlots(legacy)).toEqual({
      top: ['shirt'],
      bottom: [],
      footwear: [],
      accessories: [],
      hair: [],
    });
  });
});
