/**
 * The Inventory a Workbench vantage looks at — a port of v4
 * `lib/hooks/use-inventory-vantage-items.ts` (`e1872c94f`), the Inventory twin
 * of `wardrobe-vantage-items.ts` over the same vantage (`WardrobeVantage`).
 *
 * A character vantage reads that character's whole reachable pool (the
 * character tier's `shared` listing: own vault, groups, General — NO project
 * tier, the Workbench has no chat); a container vantage reads that one shared
 * tier. Archived objects are always included, flagged, so a reference to a
 * retired object still resolves and can say so. `owns` answers whether an
 * object lives in the vantage's own tier (v4's `pool.owns`: an origin-less
 * object counts as own).
 *
 * v5 shape: a small signal loader (the wardrobe loader's), consumed by
 * P4.D334's Workbench behind `P4D330_SPA_LANDED` (C10). It mints no query key.
 *
 * @module inventory/inventory-vantage-items
 */

import { computed, signal } from '@angular/core';

import type { CoreClient } from '../core/core-client';
import type { ResolvedInventoryItem } from '../core/core-contract';
import type { WardrobeVantage } from '../pascal/tool-draft';
import { encodeInventoryContainer, type InventoryContainer } from './inventory-container';
import { listInventory } from './inventory.api';

/** v4 `ResolvedInventoryPool` (`lib/inventory/refs.ts`) as the loader answers it. */
export interface InventoryVantagePool {
  items: ResolvedInventoryItem[];
  owns: (id: string) => boolean;
}

/** The vantage's container, or null for `none` / `null` (v4 `:37-42`). */
export function vantageContainer(vantage: WardrobeVantage | null): InventoryContainer | null {
  if (vantage?.kind === 'character') return { scope: 'character', id: vantage.characterId };
  if (vantage?.kind === 'container') return vantage.container;
  return null;
}

export class InventoryVantageItemsLoader {
  readonly items = signal<ResolvedInventoryItem[]>([]);
  readonly loading = signal(false);
  /** True once the read has settled; false for the `none` vantage. */
  readonly fetched = signal(false);
  private readonly scope = signal<InventoryContainer['scope'] | null>(null);

  /** v4 `:44-52` — own = the vantage's own tier (an origin-less object counts). */
  readonly pool = computed<InventoryVantagePool>(() => {
    const scope = this.scope();
    const items = this.items();
    const own = new Set(items.filter((item) => (item.origin ? item.origin.scope === scope : true)).map((i) => i.id));
    return { items, owns: (id) => own.has(id) };
  });

  private current = 'null';

  constructor(private readonly core: CoreClient) {}

  /** Load the vantage's Inventory. `null` (or the `none` vantage) reads nothing. */
  async load(vantage: WardrobeVantage | null): Promise<void> {
    const container = vantageContainer(vantage);
    const key = container ? encodeInventoryContainer(container) : 'null';
    this.current = key;
    this.scope.set(container?.scope ?? null);
    if (!container) {
      this.items.set([]);
      this.loading.set(false);
      this.fetched.set(false);
      return;
    }
    this.loading.set(true);
    this.fetched.set(false);
    let items: ResolvedInventoryItem[];
    try {
      items = await listInventory(this.core, container, {
        includeArchived: true,
        shared: vantage?.kind === 'character',
      });
    } catch {
      items = [];
    }
    // A slow read for an abandoned vantage never lands.
    if (this.current !== key) return;
    this.items.set(items);
    this.loading.set(false);
    this.fetched.set(true);
  }
}
