/**
 * The per-chat outfit store — a port of v4 `lib/hooks/use-outfit.ts`. Manages
 * equipped-outfit state for characters in a chat session: fetches the equipped
 * slots (`chatOutfitGet`, v4 `:224`), resolves item details from each
 * character's wardrobe, and provides the per-mode equip primitives mirroring
 * the `wardrobe_wear`/`wardrobe_take_off` LLM tools surface (the four mutating
 * call sites v4 `:300` wear, `:340` replace, `:377` add_to_slot, `:416`
 * remove/clear — chatEquip's non-`set_all` callers).
 *
 * Pass `chatId === null` for chat-less surfaces (the wardrobe control dialog
 * opened from the character page) — the equip primitives become no-ops and
 * `refreshOutfit` returns null without firing (v4 `:15-17`).
 */

import { signal } from '@angular/core';

import type { CoreClient } from '../core/core-client';
import {
  cloneSlots,
  computeDisplacedSlots,
  EMPTY_EQUIPPED_SLOTS,
  WARDROBE_SLOT_TYPES,
  type EquippedSlots,
} from './equipped-slots';
import type { WardrobeItemDto, WardrobeSlotType } from '../core/core-contract';
import { dispatchWardrobe, type ChatEquipRequest } from './wardrobe.api';

/** Summary of a wardrobe item for display (v4 `use-outfit.ts:32-41`). */
export interface WardrobeItemSummary {
  id: string;
  title: string;
  types: string[];
  isDefault: boolean;
  /** IDs of component items if this item is a composite. */
  componentItemIds: string[];
  /** Composite-only: clear the designated slots on equip instead of layering. */
  replace?: boolean;
}

/** Per-slot resolved item details for display (v4 `:44`). */
export type ResolvedSlotItems = Record<string, Array<{ itemId: string; title: string }>>;

/** Per-character outfit state (v4 `:47-51`). */
export interface CharacterOutfitState {
  slots: EquippedSlots;
  /** Full per-slot array view: each slot is layered leaf items, in order. */
  itemsBySlot: ResolvedSlotItems;
}

/** Full outfit state, keyed by character ID (v4 `:54`). */
export type OutfitState = Record<string, CharacterOutfitState>;

/** Per-character wardrobe items cache (v4 `:57`). */
export type WardrobeCache = Record<string, WardrobeItemSummary[]>;

/**
 * Walk composites down to leaves using the cached wardrobe-summary list
 * (v4 `use-outfit.ts:70-101` `expandSummaryComposites`). Cycle-tolerant;
 * unknown ids are surfaced as-is so the caller can ignore them.
 */
export function expandSummaryComposites(
  rootIds: readonly string[],
  itemsById: Map<string, WardrobeItemSummary>,
  maxDepth = 4,
): string[] {
  const out: string[] = [];
  const seen = new Set<string>();

  const visit = (id: string, path: string[], depth: number): void => {
    const item = itemsById.get(id);
    if (!item) {
      if (!seen.has(id)) {
        seen.add(id);
        out.push(id);
      }
      return;
    }
    if (path.includes(id)) return; // cycle, drop branch
    if (depth >= maxDepth) {
      if (!seen.has(id)) {
        seen.add(id);
        out.push(id);
      }
      return;
    }
    if (item.componentItemIds.length === 0) {
      if (!seen.has(id)) {
        seen.add(id);
        out.push(id);
      }
      return;
    }
    const nextPath = [...path, id];
    for (const child of item.componentItemIds) {
      visit(child, nextPath, depth + 1);
    }
  };

  for (const root of rootIds) visit(root, [], 0);
  return out;
}

/**
 * Resolve item titles for equipped slots using a wardrobe-summary list
 * (v4 `use-outfit.ts:185-213` `resolveItemDetails`): each slot's equipped IDs
 * expand through composites to leaf titles, projected back into the slots
 * their own types cover.
 */
export function resolveItemDetails(
  slots: EquippedSlots,
  items: WardrobeItemSummary[],
): ResolvedSlotItems {
  const itemsById = new Map<string, WardrobeItemSummary>();
  for (const item of items) itemsById.set(item.id, item);

  const bySlot: ResolvedSlotItems = { top: [], bottom: [], footwear: [], accessories: [] };

  for (const slot of WARDROBE_SLOT_TYPES) {
    const equippedIds = slots[slot] ?? [];
    if (equippedIds.length === 0) continue;

    const leafIds = expandSummaryComposites(equippedIds, itemsById);
    const seen = new Set<string>();
    for (const leafId of leafIds) {
      if (seen.has(leafId)) continue;
      const leaf = itemsById.get(leafId);
      if (!leaf) continue;
      if (!leaf.types.includes(slot)) continue;
      bySlot[slot].push({ itemId: leaf.id, title: leaf.title });
      seen.add(leafId);
    }
  }

  return bySlot;
}

