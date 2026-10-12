/**
 * Oracle case (P4.D279 keystone, C1 §6): `lib/wardrobe/slot-ops.ts` (v4
 * `9ecebf986`) — the pure math of putting things on and taking them off.
 *
 * Drives the REAL `slot-ops.ts` (`analyzeComposite`, `dissolveCompositeToLeaves`,
 * `layLeavesIntoSlots`, `dissolveCompositesInSlots` with and without `only`,
 * `wornBundlesFor`, `wearItemIntoSlots`, `addItemToSlot`,
 * `computeDisplacedSlots` × all five modes incl. the missing-item / missing-slot
 * fallbacks, `wornBundlesForMode` × all five modes) plus `isComposite` from
 * `lib/schemas/wardrobe.types.ts` — no mocks, no reimplementation.
 *
 * The corpus is v4's own `__tests__/unit/lib/wardrobe/slot-ops.test.ts`
 * case-for-case (the pure describes; the six `applyDisplacement (persisted)`
 * cases ride the tier-2 `outfit-displacement-tier2` family), plus the shapes
 * the test leaves implicit: every mode through the table with and without its
 * required `item` / `slot`, `wornBundlesForMode` per mode, `analyzeComposite`'s
 * `{cycles, truncated, unresolved}` on a cycle, a depth-truncated chain, an
 * orphan and a leaf, and `only` against an id that is not a composite.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx <v5>/harness/oracle/cases/slot-ops.ts > /tmp/oracle-slot-ops.ndjson
 */

import {
  addItemToSlot,
  analyzeComposite,
  computeDisplacedSlots,
  dissolveCompositeToLeaves,
  dissolveCompositesInSlots,
  layLeavesIntoSlots,
  wearItemIntoSlots,
  wornBundlesFor,
  wornBundlesForMode,
} from '@/lib/wardrobe/slot-ops';
import type { ComputeDisplacedOptions, DisplacementMode, WearableNode } from '@/lib/wardrobe/slot-ops';
import { isComposite } from '@/lib/schemas/wardrobe.types';
import type { EquippedSlots, WardrobeItem, WardrobeItemType } from '@/lib/schemas/wardrobe.types';

const NOW = '2026-01-01T00:00:00.000Z';

function makeItem(
  id: string,
  types: string[],
  componentItemIds: string[] = [],
  extra: Record<string, unknown> = {},
): WardrobeItem {
  return {
    id,
    characterId: 'c1c1c1c1-0000-0000-0000-000000000001',
    title: id,
    types,
    componentItemIds,
    isDefault: false,
    archivedAt: null,
    createdAt: NOW,
    updatedAt: NOW,
    ...extra,
  } as unknown as WardrobeItem;
}

const empty = (): EquippedSlots => ({ top: [], bottom: [], footwear: [], accessories: [], hair: [] });
const slots = (
  top: string[] = [],
  bottom: string[] = [],
  footwear: string[] = [],
  accessories: string[] = [],
  hair: string[] = [],
): EquippedSlots => ({ top, bottom, footwear, accessories, hair });

// ── The item universe (v4's test fixtures, by name). ────────────────────────

