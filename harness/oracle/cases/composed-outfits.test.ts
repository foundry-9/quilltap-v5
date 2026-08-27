/**
 * Tier-1 oracle — v4's composed-outfit pool split (`aec86a613`).
 *
 * Imports v4's REAL `lib/wardrobe/composed-outfits.ts` (which in turn imports
 * the real `isBundle` from `lib/wardrobe/dissolve-bundles.ts`) and records what
 * `selectComposedOutfits` / `selectGarments` actually return over a fixed
 * corpus. Nothing here reimplements the split.
 *
 * WHY beyond the 1:1 transcription of v4's own unit test: the sort is
 * `a.title.localeCompare(b.title)`, whose ordering for accents, case, digits,
 * punctuation and combining marks is an ICU question a transcribed suite never
 * asks. The corpus asks it, so v5's twin is pinned to v4's ACTUAL collation
 * rather than to the seven titles v4's suite happened to use.
 *
 * The output is a JSON vector file consumed by
 * `apps/web/src/app/wardrobe/composed-outfits.spec.ts` — the SPA has no jest,
 * so the comparand is committed rather than diffed in Rust.
 *
 * Regenerate (from a v4 worktree PINNED at `aec86a613` — the drift commit IS
 * the spec here; drift-ledger §5.1. Node 24; jest ignores `/.claude/` paths so
 * the case is mirrored to /tmp first):
 *
 *   V5=~/source/quilltap-v5
 *   PIN=/tmp/qt-v4-pin-p4d130-aec86a613
 *   mkdir -p /tmp/qt-oracle-composed-outfits
 *   cp $V5/harness/oracle/cases/composed-outfits.test.ts /tmp/qt-oracle-composed-outfits/
 *   cd $PIN
 *   PATH=~/.nvm/versions/node/v24.13.1/bin:$PATH \
 *   QT_ORACLE_OUT=$V5/apps/web/src/app/wardrobe/__fixtures__/composed-outfits-vectors.json \
 *     npx jest --silent --roots "$PWD" --roots /tmp/qt-oracle-composed-outfits \
 *       -- "composed-outfits\.test\.ts$"
 *
 * Verify the pin: the module does not exist before `aec86a613`, so a run from
 * a baseline-pinned tree fails to resolve the import outright.
 *
 * @module harness/oracle/cases/composed-outfits
 */

import { writeFileSync } from 'fs'

import { describe, expect, it } from '@jest/globals'

import { selectComposedOutfits, selectGarments } from '@/lib/wardrobe/composed-outfits'
import type { WardrobeItem, WardrobeItemType } from '@/lib/schemas/wardrobe.types'

const NOW = '2026-01-01T00:00:00.000Z'

function makeItem(
  id: string,
  title: string,
  types: WardrobeItemType[],
  componentItemIds: string[] = [],
): WardrobeItem {
  return {
    id,
    characterId: 'char-1',
    title,
    types,
    componentItemIds,
    isDefault: false,
    replace: false,
    createdAt: NOW,
    updatedAt: NOW,
  }
}

interface CaseSpec {
  name: string
  items: Array<{
    id: string
    title: string
    types: WardrobeItemType[]
    componentItemIds: string[]
  }>
}

/**
 * The corpus. Every case names WHY it exists — a vector whose purpose is not
 * stated is a vector nobody can maintain.
 */
