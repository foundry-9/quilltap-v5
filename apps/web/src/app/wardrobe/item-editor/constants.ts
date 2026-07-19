/**
 * Shared constants and the candidate-grouping helper for the WardrobeItemEditor
 * component picker (v4 `components/wardrobe/wardrobe-item-editor/constants.ts`).
 */

import type { WardrobeSlotType } from '../../core/core-contract';
import type { CandidateGroup, CandidateItem } from './types';

/** v4 `constants.ts:9-15`. */
export const GROUP_LABEL: Record<CandidateGroup, string> = {
  top: 'Tops',
  bottom: 'Bottoms',
  footwear: 'Footwear',
  accessories: 'Accessories',
  multi: 'Multi-slot',
};

/** v4 `constants.ts:17`. */
export const GROUP_ORDER: CandidateGroup[] = ['top', 'bottom', 'footwear', 'accessories', 'multi'];

/** v4 `constants.ts:19-24` — the slot-color badge classes, shared across the
 *  wardrobe family's rows/cards/pickers. */
export const TYPE_BADGE_CLASS: Record<WardrobeSlotType, string> = {
  top: 'qt-badge-wardrobe-top',
  bottom: 'qt-badge-wardrobe-bottom',
  footwear: 'qt-badge-wardrobe-footwear',
  accessories: 'qt-badge-wardrobe-accessories',
};

/** v4 `constants.ts:26-29`. */
export function getCandidateGroup(c: CandidateItem): CandidateGroup {
  if (c.types.length > 1) return 'multi';
  return (c.types[0] as CandidateGroup) ?? 'multi';
}
