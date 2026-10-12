/**
 * Oracle case (P4.D71 unit 1, re-aimed at `9ecebf986` by P4.D279): bundle
 * dissolution — the pure half of v4 `61574563` "wearing a bundled outfit breaks
 * it apart automatically", as it now lives in `lib/wardrobe/slot-ops.ts`.
 *
 * v4 `9ecebf986` deleted `dissolve-bundles.ts` and rebuilt it as `slot-ops.ts`
 * ("bundle" → "composite"); `wearItemIntoSlots` / `addItemToSlot` /
 * `wornBundlesFor` moved there from `outfit-displacement.ts`, and
 * `replaceItemIntoSlots` went private behind `computeDisplacedSlots({ mode:
 * 'replace' })`. This case keeps the P4.D71 corpus and row kinds so the v5
 * `dissolve_bundles` SHIM (the old names over `slot_ops`) stays differentially
 * pinned until its callers are re-aimed and it is deleted:
 *
 *   - `isBundle` → `isComposite` (`@/lib/schemas/wardrobe.types`);
 *   - `slotsCoveredBy` is private at the pin (`slotsOf`), so the shape row
 *     carries `coveredSlots` DERIVED through the public surface: the slots
 *     `computeDisplacedSlots({ mode: 'replace', item })` without a lookup puts
 *     the item's id in, in canonical slot order (a set — `slotsOf`'s own order
 *     is the item's `types` order, which the derivation cannot see);
 *   - `dissolveBundleToLeaves` → `dissolveCompositeToLeaves`;
 *   - `dissolveBundlesInSlots` and `…WithCredit` → ONE
 *     `dissolveCompositesInSlots` returning `{ slots, wornBundles }`.
 *
 * Drives the REAL `lib/wardrobe/slot-ops.ts` plus `lib/wardrobe/
 * default-outfit.ts`'s `buildDefaultOutfitWithCredit` — no mocks, no
 * reimplementation.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx <v5>/harness/oracle/cases/dissolve-bundles.ts \
 *     > /tmp/oracle-dissolve-bundles.ndjson
 */

import {
  addItemToSlot,
  computeDisplacedSlots,
  dissolveCompositeToLeaves,
  dissolveCompositesInSlots,
  layLeavesIntoSlots,
  wearItemIntoSlots,
  wornBundlesFor,
} from '@/lib/wardrobe/slot-ops';
import { buildDefaultOutfitWithCredit } from '@/lib/wardrobe/default-outfit';
import { WARDROBE_SLOT_TYPES, isComposite } from '@/lib/schemas/wardrobe.types';
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
): EquippedSlots => ({ top, bottom, footwear, accessories, hair: [] });

