/**
 * Shared types for the WardrobeItemEditor and its subcomponents
 * (v4 `components/wardrobe/wardrobe-item-editor/types.ts`).
 */

import type { WardrobeSlotType } from '../../core/core-contract';

/** A wardrobe item summary shape used by the components multi-select
 *  (v4 `types.ts:8-15`). */
export interface CandidateItem {
  id: string;
  title: string;
  types: WardrobeSlotType[];
  componentItemIds: string[];
  /** Whether this is a shared archetype (no characterId) */
  isShared: boolean;
}

/** v4 `types.ts:17`. */
export type CandidateGroup = 'top' | 'bottom' | 'footwear' | 'accessories' | 'multi';
