/**
 * The Proving Bench's Inventory lens — a mock bag an author types so `when`
 * rows and gates on `inventory` / `inventoryCount` can be proved without a
 * character (v4 `lib/pascal/bench-inventory.ts` at `322a5488e`; spec
 * inventory-tools-and-pascal §5.8).
 *
 * Each row names a reference, its state, optionally how many kin are
 * equipped (default: 1 when equipped, else 0), and optionally a `metadata`
 * object for the `inventoryAggregate` subject. A reference no row names reads
 * `absent` with a count of 0.
 *
 * For aggregates a row is an object named `ref` (its title is `ref` too)
 * carrying that metadata, placed by its state: `equipped` in all three
 * scopes, `carried` in `owned` and `reachable`, `reachable` in `reachable`
 * only, `absent` in none. An equipped row with `count: n` stands for n
 * copies, so `equipped.count` agrees with `inventoryCount`. The bench cannot
 * tell whether an object exists, so `item()` is always null:
 * `{{inventory.<ref>.title}}` and the other fields render as written, and
 * every `$inventory` operand falls back (R-U).
 *
 * CLIENT-SAFE: pure. v4's Zod schemas (`MockInventoryRowSchema`,
 * `MockInventorySchema`) are the SERVER's; the SPA has no Zod, and
 * `MockInventoryRow` comes from the contract.
 */

import type { MockInventoryRow } from '../screens/custom-tools/p4d330-stand-ins';
import type { AggregateObject, AggregateScope } from './inventory-aggregate';
import type { InventoryContext, InventoryState } from './inventory-refs';

/** A context over mock rows. Later rows for the same reference win. */
export function mockInventoryContext(rows: readonly MockInventoryRow[]): InventoryContext {
  const byRef = new Map<string, { ref: string; state: InventoryState; count: number; metadata: Record<string, unknown> }>();
  for (const row of rows) {
    const key = row.ref.trim().toLowerCase();
    if (!key) continue;
    byRef.set(key, {
      ref: row.ref.trim(),
      state: row.state,
      count: row.count ?? (row.state === 'equipped' ? 1 : 0),
      metadata: row.metadata ?? {},
    });
  }
  const lookup = (ref: string) => byRef.get(ref.trim().toLowerCase());
  const inScope: Record<AggregateScope, ReadonlySet<InventoryState>> = {
    equipped: new Set(['equipped']),
    owned: new Set(['equipped', 'carried']),
    reachable: new Set(['equipped', 'carried', 'reachable']),
  };
  return {
    characterId: 'bench',
    chatId: null,
    state: (ref) => lookup(ref)?.state ?? 'absent',
    equippedKinCount: (ref) => lookup(ref)?.count ?? 0,
    item: () => null,
    scopeObjects: (scope) => {
      const objects: AggregateObject[] = [];
      for (const row of byRef.values()) {
        if (!inScope[scope].has(row.state)) continue;
        const copies = row.state === 'equipped' ? row.count : 1;
        for (let i = 0; i < copies; i++) objects.push({ name: row.ref, title: row.ref, metadata: row.metadata });
      }
      return objects;
    },
  };
}
