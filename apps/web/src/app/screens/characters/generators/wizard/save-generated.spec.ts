import { describe, expect, it } from 'vitest';

import type { CoreClient } from '../../../../core/core-client';
import { saveGeneratedWardrobeItems } from './save-generated';

/**
 * `characterWardrobeCreate` answers `{ wardrobeItem }` (server:
 * `characters.rs` `wrap_obj(out, "wardrobeItem")`; v4 `save-generated-
 * wardrobe.ts:47` reads `data?.wardrobeItem?.id`). The `p4.9k` round's §3
 * unification review found the helper reading `item`, so every composite was
 * created with `componentItemIds: []` — and the hosts' jsdom stubs answered
 * the same wrong key, which is how it stayed green. This pin drives the
 * helper over a stub that answers the REAL envelope and asserts the
 * component id actually reached the composite's create body.
 */
function stubClient(seen: Array<{ type: string; [k: string]: unknown }>): CoreClient {
  return {
    dispatchData: async (req: { type: string; [k: string]: unknown }) => {
      seen.push(req);
      const item = req['item'] as { title: string };
      return { wardrobeItem: { id: `w-${item.title}` } };
    },
  } as unknown as CoreClient;
}

describe('saveGeneratedWardrobeItems', () => {
  it('resolves a composite\'s component titles to the ids the API minted (leaf first)', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const result = await saveGeneratedWardrobeItems(stubClient(seen), 'c1', [
      { title: 'Evening Outfit', description: 'the whole ensemble', types: ['outfit'], components: ['Wool Coat'] },
      { title: 'Wool Coat', description: 'a coat', types: ['top'] },
    ]);

    expect(result).toEqual({ saved: 2, outfits: 1 });
    expect(seen.map((r) => (r['item'] as { title: string }).title)).toEqual(['Wool Coat', 'Evening Outfit']);
    const composite = seen[1]['item'] as { componentItemIds: string[] };
    expect(composite.componentItemIds).toEqual(['w-Wool Coat']);
  });
});