const CASES: CaseSpec[] = [
  {
    // The plain split: composites out, leaves (multi-slot ones included) in.
    name: 'mixed pool',
    items: [
      { id: 'dress', title: 'Green Dress', types: ['top', 'bottom'], componentItemIds: [] },
      { id: 'boots', title: 'Ankle Boots', types: ['footwear'], componentItemIds: [] },
      {
        id: 'sunday',
        title: 'Sunday Best',
        types: ['top', 'bottom', 'footwear'],
        componentItemIds: ['dress', 'boots'],
      },
      {
        id: 'jewelry',
        title: 'Aunt Dahlia’s Pearls',
        types: ['accessories'],
        componentItemIds: ['earrings'],
      },
    ],
  },
  {
    // Nothing composite at all — the pull-down's renders-nothing gate.
    name: 'no composites',
    items: [
      { id: 'a', title: 'Ankle Boots', types: ['footwear'], componentItemIds: [] },
      { id: 'b', title: 'Green Dress', types: ['top', 'bottom'], componentItemIds: [] },
    ],
  },
  {
    // An empty pool — the same gate, from the other side.
    name: 'empty pool',
    items: [],
  },
  {
    // localeCompare vs a byte sort: lowercase 'a' sorts BEFORE uppercase 'B'
    // under ICU and AFTER it under a code-unit compare. A `<`-based sort in the
    // twin would reverse these.
    name: 'collation: case is not code-unit order',
    items: [
      { id: 'c1', title: 'banded Cape', types: ['top'], componentItemIds: ['x'] },
      { id: 'c2', title: 'Banded Cape', types: ['top'], componentItemIds: ['x'] },
      { id: 'c3', title: 'Ashen Coat', types: ['top'], componentItemIds: ['x'] },
      { id: 'c4', title: 'apple Coat', types: ['top'], componentItemIds: ['x'] },
    ],
  },
  {
    // Accents and combining marks: 'é' collates next to 'e', not after 'z'.
    // NFC vs NFD spellings of the same word must land adjacent too.
    name: 'collation: accents and combining marks',
    items: [
      { id: 'd1', title: 'Zouave Jacket', types: ['top'], componentItemIds: ['x'] },
      { id: 'd2', title: 'Étoile Wrap', types: ['top'], componentItemIds: ['x'] },
      { id: 'd3', title: 'Etoile Wrap', types: ['top'], componentItemIds: ['x'] },
      { id: 'd4', title: 'Étoile Wrap', types: ['top'], componentItemIds: ['x'] },
    ],
  },
  {
    // Punctuation and digits: a leading quote/space/paren and numeric titles.
    // localeCompare is not a numeric sort — '10' precedes '9'.
    name: 'collation: punctuation and digits',
    items: [
      { id: 'e1', title: '9 Buttons', types: ['top'], componentItemIds: ['x'] },
      { id: 'e2', title: '10 Buttons', types: ['top'], componentItemIds: ['x'] },
      { id: 'e3', title: '“Quoted” Kit', types: ['top'], componentItemIds: ['x'] },
      { id: 'e4', title: ' Leading Space', types: ['top'], componentItemIds: ['x'] },
      { id: 'e5', title: '(Parenthetical)', types: ['top'], componentItemIds: ['x'] },
    ],
  },
  {
    // Ties: two composites with the SAME title. localeCompare returns 0 and
    // Array#sort is stable in modern V8, so input order survives — pinning it
    // stops a twin from "helpfully" adding an id tiebreak.
    name: 'collation: equal titles keep input order',
    items: [
      { id: 'f2', title: 'Same Name', types: ['top'], componentItemIds: ['x'] },
      { id: 'f1', title: 'Same Name', types: ['bottom'], componentItemIds: ['x'] },
      { id: 'f3', title: 'Same Name', types: ['hair'], componentItemIds: ['x'] },
    ],
  },
  {
    // Garment order is the CALLER's — deliberately reverse-alphabetical so a
    // twin that sorted both halves would show up here.
    name: 'garment order is the caller’s',
    items: [
      { id: 'g1', title: 'Zebra Stripes', types: ['top'], componentItemIds: [] },
      { id: 'g2', title: 'Alpine Tweeds', types: ['bottom'], componentItemIds: [] },
      { id: 'g3', title: 'Masquerade Kit', types: ['top'], componentItemIds: ['g1'] },
      { id: 'g4', title: 'Boater Hat', types: ['accessories'], componentItemIds: [] },
    ],
  },
  {
    // A single-slot composite — the pull-down is its only route on.
    name: 'single-slot composite',
    items: [
      { id: 'h1', title: 'Aunt Dahlia’s Pearls', types: ['accessories'], componentItemIds: ['h9'] },
    ],
  },
]

describe('composed-outfits oracle', () => {
  it('records v4’s real pool split over the corpus', () => {
    const out = CASES.map((c) => {
      const items = c.items.map((i) => makeItem(i.id, i.title, i.types, i.componentItemIds))
      return {
        name: c.name,
        items: c.items,
        composedOutfitIds: selectComposedOutfits(items).map((i) => i.id),
        garmentIds: selectGarments(items).map((i) => i.id),
      }
    })

    const dest = process.env['QT_ORACLE_OUT']
    if (dest) writeFileSync(dest, JSON.stringify(out, null, 2) + '\n')
    expect(out).toHaveLength(CASES.length)
  })
})