// ── The item universe. Every case names its items out of this list, so the
// Rust side rebuilds the identical lookup from the emitted `items` array. ──

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
  makeItem('man-in-black', ['top', 'bottom', 'footwear', 'accessories'], [
    'shirt',
    'trousers',
    'boots',
    'gloves',
  ]),
  makeItem(
    'man-in-black-replacing',
    ['top', 'bottom', 'footwear', 'accessories'],
    ['shirt', 'trousers', 'boots', 'gloves'],
    { replace: true },
  ),
  // A replacing LEAF — the flag-driven rule on a plain garment.
  makeItem('cloak-replacing', ['top', 'accessories'], [], { replace: true }),
  // Types out of canonical order, with an unrecognized one in the middle.
  makeItem('odd-order', ['accessories', 'hat', 'top']),
  makeItem('jewelry', ['accessories'], ['locket', 'ring']),
  makeItem('gala', ['top', 'accessories'], ['gown', 'jewelry']),
  makeItem('top-only', ['top'], ['shirt', 'boots']),
  makeItem('top-and-bottom-one-part', ['top', 'bottom'], ['shirt']),
  makeItem('orphan', ['top'], ['nowhere-1', 'nowhere-2']),
  makeItem('selfish', ['top'], ['selfish', 'shirt']),
  makeItem('junk-bundle', ['top'], ['shirt', 'junk']),
  makeItem('kit', ['top'], ['shirt', 'hat']),
  makeItem('gown-bundle', ['accessories'], ['gown']),
  // A depth-5 chain: expansion is bounded at COMPOSITE_MAX_DEPTH (4), so the
  // deepest link truncates and surfaces as a leaf in its own right.
  makeItem('chain-1', ['top'], ['chain-2']),
  makeItem('chain-2', ['top'], ['chain-3']),
  makeItem('chain-3', ['top'], ['chain-4']),
  makeItem('chain-4', ['top'], ['chain-5']),
  makeItem('chain-5', ['top'], ['shirt']),
  // A mutual cycle between two bundles.
  makeItem('yin', ['top'], ['yang', 'shirt']),
  makeItem('yang', ['bottom'], ['yin', 'trousers']),
  // The ONLY shape that makes a bundle emit ITSELF as a leaf: a chain long
  // enough that the loop back to the bundle lands at COMPOSITE_MAX_DEPTH, where
  // expansion truncates and emits the id rather than recognizing the cycle. The
  // `leafId === item.id` echo guard is what stops the bundle wearing itself —
  // and the `selfish` case above cannot reach it (its self-reference is caught
  // one level in, as a cycle, and emits nothing).
  makeItem('loop-1', ['top'], ['loop-2']),
  makeItem('loop-2', ['top'], ['loop-3']),
  makeItem('loop-3', ['top'], ['loop-4']),
  makeItem('loop-4', ['top'], ['loop-5']),
  makeItem('loop-5', ['top'], ['loop-1']),
  // The same shape with a resolvable garment alongside, so the echo is dropped
  // while the real leaf survives (rather than collapsing to store-whole).
  makeItem('echo-1', ['top', 'footwear'], ['echo-2', 'boots']),
  makeItem('echo-2', ['top'], ['echo-3']),
  makeItem('echo-3', ['top'], ['echo-4']),
  makeItem('echo-4', ['top'], ['echo-5']),
  makeItem('echo-5', ['top'], ['echo-1']),
  // Two independent bundles, for the multi-bundle snapshot case.
  makeItem('feet-kit', ['footwear'], ['boots']),
];

const BY_ID = new Map(ITEMS.map((i) => [i.id, i]));
const item = (id: string): WardrobeItem => {
  const found = BY_ID.get(id);
  if (!found) throw new Error(`unknown fixture item: ${id}`);
  return found;
};

function emit(row: Record<string, unknown>): void {
  process.stdout.write(`${JSON.stringify(row)}\n`);
}

// A stable, machine-checkable marker so a stale oracle file cannot pass
// silently (`oracle-regen-silent-stale-pass`).
emit({ kind: 'meta', baseline: 'p4.d279-dissolve-bundles-shim', items: ITEMS });

// ── isComposite + the derived covered-slot set ──────────────────────────────

/** The slots a lookup-less `replace` puts the item's id in — `slotsOf` as a set. */
function coveredSlots(node: { id: string; types: readonly string[] }): WardrobeItemType[] {
  const out = computeDisplacedSlots(empty(), { mode: 'replace', item: node });
  return WARDROBE_SLOT_TYPES.filter((slot) => out[slot].includes(node.id));
}

for (const id of ITEMS.map((i) => i.id)) {
  emit({ kind: 'shape', id, coveredSlots: coveredSlots(item(id)), isBundle: isComposite(item(id)) });
}
// The structural shape v4's test asserts directly: a node with no
// `componentItemIds` key at all.
emit({
  kind: 'shape_bare',
  id: 'bare',
  node: { id: 'bare', types: ['top'] },
  coveredSlots: coveredSlots({ id: 'bare', types: ['top'] }),
  isBundle: isComposite({ id: 'bare', types: ['top'] } as never),
});

// ── dissolveCompositeToLeaves ───────────────────────────────────────────────

interface DissolveCase {
  id: string;
  itemId: string;
  /** ids visible to the lookup; null = no lookup supplied at all. */
  lookup: string[] | null;
}

