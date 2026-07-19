/**
 * Equipped-slots types + the pure wardrobe helpers behind every "put it on"
 * gesture — a port of v4's client-side wardrobe helper family:
 *
 *  - `lib/schemas/wardrobe.types.ts` (`EquippedSlots`, `EMPTY_EQUIPPED_SLOTS`)
 *  - `lib/wardrobe/outfit-displacement.ts` (`wearItemIntoSlots`)
 *  - `lib/wardrobe/bundle-mutations.ts` (`cloneSlots`, `takeOffBundleFromSlots`,
 *    `breakApartBundleInSlots`)
 *  - `lib/wardrobe/default-outfit.ts` (`buildDefaultOutfit`)
 *  - `lib/wardrobe/group-equipped.ts` (`groupEquippedSlots`)
 *  - `lib/wardrobe/next-copy-title.ts` (`nextCopyTitle`)
 *  - `lib/wardrobe/composite-types.ts` (`unionTypes`)
 *  - `components/wardrobe/wardrobe-control-dialog.tsx:71-81`
 *    (`equippedSlotsEqual`)
 *
 * The slot vocabulary is the one already landed in
 * `screens/prospero/wardrobe.api.ts` (`WARDROBE_SLOT_TYPES`) — re-exported
 * here rather than re-derived, per the P4.9f2 order.
 */

import { WARDROBE_SLOT_TYPES } from '../screens/prospero/wardrobe.api';
import type {
  EquippedSlots,
  WardrobeItemDto,
  WardrobeSlotType,
} from '../core/core-contract';

export { WARDROBE_SLOT_TYPES };
export type { EquippedSlots, WardrobeItemDto, WardrobeSlotType };

// `EquippedSlots` itself moved to `core-contract.ts` at the round's
// unification (it is a wire payload shape — the `chatEquip` /
// `wardrobePreviewAvatar` bodies carry it) and is re-exported above, the
// `WardrobeItemDto` precedent. Its v4 source is unchanged:
// `lib/schemas/wardrobe.types.ts:121-126`.

/** Per-chat equipped outfit state, keyed by characterId (v4 `:131`). */
export type EquippedOutfitState = Record<string, EquippedSlots>;

/** Empty equipped slots (v4 `EMPTY_EQUIPPED_SLOTS`, `wardrobe.types.ts:154`).
 *  Always spread (`{...EMPTY_EQUIPPED_SLOTS}` still shares the arrays — use
 *  `freshSlots()` for a mutable copy, as v4's callers do via `cloneSlots`). */
export const EMPTY_EQUIPPED_SLOTS: Readonly<EquippedSlots> = {
  top: [],
  bottom: [],
  footwear: [],
  accessories: [],
};

/** A brand-new all-empty snapshot (v4 `outfit-displacement.ts:46-48`). */
export function freshSlots(): EquippedSlots {
  return { top: [], bottom: [], footwear: [], accessories: [] };
}

/** Deep copy (v4 `bundle-mutations.ts:13-20`). */
export function cloneSlots(slots: EquippedSlots): EquippedSlots {
  return {
    top: [...slots.top],
    bottom: [...slots.bottom],
    footwear: [...slots.footwear],
    accessories: [...slots.accessories],
  };
}

/** Deep array equality on the four EquippedSlots arrays, in order
 *  (v4 `wardrobe-control-dialog.tsx:71-81`). */
export function equippedSlotsEqual(a: EquippedSlots, b: EquippedSlots): boolean {
  for (const slot of WARDROBE_SLOT_TYPES) {
    const av = a[slot];
    const bv = b[slot];
    if (av.length !== bv.length) return false;
    for (let i = 0; i < av.length; i++) {
      if (av[i] !== bv[i]) return false;
    }
  }
  return true;
}

/** The minimal item shape the wear rule needs (v4 `outfit-displacement.ts:76`). */
export interface WearableItem {
  id: string;
  types: WardrobeSlotType[];
  replace?: boolean;
}

/**
 * Pure flag-driven wear (v4 `outfit-displacement.ts:74-87`): for each slot in
 * `item.types`, replace the slot with `[item.id]` when `item.replace` is true,
 * otherwise append `item.id` (layering, no-op if already present). The single
 * rule behind every "put it on" gesture.
 */
export function wearItemIntoSlots(currentSlots: EquippedSlots, item: WearableItem): EquippedSlots {
  const slots = cloneSlots(currentSlots);
  for (const slotType of item.types) {
    if (item.replace) {
      slots[slotType] = [item.id];
    } else if (!slots[slotType].includes(item.id)) {
      slots[slotType] = [...slots[slotType], item.id];
    }
  }
  return slots;
}

/**
 * Build a per-slot snapshot from the items marked `isDefault: true`, skipping
 * archived ones (v4 `default-outfit.ts:13-20`).
 */