const ITEMS: WardrobeItem[] = [
  makeItem('shirt', ['top']),
  makeItem('trousers', ['bottom']),
  makeItem('boots', ['footwear']),
  makeItem('gloves', ['accessories']),
  makeItem('gown', ['top', 'bottom']),
  makeItem('locket', ['accessories']),
  makeItem('ring', ['accessories']),
  makeItem('hat', ['accessories']),
  makeItem('junk', ['hat']),
  makeItem('man-in-black', ['top', 'bottom', 'footwear', 'accessories'], ['shirt', 'trousers', 'boots', 'gloves']),
  makeItem(
    'man-in-black-replacing',
    ['top', 'bottom', 'footwear', 'accessories'],
    ['shirt', 'trousers', 'boots', 'gloves'],
    { replace: true },
  ),
  makeItem('jewelry', ['accessories'], ['locket', 'ring']),
  makeItem('gala', ['top', 'accessories'], ['gown', 'jewelry']),
  makeItem('top-only', ['top'], ['shirt', 'boots']),
  makeItem('top-only-bottom-too', ['top', 'bottom'], ['shirt']),
  makeItem('orphan', ['top'], ['nowhere-1', 'nowhere-2']),
  makeItem('selfish', ['top'], ['selfish', 'shirt']),
  makeItem('bundle-with-junk', ['top'], ['shirt', 'junk']),
  makeItem('kit', ['top'], ['shirt', 'hat']),
  makeItem('kit-accessories', ['accessories'], ['hat']),
  makeItem('chain-1', ['top'], ['chain-2']),
  makeItem('chain-2', ['top'], ['chain-3']),
  makeItem('chain-3', ['top'], ['chain-4']),
  makeItem('chain-4', ['top'], ['chain-5']),
  makeItem('chain-5', ['top'], ['shirt']),
  makeItem('yin', ['top'], ['yang', 'shirt']),
  makeItem('yang', ['bottom'], ['yin', 'trousers']),
];

const BY_ID = new Map(ITEMS.map((i) => [i.id, i]));
const item = (id: string): WardrobeItem => {
  const found = BY_ID.get(id);
  if (!found) throw new Error(`unknown fixture item: ${id}`);
  return found;
};
const ALL = ITEMS.map((i) => i.id);
const WARDROBE = ['shirt', 'trousers', 'boots', 'gloves', 'man-in-black'];
const lookupOf = (ids: string[] | null): Map<string, WearableNode> | undefined =>
  ids === null ? undefined : new Map(ids.map((id) => [id, item(id)]));

function emit(row: Record<string, unknown>): void {
  process.stdout.write(`${JSON.stringify(row)}\n`);
}

emit({ kind: 'meta', baseline: 'p4.d279-slot-ops', items: ITEMS });

// ── isComposite ─────────────────────────────────────────────────────────────

for (const id of ALL) emit({ kind: 'is_composite', id, out: isComposite(item(id)) });
emit({ kind: 'is_composite_bare', id: 'bare', node: { id: 'x', types: ['top'] }, out: isComposite({ id: 'x', types: ['top'] } as never) });

// ── analyzeComposite + dissolveCompositeToLeaves ────────────────────────────

const analyzeCases: Array<{ id: string; itemId: string; lookup: string[] | null }> = [
  { id: 'four-slot', itemId: 'man-in-black', lookup: WARDROBE },
  { id: 'nested', itemId: 'gala', lookup: ['jewelry', 'locket', 'ring', 'gala', 'gown'] },
  { id: 'plain-garment', itemId: 'shirt', lookup: WARDROBE },
  { id: 'no-lookup', itemId: 'man-in-black', lookup: null },
  { id: 'nothing-resolves', itemId: 'orphan', lookup: ['orphan'] },
  { id: 'self-referential', itemId: 'selfish', lookup: ['selfish', 'shirt'] },
  { id: 'unrecognized-slot-part', itemId: 'bundle-with-junk', lookup: ['shirt', 'junk', 'bundle-with-junk'] },
  { id: 'mutual-cycle', itemId: 'yin', lookup: ['yin', 'yang', 'shirt', 'trousers'] },
  { id: 'depth-truncated-chain', itemId: 'chain-1', lookup: ['chain-1', 'chain-2', 'chain-3', 'chain-4', 'chain-5', 'shirt'] },
  { id: 'empty-lookup', itemId: 'man-in-black', lookup: [] },
  { id: 'partly-missing', itemId: 'man-in-black', lookup: ['shirt', 'boots', 'man-in-black'] },
];

