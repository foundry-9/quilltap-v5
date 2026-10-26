/**
 * Oracle case (P4.D324, the Inventory keystone — v4 `4fbdad186`): resolution
 * `lib/inventory/resolve.ts` + `normalizeEquippedInventory`
 * (`chats.repository.ts`), pure.
 *
 * Drives v4's REAL `withInventoryDefaults`, `overriddenFieldsOf`,
 * `resolveItem`, `inventoryDisplayName`, `inventoryLabel`, `isKin`,
 * `isEquippable`, `kitState` and `normalizeEquippedInventory` over sparse /
 * explicit-null / missing-parent / non-archetype-parent / origin-tagged
 * records, the `kitState` matrix and the carried-map cells. Each row carries
 * its INPUT and v4's result `JSON.stringify`d (KEY ORDER is the contract — the
 * Rust side compares `serde_json::to_string` bytes).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx $V5W/harness/oracle/cases/inventory-resolve.ts \
 *     > /tmp/oracle-inv-resolve.ndjson
 */

import {
  inventoryDisplayName,
  inventoryLabel,
  isEquippable,
  isKin,
  kitState,
  overriddenFieldsOf,
  resolveItem,
  withInventoryDefaults,
} from '@/lib/inventory/resolve';
import { normalizeEquippedInventory } from '@/lib/database/repositories/chats.repository';

const BASELINE = 'p4.d324-inventory-resolve';

function emit(row: Record<string, unknown>): void {
  process.stdout.write(`${JSON.stringify(row)}\n`);
}

const TS = '2026-02-01T00:00:00.000Z';
const TS2 = '2026-02-02T00:00:00.000Z';
const ORIGIN_V = { scope: 'character', id: 'char-1', name: 'Vivienne' };
const ORIGIN_G = { scope: 'general', id: null, name: 'Quilltap General' };

const ARCHETYPE = {
  id: 'arch-sword',
  name: 'sword',
  title: 'Longsword',
  description: 'A plain steel longsword.',
  imageDescription: 'a steel longsword on a rack',
  kind: 'item',
  isArchetype: true,
  equippedByDefault: true,
  maxEquipped: 2,
  canCopy: true,
  emoji: '🗡️',
  svg: null,
  imageFileId: 'file-arch',
  metadata: { weight: 3, material: 'steel' },
  parentId: null,
  itemIds: [],
  folder: 'Weapons',
  archivedAt: null,
  createdAt: TS,
  updatedAt: TS,
  origin: ORIGIN_G,
};
const NOT_ARCHETYPE = { ...ARCHETYPE, id: 'plain-lamp', name: 'lamp', title: null, isArchetype: false };
const SPARSE_ARCHETYPE = { id: 'arch-sparse', name: 'cup', isArchetype: true, createdAt: TS, updatedAt: TS };

const records: Array<[string, Record<string, unknown>, Record<string, unknown> | null]> = [
  ['standalone-full', { ...NOT_ARCHETYPE, id: 'lamp-1' }, null],
  ['standalone-sparse', { id: 's-1', name: 'pebble', createdAt: TS, updatedAt: TS }, null],
  ['standalone-file-stem', { id: 's-2', fileStem: 'Old Name', createdAt: TS, updatedAt: TS }, null],
  ['standalone-nameless', { id: 's-3', createdAt: TS, updatedAt: TS }, null],
  ['standalone-null-name', { id: 's-4', name: null, folder: null, parentId: null, createdAt: TS }, null],
  ['standalone-origin', { id: 's-5', name: 'map', origin: ORIGIN_V, mountPointId: 'mp-1', createdAt: TS, updatedAt: TS }, null],
  ['kit', { id: 'kit-1', name: 'travel kit', kind: 'kit', itemIds: ['a', 'b'], createdAt: TS, updatedAt: TS }, null],
  ['archetype-itself', ARCHETYPE, null],
  ['instance-bare', { id: 'inst-1', parentId: 'arch-sword', createdAt: TS2, updatedAt: TS2 }, ARCHETYPE],
  [
    'instance-overrides',
    {
      id: 'inst-2',
      parentId: 'arch-sword',
      title: 'Excalibur',
      description: null,
      metadata: { material: 'starsteel', glow: true },
      folder: 'Mine',
      createdAt: TS2,
      updatedAt: TS2,
      origin: ORIGIN_V,
      mountPointId: 'mp-v',
    },
    ARCHETYPE,
  ],
  ['instance-explicit-nulls', { id: 'inst-3', parentId: 'arch-sword', title: null, emoji: null, svg: null, imageFileId: null, maxEquipped: null, createdAt: TS2 }, ARCHETYPE],
  ['instance-archived', { id: 'inst-4', parentId: 'arch-sword', archivedAt: TS2, createdAt: TS2, updatedAt: TS2 }, ARCHETYPE],
  ['instance-shape-keys-ignored', { id: 'inst-5', parentId: 'arch-sword', kind: 'kit', isArchetype: true, itemIds: ['x'], createdAt: TS2 }, ARCHETYPE],
  ['instance-missing-parent', { id: 'inst-6', parentId: 'gone', title: 'Orphan', createdAt: TS2, updatedAt: TS2, origin: ORIGIN_V }, null],
  ['instance-non-archetype-parent', { id: 'inst-7', parentId: 'plain-lamp', name: 'my lamp', createdAt: TS2 }, NOT_ARCHETYPE],
  ['instance-sparse-parent', { id: 'inst-8', parentId: 'arch-sparse', createdAt: TS2, updatedAt: TS2 }, SPARSE_ARCHETYPE],
  ['instance-parent-null-title', { id: 'inst-9', parentId: 'arch-sparse', title: 'Mug', createdAt: TS2 }, SPARSE_ARCHETYPE],
];

