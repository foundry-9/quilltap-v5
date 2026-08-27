import { describe, expect, it } from 'vitest';

import type { WardrobeItemDto, WardrobeSlotType } from '../core/core-contract';
import { selectComposedOutfits, selectGarments } from './composed-outfits';
import vectors from './__fixtures__/composed-outfits-vectors.json';

/**
 * A 1:1 transcription of v4's own
 * `__tests__/unit/lib/wardrobe/composed-outfits.test.ts` (`aec86a613`, 69
 * lines) — the client-twin parity discipline: every v4 case, same names, same
 * expectations, same fixture items. `describe`/`it` names are v4's verbatim so
 * a future v4 edit diffs against this file by name.
 */

const NOW = '2026-01-01T00:00:00.000Z';

function makeItem(
  id: string,
  types: WardrobeSlotType[],
  componentItemIds: string[] = [],
  title?: string,
): WardrobeItemDto {
  return {
    id,
    characterId: 'char-1',
    title: title ?? id,
    types,
    componentItemIds,
    isDefault: false,
    replace: false,
    createdAt: NOW,
    updatedAt: NOW,
  };
}

const dress = makeItem('dress', ['top', 'bottom'], [], 'Green Dress');
const boots = makeItem('boots', ['footwear'], [], 'Ankle Boots');
const sunday = makeItem('sunday', ['top', 'bottom', 'footwear'], ['dress', 'boots'], 'Sunday Best');
const jewelry = makeItem('jewelry', ['accessories'], ['earrings'], 'Aunt Dahlia’s Pearls');

describe('selectComposedOutfits', () => {
  it('keeps composites and drops leaves', () => {
    const result = selectComposedOutfits([dress, boots, sunday, jewelry]);
    expect(result.map((i) => i.id)).toEqual(['jewelry', 'sunday']);
  });

  it('keeps a single-slot composite — the pull-down is its only way on', () => {
    expect(selectComposedOutfits([jewelry]).map((i) => i.id)).toEqual(['jewelry']);
  });

  it('does not treat a multi-slot leaf as an outfit', () => {
    expect(selectComposedOutfits([dress])).toEqual([]);
  });

  it('sorts by title', () => {
    const zebra = makeItem('zebra', ['top'], ['x'], 'Zebra Stripes');
    const alpha = makeItem('alpha', ['top'], ['y'], 'Alpine Tweeds');
    expect(selectComposedOutfits([zebra, alpha]).map((i) => i.title)).toEqual([
      'Alpine Tweeds',
      'Zebra Stripes',
    ]);
  });

  it('returns an empty list for a pool with no composites', () => {
    expect(selectComposedOutfits([dress, boots])).toEqual([]);
  });
});

describe('selectGarments', () => {
  it('keeps leaves — multi-slot ones included — and drops composites', () => {
    const result = selectGarments([dress, boots, sunday, jewelry]);
    expect(result.map((i) => i.id)).toEqual(['dress', 'boots']);
  });

  it('preserves the caller’s order', () => {
    expect(selectGarments([boots, dress]).map((i) => i.id)).toEqual(['boots', 'dress']);
  });
});

/**
 * Beyond the transcription: v5-side properties v4's suite cannot state,
 * because v4's `.filter().sort()` on a fresh array is idiomatic there and the
 * generic signature is v5's own. Both are load-bearing — the composer hands
 * these functions a signal's value, and mutating it in place would be a
 * cross-render defect no v4 test could see.
 */
describe('selectComposedOutfits (v5 twin properties)', () => {
  it('does not sort the caller’s array in place', () => {
    const zebra = makeItem('zebra', ['top'], ['x'], 'Zebra Stripes');
    const alpha = makeItem('alpha', ['top'], ['y'], 'Alpine Tweeds');
    const pool = [zebra, alpha];
    selectComposedOutfits(pool);
    expect(pool.map((i) => i.id)).toEqual(['zebra', 'alpha']);
  });
});

/**
 * The recorded differential. `composed-outfits-vectors.json` is the output of
 * v4's REAL `lib/wardrobe/composed-outfits.ts` over a fixed corpus, recorded
 * from a worktree pinned at `aec86a613` (recipe:
 * `harness/oracle/cases/composed-outfits.test.ts`). The transcription above
 * reproduces v4's seven assertions; these vectors reach what those seven
 * cannot — the ICU collation `localeCompare` actually applies. A `<`-based or
 * code-unit sort passes every transcribed case and fails these.
 */
describe('composed-outfits vs v4\u2019s recorded output (aec86a613)', () => {
  for (const v of vectors) {
    it(`matches v4 on: ${v.name}`, () => {
      const items = v.items.map((i) =>
        makeItem(i.id, i.types as WardrobeSlotType[], i.componentItemIds, i.title),
      );
      expect(selectComposedOutfits(items).map((i) => i.id)).toEqual(v.composedOutfitIds);
      expect(selectGarments(items).map((i) => i.id)).toEqual(v.garmentIds);
    });
  }

  it('the corpus is non-trivial \u2014 it discriminates ICU collation from a code-unit sort', () => {
    // A guard on the corpus itself, so a future regeneration that silently
    // dropped the collation cases cannot pass as coverage.
    const collation = vectors.filter((v) => v.name.startsWith('collation:'));
    expect(collation.length).toBeGreaterThanOrEqual(4);
    const caseCase = vectors.find((v) => v.name === 'collation: case is not code-unit order');
    // 'apple Coat' sorts BEFORE 'Ashen Coat' under ICU; a code-unit sort puts
    // every capital first, so this order is the discriminator.
    expect(caseCase?.composedOutfitIds).toEqual(['c4', 'c3', 'c1', 'c2']);
  });
});