for (const c of analyzeCases) {
  emit({
    kind: 'analyze',
    id: c.id,
    itemId: c.itemId,
    lookup: c.lookup,
    out: analyzeComposite(item(c.itemId), lookupOf(c.lookup)),
    leaves: dissolveCompositeToLeaves(item(c.itemId), lookupOf(c.lookup)),
  });
}

// ── layLeavesIntoSlots ──────────────────────────────────────────────────────

const LEAVES: Array<{ id: string; slots: WardrobeItemType[] }> = [
  { id: 'shirt', slots: ['top'] },
  { id: 'boots', slots: ['footwear'] },
];

const layCases: Array<{ id: string; compositeId: string; current: EquippedSlots; clear: boolean }> = [
  { id: 'layers-when-not-replacing', compositeId: 'man-in-black', current: slots(['vest'], [], ['sandals']), clear: false },
  { id: 'clears-landing-slots', compositeId: 'man-in-black', current: slots(['vest'], ['skirt'], ['sandals'], ['hat']), clear: true },
  { id: 'clears-union-beyond-composite', compositeId: 'top-only', current: slots(['vest'], [], ['sandals']), clear: true },
  { id: 'no-duplicate', compositeId: 'man-in-black', current: slots(['shirt']), clear: false },
];

for (const c of layCases) {
  emit({
    kind: 'lay',
    id: c.id,
    compositeId: c.compositeId,
    leaves: LEAVES,
    current: c.current,
    clearCoveredSlots: c.clear,
    out: layLeavesIntoSlots(c.current, item(c.compositeId), LEAVES, { clearCoveredSlots: c.clear }),
  });
}

// ── wearItemIntoSlots / addItemToSlot (direct) ──────────────────────────────

const wearCases: Array<{ id: string; itemId: string; current: EquippedSlots; lookup: string[] | null }> = [
  { id: 'stores-parts', itemId: 'man-in-black', current: empty(), lookup: WARDROBE },
  { id: 'layers-parts', itemId: 'man-in-black', current: slots(['vest']), lookup: WARDROBE },
  { id: 'honours-replace', itemId: 'man-in-black-replacing', current: slots(['vest'], ['skirt'], ['sandals'], ['hat']), lookup: WARDROBE },
  { id: 'whole-without-lookup', itemId: 'man-in-black', current: empty(), lookup: null },
  { id: 'plain-garment', itemId: 'shirt', current: empty(), lookup: WARDROBE },
];

for (const c of wearCases) {
  emit({
    kind: 'wear',
    id: c.id,
    itemId: c.itemId,
    current: c.current,
    lookup: c.lookup,
    out: wearItemIntoSlots(c.current, item(c.itemId), lookupOf(c.lookup)),
  });
}

const addCases: Array<{ id: string; itemId: string; slot: WardrobeItemType; current: EquippedSlots; lookup: string[] | null }> = [
  { id: 'parts-covering-slot', itemId: 'man-in-black', slot: 'top', current: empty(), lookup: WARDROBE },
  { id: 'falls-back-to-composite-id', itemId: 'top-only-bottom-too', slot: 'bottom', current: empty(), lookup: ['shirt', 'top-only-bottom-too'] },
];

for (const c of addCases) {
  emit({
    kind: 'add',
    id: c.id,
    itemId: c.itemId,
    slot: c.slot,
    current: c.current,
    lookup: c.lookup,
    out: addItemToSlot(c.current, c.slot, item(c.itemId), lookupOf(c.lookup)),
  });
}

// ── wornBundlesFor ──────────────────────────────────────────────────────────

const wornCases: Array<{ id: string; itemId: string; lookup: string[] | null; onlySlot?: WardrobeItemType }> = [
  { id: 'composite-and-leaves', itemId: 'man-in-black', lookup: WARDROBE },
  { id: 'narrowed-to-footwear', itemId: 'man-in-black', lookup: WARDROBE, onlySlot: 'footwear' },
  { id: 'leaf-claims-nothing', itemId: 'shirt', lookup: WARDROBE },
  { id: 'no-lookup-claims-nothing', itemId: 'man-in-black', lookup: null },
  { id: 'narrowed-to-a-slot-nothing-covers', itemId: 'man-in-black', lookup: WARDROBE, onlySlot: 'hair' },
];

