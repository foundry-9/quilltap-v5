/**
 * The one folder-browser rule that is the wardrobe's own — which tier a
 * listed garment sits in. The generic rules are `item-browser/folder-view`.
 * A transcription of v4 `lib/wardrobe/folder-view.ts` at the pin `624d0d25c`
 * (33 lines: `b2b783b47` moved the view model to `lib/folder-browser/view.ts`
 * and `600c2906f` deleted the re-exports, so there are none here either).
 *
 * Client-safe: pure.
 *
 * @module wardrobe/folder-view
 */

import type { WardrobeItemDto } from '../core/core-contract';
import { OWN_TIER } from '../item-browser/folder-view';
import { containerForListedItem, encodeWardrobeContainer, type WardrobeOrigin } from './wardrobe-container';

type ViewItem = Pick<WardrobeItemDto, 'id' | 'characterId' | 'archivedAt'> & {
  folder?: string | null;
  origin?: WardrobeOrigin | null;
};

/**
 * The tier an item sits in from a view: `own` for a manageable item (or any
 * item outside a character's merged view), otherwise the wardrobe it was
 * borrowed from.
 */
export function viewTierOf(item: ViewItem, manageable: boolean, characterView: boolean): string {
  if (!characterView || manageable) return OWN_TIER;
  return encodeWardrobeContainer(containerForListedItem(item));
}