export function buildDefaultOutfit(items: WardrobeItemDto[]): EquippedSlots {
  const next = freshSlots();
  for (const item of items) {
    if (!item.isDefault || item.archivedAt) continue;
    for (const slot of item.types) next[slot].push(item.id);
  }
  return next;
}

// ---------------------------------------------------------------------------
// Bundle grouping (v4 `lib/wardrobe/group-equipped.ts`)
// ---------------------------------------------------------------------------

/** v4 `group-equipped.ts:21-32`. */
export interface EquippedBundle {
  /** The composite item's id. */
  compositeId: string;
  /** Slots this composite occupies in the current snapshot, in canonical order. */
  occupiedSlots: WardrobeSlotType[];
  /**
   * Whether the composite's id appears in every slot it claims via `types`.
   * Useful for surfacing "partially worn" bundles where a slot was removed
   * after the bundle was put on.
   */
  allOccupied: boolean;
}

/** v4 `group-equipped.ts:34-42`. */
export interface GroupedEquipped {
  bundles: EquippedBundle[];
  /**
   * Per-slot ids that should still be rendered as chips in slot rows. Bundle
   * composite ids are removed from this list when the bundle has fully claimed
   * the slot — but layered leaves alongside a bundle remain.
   */
  slotRemainders: EquippedSlots;
}

/**
 * Group an equipped-slots snapshot into bundles + remainders
 * (v4 `group-equipped.ts:61-113`). Rules:
 *  - A composite enters `bundles` when it occupies ≥ 2 slots in the snapshot.
 *  - Bundle composite ids are removed from `slotRemainders`; layered leaves
 *    that share a slot with a bundle composite remain.
 *  - Single-slot composites stay inline in `slotRemainders` (the renderer
 *    decorates them with a `· bundle` note).
 *  - Orphaned ids (not present in `items`) pass through as-is in
 *    `slotRemainders` and never enter `bundles`.
 */
export function groupEquippedSlots(
  slots: EquippedSlots,
  items: WardrobeItemDto[],
): GroupedEquipped {
  const itemsById = new Map<string, WardrobeItemDto>(items.map((i) => [i.id, i]));

  // Pass 1: collect every composite's per-snapshot occupied-slot list.
  const compositeSlots = new Map<string, WardrobeSlotType[]>();
  for (const slot of WARDROBE_SLOT_TYPES) {
    const ids = slots[slot] ?? [];
    for (const id of ids) {
      const item = itemsById.get(id);
      if (!item || (item.componentItemIds ?? []).length === 0) continue;
      const list = compositeSlots.get(id) ?? [];
      if (!list.includes(slot)) list.push(slot);
      compositeSlots.set(id, list);
    }
  }

  // Pass 2: promote composites that occupy ≥ 2 slots into bundles.
  const bundles: EquippedBundle[] = [];
  const bundleIds = new Set<string>();
  for (const [compositeId, occupied] of compositeSlots) {
    if (occupied.length < 2) continue;
    bundleIds.add(compositeId);
    const composite = itemsById.get(compositeId);
    const allOccupied = (composite?.types ?? []).every((t) => occupied.includes(t));
    bundles.push({ compositeId, occupiedSlots: occupied, allOccupied });
  }

  // Sort bundles by their first occupied slot for stable rendering order
  // (v4 `:93-97`).
  bundles.sort((a, b) => {
    const ia = WARDROBE_SLOT_TYPES.indexOf(a.occupiedSlots[0]);
    const ib = WARDROBE_SLOT_TYPES.indexOf(b.occupiedSlots[0]);
    return ia - ib;
  });

  // Pass 3: build slot remainders — drop bundle composite ids from each slot.
  const slotRemainders = freshSlots();
  for (const slot of WARDROBE_SLOT_TYPES) {
    const ids = slots[slot] ?? [];
    slotRemainders[slot] = ids.filter((id) => !bundleIds.has(id));
  }

  return { bundles, slotRemainders };
}

/** Remove a bundle's composite id from every slot it occupies
 *  (v4 `bundle-mutations.ts:23-32`). */
export function takeOffBundleFromSlots(slots: EquippedSlots, bundle: EquippedBundle): EquippedSlots {
  const next = cloneSlots(slots);
  for (const slot of bundle.occupiedSlots) {
    next[slot] = next[slot].filter((id) => id !== bundle.compositeId);
  }
  return next;
}

/**
 * Replace a bundle's composite id with its direct component ids in every slot
 * it occupies; multi-slot leaves go into all slots they cover
 * (v4 `bundle-mutations.ts:38-56`).
 */
