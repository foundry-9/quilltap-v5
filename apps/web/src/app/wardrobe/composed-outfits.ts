/**
 * Composed outfits — the pool split the composer UI runs on. A client twin of
 * v4 `lib/wardrobe/composed-outfits.ts` (new at `aec86a613`, "an outfit
 * pull-down above the slot rows, garments only in the slot pickers").
 *
 * A wardrobe item is either a **garment** (a leaf: a shirt, a pair of boots,
 * or a dress covering `["top","bottom"]` — multi-slot, but still one thing you
 * put on) or a **composed outfit** (a composite: an item assembled out of
 * other items via `componentItemIds`). The distinction is `isBundle`, and this
 * module is only the sorted selection built on top of it.
 *
 * The composer surfaces the two differently: outfits hang off the single
 * "Wear an outfit" pull-down at the top, garments fill the per-slot pickers.
 * Without that split every bundle appeared once per slot it covered, three
 * rows deep, crowding out the garments actually meant for the slot.
 *
 * ⚠ `aec86a613` shipped NO server change — no verb, no schema, no wire byte.
 * The split is a pure presentation rule, and the equip path it feeds is the
 * pre-existing `onAddToSlot` callback (v5: the composer's `addToSlot` output).
 */

import { isBundle, type WearableNode } from './dissolve-bundles';

/**
 * The minimum this module needs on top of {@link WearableNode}: a title to
 * sort by. Stated structurally so `WardrobeItemDto` and the lighter item
 * summaries both satisfy it without conversion, and generic so the caller
 * gets its own element type back.
 */
export type TitledWearable = WearableNode & { title: string };

/**
 * The composed outfits in a wearable pool, title-sorted for the pull-down.
 *
 * Every composite qualifies, single-slot ones included — the slot pickers no
 * longer offer them, so this list is their only way onto a character.
 * Archived items are already gone from the pool the composer is handed (see
 * `mergeWearablePool`); nothing is re-filtered here.
 */
export function selectComposedOutfits<T extends TitledWearable>(items: readonly T[]): T[] {
  return items.filter((item) => isBundle(item)).sort((a, b) => a.title.localeCompare(b.title));
}

/**
 * The garments in a wearable pool — everything that isn't a composed outfit.
 * Order is the caller's; the slot pickers apply their own filtering.
 */
export function selectGarments<T extends TitledWearable>(items: readonly T[]): T[] {
  return items.filter((item) => !isBundle(item));
}