const dissolveCases: DissolveCase[] = [
  { id: 'four-slot-bundle', itemId: 'man-in-black', lookup: ['shirt', 'trousers', 'boots', 'gloves', 'man-in-black'] },
  { id: 'nested-bundle', itemId: 'gala', lookup: ['jewelry', 'locket', 'ring', 'gala', 'gown'] },
  { id: 'plain-garment', itemId: 'shirt', lookup: ['shirt', 'trousers', 'boots', 'gloves', 'man-in-black'] },
  { id: 'no-lookup', itemId: 'man-in-black', lookup: null },
  { id: 'nothing-resolves', itemId: 'orphan', lookup: ['orphan'] },
  { id: 'self-referential', itemId: 'selfish', lookup: ['selfish', 'shirt'] },
  { id: 'unrecognized-slot-part', itemId: 'junk-bundle', lookup: ['shirt', 'junk', 'junk-bundle'] },
  { id: 'components-partly-missing', itemId: 'man-in-black', lookup: ['shirt', 'boots', 'man-in-black'] },
  { id: 'multi-slot-leaf', itemId: 'gown-bundle', lookup: ['gown', 'gown-bundle'] },
  {
    id: 'depth-truncated-chain',
    itemId: 'chain-1',
    lookup: ['chain-1', 'chain-2', 'chain-3', 'chain-4', 'chain-5', 'shirt'],
  },
  { id: 'mutual-cycle', itemId: 'yin', lookup: ['yin', 'yang', 'shirt', 'trousers'] },
  // The bundle emitted as its OWN leaf by depth truncation: the echo guard must
  // drop it. Without the guard this dissolves to `[{ id: 'loop-1' }]` and the
  // bundle wears itself.
  { id: 'self-echo-by-truncation', itemId: 'loop-1', lookup: ['loop-1', 'loop-2', 'loop-3', 'loop-4', 'loop-5'] },
  {
    id: 'self-echo-with-a-real-leaf',
    itemId: 'echo-1',
    lookup: ['echo-1', 'echo-2', 'echo-3', 'echo-4', 'echo-5', 'boots'],
  },
  { id: 'empty-lookup', itemId: 'man-in-black', lookup: [] },
  // A leaf whose own types are out of canonical order with junk in them.
  { id: 'odd-order-leaf', itemId: 'odd-kit', lookup: ['odd-order'] },
];

// `odd-kit` is not in ITEMS (it would perturb the shape rows' count); make it
// on the fly for the one dissolve case that needs a composite over `odd-order`.
const ODD_KIT = makeItem('odd-kit', ['top'], ['odd-order']);
const itemOrOdd = (id: string): WardrobeItem => (id === 'odd-kit' ? ODD_KIT : item(id));

const lookupOf = (ids: string[] | null): Map<string, WardrobeItem> | undefined =>
  ids === null ? undefined : new Map(ids.map((id) => [id, itemOrOdd(id)]));

for (const c of dissolveCases) {
  emit({
    kind: 'dissolve',
    id: c.id,
    itemId: c.itemId,
    ...(c.itemId === 'odd-kit' ? { node: ODD_KIT } : {}),
    lookup: c.lookup,
    out: dissolveCompositeToLeaves(itemOrOdd(c.itemId), lookupOf(c.lookup)),
  });
}

// ── layLeavesIntoSlots ──────────────────────────────────────────────────────

interface LayCase {
  id: string;
  bundleId: string;
  leaves: Array<{ id: string; slots: WardrobeItemType[] }>;
  current: EquippedSlots;
  clearCoveredSlots: boolean;
}

const SHIRT_BOOTS: Array<{ id: string; slots: WardrobeItemType[] }> = [
  { id: 'shirt', slots: ['top'] },
  { id: 'boots', slots: ['footwear'] },
];

const layCases: LayCase[] = [
  {
    id: 'layer-over-worn',
    bundleId: 'man-in-black',
    leaves: SHIRT_BOOTS,
    current: slots(['vest'], [], ['sandals'], []),
    clearCoveredSlots: false,
  },
  {
    id: 'replace-clears-landing-slots',
    bundleId: 'man-in-black',
    leaves: SHIRT_BOOTS,
    current: slots(['vest'], ['skirt'], ['sandals'], ['hat']),
    clearCoveredSlots: true,
  },
  {
    id: 'replace-clears-union-beyond-the-bundle',
    bundleId: 'top-only',
    leaves: SHIRT_BOOTS,
    current: slots(['vest'], [], ['sandals'], []),
    clearCoveredSlots: true,
  },
  {
    id: 'no-duplicate-of-a-worn-part',
    bundleId: 'man-in-black',
    leaves: SHIRT_BOOTS,
    current: slots(['shirt'], [], [], []),
    clearCoveredSlots: false,
  },
  {
    id: 'replace-with-a-part-already-worn',
    bundleId: 'man-in-black',
    leaves: SHIRT_BOOTS,
    current: slots(['shirt', 'vest'], [], ['boots'], []),
    clearCoveredSlots: true,
  },
  {
    id: 'multi-slot-leaf',
    bundleId: 'gown-bundle',
    leaves: [{ id: 'gown', slots: ['top', 'bottom'] }],
    current: slots(['vest'], ['skirt'], [], ['hat']),
    clearCoveredSlots: true,
  },
  {
    id: 'no-leaves-clearing',
    bundleId: 'man-in-black',
    leaves: [],
    current: slots(['vest'], ['skirt'], ['sandals'], ['hat']),
    clearCoveredSlots: true,
  },
  // The composite's own `types` carry an unrecognized name: `slotsOf` drops
  // it from the cleared union.
  {
    id: 'odd-order-composite-clearing',
    bundleId: 'odd-order',
    leaves: SHIRT_BOOTS,
    current: slots(['vest'], ['skirt'], ['sandals'], ['hat']),
    clearCoveredSlots: true,
  },
];