export function breakApartBundleInSlots(
  slots: EquippedSlots,
  bundle: EquippedBundle,
  itemsById: Map<string, WardrobeItemDto>,
): EquippedSlots {
  const composite = itemsById.get(bundle.compositeId);
  if (!composite) return slots;
  const next = cloneSlots(slots);
  for (const slot of bundle.occupiedSlots) {
    const replacementIds = (composite.componentItemIds ?? []).filter((leafId) => {
      const leaf = itemsById.get(leafId);
      return leaf?.types.includes(slot) ?? false;
    });
    next[slot] = next[slot].flatMap((id) => (id === bundle.compositeId ? replacementIds : [id]));
  }
  return next;
}

// ---------------------------------------------------------------------------
// Optimistic-update displacement (v4 outfit-displacement.ts:192-256)
// ---------------------------------------------------------------------------

/** v4 `outfit-displacement.ts:194`. */
export type DisplacementMode = 'wear' | 'replace' | 'add_to_slot' | 'remove_from_slot' | 'clear_slot';

/** v4 `outfit-displacement.ts:196-205`. */
export interface ComputeDisplacedOptions {
  mode: DisplacementMode;
  /** Required for `wear`, `replace`, and `add_to_slot`. `replace` (the flag)
   *  drives `wear`'s layer-vs-replace behaviour (see `wearItemIntoSlots`). */
  item?: { id: string; types: string[]; componentItemIds?: string[]; replace?: boolean };
  /** Required for `add_to_slot`, `remove_from_slot`, `clear_slot`. */
  slot?: WardrobeSlotType;
  /** Filter target for `remove_from_slot`; omit to clear the slot. */
  itemId?: string;
}

/**
 * Pure-function displacement for frontend optimistic updates
 * (v4 `outfit-displacement.ts:207-256`) — the client mirror of the server's
 * per-mode equip arms.
 */
export function computeDisplacedSlots(
  currentSlots: EquippedSlots,
  options: ComputeDisplacedOptions,
): EquippedSlots {
  const slots = cloneSlots(currentSlots);

  if (options.mode === 'wear') {
    if (!options.item) return slots;
    return wearItemIntoSlots(slots, {
      id: options.item.id,
      types: options.item.types as WardrobeSlotType[],
      replace: options.item.replace,
    });
  }

  if (options.mode === 'replace') {
    if (!options.item) return slots;
    // v4 `replaceItemIntoSlots` (:94-103): clear each covered slot and set it
    // to just [item.id], regardless of the `replace` flag.
    for (const slotType of options.item.types as WardrobeSlotType[]) {
      slots[slotType] = [options.item.id];
    }
    return slots;
  }

  if (options.mode === 'add_to_slot') {
    if (!options.item || !options.slot) return slots;
    if (!slots[options.slot].includes(options.item.id)) {
      slots[options.slot] = [...slots[options.slot], options.item.id];
    }
    return slots;
  }

  if (options.mode === 'remove_from_slot') {
    if (!options.slot) return slots;
    if (!options.itemId) {
      slots[options.slot] = [];
    } else {
      const target = options.itemId;
      slots[options.slot] = slots[options.slot].filter((id) => id !== target);
    }
    return slots;
  }

  if (options.mode === 'clear_slot') {
    if (!options.slot) return slots;
    slots[options.slot] = [];
    return slots;
  }

  return slots;
}

// ---------------------------------------------------------------------------
// Titles + type unions
// ---------------------------------------------------------------------------

/** Matches a trailing ` (copy)` or ` (copy <N>)` suffix, case-insensitive
 *  (v4 `next-copy-title.ts:13`). */
const COPY_SUFFIX = /\s*\(copy(?:\s+\d+)?\)\s*$/i;

/**
 * Pick the next free `(copy)` / `(copy N)` title for a duplicated item
 * (v4 `next-copy-title.ts:21-29`). Any trailing copy suffix on the source is
 * stripped first, so duplicating `Shirt (copy)` yields `Shirt (copy 2)`;
 * collision is case-insensitive.
 */
export function nextCopyTitle(sourceTitle: string, existingTitles: string[]): string {
  const base = sourceTitle.replace(COPY_SUFFIX, '').trim() || sourceTitle.trim();
  const taken = new Set(existingTitles.map((t) => t.trim().toLowerCase()));

  for (let n = 1; ; n++) {
    const candidate = n === 1 ? `${base} (copy)` : `${base} (copy ${n})`;
    if (!taken.has(candidate.toLowerCase())) return candidate;
  }
}

/**
 * Compute the union of slot types across a list of components, in canonical
 * slot order (v4 `composite-types.ts:17-23`). Used to derive a composite
 * item's `types` from its components — the server runs the exact same union.
 */
export function unionTypes(components: readonly Pick<WardrobeItemDto, 'types'>[]): WardrobeSlotType[] {
  const set = new Set<WardrobeSlotType>();
  for (const c of components) {
    for (const t of c.types) set.add(t);
  }
  return WARDROBE_SLOT_TYPES.filter((s) => set.has(s));
}
