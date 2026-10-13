/**
 * A composite wardrobe item's slots — the client half of v4
 * `lib/wardrobe/composite-types.ts` (`9ecebf986`, read at the pin
 * `4a38222e6`). v4's `isComposite` (`lib/schemas/wardrobe.types.ts:390`) lives
 * once, beside the other `wardrobe.types.ts` helpers in `equipped-slots.ts`.
 *
 * `unionTypes` stays where it already lives (`equipped-slots.ts`, P4.9f2) and
 * is re-used here, so the union is spelled once.
 *
 * @module wardrobe/composite-types
 */

import type { WardrobeItemDto, WardrobeSlotType } from '../core/core-contract';
import { unionTypes } from './equipped-slots';
import { WARDROBE_SLOT_TYPES } from './slot-meta';

/**
 * A composite's `types`: every slot its components cover plus any extra slots
 * it designates (a "Naked" composite that clears accessories it holds nothing
 * for). Widens, never narrows — the one rule for create and update, client and
 * server, so an edit can never silently drop a slot the composite claimed.
 * (v4 `composite-types.ts:25-37`.)
 */
export function buildCompositeTypes(
  components: readonly Pick<WardrobeItemDto, 'types'>[],
  designated: readonly WardrobeSlotType[] = [],
): WardrobeSlotType[] {
  const set = new Set<WardrobeSlotType>([...unionTypes(components), ...designated]);
  return WARDROBE_SLOT_TYPES.filter((s) => set.has(s));
}
