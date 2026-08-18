import { describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../core/core-client';
import type { CoreRequest, WardrobeItemDto } from '../core/core-contract';
import {
  createOutfitStore,
  expandSummaryComposites,
  resolveItemDetails,
  type WardrobeItemSummary,
} from './outfit-store';

type AnyRequest = CoreRequest & Record<string, unknown>;

function stubCore(
  route: (req: AnyRequest) => Record<string, unknown> | Error,
): { core: CoreClient; seen: AnyRequest[] } {
  const seen: AnyRequest[] = [];
  const dispatchData = vi.fn(async (req: CoreRequest) => {
    const r = req as AnyRequest;
    seen.push(r);
    const out = route(r);
    if (out instanceof Error) throw out;
    return out;
  });
  return { core: { dispatchData } as unknown as CoreClient, seen };
}

function dto(partial: Partial<WardrobeItemDto> & { id: string }): WardrobeItemDto {
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

function summary(partial: Partial<WardrobeItemSummary> & { id: string }): WardrobeItemSummary {
  return {
    title: partial.id,
    types: ['top'],
    isDefault: false,
    componentItemIds: [],
    ...partial,
  };
}

describe('expandSummaryComposites (v4 use-outfit.ts:70-101)', () => {
  it('walks composites to leaves, tolerating cycles and unknown ids', () => {
    const byId = new Map(
      [
        summary({ id: 'suit', componentItemIds: ['jacket', 'pants'] }),
        summary({ id: 'jacket' }),
        summary({ id: 'pants', types: ['bottom'] }),
        summary({ id: 'loop', componentItemIds: ['loop'] }),
      ].map((s) => [s.id, s]),
    );
    expect(expandSummaryComposites(['suit', 'ghost'], byId)).toEqual([
      'jacket',
      'pants',
      'ghost',
    ]);
    // A direct cycle drops the branch rather than recursing (v4 :84).
    expect(expandSummaryComposites(['loop'], byId)).toEqual([]);
  });

  it('stops at maxDepth, surfacing the composite id itself (v4 :85-88)', () => {
    const byId = new Map(
      [
        summary({ id: 'a', componentItemIds: ['b'] }),
        summary({ id: 'b', componentItemIds: ['c'] }),
        summary({ id: 'c' }),
      ].map((s) => [s.id, s]),
    );
    expect(expandSummaryComposites(['a'], byId, 1)).toEqual(['b']);
  });
});

describe('resolveItemDetails (v4 use-outfit.ts:185-213)', () => {
  it('projects expanded leaves back only into slots their own types cover', () => {
    const items = [
      summary({ id: 'suit', types: ['top', 'bottom'], componentItemIds: ['jacket', 'pants'] }),
      summary({ id: 'jacket', title: 'Jacket', types: ['top'] }),
      summary({ id: 'pants', title: 'Pants', types: ['bottom'] }),
    ];
    const out = resolveItemDetails(
      { top: ['suit'], bottom: ['suit'], footwear: [], accessories: [], hair: [] },
      items,
    );
    expect(out['top']).toEqual([{ itemId: 'jacket', title: 'Jacket' }]);
    expect(out['bottom']).toEqual([{ itemId: 'pants', title: 'Pants' }]);
    expect(out['footwear']).toEqual([]);
  });
});

describe('createOutfitStore (v4 use-outfit.ts)', () => {
  it('chat-less: refreshOutfit returns null without firing; equips are no-ops (v4 :15-17,:219-220,:298)', async () => {
    const { core, seen } = stubCore(() => new Error('no dispatch expected'));
    const store = createOutfitStore(core, null, () => ['c1']);
    expect(await store.refreshOutfit()).toBeNull();
    expect(await store.equipItem('c1', 'i1')).toBeNull();
    expect(await store.replaceItem('c1', 'i1')).toBeNull();
    expect(await store.addToSlot('c1', 'top', 'i1')).toBeNull();
    expect(await store.removeFromSlot('c1', 'top', 'i1')).toBeNull();
    expect(seen).toEqual([]);
  });

  it('fetchWardrobeForCharacter merges personal + archetype tiers; archetypes get "(shared)" and never isDefault (v4 :152-166)', async () => {
    const { core, seen } = stubCore((req) => {
      switch (req.type as string) {
        case 'characterWardrobeList':
          return { wardrobeItems: [dto({ id: 'a', title: 'Own', isDefault: true })] };
        case 'wardrobeList':
          return {
            wardrobeItems: [
              dto({ id: 'a', title: 'Own' }),
              dto({ id: 's', title: 'Cloak', isDefault: true, characterId: null }),
            ],
          };
        default:
          return new Error(`unexpected ${req.type}`);
      }
    });
    const store = createOutfitStore(core, 'chat-1', () => []);
    const items = await store.fetchWardrobeForCharacter('c1');
    expect(items.map((i) => [i.id, i.title, i.isDefault])).toEqual([
      ['a', 'Own', true],
      ['s', 'Cloak (shared)', false],
    ]);
    // The cache short-circuits a second fetch (v4 :124-126).
    const before = seen.length;
    await store.fetchWardrobeForCharacter('c1');
    expect(seen.length).toBe(before);
    // …until invalidated (v4 :451-457).
    store.invalidateWardrobe('c1');
    await store.fetchWardrobeForCharacter('c1');
    expect(seen.length).toBeGreaterThan(before);
  });

  it('refreshOutfit merges outfit keys with known characterIds and resolves details (v4 :224-253)', async () => {
    const { core, seen } = stubCore((req) => {
      switch (req.type as string) {
        case 'chatOutfitGet':
          return {
            equippedOutfit: {
              c1: { top: ['shirt'], bottom: [], footwear: [], accessories: [], hair: [] },
            },
          };
        case 'characterWardrobeList':
          return (req as { characterId?: string }).characterId === 'c1'
            ? { wardrobeItems: [dto({ id: 'shirt', title: 'Shirt' })] }
            : { wardrobeItems: [] };
        case 'wardrobeList':
          return { wardrobeItems: [] };
        default:
          return new Error(`unexpected ${req.type}`);
      }
    });
    const store = createOutfitStore(core, 'chat-1', () => ['c1', 'c2']);
    const state = await store.refreshOutfit();
    expect(seen[0]).toEqual({ type: 'chatOutfitGet', chatId: 'chat-1' });
    // c1 has slots + items; c2 has neither → excluded (v4 :245 comment).
    expect(Object.keys(state ?? {})).toEqual(['c1']);
    expect(state?.['c1'].itemsBySlot['top']).toEqual([{ itemId: 'shirt', title: 'Shirt' }]);
  });

  it('the four equip actions send v4\'s exact per-mode bodies (:303,:343,:380,:419-424) and update optimistically', async () => {
    const { core, seen } = stubCore((req) => {
      switch (req.type as string) {
        case 'chatOutfitGet':
          return {
            equippedOutfit: { c1: { top: [], bottom: [], footwear: [], accessories: [], hair: [] } },
          };
        case 'characterWardrobeList':
          return {
            wardrobeItems: [
              dto({ id: 'shirt', title: 'Shirt' }),
              dto({ id: 'hat', title: 'Hat', types: ['accessories'] }),
            ],
          };
        case 'wardrobeList':
          return { wardrobeItems: [] };
        case 'chatEquip':
          return { equippedSlots: {} };
        default:
          return new Error(`unexpected ${req.type}`);
      }
    });
    const store = createOutfitStore(core, 'chat-1', () => ['c1']);
    await store.refreshOutfit();
    seen.length = 0;

    await store.equipItem('c1', 'shirt');
    await store.replaceItem('c1', 'shirt');
    await store.addToSlot('c1', 'accessories', 'hat');
    await store.removeFromSlot('c1', 'accessories', 'hat');
    await store.removeFromSlot('c1', 'top', null);

    expect(seen).toEqual([
      { type: 'chatEquip', chatId: 'chat-1', characterId: 'c1', mode: 'wear', itemId: 'shirt' },
      { type: 'chatEquip', chatId: 'chat-1', characterId: 'c1', mode: 'replace', itemId: 'shirt' },
      {
        type: 'chatEquip',
        chatId: 'chat-1',
        characterId: 'c1',
        mode: 'add_to_slot',
        slot: 'accessories',
        itemId: 'hat',
      },
      {
        type: 'chatEquip',
        chatId: 'chat-1',
        characterId: 'c1',
        mode: 'remove_from_slot',
        slot: 'accessories',
        itemId: 'hat',
      },
      // A missing itemId clears the slot — and the body OMITS itemId (v4 :419-424).
      { type: 'chatEquip', chatId: 'chat-1', characterId: 'c1', mode: 'clear_slot', slot: 'top' },
    ]);

    // The optimistic state reflects the final mutations.
    const state = store.outfitState();
    expect(state['c1'].slots.top).toEqual([]);
    expect(state['c1'].slots.accessories).toEqual([]);
  });

  it('a failed equip dispatch returns null and leaves state untouched (v4 :305-308)', async () => {
    const { core } = stubCore((req) => {
      switch (req.type as string) {
        case 'chatOutfitGet':
          return {
            equippedOutfit: { c1: { top: ['x'], bottom: [], footwear: [], accessories: [], hair: [] } },
          };
        case 'characterWardrobeList':
          return { wardrobeItems: [dto({ id: 'x' })] };
        case 'wardrobeList':
          return { wardrobeItems: [] };
        case 'chatEquip':
          return new Error('boom');
        default:
          return new Error(`unexpected ${req.type}`);
      }
    });
    const store = createOutfitStore(core, 'chat-1', () => ['c1']);
    await store.refreshOutfit();
    expect(await store.removeFromSlot('c1', 'top', 'x')).toBeNull();
    expect(store.outfitState()['c1'].slots.top).toEqual(['x']);
  });
  /**
   * v4 4.8.2 `61574563`: `use-outfit.ts`'s three optimistic `computeDisplacedSlots`
   * calls each hand the cached wardrobe in as `itemsById`, so the optimistic
   * paint dissolves a bundle exactly as the server's persisted result will.
   * Drop the lookup at any one of the three and this goes red.
   */
  it('the optimistic equip paths dissolve a bundle into its parts (v4 use-outfit.ts:321,:361,:404)', async () => {
    const { core } = stubCore((req) => {
      switch (req.type as string) {
        case 'chatOutfitGet':
          return {
            equippedOutfit: { c1: { top: [], bottom: [], footwear: [], accessories: [], hair: [] } },
          };
        case 'characterWardrobeList':
          return {
            wardrobeItems: [
              dto({ id: 'leaf-top', title: 'Leaf Top', types: ['top'] }),
              dto({ id: 'leaf-shoe', title: 'Leaf Shoe', types: ['footwear'] }),
              dto({
                id: 'bundle',
                title: 'Man in Black',
                types: ['top', 'footwear'],
                componentItemIds: ['leaf-top', 'leaf-shoe'],
              }),
            ],
          };
        case 'wardrobeList':
          return { wardrobeItems: [] };
        case 'chatEquip':
          return { equippedSlots: {} };
        default:
          return new Error(`unexpected ${req.type}`);
      }
    });
    const store = createOutfitStore(core, 'chat-1', () => ['c1']);
    await store.refreshOutfit();

    await store.equipItem('c1', 'bundle');
    expect(store.outfitState()['c1'].slots).toEqual({
      top: ['leaf-top'],
      bottom: [],
      footwear: ['leaf-shoe'],
      accessories: [],
      hair: [],
    });

    await store.removeFromSlot('c1', 'top', null);
    await store.removeFromSlot('c1', 'footwear', null);
    await store.replaceItem('c1', 'bundle');
    expect(store.outfitState()['c1'].slots.top).toEqual(['leaf-top']);
    expect(store.outfitState()['c1'].slots.footwear).toEqual(['leaf-shoe']);

    await store.removeFromSlot('c1', 'top', null);
    await store.addToSlot('c1', 'top', 'bundle');
    expect(store.outfitState()['c1'].slots.top).toEqual(['leaf-top']);
  });
});