/** The store handle (v4's hook return, `use-outfit.ts:465-480`). */
export interface OutfitStore {
  readonly outfitState: ReturnType<typeof signal<OutfitState>>;
  readonly wardrobeCache: ReturnType<typeof signal<WardrobeCache>>;
  readonly loading: ReturnType<typeof signal<boolean>>;
  refreshOutfit(): Promise<OutfitState | null>;
  invalidateWardrobe(characterId?: string): void;
  fetchWardrobeForCharacter(characterId: string): Promise<WardrobeItemSummary[]>;
  /** Wear an item across every slot it covers, honoring its `replace` flag. */
  equipItem(characterId: string, itemId: string): Promise<ResolvedSlotItems | null>;
  /** Force-swap an item: clear the slots it covers, then set it to that item. */
  replaceItem(characterId: string, itemId: string): Promise<ResolvedSlotItems | null>;
  /** Append an item to a slot's array (layering). */
  addToSlot(
    characterId: string,
    slot: WardrobeSlotType,
    itemId: string,
  ): Promise<ResolvedSlotItems | null>;
  /** Remove a specific item from a slot, or clear the slot when itemId is null. */
  removeFromSlot(
    characterId: string,
    slot: WardrobeSlotType,
    itemId?: string | null,
  ): Promise<ResolvedSlotItems | null>;
}

/**
 * Create the store for one (chatId, characterIds) context. The inner dialog
 * remounts per `${characterId}|${chatId}` context (v4's key), so a store
 * instance's `chatId` is fixed for its lifetime; `characterIds` is a getter
 * because the selected character can change within a chat-less mount.
 */
