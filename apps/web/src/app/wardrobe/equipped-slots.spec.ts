import { describe, expect, it } from 'vitest';

import type { WardrobeItemDto } from '../core/core-contract';
import {
  breakApartBundleInSlots,
  buildDefaultOutfit,
  cloneSlots,
  EMPTY_EQUIPPED_SLOTS,
  equippedSlotsEqual,
  freshSlots,
  groupEquippedSlots,
  nextCopyTitle,
  takeOffBundleFromSlots,
  unionTypes,
  wearItemIntoSlots,
  type EquippedSlots,
} from './equipped-slots';

function item(partial: Partial<WardrobeItemDto> & { id: string }): WardrobeItemDto {
  return {
    title: partial.id,
    types: ['top'],
    componentItemIds: [],
    isDefault: false,
    replace: false,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    ...partial,
  } as WardrobeItemDto;
}

describe('equippedSlotsEqual (v4 wardrobe-control-dialog.tsx:71-81)', () => {
  it('compares the four slot arrays element-wise, in order', () => {
    const a: EquippedSlots = { top: ['x'], bottom: [], footwear: ['y'], accessories: [] };
    expect(equippedSlotsEqual(a, cloneSlots(a))).toBe(true);
    expect(equippedSlotsEqual(a, { ...cloneSlots(a), top: ['z'] })).toBe(false);
    // Order matters — v4 walks index-by-index, no set semantics.
    expect(
      equippedSlotsEqual(
        { top: ['a', 'b'], bottom: [], footwear: [], accessories: [] },
        { top: ['b', 'a'], bottom: [], footwear: [], accessories: [] },
      ),
    ).toBe(false);
  });
});

describe('wearItemIntoSlots (v4 outfit-displacement.ts:74-87)', () => {
  it('layers into each covered slot when replace is falsy (append, no-op if present)', () => {
    const start: EquippedSlots = { top: ['shirt'], bottom: [], footwear: [], accessories: [] };
    const next = wearItemIntoSlots(start, { id: 'sweater', types: ['top'] });
    expect(next.top).toEqual(['shirt', 'sweater']);
    // No-op when already present (v4 :82 `includes` guard).
    expect(wearItemIntoSlots(next, { id: 'sweater', types: ['top'] }).top).toEqual([
      'shirt',
      'sweater',
    ]);
    // The input snapshot is never mutated (v4 clones first).
    expect(start.top).toEqual(['shirt']);
  });

  it('replaces each covered slot with just [item.id] when replace is true (v4 :80-81)', () => {
    const start: EquippedSlots = {
      top: ['shirt'],
      bottom: ['jeans'],
      footwear: ['boots'],
      accessories: [],
    };
    const next = wearItemIntoSlots(start, { id: 'dress', types: ['top', 'bottom'], replace: true });
    expect(next.top).toEqual(['dress']);
    expect(next.bottom).toEqual(['dress']);
    expect(next.footwear).toEqual(['boots']);
  });
});

describe('buildDefaultOutfit (v4 default-outfit.ts:13-20)', () => {
  it('collects isDefault items into every slot their types cover, skipping archived', () => {
    const items = [
      item({ id: 'shirt', types: ['top'], isDefault: true }),
      item({ id: 'dress', types: ['top', 'bottom'], isDefault: true }),
      item({ id: 'old', types: ['top'], isDefault: true, archivedAt: '2026-01-01T00:00:00.000Z' }),
      item({ id: 'hat', types: ['accessories'], isDefault: false }),
    ];
    const out = buildDefaultOutfit(items);
    expect(out.top).toEqual(['shirt', 'dress']);
    expect(out.bottom).toEqual(['dress']);
    expect(out.accessories).toEqual([]);
  });
});

