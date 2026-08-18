/**
 * Shared constants and the candidate-grouping helper for the WardrobeItemEditor
 * component picker (v4 `components/wardrobe/wardrobe-item-editor/constants.ts`).
 */

import type { WardrobeSlotType } from '../../core/core-contract';
import { WARDROBE_SLOT_META, WARDROBE_SLOT_TYPES } from '../slot-meta';
import type { CandidateGroup, CandidateItem } from './types';

/** v4 `constants.ts:9-15` — the slot group headers, from the registry. */
export const GROUP_LABEL: Record<CandidateGroup, string> = {
  ...(Object.fromEntries(
    WARDROBE_SLOT_TYPES.map((slot) => [slot, WARDROBE_SLOT_META[slot].groupLabel]),
  ) as Record<WardrobeSlotType, string>),
  multi: 'Multi-slot',
};

/** Slot groups in canonical order; the multi-slot catch-all always sits last
 *  (v4 `constants.ts:17`). */
export const GROUP_ORDER: CandidateGroup[] = [...WARDROBE_SLOT_TYPES, 'multi'];

// v4's `TYPE_BADGE_CLASS` (and the `SLOT_LABEL` twin that lived in
// `equipped-slot-row.ts`) were deleted with `4423ad10`: badge classes and
// labels come from `WARDROBE_SLOT_META` now.

/** v4 `constants.ts:26-29`. */
export function getCandidateGroup(c: CandidateItem): CandidateGroup {
  if (c.types.length > 1) return 'multi';
  return (c.types[0] as CandidateGroup) ?? 'multi';
}