for (const c of layCases) {
  emit({
    kind: 'lay',
    id: c.id,
    bundleId: c.bundleId,
    leaves: c.leaves,
    current: c.current,
    clearCoveredSlots: c.clearCoveredSlots,
    out: layLeavesIntoSlots(c.current, item(c.bundleId), c.leaves, {
      clearCoveredSlots: c.clearCoveredSlots,
    }),
  });
}

// ── the pure wear primitives ────────────────────────────────────────────────

interface WearCase {
  id: string;
  mode: 'wear' | 'replace' | 'add_to_slot';
  itemId: string;
  current: EquippedSlots;
  lookup: string[] | null;
  slot?: WardrobeItemType;
}

const ALL = ITEMS.map((i) => i.id);

const wearCases: WearCase[] = [
  { id: 'wear-bundle-stores-parts', mode: 'wear', itemId: 'man-in-black', current: empty(), lookup: ALL },
  { id: 'wear-bundle-layers', mode: 'wear', itemId: 'man-in-black', current: slots(['vest']), lookup: ALL },
  {
    id: 'wear-bundle-replace-flag',
    mode: 'wear',
    itemId: 'man-in-black-replacing',
    current: slots(['vest'], ['skirt'], ['sandals'], ['hat']),
    lookup: ALL,
  },
  { id: 'wear-bundle-no-lookup', mode: 'wear', itemId: 'man-in-black', current: empty(), lookup: null },
  { id: 'wear-plain', mode: 'wear', itemId: 'shirt', current: empty(), lookup: ALL },
  {
    id: 'wear-replacing-leaf',
    mode: 'wear',
    itemId: 'cloak-replacing',
    current: slots(['vest', 'shirt'], ['skirt'], [], ['hat']),
    lookup: ALL,
  },
  { id: 'wear-odd-order-leaf', mode: 'wear', itemId: 'odd-order', current: slots(['vest']), lookup: ALL },
  { id: 'wear-unresolvable-bundle', mode: 'wear', itemId: 'orphan', current: empty(), lookup: ['orphan'] },
  { id: 'wear-nested-bundle', mode: 'wear', itemId: 'gala', current: empty(), lookup: ALL },
  { id: 'wear-self-echo-bundle', mode: 'wear', itemId: 'loop-1', current: empty(), lookup: ALL },
  { id: 'wear-self-echo-with-a-real-leaf', mode: 'wear', itemId: 'echo-1', current: empty(), lookup: ALL },
  { id: 'wear-multi-slot-leaf', mode: 'wear', itemId: 'gown-bundle', current: slots(['vest'], ['skirt']), lookup: ALL },
  {
    id: 'replace-bundle',
    mode: 'replace',
    itemId: 'man-in-black',
    current: slots(['vest', 'coat'], ['skirt'], [], []),
    lookup: ALL,
  },
  { id: 'replace-plain', mode: 'replace', itemId: 'shirt', current: slots(['vest', 'coat']), lookup: ALL },
  { id: 'replace-no-lookup', mode: 'replace', itemId: 'man-in-black', current: slots(['vest']), lookup: null },
  { id: 'replace-odd-order-leaf', mode: 'replace', itemId: 'odd-order', current: slots(['vest'], [], [], ['hat']), lookup: ALL },
  { id: 'add-bundle-part-covers-slot', mode: 'add_to_slot', itemId: 'man-in-black', current: empty(), lookup: ALL, slot: 'top' },
  {
    id: 'add-bundle-no-part-covers-slot',
    mode: 'add_to_slot',
    itemId: 'top-and-bottom-one-part',
    current: empty(),
    lookup: ALL,
    slot: 'bottom',
  },
  { id: 'add-plain', mode: 'add_to_slot', itemId: 'shirt', current: slots(['vest']), lookup: ALL, slot: 'top' },
  { id: 'add-already-present', mode: 'add_to_slot', itemId: 'shirt', current: slots(['shirt']), lookup: ALL, slot: 'top' },
  { id: 'add-no-lookup', mode: 'add_to_slot', itemId: 'man-in-black', current: empty(), lookup: null, slot: 'bottom' },
  {
    id: 'add-multi-part-slot',
    mode: 'add_to_slot',
    itemId: 'jewelry',
    current: slots([], [], [], ['hat']),
    lookup: ALL,
    slot: 'accessories',
  },
];