describe('groupEquippedSlots (v4 group-equipped.ts:61-113)', () => {
  const suit = item({ id: 'suit', types: ['top', 'bottom'], componentItemIds: ['jacket', 'pants'] });
  const jacket = item({ id: 'jacket', types: ['top'] });
  const pants = item({ id: 'pants', types: ['bottom'] });
  const scarf = item({ id: 'scarf', types: ['accessories'] });
  const all = [suit, jacket, pants, scarf];

  it('promotes a composite occupying ≥2 slots into a bundle and strips it from remainders', () => {
    const grouped = groupEquippedSlots(
      { top: ['suit'], bottom: ['suit'], footwear: [], accessories: ['scarf'] },
      all,
    );
    expect(grouped.bundles).toEqual([
      { compositeId: 'suit', occupiedSlots: ['top', 'bottom'], allOccupied: true },
    ]);
    expect(grouped.slotRemainders.top).toEqual([]);
    expect(grouped.slotRemainders.accessories).toEqual(['scarf']);
  });

  it('keeps layered leaves that share a slot with a bundle composite (v4 rule 2)', () => {
    const grouped = groupEquippedSlots(
      { top: ['suit', 'jacket'], bottom: ['suit'], footwear: [], accessories: [] },
      all,
    );
    expect(grouped.slotRemainders.top).toEqual(['jacket']);
  });

  it('leaves single-slot composites inline in remainders (v4 rule 3)', () => {
    const oneSlot = item({ id: 'combo', types: ['top'], componentItemIds: ['jacket'] });
    const grouped = groupEquippedSlots(
      { top: ['combo'], bottom: [], footwear: [], accessories: [] },
      [...all, oneSlot],
    );
    expect(grouped.bundles).toEqual([]);
    expect(grouped.slotRemainders.top).toEqual(['combo']);
  });

  it('passes orphaned ids through as-is and never bundles them (v4 rule 4)', () => {
    const grouped = groupEquippedSlots(
      { top: ['ghost'], bottom: ['ghost'], footwear: [], accessories: [] },
      all,
    );
    expect(grouped.bundles).toEqual([]);
    expect(grouped.slotRemainders.top).toEqual(['ghost']);
  });

  it('flags partially-worn bundles via allOccupied (v4 :86-89)', () => {
    const cover3 = item({
      id: 'outfit3',
      types: ['top', 'bottom', 'footwear'],
      componentItemIds: ['jacket', 'pants'],
    });
    const grouped = groupEquippedSlots(
      { top: ['outfit3'], bottom: ['outfit3'], footwear: [], accessories: [] },
      [...all, cover3],
    );
    expect(grouped.bundles[0].allOccupied).toBe(false);
  });
});

describe('bundle mutations (v4 bundle-mutations.ts)', () => {
  const suit = item({ id: 'suit', types: ['top', 'bottom'], componentItemIds: ['jacket', 'pants'] });
  const jacket = item({ id: 'jacket', types: ['top'] });
  const pants = item({ id: 'pants', types: ['bottom'] });
  const byId = new Map([suit, jacket, pants].map((i) => [i.id, i]));
  const bundle = { compositeId: 'suit', occupiedSlots: ['top', 'bottom'] as const, allOccupied: true };

  it('takeOffBundleFromSlots removes the composite id from every occupied slot (v4 :23-32)', () => {
    const next = takeOffBundleFromSlots(
      { top: ['suit', 'shirt'], bottom: ['suit'], footwear: [], accessories: [] },
      { ...bundle, occupiedSlots: [...bundle.occupiedSlots] },
    );
    expect(next.top).toEqual(['shirt']);
    expect(next.bottom).toEqual([]);
  });

  it('breakApartBundleInSlots replaces the composite with its slot-matching components (v4 :38-56)', () => {
    const next = breakApartBundleInSlots(
      { top: ['suit'], bottom: ['suit', 'belt'], footwear: [], accessories: [] },
      { ...bundle, occupiedSlots: [...bundle.occupiedSlots] },
      byId,
    );
    expect(next.top).toEqual(['jacket']);
    expect(next.bottom).toEqual(['pants', 'belt']);
  });

  it('breakApart is a no-op when the composite is unknown (v4 :44)', () => {
    const slots: EquippedSlots = { top: ['ghost'], bottom: [], footwear: [], accessories: [] };
    expect(
      breakApartBundleInSlots(
        slots,
        { compositeId: 'ghost', occupiedSlots: ['top'], allOccupied: true },
        byId,
      ),
    ).toBe(slots);
  });
});

describe('nextCopyTitle (v4 next-copy-title.ts:21-29)', () => {
  it('appends (copy), escalating past taken titles case-insensitively', () => {
    expect(nextCopyTitle('Shirt', [])).toBe('Shirt (copy)');
    expect(nextCopyTitle('Shirt', ['shirt (COPY)'])).toBe('Shirt (copy 2)');
    expect(nextCopyTitle('Shirt', ['Shirt (copy)', 'Shirt (copy 2)'])).toBe('Shirt (copy 3)');
  });

  it('strips an existing copy suffix first, so a copy of a copy escalates (v4 :22)', () => {
    expect(nextCopyTitle('Shirt (copy)', ['Shirt (copy)'])).toBe('Shirt (copy 2)');
    expect(nextCopyTitle('Shirt (copy 4)', [])).toBe('Shirt (copy)');
  });
});

describe('unionTypes (v4 composite-types.ts:17-23)', () => {
  it('unions component types in canonical slot order regardless of input order', () => {
    expect(
      unionTypes([{ types: ['accessories'] }, { types: ['bottom', 'top'] }]),
    ).toEqual(['top', 'bottom', 'accessories']);
  });
});

describe('freshSlots / EMPTY_EQUIPPED_SLOTS', () => {
  it('freshSlots returns independent arrays; EMPTY is a shared frozen shape', () => {
    const a = freshSlots();
    a.top.push('x');
    expect(freshSlots().top).toEqual([]);
    expect(EMPTY_EQUIPPED_SLOTS.top).toEqual([]);
  });
});