const names: Array<[string, Record<string, unknown>]> = [
  ['name-only', { name: 'sword' }],
  ['title-same-case', { name: 'Sword', title: 'sword' }],
  ['title-differs', { name: 'sword', title: 'Excalibur' }],
  ['title-padded', { name: ' sword ', title: '  Excalibur  ' }],
  ['title-blank', { name: 'sword', title: '   ' }],
  ['title-null', { name: 'sword', title: null }],
  ['title-same-padded', { name: ' sword', title: 'SWORD ' }],
  ['unicode-case', { name: 'éclair', title: 'ÉCLAIR' }],
  ['sharp-s', { name: 'strasse', title: 'STRASSE' }],
  ['dotted-i', { name: 'i̇stanbul', title: 'İstanbul' }],
];

const kin: Array<[string, string, string]> = [
  ['same', 'Lantern', 'Lantern'],
  ['case', 'Lantern', 'lantern '],
  ['padded', '  lantern', 'LANTERN'],
  ['different', 'lantern', 'lamp'],
  ['unicode', 'Éclair', 'éclair'],
  ['empty', '', ' '],
];

const equippable: Array<[string, Record<string, unknown>]> = [
  ['item', { isArchetype: false, kind: 'item', archivedAt: null }],
  ['archetype', { isArchetype: true, kind: 'item', archivedAt: null }],
  ['kit', { isArchetype: false, kind: 'kit', archivedAt: null }],
  ['archived', { isArchetype: false, kind: 'item', archivedAt: TS }],
  ['archived-empty-string', { isArchetype: false, kind: 'item', archivedAt: '' }],
];

const MEMBERS: Record<string, Record<string, unknown>> = {
  a: { isArchetype: false, kind: 'item', archivedAt: null },
  b: { isArchetype: false, kind: 'item', archivedAt: null },
  arch: { isArchetype: true, kind: 'item', archivedAt: null },
  subkit: { isArchetype: false, kind: 'kit', archivedAt: null },
  dead: { isArchetype: false, kind: 'item', archivedAt: TS },
};
const kits: Array<[string, string[], Array<{ id: string; parentId?: string | null }>]> = [
  ['none-carried', ['a', 'b'], []],
  ['some', ['a', 'b'], [{ id: 'a' }]],
  ['all', ['a', 'b'], [{ id: 'b' }, { id: 'a' }]],
  ['archetype-by-instance', ['a', 'arch'], [{ id: 'a' }, { id: 'x', parentId: 'arch' }]],
  ['archetype-not-by-id', ['arch'], [{ id: 'arch' }]],
  ['skips-missing-kit-archived', ['a', 'missing', 'subkit', 'dead'], [{ id: 'a' }]],
  ['only-skipped', ['missing', 'subkit', 'dead'], [{ id: 'dead' }]],
  ['empty-kit', [], [{ id: 'a' }]],
  ['duplicate-member', ['a', 'a', 'b'], [{ id: 'a' }]],
];

const carried: Array<[string, unknown]> = [
  ['object', { a: ['x', 'y'], b: [] }],
  ['string', '{"a":["x","y"]}'],
  ['junk-string', 'junk'],
  ['array', ['a']],
  ['array-string', '[]'],
  ['null', null],
  ['undefined', undefined],
  ['number', 5],
  ['non-array-dropped', { a: 'nope', b: ['z', 1, null, 'w'] }],
  ['string-non-array', '{"a":"nope","b":["z"]}'],
  ['duplicate-key-string', '{"a":["1"],"b":["2"],"a":["3"]}'],
  ['nested-object', { a: { x: 1 }, b: ['k'] }],
  ['empty-object', {}],
];

emit({ kind: 'meta', baseline: BASELINE });

for (const [id, record, parent] of records) {
  const lookup = (pid: string) => (parent && parent.id === pid ? (parent as never) : undefined);
  emit({ kind: 'defaults', id, record, out: JSON.stringify(withInventoryDefaults(record as never)) });
  emit({ kind: 'overridden', id, record, out: JSON.stringify(overriddenFieldsOf(record as never)) });
  emit({ kind: 'resolve', id, record, parent, out: JSON.stringify(resolveItem(record as never, lookup)) });
}
for (const [id, item] of names) {
  emit({
    kind: 'display',
    id,
    item,
    display: inventoryDisplayName(item as never),
    label: inventoryLabel(item as never),
  });
}
for (const [id, a, b] of kin) {
  emit({ kind: 'kin', id, a, b, out: isKin({ name: a }, { name: b }) });
}
for (const [id, item] of equippable) {
  emit({ kind: 'equippable', id, item, out: isEquippable(item as never) });
}
for (const [id, itemIds, equipped] of kits) {
  emit({
    kind: 'kit_state',
    id,
    itemIds,
    equipped,
    members: MEMBERS,
    out: kitState({ itemIds }, equipped as never, (m) => MEMBERS[m] as never),
  });
}
for (const [id, raw] of carried) {
  emit({
    kind: 'normalize',
    id,
    raw: raw === undefined ? { undefined: true } : { value: raw },
    out: JSON.stringify(normalizeEquippedInventory(raw)),
  });
}
