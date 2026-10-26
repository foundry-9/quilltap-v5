/**
 * Oracle case (P4.D324, the Inventory keystone — v4 `4fbdad186`): the
 * Inventory's Zod model `lib/schemas/inventory.types.ts`, pure.
 *
 * Drives v4's REAL schemas — the full object (`InventoryItemSchema`, strict),
 * the stored file (`InventoryItemFileSchema` — sparse, strip) and the full
 * file (`InventoryItemSchema.omit({folder})`, the reader's non-instance
 * parse), and the ten request-body schemas (`createInventorySchema`,
 * `updateInventorySchema`, `clearInventoryOverrideSchema`,
 * `instantiateInventorySchema`, `inventoryFolderPathBodySchema`,
 * `renameInventoryFolderSchema`, `fileInventoryItemsSchema`,
 * `chatInventoryEquipSchema`, `chatInventoryResetSchema`,
 * `inventoryTransferSchema`) — over a probe matrix (every bound at / past its
 * edge, unknown keys on strict vs strip, wrong types, `parentId` set,
 * `fields: []`, `ids` 0 / 1 / 200 / 201, `overrides.imageFileId` (not
 * pickable), a 901-char folder, an astral-plane name at 200 code points —
 * Zod 4.6.7 `.max()` counts code points), each emitted as `{ok, data,
 * issues}` (`JSON.stringify` of the parsed data — KEY ORDER included).
 * Plus `inventoryShapeIssues` over the shape combinations and the two key
 * lists. No mocks, no reimplementation.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx $V5W/harness/oracle/cases/inventory-types.ts \
 *     > /tmp/oracle-inv-types.ndjson
 */

import {
  INVENTORY_FILE_KEYS,
  INVENTORY_INHERITABLE_FIELDS,
  INVENTORY_KIT_MAX_MEMBERS,
  INVENTORY_SVG_MAX_BYTES,
  InventoryItemFileSchema,
  InventoryItemSchema,
  chatInventoryEquipSchema,
  chatInventoryResetSchema,
  clearInventoryOverrideSchema,
  createInventorySchema,
  fileInventoryItemsSchema,
  instantiateInventorySchema,
  inventoryFolderPathBodySchema,
  inventoryShapeIssues,
  inventoryTransferSchema,
  renameInventoryFolderSchema,
  updateInventorySchema,
} from '@/lib/schemas/inventory.types';

const BASELINE = 'p4.d324-inventory-types';

const SCHEMAS: Record<string, { safeParse(input: unknown): { success: boolean; data?: unknown; error?: { issues: unknown[] } } }> = {
  item: InventoryItemSchema,
  file: InventoryItemFileSchema,
  fullFile: InventoryItemSchema.omit({ folder: true }),
  create: createInventorySchema,
  update: updateInventorySchema,
  clearOverride: clearInventoryOverrideSchema,
  instantiate: instantiateInventorySchema,
  folderPath: inventoryFolderPathBodySchema,
  renameFolder: renameInventoryFolderSchema,
  fileItems: fileInventoryItemsSchema,
  chatEquip: chatInventoryEquipSchema,
  chatReset: chatInventoryResetSchema,
  transfer: inventoryTransferSchema,
};

function emit(row: Record<string, unknown>): void {
  process.stdout.write(`${JSON.stringify(row)}\n`);
}

const U1 = 'a1000000-0000-4000-8000-0000000000a1';
const U2 = 'b2000000-0000-4000-8000-0000000000b2';
const T0 = '2026-10-20T12:00:00.000Z';
const ids = (n: number) => Array.from({ length: n }, (_, i) => `c${String(i).padStart(7, '0')}-0000-4000-8000-000000000000`);
const astral200 = '🎩'.repeat(200);
const astral201 = '🎩'.repeat(201);

// A full object at every key, as a file (no folder).
const FULL = {
  id: U1, kind: 'item', name: 'Lantern', title: 'The Lamp', description: 'A brass lantern.',
  imageDescription: 'brass, glass', equippedByDefault: true, maxEquipped: 2, canCopy: true,
  isArchetype: false, parentId: null, itemIds: [], emoji: '🏮', svg: null, imageFileId: null,
  metadata: { weight: 1.5, tags: ['light'] }, archivedAt: null, createdAt: T0, updatedAt: T0,
};
const MIN_FILE = { id: U1, name: 'Lantern', createdAt: T0, updatedAt: T0 };
const INSTANCE = { id: U2, parentId: U1, title: null, createdAt: T0, updatedAt: T0 };

