/**
 * Oracle case (P4.D279, the wardrobe keystone — v4 `4a38222e6`): the composite
 * type union and the four pure slot/composite helpers.
 *
 * Drives the REAL `@/lib/wardrobe/composite-types` (`unionTypes`,
 * `buildCompositeTypes`) and the REAL `@/lib/schemas/wardrobe.types`
 * (`isComposite`, `addIdToSlot`, `removeIdFromSlot`, `equippedSlotsEqual`,
 * `makeEmptyEquippedSlots`) — no mocks, no reimplementation.
 *
 * The corpus is the `buildCompositeTypes` describe of v4's
 * `__tests__/unit/lib/wardrobe/item-mutations.test.ts` case-for-case (the pin
 * carries no `composite-types.test.ts`; `unionTypes` has no unit suite of its
 * own there), plus the shapes those leave implicit: a WIDEN row where the
 * designated set is a subset of the components' union and one where it adds a
 * slot, empty components, duplicate ids through add/remove, unknown slot names
 * in `types` (v4 filters down to the canonical five), a missing `hair` key on
 * a four-slot bag (the pre-hair persisted shape), and order-sensitive equality.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx <v5>/harness/oracle/cases/composite-types.ts \
 *     > /tmp/oracle-composite-types.ndjson
 */

import { buildCompositeTypes, unionTypes } from '@/lib/wardrobe/composite-types';
import {
  addIdToSlot,
  equippedSlotsEqual,
  isComposite,
  makeEmptyEquippedSlots,
  removeIdFromSlot,
  WARDROBE_SLOT_TYPES,
} from '@/lib/schemas/wardrobe.types';
import type { EquippedSlots, WardrobeItemType } from '@/lib/schemas/wardrobe.types';

const BASELINE = 'p4.d279-composite-types';

function emit(row: Record<string, unknown>): void {
  process.stdout.write(`${JSON.stringify(row)}\n`);
}

/** A component as `unionTypes` reads it — `types` only; the strings are passed
 * through UNCHECKED so an unknown slot name reaches v4's filter as it would
 * from a hand-edited frontmatter. */
const comp = (types: string[]) => ({ types: types as WardrobeItemType[] });

emit({ kind: 'meta', baseline: BASELINE, slotTypes: [...WARDROBE_SLOT_TYPES] });

// ── unionTypes ──────────────────────────────────────────────────────────────
const unionCases: Array<[string, string[][]]> = [
  ['canonical-reorder', [['accessories', 'top'], ['bottom', 'hair']]],
  ['empty', []],
  ['one-empty-component', [[]]],
  ['duplicates-across', [['top'], ['top', 'bottom'], ['bottom']]],
  ['all-five-reversed', [['hair', 'accessories', 'footwear', 'bottom', 'top']]],
  ['unknown-slot-dropped', [['cape', 'top'], ['hat']]],
  ['duplicates-within', [['top', 'top', 'footwear', 'footwear']]],
  ['hair-only', [['hair']]],
];
for (const [id, components] of unionCases) {
  emit({ kind: 'union', id, components, out: unionTypes(components.map(comp)) });
}

// ── buildCompositeTypes ─────────────────────────────────────────────────────
const shirt = ['top'];
const trousers = ['bottom'];
const boots = ['footwear'];
const buildCases: Array<[string, string[][], string[] | undefined]> = [
  // item-mutations.test.ts — "unions the components with any designated extras, in canonical slot order"
  ['boots-shirt-plus-accessories', [boots, shirt], ['accessories']],
  // "never narrows below what was designated"
  ['shirt-designated-wider', [shirt], ['top', 'bottom', 'hair']],
  // WIDEN: designated ⊂ union — the components' slots all survive
  ['designated-subset', [shirt, trousers, boots], ['bottom']],
  ['designated-equals-union', [shirt, trousers], ['bottom', 'top']],
  // WIDEN: designated adds a slot the components lack
  ['designated-adds-hair', [shirt, trousers], ['hair']],
  ['empty-components-designated', [], ['accessories', 'top']],
  ['empty-both', [], []],
  ['designated-omitted', [boots, shirt], undefined],
  ['designated-duplicates', [shirt], ['top', 'top', 'footwear', 'footwear']],
  ['unknown-in-both', [['cape', 'top']], ['hat', 'hair']],
  ['empty-component-only', [[]], ['bottom']],
  ['all-five-from-pieces', [shirt, ['hair'], boots], ['accessories', 'bottom']],
];
for (const [id, components, designated] of buildCases) {
  emit({
    kind: 'build',
    id,
    components,
    designated: designated ?? null,
    out:
      designated === undefined
        ? buildCompositeTypes(components.map(comp))
        : buildCompositeTypes(components.map(comp), designated as WardrobeItemType[]),
  });
}