for (const c of wearCases) {
  const lookup = lookupOf(c.lookup);
  const target = item(c.itemId);
  let out: EquippedSlots;
  if (c.mode === 'wear') {
    out = wearItemIntoSlots(c.current, target, lookup);
  } else if (c.mode === 'replace') {
    // `replaceItemIntoSlots` is private at the pin — reached through the table.
    out = computeDisplacedSlots(c.current, { mode: 'replace', item: target, itemsById: lookup });
  } else {
    out = addItemToSlot(c.current, c.slot as WardrobeItemType, target, lookup);
  }
  emit({
    kind: 'wear',
    id: c.id,
    mode: c.mode,
    itemId: c.itemId,
    slot: c.slot ?? null,
    current: c.current,
    lookup: c.lookup,
    out,
  });
}

// ── dissolveCompositesInSlots (the old `dissolveBundlesInSlots` + `…WithCredit`) ──

interface SnapshotCase {
  id: string;
  current: EquippedSlots;
  lookup: string[];
}

const snapshotCases: SnapshotCase[] = [
  {
    id: 'substitute-in-place',
    current: slots(['undershirt', 'man-in-black', 'scarf'], ['man-in-black'], ['man-in-black'], ['man-in-black']),
    lookup: ALL,
  },
  { id: 'part-into-an-unclaimed-slot', current: slots(['kit']), lookup: ALL },
  { id: 'nothing-is-a-bundle', current: slots(['shirt'], ['trousers']), lookup: ALL },
  { id: 'unresolvable-bundle-stays', current: slots(['orphan']), lookup: ['orphan'] },
  { id: 'nested-bundle', current: slots(['gala']), lookup: ALL },
  { id: 'two-bundles', current: slots(['kit'], [], ['feet-kit'], []), lookup: ALL },
  {
    id: 'same-bundle-twice-in-one-slot',
    current: slots(['man-in-black', 'man-in-black']),
    lookup: ALL,
  },
  { id: 'empty-snapshot', current: empty(), lookup: ALL },
  { id: 'unknown-id-untouched', current: slots(['ghost']), lookup: ALL },
  { id: 'mutual-cycle', current: slots(['yin'], ['yang']), lookup: ALL },
  { id: 'self-echo-bundle', current: slots(['loop-1']), lookup: ALL },
  { id: 'self-echo-with-a-real-leaf', current: slots(['echo-1']), lookup: ALL },
  {
    id: 'leaf-already-worn-elsewhere',
    current: slots(['shirt', 'man-in-black']),
    lookup: ALL,
  },
];

for (const c of snapshotCases) {
  emit({
    kind: 'snapshot',
    id: c.id,
    current: c.current,
    lookup: c.lookup,
    out: dissolveCompositesInSlots(c.current, lookupOf(c.lookup) as never).slots,
  });
}

// ── P4.D262: the wear ledger's credit widenings (`3ee3b1342`) ──────────────

for (const c of snapshotCases) {
  emit({
    kind: 'snapshot_credit',
    id: c.id,
    current: c.current,
    lookup: c.lookup,
    out: dissolveCompositesInSlots(c.current, lookupOf(c.lookup) as never),
  });
}

interface WornBundlesForCase {
  id: string;
  itemId: string;
  /** ids visible to the lookup; null = no lookup supplied at all. */
  lookup: string[] | null;
  onlySlot?: string;
}

