/**
 * The wardrobe the Workbench picker specs browse — v4 `5cb0e0190`'s
 * `__tests__/unit/components/custom-tools/wardrobe-reference-picker.test.tsx:31-71`
 * fixture, transcribed: Imogen's own `Linen Shirt` (top), `Sundress` (top +
 * bottom), archived `Old Coat` (top) and the outfit `Garden Party` over `shirt`
 * + `boots`; Quilltap General's `Riding Boots` (footwear).
 *
 * v4 stubs `fetch` by URL; v5's reads are dispatch verbs, so
 * {@link wardrobeDispatchStub} answers them by request type instead. It records
 * every request and imports nothing from vitest, so it compiles in the app's
 * tsconfig too.
 */

import type { CoreRequest, WardrobeItemDto, WardrobeOrigin } from '../../core/core-contract';

export const STAMP = '2026-01-01T00:00:00.000Z';
export const IMOGEN = 'char-imogen';
export const OWN: WardrobeOrigin = { scope: 'character', id: IMOGEN, name: 'Imogen' };
export const GENERAL: WardrobeOrigin = { scope: 'general', id: null, name: 'Quilltap General' };

export function poolItem(
  id: string,
  title: string,
  types: WardrobeItemDto['types'],
  extra: Partial<WardrobeItemDto> = {},
): WardrobeItemDto {
  return {
    id,
    characterId: IMOGEN,
    title,
    types,
    componentItemIds: [],
    isDefault: false,
    replace: false,
    archivedAt: null,
    createdAt: STAMP,
    updatedAt: STAMP,
    ...extra,
  };
}

export const OWN_ITEMS: WardrobeItemDto[] = [
  { ...poolItem('shirt', 'Linen Shirt', ['top']), origin: OWN },
  { ...poolItem('dress', 'Sundress', ['top', 'bottom']), origin: OWN },
  { ...poolItem('coat', 'Old Coat', ['top'], { archivedAt: STAMP }), origin: OWN },
  {
    ...poolItem('ensemble', 'Garden Party', ['top', 'bottom'], {
      componentItemIds: ['shirt', 'boots'],
    }),
    origin: OWN,
  },
];

export const GENERAL_ITEMS: WardrobeItemDto[] = [
  { ...poolItem('boots', 'Riding Boots', ['footwear'], { characterId: null }), origin: GENERAL },
];

export interface WardrobeStubState {
  own: WardrobeItemDto[];
  general: WardrobeItemDto[];
  calls: CoreRequest[];
}

/** A `CoreClient`-shaped stub whose `dispatchData` answers the wardrobe reads. */
export function wardrobeDispatchStub(state: WardrobeStubState) {
  return {
    dispatchData: async (req: CoreRequest): Promise<Record<string, unknown>> => {
      state.calls.push(req);
      if (req.type === 'characterWardrobeList') {
        return { wardrobeItems: (req as { scope?: string }).scope === 'group' ? [] : state.own };
      }
      if (req.type === 'wardrobeList') return { wardrobeItems: state.general };
      return { wardrobeItems: [] };
    },
  };
}
