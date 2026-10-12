import { describe, expect, it, vi } from 'vitest';

import {
  addIdToSlot,
  EMPTY_EQUIPPED_SLOTS,
  isComposite,
  removeIdFromSlot,
  type EquippedBundle,
  type EquippedSlots,
  type WardrobeItemDto,
} from './equipped-slots';
import {
  addToSlotGesture,
  archivedWearMessage,
  breakApartBundleGesture,
  clearSlotGesture,
  createComposerHandlers,
  removeFromSlotGesture,
  takeOffBundleGesture,
  wearGesture,
  wearRefusal,
  type StagedGesture,
} from './staged-live-outfits';

/**
 * P4.D285 — the composer gestures (v4 `4a38222e6` `lib/wardrobe/staged-live-
 * outfits.ts:166-229`), `useComposerHandlers` as `createComposerHandlers`, and
 * bug 191's `wearRefusal` (`lib/wardrobe/wearable.ts`).
 *
 * The first describe pair is v4 `__tests__/unit/lib/wardrobe/composer-
 * gestures.test.ts` transcribed case for case (all five); only the item
 * builder's type differs (`WardrobeItemDto`). The rest pin v5's own seams.
 */

const slots = (partial: Partial<EquippedSlots>): EquippedSlots => ({
  ...EMPTY_EQUIPPED_SLOTS,
  ...partial,
});

const item = (
  id: string,
  types: WardrobeItemDto['types'],
  extra: Partial<WardrobeItemDto> = {},
): WardrobeItemDto =>
  ({ id, title: id, types, replace: false, componentItemIds: [], ...extra }) as unknown as WardrobeItemDto;

const pool = (...items: WardrobeItemDto[]) => new Map(items.map((i) => [i.id, i]));

describe('wearGesture', () => {
  it('wears a garment across every slot it covers', () => {
    const dress = item('dress', ['top', 'bottom']);
    const g = wearGesture('top', 'dress', pool(dress));
    expect(g.mutate(slots({}))).toEqual(slots({ top: ['dress'], bottom: ['dress'] }));
    expect(g.wornBundleIds).toEqual([]);
  });

  it('dissolves an outfit into its leaves and claims the outfit for the ledger', () => {
    const hat = item('hat', ['accessories']);
    const boots = item('boots', ['footwear']);
    const rambler = item('rambler', ['accessories', 'footwear'], {
      componentItemIds: ['hat', 'boots'],
    });
    const g = wearGesture('accessories', 'rambler', pool(hat, boots, rambler));
    expect(g.mutate(slots({}))).toEqual(slots({ accessories: ['hat'], footwear: ['boots'] }));
    expect(g.wornBundleIds).toEqual(['rambler']);
  });

  it('appends an id the pool does not know to the slot it started in, once', () => {
    const g = wearGesture('top', 'ghost', pool());
    const once = g.mutate(slots({ top: ['shirt'] }));
    expect(once).toEqual(slots({ top: ['shirt', 'ghost'] }));
    expect(g.mutate(once)).toEqual(once);
  });
});

describe('slot gestures', () => {
  it('layers into one slot, removes one id, and clears a slot', () => {
    const coat = item('coat', ['top']);
    expect(addToSlotGesture('top', coat, pool(coat)).mutate(slots({ top: ['shirt'] }))).toEqual(
      slots({ top: ['shirt', 'coat'] }),
    );
    expect(removeFromSlotGesture('top', 'shirt').mutate(slots({ top: ['shirt', 'coat'] }))).toEqual(
      slots({ top: ['coat'] }),
    );
    expect(clearSlotGesture('top').mutate(slots({ top: ['shirt', 'coat'] }))).toEqual(slots({}));
  });

  it('never mutates the slots it is given', () => {
    const before = slots({ top: ['shirt'] });
    removeFromSlotGesture('top', 'shirt').mutate(before);
    clearSlotGesture('top').mutate(before);
    expect(before).toEqual(slots({ top: ['shirt'] }));
  });
});

