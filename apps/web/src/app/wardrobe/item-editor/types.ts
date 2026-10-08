/**
 * Shared types for the WardrobeItemEditor and its subcomponents
 * (v4 `components/wardrobe/wardrobe-item-editor/types.ts`).
 */

import type { WardrobeOrigin, WardrobeSlotType } from '../../core/core-contract';

/** A wardrobe item summary shape used by the components multi-select
 *  (v4 `types.ts:8-15`). */
export interface CandidateItem {
  id: string;
  title: string;
  types: WardrobeSlotType[];
  componentItemIds: string[];
  /**
   * Where a borrowed candidate hangs, as its collection read reported it.
   * Null for an item that lives in the wardrobe being edited (and for one that
   * arrived without an origin) — those get no chip. (v4 `cc80dc89d`,
   * `types.ts:13-19` — replaced `isShared`.)
   */
  origin: WardrobeOrigin | null;
}

/** v4 `types.ts:17` — every slot, plus the multi-slot catch-all. */
export type CandidateGroup = WardrobeSlotType | 'multi';
