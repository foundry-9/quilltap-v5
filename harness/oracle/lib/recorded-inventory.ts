/**
 * P4.D329 — the RECORDED-context idiom (the order's R-3).
 *
 * A row that reads an Inventory bag builds v4's REAL context
 * (`inventoryContextFrom` over literal resolved items — the exact shape v4's
 * own `custom-tools-inventory.test.ts` builds as `vivienne()`), wraps it in a
 * recording proxy, and emits every call's argument and answer in call order.
 * The v5 harness implements C2's `InventoryContext` trait by LOOKUP over that
 * table (`crates/quilltap-harness/tests/recorded_inventory/mod.rs`), so the
 * rows pin the deal, the render and the gate WITHOUT re-implementing
 * `resolveInventoryRef` (P4.D328's `inventory_refs_equivalence` pins that) —
 * and the recorded answers stay v4's on the union.
 *
 * A miss on the v5 side PANICS naming the method and argument: a render or a
 * trim difference between the two sides surfaces as a named miss, never as a
 * silent fallback.
 */

import { inventoryContextFrom, type InventoryContext } from '@/lib/inventory/refs';
import type { ResolvedInventoryItem } from '@/lib/schemas/inventory.types';

export interface InventoryCall {
  method: 'state' | 'equippedKinCount' | 'item' | 'scopeObjects';
  arg: string;
  answer: unknown;
}

export interface RecordedInventory {
  characterId: string;
  chatId: string | null;
  calls: InventoryCall[];
}

const T = '2026-10-18T00:00:00.000Z';

/** One resolved object (v4's own test's `obj`). */
export function obj(id: string, fields: Partial<ResolvedInventoryItem> = {}): ResolvedInventoryItem {
  return {
    id,
    kind: 'item',
    name: 'sword',
    title: null,
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
    ...fields,
  } as ResolvedInventoryItem;
}

/**
 * Vivienne (v4's own specimen): an equipped Excalibur (a sword, damage 2d6,
 * bonus 3), two equipped rings, a carried lantern, a reachable project rope —
 * plus this corpus's additions: a metadata list on the lantern and an untitled
 * whitespace title on a spare blade.
 */
export function vivienneItems(): { items: ResolvedInventoryItem[]; own: string[]; equipped: string[] } {
  const items = [
    obj('excalibur', {
      title: 'Excalibur',
      description: 'A blade of legend',
      metadata: { damage: '2d6', bonus: 3, edge: 1.5, tags: ['keen', 'old'], weight: 3 },
    }),
    obj('r1', { name: 'ring', metadata: { toolVersion: 2, toolAbilities: ['scry'] } }),
    obj('r2', { name: 'ring', metadata: { toolVersion: 5, toolAbilities: ['ward', 'scry'] } }),
    obj('lantern', { name: 'lantern', metadata: { lumens: 40, toolAbilities: 'light', notes: null } }),
    obj('rope', { name: 'rope', origin: { scope: 'project', id: 'p', name: 'Project' } as never }),
    obj('spare', { name: 'blade', title: '   ', description: null as never }),
  ];
  return {
    items,
    own: ['excalibur', 'r1', 'r2', 'lantern', 'spare'],
    equipped: ['excalibur', 'r1', 'r2'],
  };
}

/** v4's REAL context over literal items. */
export function contextOver(
  characterId: string,
  chatId: string | null,
  spec: { items: ResolvedInventoryItem[]; own: string[]; equipped: string[] },
): InventoryContext {
  const own = new Set(spec.own);
  return inventoryContextFrom(characterId, chatId, { items: spec.items, owns: (id) => own.has(id) }, new Set(spec.equipped));
}

/** Wrap a context so every answer is recorded. */
export function recording(ctx: InventoryContext): { ctx: InventoryContext; recorded: RecordedInventory } {
  const recorded: RecordedInventory = { characterId: ctx.characterId, chatId: ctx.chatId, calls: [] };
  const record = <T>(method: InventoryCall['method'], arg: string, answer: T): T => {
    recorded.calls.push({ method, arg, answer: JSON.parse(JSON.stringify(answer ?? null)) });
    return answer;
  };
  const proxy: InventoryContext = {
    characterId: ctx.characterId,
    chatId: ctx.chatId,
    state: (ref) => record('state', ref, ctx.state(ref)),
    equippedKinCount: (ref) => record('equippedKinCount', ref, ctx.equippedKinCount(ref)),
    item: (ref) => record('item', ref, ctx.item(ref)),
    scopeObjects: (scope) => record('scopeObjects', scope, ctx.scopeObjects(scope)),
  };
  return { ctx: proxy, recorded };
}

/** A fresh recorded Vivienne. */
export function recordedVivienne(): { ctx: InventoryContext; recorded: RecordedInventory } {
  return recording(contextOver('c-viv', 'chat-1', vivienneItems()));
}
