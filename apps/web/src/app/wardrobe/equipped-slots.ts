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
 *
 * The slot vocabulary is the registry's (`slot-meta.ts`, P4.D88) —
 * re-exported here rather than re-derived, per the P4.9f2 order.
 */

import { cloneSlots, freshSlots, WARDROBE_SLOT_TYPES } from './slot-meta';
import type {
  EquippedSlots,
  WardrobeItemDto,
  WardrobeSlotType,
} from '../core/core-contract';
import {
  dissolveBundlesInSlots,
  dissolveBundleToLeaves,
  layLeavesIntoSlots,
  type WearableLookup,
} from './dissolve-bundles';

export { cloneSlots, freshSlots, WARDROBE_SLOT_TYPES };
export type { EquippedSlots, WardrobeItemDto, WardrobeSlotType };

// `EquippedSlots` itself moved to `core-contract.ts` at the round's
// unification (it is a wire payload shape — the `chatEquip` /
// `wardrobePreviewAvatar` bodies carry it) and is re-exported above, the
// `WardrobeItemDto` precedent. Its v4 source is unchanged:
// `lib/schemas/wardrobe.types.ts:121-126`.

/** Per-chat equipped outfit state, keyed by characterId (v4 `:131`). */
export type EquippedOutfitState = Record<string, EquippedSlots>;

/** Empty equipped slots (v4 `EMPTY_EQUIPPED_SLOTS`, `wardrobe.types.ts:237`).
 *  Always spread (`{...EMPTY_EQUIPPED_SLOTS}` still shares the arrays — use
 *  `freshSlots()` for a mutable copy, as v4's callers do via `cloneSlots`). */
export const EMPTY_EQUIPPED_SLOTS: Readonly<EquippedSlots> = freshSlots();

// `equippedSlotsEqual` MOVED to `staged-live-outfits.ts` at 4.8.2 (v4
// `07d4ccce` lifted it out of the dialog into that module alongside the rebase
// and classification helpers it serves). Import it from there.

/** The minimal item shape the wear rule needs (v4 `outfit-displacement.ts:117`,
 *  widened with `componentItemIds` at 4.8.2). */
export interface WearableItem {
  id: string;
  types: WardrobeSlotType[];
  replace?: boolean;
  componentItemIds?: readonly string[];
}

/**
 * Pure flag-driven wear (v4 `outfit-displacement.ts:114-142`): for each slot in
 * `item.types`, replace the slot with `[item.id]` when `item.replace` is true,
 * otherwise append `item.id` (layering, no-op if already present). The single
 * rule behind every "put it on" gesture.
 *
 * Since 4.8.2 a bundle goes on as its PARTS, not as itself: given an
 * `itemsById` lookup that resolves its components, the leaves are laid into
 * the slots their own `types` declare and the bundle's id is never stored.
 * `replace` still governs — it clears the union of the slots the bundle
 * designates and the slots its pieces land in. Without a lookup (or when the
 * parts can't be resolved) the bundle is stored whole, the pre-4.8.2 behavior,
 * and read-time expansion still covers it.
 */