// ── isComposite ─────────────────────────────────────────────────────────────
const compositeCases: Array<[string, { componentItemIds?: readonly string[] | null }]> = [
  ['two-ids', { componentItemIds: ['a', 'b'] }],
  ['one-id', { componentItemIds: ['a'] }],
  ['empty', { componentItemIds: [] }],
  ['null', { componentItemIds: null }],
  ['undefined', { componentItemIds: undefined }],
  ['absent', {}],
];
for (const [id, item] of compositeCases) {
  emit({
    kind: 'is_composite',
    id,
    // Absent / null / undefined all serialise as null — v4 reads them alike
    // (`?.length ?? 0`), the Rust side as the empty list.
    componentItemIds: item.componentItemIds ?? null,
    out: isComposite(item),
  });
}

// ── the slot bags ───────────────────────────────────────────────────────────
function bag(partial: Partial<Record<WardrobeItemType, string[]>>): EquippedSlots {
  return { ...makeEmptyEquippedSlots(), ...partial };
}
const worn = bag({ top: ['t1', 't2'], bottom: ['b1'], footwear: ['f1'], hair: ['h1'] });
/** A four-slot bag persisted before the hair slot existed — no `hair` key at
 * all. `cloneEquippedSlots` tolerates it (`slots[s] ?? []`). */
const preHair = { top: ['t1'], bottom: [], footwear: ['f1'], accessories: ['a1'] } as unknown as EquippedSlots;

const addCases: Array<[string, EquippedSlots, WardrobeItemType, string]> = [
  ['append-top', worn, 'top', 't3'],
  ['already-there-noop', worn, 'top', 't1'],
  ['empty-slot', worn, 'accessories', 'a1'],
  ['fresh-bag', makeEmptyEquippedSlots(), 'hair', 'h9'],
  ['pre-hair-add-hair', preHair, 'hair', 'h1'],
  ['pre-hair-add-top', preHair, 'top', 't2'],
  ['same-id-other-slot', worn, 'bottom', 't1'],
];
for (const [id, slots, slot, itemId] of addCases) {
  emit({ kind: 'add', id, slots, slot, itemId, out: addIdToSlot(slots, slot, itemId) });
}

const dupBag = bag({ top: ['t1', 't2', 't1'], accessories: ['a1', 'a1'] });
const removeCases: Array<[string, EquippedSlots, WardrobeItemType, string | undefined]> = [
  ['one-of-two', worn, 'top', 't1'],
  ['last-in-slot', worn, 'bottom', 'b1'],
  ['absent-id-noop', worn, 'top', 'zz'],
  ['clear-slot', worn, 'top', undefined],
  ['clear-empty-slot', worn, 'accessories', undefined],
  ['duplicates-all-removed', dupBag, 'top', 't1'],
  ['duplicates-cleared', dupBag, 'accessories', undefined],
  ['pre-hair-clear-hair', preHair, 'hair', undefined],
  ['pre-hair-remove-footwear', preHair, 'footwear', 'f1'],
];
for (const [id, slots, slot, itemId] of removeCases) {
  emit({
    kind: 'remove',
    id,
    slots,
    slot,
    itemId: itemId ?? null,
    out: itemId === undefined ? removeIdFromSlot(slots, slot) : removeIdFromSlot(slots, slot, itemId),
  });
}

const equalCases: Array<[string, EquippedSlots, EquippedSlots]> = [
  ['same-ref', worn, worn],
  ['structural-copy', worn, bag({ top: ['t1', 't2'], bottom: ['b1'], footwear: ['f1'], hair: ['h1'] })],
  ['order-differs', worn, bag({ top: ['t2', 't1'], bottom: ['b1'], footwear: ['f1'], hair: ['h1'] })],
  ['one-missing', worn, bag({ top: ['t1'], bottom: ['b1'], footwear: ['f1'], hair: ['h1'] })],
  ['extra-accessory', worn, bag({ top: ['t1', 't2'], bottom: ['b1'], footwear: ['f1'], accessories: ['a1'], hair: ['h1'] })],
  ['both-empty', makeEmptyEquippedSlots(), makeEmptyEquippedSlots()],
  ['pre-hair-vs-explicit-empty-hair', preHair, bag({ top: ['t1'], footwear: ['f1'], accessories: ['a1'] })],
  ['pre-hair-vs-hair-worn', preHair, bag({ top: ['t1'], footwear: ['f1'], accessories: ['a1'], hair: ['h1'] })],
  ['duplicates-matter', dupBag, bag({ top: ['t1', 't2'], accessories: ['a1'] })],
];
for (const [id, a, b] of equalCases) {
  emit({ kind: 'equal', id, a, b, out: equippedSlotsEqual(a, b) });
}