export function createOutfitStore(
  core: CoreClient,
  chatId: string | null,
  characterIds: () => string[],
): OutfitStore {
  const outfitState = signal<OutfitState>({});
  const wardrobeCache = signal<WardrobeCache>({});
  const loading = signal(false);

  // Track which character wardrobes we've already fetched (v4 :113-118 — the
  // plain map mirrors v4's refs; the signal copy drives renders).
  const fetchedWardrobes = new Set<string>();
  let wardrobeCacheRef: WardrobeCache = {};

  async function fetchWardrobeForCharacter(characterId: string): Promise<WardrobeItemSummary[]> {
    if (fetchedWardrobes.has(characterId)) {
      return wardrobeCacheRef[characterId] ?? [];
    }

    // Fetch personal wardrobe and shared archetypes in parallel (v4 :130-133 —
    // note: NOT the project tier; the hook reads personal + global only).
    const [personal, archetype] = await Promise.all([
      core.dispatchData({ type: 'characterWardrobeList', characterId }).catch(() => null),
      dispatchWardrobe(core, { type: 'wardrobeList' }).catch(() => null),
    ]);

    const personalItems: WardrobeItemSummary[] = [];
    for (const item of (personal?.['wardrobeItems'] as WardrobeItemDto[] | undefined) ?? []) {
      personalItems.push({
        id: item.id,
        title: item.title,
        types: item.types,
        isDefault: item.isDefault ?? false,
        componentItemIds: Array.isArray(item.componentItemIds) ? item.componentItemIds : [],
        replace: item.replace === true,
      });
    }
    for (const item of (archetype?.['wardrobeItems'] as WardrobeItemDto[] | undefined) ?? []) {
      // Avoid duplicates (v4 :156); archetypes render "(shared)" and are
      // never default (v4 :159-161).
      if (!personalItems.some((p) => p.id === item.id)) {
        personalItems.push({
          id: item.id,
          title: `${item.title} (shared)`,
          types: item.types,
          isDefault: false,
          componentItemIds: Array.isArray(item.componentItemIds) ? item.componentItemIds : [],
          replace: item.replace === true,
        });
      }
    }

    fetchedWardrobes.add(characterId);
    wardrobeCacheRef = { ...wardrobeCacheRef, [characterId]: personalItems };
    wardrobeCache.set(wardrobeCacheRef);
    return personalItems;
  }

  async function refreshOutfit(): Promise<OutfitState | null> {
    if (!chatId) return null;

    loading.set(true);
    try {
      const data = await dispatchWardrobe(core, { type: 'chatOutfitGet', chatId });
      const equippedOutfit = (data['equippedOutfit'] as Record<string, EquippedSlots>) ?? {};

      // Merge character IDs from equipped outfit with all known IDs (v4 :234-237).
      const allCharacterIds = new Set([...Object.keys(equippedOutfit), ...characterIds()]);
      const newOutfitState: OutfitState = {};

      await Promise.all(
        Array.from(allCharacterIds).map(async (characterId) => {
          const slots = equippedOutfit[characterId] ?? cloneSlots(EMPTY_EQUIPPED_SLOTS);
          const wardrobeItems = await fetchWardrobeForCharacter(characterId);
          // Only include characters that actually have wardrobe items (v4 :245).
          if (wardrobeItems.length > 0 || equippedOutfit[characterId]) {
            newOutfitState[characterId] = {
              slots,
              itemsBySlot: resolveItemDetails(slots, wardrobeItems),
            };
          }
        }),
      );

      outfitState.set(newOutfitState);
      return newOutfitState;
    } catch {
      return null;
    } finally {
      loading.set(false);
    }
  }

  /** v4 `:269-289` — optimistic local mutation; null when the character has no
   *  cached outfit yet (a refreshOutfit picks it up after the round-trip). */
  function applyOptimistic(
    characterId: string,
    mutate: (slots: EquippedSlots, items: WardrobeItemSummary[]) => EquippedSlots,
  ): ResolvedSlotItems | null {
    const items = wardrobeCacheRef[characterId] ?? [];
    const currentChar = outfitState()[characterId];
    if (!currentChar) return null;

    const newSlots = mutate(currentChar.slots, items);
    const itemsBySlot = resolveItemDetails(newSlots, items);
    outfitState.update((prev) => ({ ...prev, [characterId]: { slots: newSlots, itemsBySlot } }));
    return itemsBySlot;
  }

  async function equipVia(
    request: Omit<ChatEquipRequest, 'type' | 'chatId'>,
    mutate: (slots: EquippedSlots, items: WardrobeItemSummary[]) => EquippedSlots,
  ): Promise<ResolvedSlotItems | null> {
    if (!chatId) return null;
    try {
      await dispatchWardrobe(core, { type: 'chatEquip', chatId, ...request });
      return applyOptimistic(request.characterId, mutate);
    } catch {
      return null;
    }
  }

  return {
    outfitState,
    wardrobeCache,
    loading,
    refreshOutfit,
    fetchWardrobeForCharacter,
    /** v4 `:451-457` — invalidate one character's cache (or all). */
    invalidateWardrobe(characterId?: string): void {
      if (characterId) fetchedWardrobes.delete(characterId);
      else fetchedWardrobes.clear();
    },
    /** v4 `:296-329` (`mode: 'wear'`). */
    equipItem(characterId, itemId) {
      return equipVia({ characterId, mode: 'wear', itemId }, (slots, items) => {
        const item = items.find((i) => i.id === itemId);
        if (!item) return slots;
        return computeDisplacedSlots(slots, {
          mode: 'wear',
          item: {
            id: item.id,
            types: item.types,
            componentItemIds: item.componentItemIds,
            replace: item.replace,
          },
        });
      });
    },
    /** v4 `:336-364` (`mode: 'replace'`). */
    replaceItem(characterId, itemId) {
      return equipVia({ characterId, mode: 'replace', itemId }, (slots, items) => {
        const item = items.find((i) => i.id === itemId);
        if (!item) return slots;
        return computeDisplacedSlots(slots, {
          mode: 'replace',
          item: { id: item.id, types: item.types },
        });
      });
    },
    /** v4 `:369-402` (`mode: 'add_to_slot'`). */
    addToSlot(characterId, slot, itemId) {
      return equipVia({ characterId, mode: 'add_to_slot', slot, itemId }, (slots, items) => {
        const item = items.find((i) => i.id === itemId);
        if (!item) return slots;
        return computeDisplacedSlots(slots, {
          mode: 'add_to_slot',
          slot,
          item: { id: item.id, types: item.types },
        });
      });
    },
    /** v4 `:407-444` (`mode: 'remove_from_slot' | 'clear_slot'` — a missing
     *  itemId clears the slot). */
    removeFromSlot(characterId, slot, itemId) {
      const isClear = !itemId;
      return equipVia(
        {
          characterId,
          mode: isClear ? 'clear_slot' : 'remove_from_slot',
          slot,
          ...(itemId ? { itemId } : {}),
        },
        (slots) =>
          computeDisplacedSlots(slots, {
            mode: isClear ? 'clear_slot' : 'remove_from_slot',
            slot,
            ...(itemId ? { itemId } : {}),
          }),
      );
    },
  };
}