const probes: Array<[string, string, unknown]> = [
  // ── the full object / the files ──────────────────────────────────────────
  ['item-full-with-folder', 'item', { ...FULL, folder: 'Satchel' }],
  ['item-min', 'item', { ...MIN_FILE }],
  ['item-unknown-key', 'item', { ...MIN_FILE, foo: 1 }],
  ['item-bad-folder', 'item', { ...MIN_FILE, folder: 'images' }],
  ['item-bad-uuid', 'item', { ...MIN_FILE, id: 'nope' }],
  ['item-name-empty', 'item', { ...MIN_FILE, name: '' }],
  ['item-name-astral-200', 'item', { ...MIN_FILE, name: astral200 }],
  ['item-name-astral-201', 'item', { ...MIN_FILE, name: astral201 }],
  ['item-max-equipped-0', 'item', { ...MIN_FILE, maxEquipped: 0 }],
  ['item-max-equipped-1000', 'item', { ...MIN_FILE, maxEquipped: 1000 }],
  ['item-max-equipped-1.5', 'item', { ...MIN_FILE, maxEquipped: 1.5 }],
  ['item-kind-bad', 'item', { ...MIN_FILE, kind: 'box' }],
  ['item-svg-over', 'item', { ...MIN_FILE, svg: 'x'.repeat(INVENTORY_SVG_MAX_BYTES + 1) }],
  ['item-item-ids-201', 'item', { ...MIN_FILE, itemIds: ids(201) }],
  ['item-item-ids-bad', 'item', { ...MIN_FILE, itemIds: ['x', U2] }],
  ['item-metadata-array', 'item', { ...MIN_FILE, metadata: [1] }],
  ['item-timestamps-missing', 'item', { id: U1, name: 'x' }],
  ['item-timestamp-bad', 'item', { ...MIN_FILE, createdAt: 'yesterday' }],
  ['item-not-object', 'item', 'string'],
  ['item-max-equipped-1e300', 'item', { ...MIN_FILE, maxEquipped: 1e300 }],
  ['item-max-equipped-neg-1e300', 'item', { ...MIN_FILE, maxEquipped: -1e300 }],
  ['item-max-equipped-string', 'item', { ...MIN_FILE, maxEquipped: '2' }],
  ['item-folder-901', 'item', { ...MIN_FILE, folder: 'f'.repeat(901) }],
  ['item-folder-hand-made', 'item', { ...MIN_FILE, folder: 'Hats: Summer' }],
  ['item-metadata-numeric-keys', 'item', { ...MIN_FILE, metadata: JSON.parse('{"b":1,"2":2,"1":0,"a":{"9":1,"x":2}}') }],
  ['item-parent-uuid-bad', 'item', { ...MIN_FILE, parentId: 'x' }],
  ['full-file', 'fullFile', { ...FULL }],
  ['full-file-min', 'fullFile', { ...MIN_FILE }],
  ['full-file-folder-key', 'fullFile', { ...MIN_FILE, folder: 'x' }],
  ['full-file-reordered', 'fullFile', { updatedAt: T0, createdAt: T0, name: 'Z', id: U1, kind: 'kit', itemIds: [U2] }],
  ['full-file-null-title', 'fullFile', { ...MIN_FILE, title: null }],
  ['full-file-wrong-types', 'fullFile', { ...MIN_FILE, title: 5, equippedByDefault: 'yes', canCopy: null }],
  ['file-instance', 'file', { ...INSTANCE }],
  ['file-instance-overrides', 'file', { ...INSTANCE, description: 'mine', maxEquipped: 3, metadata: { a: 1 } }],
  ['file-instance-unknown', 'file', { ...INSTANCE, foo: 1, folder: 'x' }],
  ['file-instance-reordered', 'file', { updatedAt: T0, title: 'T', parentId: U1, createdAt: T0, id: U2 }],
  ['file-instance-no-id', 'file', { parentId: U1, createdAt: T0, updatedAt: T0 }],
  ['file-instance-null-kind', 'file', { ...INSTANCE, kind: null }],
  ['file-full', 'file', { ...FULL }],
  // ── create ───────────────────────────────────────────────────────────────
  ['create-min', 'create', { name: 'Lantern' }],
  ['create-full', 'create', { name: 'Lantern', title: 'The Lamp', description: 'd', imageDescription: 'i', equippedByDefault: true, maxEquipped: 3, canCopy: true, isArchetype: true, emoji: '🏮', svg: '<svg/>', metadata: { a: 1 }, folder: 'Satchel/Inner' }],
  ['create-strip', 'create', { name: 'Lantern', id: U1, createdAt: T0, archivedAt: T0, imageFileId: U2, foo: 1 }],
  ['create-parent', 'create', { name: 'x', parentId: U1 }],
  ['create-kit-empty', 'create', { name: 'Kit', kind: 'kit' }],
  ['create-kit', 'create', { name: 'Kit', kind: 'kit', itemIds: [U1, U2] }],
  ['create-archetype-kit', 'create', { name: 'x', kind: 'kit', isArchetype: true }],
  ['create-archetype-parent', 'create', { name: 'x', isArchetype: true, parentId: U1 }],
  ['create-item-members', 'create', { name: 'x', itemIds: [U1] }],
  ['create-no-name', 'create', {}],
  ['create-name-bad-type', 'create', { name: 5 }],
  ['create-name-201', 'create', { name: 'n'.repeat(201) }],
  ['create-title-201', 'create', { name: 'x', title: 't'.repeat(201) }],
  ['create-description-4001', 'create', { name: 'x', description: 'd'.repeat(4001) }],
  ['create-image-description-2001', 'create', { name: 'x', imageDescription: 'd'.repeat(2001) }],
  ['create-emoji-17', 'create', { name: 'x', emoji: 'e'.repeat(17) }],
  ['create-folder-900', 'create', { name: 'x', folder: 'f'.repeat(900) }],
  ['create-folder-901', 'create', { name: 'x', folder: 'f'.repeat(901) }],
  ['create-folder-images', 'create', { name: 'x', folder: 'images' }],
  ['create-multi-issue', 'create', { name: '', maxEquipped: 0, kind: 'box', itemIds: 'x' }],
  ['create-shape-and-field', 'create', { name: '', kind: 'kit' }],
  ['create-item-ids-bad-uuid', 'create', { name: 'x', itemIds: ['nope'] }],
  ['create-kit-bad-uuid', 'create', { name: 'x', kind: 'kit', itemIds: ['nope'] }],
  ['create-svg-not-string', 'create', { name: 'x', svg: 5 }],
  ['create-name-too-long-and-kit', 'create', { name: 'n'.repeat(201), kind: 'kit' }],
  ['create-parent-bad-uuid', 'create', { name: 'x', parentId: 'nope' }],
  ['create-null', 'create', null],
  ['create-array', 'create', [1]],
  // ── update ───────────────────────────────────────────────────────────────
  ['update-empty', 'update', {}],
  ['update-title-null', 'update', { title: null }],
  ['update-fields', 'update', { name: 'New', description: null, maxEquipped: 5, archived: true, imageFileId: U2, folder: 'x' }],
  ['update-image-null', 'update', { imageFileId: null }],
  ['update-image-bad', 'update', { imageFileId: 'nope' }],
  ['update-parent-stripped', 'update', { parentId: U1, id: U1, foo: 1 }],
  ['update-archived-bad', 'update', { archived: 'yes' }],
  ['update-name-empty', 'update', { name: '' }],
  ['update-kind-kit', 'update', { kind: 'kit', itemIds: [U1] }],
  ['update-reordered', 'update', { folder: 'x', name: 'n', archived: false, title: 't' }],
  ['update-metadata-null', 'update', { metadata: null }],
  // ── clearOverride ────────────────────────────────────────────────────────
  ['clear-one', 'clearOverride', { fields: ['title'] }],
  ['clear-all', 'clearOverride', { fields: [...INVENTORY_INHERITABLE_FIELDS] }],
  ['clear-empty', 'clearOverride', { fields: [] }],
  ['clear-bad', 'clearOverride', { fields: ['kind'] }],
  ['clear-missing', 'clearOverride', {}],
  ['clear-not-array', 'clearOverride', { fields: 'title' }],
  // ── instantiate ──────────────────────────────────────────────────────────
  ['inst-min', 'instantiate', { archetypeId: U1 }],
  ['inst-full', 'instantiate', { archetypeId: U1, folder: 'Satchel', projectId: 'p1', chatId: null, overrides: { title: 'Mine', description: null, maxEquipped: 2, metadata: { a: 1 } } }],
  ['inst-image-override', 'instantiate', { archetypeId: U1, overrides: { imageFileId: U2, foo: 1 } }],
  ['inst-bad-archetype', 'instantiate', { archetypeId: 'nope' }],
  ['inst-missing-archetype', 'instantiate', {}],
  ['inst-overrides-bad', 'instantiate', { archetypeId: U1, overrides: { name: '', maxEquipped: 0 } }],
  ['inst-overrides-null', 'instantiate', { archetypeId: U1, overrides: null }],
  ['inst-project-number', 'instantiate', { archetypeId: U1, projectId: 5 }],
  // ── folders ──────────────────────────────────────────────────────────────
  ['folder-ok', 'folderPath', { path: 'Satchel' }],
  ['folder-empty', 'folderPath', { path: '' }],
  ['folder-missing', 'folderPath', {}],
  ['folder-901', 'folderPath', { path: 'f'.repeat(901) }],
  ['folder-number', 'folderPath', { path: 5 }],
  ['rename-ok', 'renameFolder', { path: 'A', name: 'B' }],
  ['rename-empty-both', 'renameFolder', { path: '', name: '' }],
  ['rename-name-201', 'renameFolder', { path: 'A', name: 'n'.repeat(201) }],
  ['rename-missing', 'renameFolder', {}],
  ['file-items-ok', 'fileItems', { itemIds: [U1, 'x'], folder: '' }],
  ['file-items-empty', 'fileItems', { itemIds: [], folder: 'x' }],
  ['file-items-blank-id', 'fileItems', { itemIds: [''], folder: 'x' }],
  ['file-items-501', 'fileItems', { itemIds: Array.from({ length: 501 }, (_, i) => `i${i}`), folder: 'x' }],
  ['file-items-no-folder', 'fileItems', { itemIds: ['a'] }],
  // ── the chat actions ─────────────────────────────────────────────────────
  ['equip-ok', 'chatEquip', { characterId: 'k1', ids: [U1] }],
  ['equip-0', 'chatEquip', { characterId: 'k1', ids: [] }],
  ['equip-200', 'chatEquip', { characterId: 'k1', ids: ids(INVENTORY_KIT_MAX_MEMBERS) }],
  ['equip-201', 'chatEquip', { characterId: 'k1', ids: ids(201) }],
  ['equip-bad-id', 'chatEquip', { characterId: 'k1', ids: ['x'] }],
  ['equip-no-character', 'chatEquip', { characterId: '', ids: [U1] }],
  ['equip-missing', 'chatEquip', {}],
  ['reset-ok', 'chatReset', { characterId: 'k1' }],
  ['reset-empty', 'chatReset', { characterId: '' }],
  ['reset-missing', 'chatReset', { foo: 1 }],
  // ── transfer ─────────────────────────────────────────────────────────────
  ['transfer-ok', 'transfer', { action: 'move', itemId: U1, from: { scope: 'character', id: 'k1' }, to: { scope: 'general' } }],
  ['transfer-null-id', 'transfer', { action: 'copy', itemId: U1, from: { scope: 'project', id: null }, to: { scope: 'group', id: 'g1' } }],
  ['transfer-bad-action', 'transfer', { action: 'swap', itemId: U1, from: { scope: 'general' }, to: { scope: 'general' } }],
  ['transfer-bad-scope', 'transfer', { action: 'move', itemId: 'x', from: { scope: 'world' }, to: {} }],
  ['transfer-missing', 'transfer', {}],
  ['transfer-extra', 'transfer', { action: 'move', itemId: U1, from: { scope: 'general', x: 1 }, to: { scope: 'general' }, y: 2 }],
];