for (const c of wornCases) {
  emit({
    kind: 'worn_for',
    id: c.id,
    itemId: c.itemId,
    lookup: c.lookup,
    onlySlot: c.onlySlot ?? null,
    out: wornBundlesFor(item(c.itemId), lookupOf(c.lookup), c.onlySlot),
  });
}

// ── computeDisplacedSlots × every mode (+ wornBundlesForMode) ───────────────

interface ComputeCase {
  id: string;
  mode: DisplacementMode;
  itemId?: string;
  slot?: WardrobeItemType;
  /** v4's `itemId` filter for `remove_from_slot`. */
  removeId?: string;
  lookup: string[] | null;
  current: EquippedSlots;
}

const WORN = slots(['vest', 'shirt'], ['skirt'], [], [], []);

const computeCases: ComputeCase[] = [
  // v4's own `replace` + take-off cases.
  { id: 'replace-composite', mode: 'replace', itemId: 'man-in-black', lookup: WARDROBE, current: slots(['vest', 'coat'], ['skirt']) },
  { id: 'remove-one-id', mode: 'remove_from_slot', slot: 'top', removeId: 'vest', lookup: null, current: WORN },
  { id: 'remove-without-id-clears', mode: 'remove_from_slot', slot: 'top', lookup: null, current: WORN },
  { id: 'clear-slot', mode: 'clear_slot', slot: 'top', lookup: null, current: WORN },
  // The remaining modes through the table.
  { id: 'wear-composite', mode: 'wear', itemId: 'man-in-black', lookup: WARDROBE, current: slots(['vest']) },
  { id: 'wear-replacing-composite', mode: 'wear', itemId: 'man-in-black-replacing', lookup: WARDROBE, current: WORN },
  { id: 'wear-leaf', mode: 'wear', itemId: 'shirt', lookup: WARDROBE, current: slots(['vest']) },
  { id: 'wear-composite-no-lookup', mode: 'wear', itemId: 'man-in-black', lookup: null, current: empty() },
  { id: 'replace-leaf', mode: 'replace', itemId: 'shirt', lookup: WARDROBE, current: WORN },
  { id: 'add-composite-part', mode: 'add_to_slot', itemId: 'man-in-black', slot: 'footwear', lookup: WARDROBE, current: WORN },
  { id: 'add-leaf', mode: 'add_to_slot', itemId: 'boots', slot: 'footwear', lookup: WARDROBE, current: WORN },
  { id: 'add-composite-no-part-covers', mode: 'add_to_slot', itemId: 'top-only-bottom-too', slot: 'bottom', lookup: ALL, current: WORN },
  { id: 'remove-missing-id-no-op', mode: 'remove_from_slot', slot: 'top', removeId: 'ghost', lookup: null, current: WORN },
  { id: 'clear-empty-slot', mode: 'clear_slot', slot: 'footwear', lookup: null, current: WORN },
  // The fallbacks: a missing `item` / `slot` is a clone.
  { id: 'wear-without-item', mode: 'wear', lookup: WARDROBE, current: WORN },
  { id: 'replace-without-item', mode: 'replace', lookup: WARDROBE, current: WORN },
  { id: 'add-without-item', mode: 'add_to_slot', slot: 'top', lookup: WARDROBE, current: WORN },
  { id: 'add-without-slot', mode: 'add_to_slot', itemId: 'shirt', lookup: WARDROBE, current: WORN },
  { id: 'add-composite-without-slot', mode: 'add_to_slot', itemId: 'man-in-black', lookup: WARDROBE, current: WORN },
  { id: 'remove-without-slot', mode: 'remove_from_slot', removeId: 'vest', lookup: null, current: WORN },
  { id: 'clear-without-slot', mode: 'clear_slot', lookup: null, current: WORN },
  // Take-off modes with an item supplied: the item is ignored and credits nothing.
  { id: 'remove-with-item-ignored', mode: 'remove_from_slot', itemId: 'man-in-black', slot: 'top', removeId: 'vest', lookup: WARDROBE, current: WORN },
  { id: 'clear-with-item-ignored', mode: 'clear_slot', itemId: 'man-in-black', slot: 'top', lookup: WARDROBE, current: WORN },
];