describe('the bundle gestures (v5 rows — legacy whole-composite cards)', () => {
  const hat = item('hat', ['accessories']);
  const boots = item('boots', ['footwear']);
  const rambler = item('rambler', ['accessories', 'footwear'], { componentItemIds: ['hat', 'boots'] });
  const bundle: EquippedBundle = {
    compositeId: 'rambler',
    occupiedSlots: ['accessories', 'footwear'],
    allOccupied: true,
  };

  it('take off removes the composite id from every slot it occupies, claiming nothing', () => {
    const g = takeOffBundleGesture(bundle);
    expect(g.mutate(slots({ accessories: ['rambler', 'pin'], footwear: ['rambler'] }))).toEqual(
      slots({ accessories: ['pin'], footwear: [] }),
    );
    expect(g.wornBundleIds).toEqual([]);
  });

  it('break apart lays each covering leaf in place of the composite id', () => {
    const g = breakApartBundleGesture(bundle, pool(hat, boots, rambler));
    expect(g.mutate(slots({ accessories: ['rambler'], footwear: ['rambler'] }))).toEqual(
      slots({ accessories: ['hat'], footwear: ['boots'] }),
    );
  });
});

describe('the slot helpers (v4 `wardrobe.types.ts:390-412`)', () => {
  it('isComposite reads componentItemIds', () => {
    expect(isComposite({ componentItemIds: ['a'] })).toBe(true);
    expect(isComposite({ componentItemIds: [] })).toBe(false);
    expect(isComposite({ componentItemIds: null })).toBe(false);
    expect(isComposite({})).toBe(false);
  });

  it('addIdToSlot appends once; removeIdFromSlot filters, or clears with no id — always a copy', () => {
    const before = slots({ top: ['a'] });
    expect(addIdToSlot(before, 'top', 'b')).toEqual(slots({ top: ['a', 'b'] }));
    expect(addIdToSlot(before, 'top', 'a')).toEqual(before);
    expect(addIdToSlot(before, 'top', 'a')).not.toBe(before);
    expect(removeIdFromSlot(slots({ top: ['a', 'b'] }), 'top', 'a')).toEqual(slots({ top: ['b'] }));
    expect(removeIdFromSlot(slots({ top: ['a', 'b'] }), 'top')).toEqual(slots({}));
    expect(before).toEqual(slots({ top: ['a'] }));
  });
});

describe('createComposerHandlers (v4 `useComposerHandlers.ts:45-67`)', () => {
  const coat = item('coat', ['top']);
  const relic = item('relic', ['accessories'], { archivedAt: '2026-01-01T00:00:00.000Z' });

  function harness(refuse?: (i: WardrobeItemDto) => boolean) {
    const applied: StagedGesture[] = [];
    const h = createComposerHandlers({
      itemsById: () => pool(coat, relic),
      apply: (g) => applied.push(g),
      refuse,
    });
    return { h, applied };
  }

  it('onAddToSlot wears the item across its slots; the others land their gesture', () => {
    const { h, applied } = harness();
    h.onAddToSlot('top', 'coat');
    h.onRemoveFromSlot('top', 'coat');
    h.onClearSlot('bottom');
    expect(applied.map((g) => g.mutate(slots({ top: ['shirt'], bottom: ['skirt'] })))).toEqual([
      slots({ top: ['shirt', 'coat'], bottom: ['skirt'] }),
      slots({ top: ['shirt'], bottom: ['skirt'] }),
      slots({ top: ['shirt'], bottom: [] }),
    ]);
  });

  it('a refused item lands nothing; an unknown id still goes in (refuse sees only known items)', () => {
    const refuse = vi.fn((i: WardrobeItemDto) => Boolean(wearRefusal(i)));
    const { h, applied } = harness(refuse);
    h.onAddToSlot('accessories', 'relic');
    expect(applied).toHaveLength(0);
    expect(refuse).toHaveBeenCalledWith(relic);
    h.onAddToSlot('top', 'ghost');
    expect(applied).toHaveLength(1);
    expect(refuse).toHaveBeenCalledTimes(1);
  });
});

describe('wearRefusal (v4 `wearable.ts:15-25`, bug 191)', () => {
  it('refuses an archived item with v4’s sentence; a live one may be worn', () => {
    expect(archivedWearMessage('Opera Gloves')).toBe(
      'Item "Opera Gloves" is archived and cannot be worn',
    );
    expect(wearRefusal({ title: 'Opera Gloves', archivedAt: '2026-01-01T00:00:00.000Z' })).toBe(
      'Item "Opera Gloves" is archived and cannot be worn',
    );
    expect(wearRefusal({ title: 'Greatcoat', archivedAt: null })).toBeNull();
    expect(wearRefusal({ title: 'Greatcoat' })).toBeNull();
  });
});