emit({
  kind: 'meta',
  baseline: BASELINE,
  fileKeys: [...INVENTORY_FILE_KEYS],
  inheritable: [...INVENTORY_INHERITABLE_FIELDS],
  svgMaxBytes: INVENTORY_SVG_MAX_BYTES,
  kitMaxMembers: INVENTORY_KIT_MAX_MEMBERS,
});

for (const [id, schema, input] of probes) {
  const r = SCHEMAS[schema].safeParse(input);
  emit({
    kind: 'parse',
    id,
    schema,
    input,
    ok: r.success,
    data: r.success ? r.data : null,
    issues: r.success ? [] : r.error!.issues,
  });
}

const shapes: Array<[string, Record<string, unknown>]> = [
  ['empty', {}],
  ['item-members', { itemIds: [U1] }],
  ['kit-ok', { kind: 'kit', itemIds: [U1] }],
  ['kit-empty', { kind: 'kit' }],
  ['kit-parent', { kind: 'kit', parentId: U1, itemIds: [U1] }],
  ['archetype-parent', { isArchetype: true, parentId: U1 }],
  ['archetype-kit', { isArchetype: true, kind: 'kit', itemIds: [U1] }],
  ['all-four', { isArchetype: true, kind: 'kit', parentId: U1 }],
  ['parent-empty-string', { parentId: '', isArchetype: true }],
  ['instance-ok', { parentId: U1 }],
];
for (const [id, item] of shapes) {
  emit({ kind: 'shape', id, item, issues: inventoryShapeIssues(item) });
}
