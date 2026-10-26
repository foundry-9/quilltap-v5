/**
 * v4's Inventory UI fixture, verbatim — `__tests__/unit/components/inventory/
 * inventory-ui.test.tsx:28-63` at `322a5488e`: Vivienne's own tier, the
 * project "The Quest", and the five objects every transcribed case reads
 * (`SWORD` with a title, an SVG glyph and a picture; `RING` with an emoji and
 * equipped by default; `LAMP` plain; `BOW` a project archetype; `KIT` naming
 * the bow and the lamp). Test-only — imported by specs alone.
 */

import type { InventoryOrigin, ResolvedInventoryItem } from '../../core/core-contract';

export const T = '2026-10-18T00:00:00.000Z';
export const OWN: InventoryOrigin = { scope: 'character', id: 'c-viv', name: 'Vivienne' };
export const PROJECT: InventoryOrigin = { scope: 'project', id: 'p-1', name: 'The Quest' };

export function obj(id: string, fields: Partial<ResolvedInventoryItem> = {}): ResolvedInventoryItem {
  return {
    id,
    kind: 'item',
    name: id,
    title: null,
    description: null,
    imageDescription: null,
    equippedByDefault: false,
    maxEquipped: 1,
    canCopy: false,
    isArchetype: false,
    parentId: null,
    itemIds: [],
    metadata: {},
    folder: '',
    archivedAt: null,
    createdAt: T,
    updatedAt: T,
    origin: OWN,
    ...fields,
  };
}

export const SWORD = obj('sword', {
  title: 'Excalibur',
  svg: '<svg viewBox="0 0 1 1"><rect/></svg>',
  imageFileId: 'f0000000-0000-4000-8000-000000000001',
});
export const RING = obj('ring', { name: 'signet ring', emoji: '💍', equippedByDefault: true });
export const LAMP = obj('lamp', { name: 'lantern' });
export const BOW = obj('bow', { name: 'magic bow', isArchetype: true, origin: PROJECT, description: 'Never misses.' });
export const KIT = obj('kit', { name: "ranger's loadout", kind: 'kit', itemIds: ['bow', 'lamp'] });