export function wearItemIntoSlots(
  currentSlots: EquippedSlots,
  item: WearableItem,
  itemsById?: WearableLookup,
): EquippedSlots {
  const leaves = dissolveBundleToLeaves(item, itemsById);
  if (leaves) {
    return layLeavesIntoSlots(currentSlots, item, leaves, {
      clearCoveredSlots: item.replace === true,
    });
  }

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
 * Pure full-slot replacement regardless of the `replace` flag
 * (v4 `outfit-displacement.ts:144-160` `replaceItemIntoSlots`). Was inlined in
 * `computeDisplacedSlots`'s `replace` arm before 4.8.2; lifted out here because
 * the bundle path needs it in both places.
 */
export function replaceItemIntoSlots(
  currentSlots: EquippedSlots,
  item: { id: string; types: WardrobeSlotType[]; componentItemIds?: readonly string[] },
  itemsById?: WearableLookup,
): EquippedSlots {
  const leaves = dissolveBundleToLeaves(item, itemsById);
  if (leaves) {
    return layLeavesIntoSlots(currentSlots, item, leaves, { clearCoveredSlots: true });
  }

  const slots = cloneSlots(currentSlots);
  for (const slotType of item.types) {
    slots[slotType] = [item.id];
  }
  return slots;
}

/**
 * Pure single-slot layering (v4 `outfit-displacement.ts:196-222`
 * `addItemToSlot`, new at 4.8.2). A bundle contributes the parts that cover
 * this slot rather than its own id; if none of them do (the caller asked for a
 * slot the bundle claims but no part fills), the bundle's id goes in as before
 * so the gesture is never silently a no-op.
 */
export function addItemToSlot(
  currentSlots: EquippedSlots,
  slot: WardrobeSlotType,
  item: { id: string; types: WardrobeSlotType[]; componentItemIds?: readonly string[] },
  itemsById?: WearableLookup,
): EquippedSlots {
  const slots = cloneSlots(currentSlots);
  const leaves = dissolveBundleToLeaves(item, itemsById);
  const forSlot = leaves?.filter((leaf) => leaf.slots.includes(slot)) ?? [];

  if (forSlot.length > 0) {
    for (const leaf of forSlot) {
      if (!slots[slot].includes(leaf.id)) slots[slot] = [...slots[slot], leaf.id];
    }
    return slots;
  }

  if (!slots[slot].includes(item.id)) {
    slots[slot] = [...slots[slot], item.id];
  }
  return slots;
}

/**
 * Deterministic layer order for a default outfit: oldest first, items lacking
 * `createdAt` last (v4 `default-outfit.ts` `sortForDefaultOutfit`).
 *
 * Ordering is observable now that personal and shared defaults can occupy the
 * same slot — slot arrays are read inner-to-outer. Both sides of the wire apply
 * this (the server's `sort_for_default_outfit`) so the composer's preview and
 * the chat that opens agree.
 */
export function sortForDefaultOutfit(items: WardrobeItemDto[]): WardrobeItemDto[] {
  return [...items].sort((a, b) => {
    const aTime = a.createdAt ? Date.parse(a.createdAt) : Number.POSITIVE_INFINITY;
    const bTime = b.createdAt ? Date.parse(b.createdAt) : Number.POSITIVE_INFINITY;
    return aTime - bTime;
  });
}

/**
 * Build a per-slot snapshot from the items marked `isDefault: true`, skipping
 * archived ones (v4 `default-outfit.ts:13-39`).
 */
export function buildDefaultOutfit(items: WardrobeItemDto[]): EquippedSlots {
  const next = freshSlots();
  for (const item of sortForDefaultOutfit(items)) {
    if (!item.isDefault || item.archivedAt) continue;
    for (const slot of item.types) next[slot].push(item.id);
  }
  // A bundle marked default goes on as its parts, like every other put-on
  // gesture — the wardrobe should never open onto a card over empty slots
  // (v4 `:36-38`, added at 4.8.2).
  return dissolveBundlesInSlots(next, new Map(items.map((i) => [i.id, i])));
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
  /**
   * Item lookup used to dissolve a bundle as it goes on (v4 `:297-305`, new at
   * 4.8.2). Omit and bundles are stored whole — correct, just less legible in
   * the slot rows.
   */
  itemsById?: WearableLookup;
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
    return wearItemIntoSlots(
      slots,
      {
        id: options.item.id,
        types: options.item.types as WardrobeSlotType[],
        componentItemIds: options.item.componentItemIds,
        replace: options.item.replace,
      },
      options.itemsById,
    );
  }

  if (options.mode === 'replace') {
    if (!options.item) return slots;
    return replaceItemIntoSlots(
      slots,
      {
        id: options.item.id,
        types: options.item.types as WardrobeSlotType[],
        componentItemIds: options.item.componentItemIds,
      },
      options.itemsById,
    );
  }

  if (options.mode === 'add_to_slot') {
    if (!options.item || !options.slot) return slots;
    return addItemToSlot(
      slots,
      options.slot,
      {
        id: options.item.id,
        types: options.item.types as WardrobeSlotType[],
        componentItemIds: options.item.componentItemIds,
      },
      options.itemsById,
    );
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