const wornBundlesForCases: WornBundlesForCase[] = [
  { id: 'four-slot-bundle', itemId: 'man-in-black', lookup: ALL },
  { id: 'four-slot-bundle-only-top', itemId: 'man-in-black', lookup: ALL, onlySlot: 'top' },
  { id: 'four-slot-bundle-only-hair', itemId: 'man-in-black', lookup: ALL, onlySlot: 'hair' },
  { id: 'nested-bundle', itemId: 'gala', lookup: ALL },
  { id: 'nested-bundle-only-accessories', itemId: 'gala', lookup: ALL, onlySlot: 'accessories' },
  { id: 'plain-garment', itemId: 'shirt', lookup: ALL },
  { id: 'no-lookup', itemId: 'man-in-black', lookup: null },
  { id: 'unresolvable', itemId: 'orphan', lookup: ALL },
  { id: 'part-into-unclaimed-slot', itemId: 'kit', lookup: ALL, onlySlot: 'accessories' },
  { id: 'multi-slot-leaf', itemId: 'gown-bundle', lookup: ALL, onlySlot: 'bottom' },
  { id: 'self-echo-with-a-real-leaf', itemId: 'echo-1', lookup: ALL },
  { id: 'mutual-cycle', itemId: 'yin', lookup: ALL },
];

for (const c of wornBundlesForCases) {
  emit({
    kind: 'worn_bundles_for',
    id: c.id,
    itemId: c.itemId,
    lookup: c.lookup,
    onlySlot: c.onlySlot ?? null,
    out: wornBundlesFor(
      item(c.itemId),
      c.lookup ? (lookupOf(c.lookup) as never) : undefined,
      c.onlySlot as WardrobeItemType | undefined,
    ),
  });
}

// `buildDefaultOutfitWithCredit` takes whole items (it reads `isDefault`,
// `archivedAt` and `createdAt`), so each case ships its own pool.
interface DefaultCase {
  id: string;
  pool: WardrobeItem[];
}

const dflt = (
  id: string,
  types: string[],
  componentItemIds: string[] = [],
  extra: Record<string, unknown> = {},
): WardrobeItem => makeItem(id, types, componentItemIds, { isDefault: true, ...extra });

const defaultCases: DefaultCase[] = [
  { id: 'no-defaults', pool: [item('shirt'), item('trousers')] },
  { id: 'leaf-defaults-layer-by-createdAt', pool: [
    dflt('coat', ['top'], [], { createdAt: '2026-03-01T00:00:00.000Z' }),
    dflt('vest', ['top'], [], { createdAt: '2026-02-01T00:00:00.000Z' }),
    dflt('boots-d', ['footwear']),
  ] },
  { id: 'default-bundle-dissolves', pool: [
    dflt('suit', ['top', 'bottom'], ['shirt', 'trousers']),
    item('shirt'),
    item('trousers'),
  ] },
  { id: 'two-default-bundles', pool: [
    dflt('suit', ['top', 'bottom'], ['shirt', 'trousers'], { createdAt: '2026-01-02T00:00:00.000Z' }),
    dflt('feet', ['footwear'], ['boots'], { createdAt: '2026-01-01T00:00:00.000Z' }),
    item('shirt'),
    item('trousers'),
    item('boots'),
  ] },
  { id: 'archived-default-skipped', pool: [
    dflt('suit', ['top', 'bottom'], ['shirt', 'trousers'], { archivedAt: '2026-01-05T00:00:00.000Z' }),
    dflt('hat-d', ['accessories']),
    item('shirt'),
    item('trousers'),
  ] },
  { id: 'unresolvable-default-bundle', pool: [dflt('orphan-d', ['top'], ['nowhere'])] },
  { id: 'default-bundle-and-its-default-leaf', pool: [
    dflt('suit', ['top', 'bottom'], ['shirt', 'trousers'], { createdAt: '2026-01-02T00:00:00.000Z' }),
    dflt('shirt-d', ['top'], [], { createdAt: '2026-01-01T00:00:00.000Z' }),
    item('shirt'),
    item('trousers'),
  ] },
  { id: 'missing-createdAt-sorts-last', pool: [
    dflt('late', ['top'], [], { createdAt: undefined }),
    dflt('early', ['top'], [], { createdAt: '2026-01-01T00:00:00.000Z' }),
  ] },
];

for (const c of defaultCases) {
  emit({ kind: 'default_credit', id: c.id, pool: c.pool, out: buildDefaultOutfitWithCredit(c.pool) });
}
