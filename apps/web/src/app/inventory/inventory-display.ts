/**
 * What an object is called and whether it can be carried — the client-safe
 * half of v4 `lib/inventory/resolve.ts:114-157` (`322a5488e`). Pinned row
 * for row against v4's REAL functions by `inventory-display.oracle.spec.ts`.
 *
 * @module inventory/inventory-display
 */

import type { InventoryItemDto, KitState } from '../core/core-contract';

/** The display name: `Excalibur (sword)` when both differ, else the one there is. */
export function inventoryDisplayName(item: Pick<InventoryItemDto, 'name' | 'title'>): string {
  const title = item.title?.trim();
  if (title && title.toLowerCase() !== item.name.trim().toLowerCase()) return `${title} (${item.name})`;
  return title || item.name;
}

/** The short label: title, else name. */
export function inventoryLabel(item: Pick<InventoryItemDto, 'name' | 'title'>): string {
  return item.title?.trim() || item.name;
}

/** Kin share a resolved name, case-insensitively. */
export function isKin(a: Pick<InventoryItemDto, 'name'>, b: Pick<InventoryItemDto, 'name'>): boolean {
  return a.name.trim().toLowerCase() === b.name.trim().toLowerCase();
}

/** An object that can be carried: not an archetype, not a kit, not archived. */
export function isEquippable(item: Pick<InventoryItemDto, 'isArchetype' | 'kind' | 'archivedAt'>): boolean {
  return !item.isArchetype && item.kind !== 'kit' && !item.archivedAt;
}

/**
 * A kit's state in a chat: how many of its resolvable members are carried.
 * An archetype member counts as carried when an instance of it is.
 */
export function kitState(
  kit: Pick<InventoryItemDto, 'itemIds'>,
  equipped: readonly Pick<InventoryItemDto, 'id' | 'parentId'>[],
  resolvable: (id: string) => Pick<InventoryItemDto, 'isArchetype' | 'kind' | 'archivedAt'> | undefined,
): KitState {
  const ids = new Set(equipped.map((item) => item.id));
  const parents = new Set(equipped.map((item) => item.parentId).filter((id): id is string => !!id));
  let total = 0;
  let carried = 0;
  for (const memberId of kit.itemIds) {
    const member = resolvable(memberId);
    if (!member || member.kind === 'kit' || member.archivedAt) continue;
    total++;
    if (member.isArchetype ? parents.has(memberId) : ids.has(memberId)) carried++;
  }
  if (total === 0 || carried === 0) return 'none';
  return carried === total ? 'all' : 'some';
}