for (const c of computeCases) {
  const options: ComputeDisplacedOptions = {
    mode: c.mode,
    ...(c.itemId ? { item: item(c.itemId) } : {}),
    ...(c.slot ? { slot: c.slot } : {}),
    ...(c.removeId ? { itemId: c.removeId } : {}),
    ...(c.lookup === null ? {} : { itemsById: lookupOf(c.lookup) }),
  };
  const before = JSON.stringify(c.current);
  const out = computeDisplacedSlots(c.current, options);
  emit({
    kind: 'compute',
    id: c.id,
    mode: c.mode,
    itemId: c.itemId ?? null,
    slot: c.slot ?? null,
    removeId: c.removeId ?? null,
    lookup: c.lookup,
    current: c.current,
    out,
    // The table never mutates its input.
    inputUntouched: JSON.stringify(c.current) === before,
    wornBundles: wornBundlesForMode(options),
  });
}

// ── dissolveCompositesInSlots (with and without `only`) ─────────────────────

interface InSlotsCase {
  id: string;
  current: EquippedSlots;
  lookup: string[];
  only?: string[];
}

const inSlotsCases: InSlotsCase[] = [
  {
    id: 'substitute-in-place',
    current: slots(['undershirt', 'man-in-black', 'scarf'], ['man-in-black'], ['man-in-black'], ['man-in-black']),
    lookup: WARDROBE,
  },
  { id: 'part-into-unoccupied-slot', current: slots(['kit']), lookup: ['shirt', 'hat', 'kit'] },
  { id: 'nothing-is-a-composite', current: slots(['shirt'], ['trousers']), lookup: WARDROBE },
  { id: 'unresolvable-stays', current: slots(['orphan']), lookup: ['orphan'] },
  { id: 'reports-dissolved', current: slots(['man-in-black']), lookup: WARDROBE },
  {
    id: 'only-the-named-composite',
    current: slots(['man-in-black'], [], [], ['kit-accessories']),
    lookup: [...WARDROBE, 'hat', 'kit-accessories'],
    only: ['kit-accessories'],
  },
  {
    id: 'only-naming-both',
    current: slots(['man-in-black'], [], [], ['kit-accessories']),
    lookup: [...WARDROBE, 'hat', 'kit-accessories'],
    only: ['kit-accessories', 'man-in-black'],
  },
  {
    id: 'only-naming-a-leaf-dissolves-nothing',
    current: slots(['man-in-black'], [], [], ['kit-accessories']),
    lookup: [...WARDROBE, 'hat', 'kit-accessories'],
    only: ['shirt'],
  },
  {
    id: 'only-empty-set-dissolves-nothing',
    current: slots(['man-in-black']),
    lookup: WARDROBE,
    only: [],
  },
  { id: 'nested', current: slots(['gala']), lookup: ALL },
  { id: 'mutual-cycle', current: slots(['yin'], ['yang']), lookup: ALL },
];

for (const c of inSlotsCases) {
  const result = dissolveCompositesInSlots(c.current, lookupOf(c.lookup) as never, c.only ? new Set(c.only) : undefined);
  emit({
    kind: 'in_slots',
    id: c.id,
    current: c.current,
    lookup: c.lookup,
    only: c.only ?? null,
    out: result,
    // v4 hands the INPUT object back when nothing dissolved.
    sameObject: result.slots === c.current,
  });
}
