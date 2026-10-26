/**
 * What an Inventory browser lists for one tier, and who may manage what — a
 * port of v4 `components/inventory/hooks/useInventoryListData.ts` over
 * `lib/hooks/use-inventory.ts` `useInventoryList` (`322a5488e`):
 *
 *  - A character container lists the character's whole reachable pool (own
 *    vault, groups, the project tier when one is named, General), each object
 *    tagged with its tier (`shared`). Only the character's own objects — an
 *    origin-less row, or one whose origin is this character — are managed
 *    here; the rest are borrowed.
 *  - A shared container (General, a project, a group) lists exactly its own
 *    `Inventory/` folder, every row manageable.
 *
 * **Recorded mechanism divergence (R-1).** v4 reads through TanStack keyed by
 * `inventoryTierKey` and refreshes by invalidating `queryKeys.inventory.lists`;
 * the SPA keeps the list in signals with an explicit {@link InventoryListData.reload}
 * (the `wardrobe-list-data.ts` idiom). `loading` keeps v4's `isLoading`
 * meaning — true only while the FIRST read for the current view is in flight,
 * so a refresh after a row action repaints in place. A late answer for a view
 * no longer asked about is dropped, as a keyed query's would be.
 *
 * Construct inside an injection context (a component's constructor): the
 * reload-on-change is an `effect`, v4's hooks refiring on their deps.
 *
 * @module inventory/inventory-list-data
 */

import { computed, effect, signal, untracked, type Signal } from '@angular/core';

import type { CoreClient } from '../core/core-client';
import type { ResolvedInventoryItem } from '../core/core-contract';
import { inventoryTierKey, type InventoryContainer } from './inventory-container';
import { listInventory } from './inventory.api';

export interface InventoryListData {
  /** The rows to list (v4's `items`). */
  readonly listItems: Signal<ResolvedInventoryItem[]>;
  readonly loading: Signal<boolean>;
  /** The last read's failure, or null (v4's `error`). */
  readonly error: Signal<unknown>;
  /**
   * True when the object lives in the tier being browsed — one stable
   * function reading the current container (v4 `:29-36`).
   */
  readonly canManage: (item: ResolvedInventoryItem) => boolean;
  /** Re-read the current view (v4's `inventory.lists` invalidation). */
  reload(): Promise<void>;
}

export interface InventoryListDataOptions {
  container: () => InventoryContainer | null;
  includeArchived: () => boolean;
  /** The project tier for a character's pool. */
  projectId?: () => string | null;
}

const EMPTY: ResolvedInventoryItem[] = [];

export function createInventoryListData(core: CoreClient, opts: InventoryListDataOptions): InventoryListData {
  const items = signal<ResolvedInventoryItem[]>(EMPTY);
  const error = signal<unknown>(null);
  const inFlight = signal(false);
  /** The view key whose answer is on display; '' before the first answer. */
  const loadedKey = signal('');

  const viewKey = (container: InventoryContainer | null, includeArchived: boolean, projectId: string | null): string =>
    container
      ? `${inventoryTierKey(container, container.scope === 'character')}|${includeArchived}|${projectId ?? ''}`
      : '';
  const currentKey = () =>
    viewKey(opts.container(), opts.includeArchived(), opts.projectId?.() ?? null);

  let seq = 0;
  const reload = async (): Promise<void> => {
    const container = opts.container();
    const includeArchived = opts.includeArchived();
    const projectId = opts.projectId?.() ?? null;
    const key = viewKey(container, includeArchived, projectId);
    const ticket = ++seq;
    if (!container) {
      items.set(EMPTY);
      error.set(null);
      inFlight.set(false);
      loadedKey.set('');
      return;
    }
    inFlight.set(true);
    try {
      const next = await listInventory(core, container, {
        includeArchived,
        shared: container.scope === 'character',
        projectId,
      });
      if (ticket !== seq) return;
      items.set(next);
      error.set(null);
      loadedKey.set(key);
    } catch (err) {
      if (ticket !== seq) return;
      error.set(err);
      if (loadedKey() !== key) items.set(EMPTY);
      loadedKey.set(key);
    } finally {
      if (ticket === seq) inFlight.set(false);
    }
  };

  // A new container, archived opt-in or project tier is a NEW REQUEST, never
  // a client-side pass over what is loaded (v4's keyed query refires).
  effect(() => {
    opts.container();
    opts.includeArchived();
    opts.projectId?.();
    untracked(() => void reload());
  });

  const canManage = (item: ResolvedInventoryItem): boolean => {
    const container = opts.container();
    if (!container) return false;
    if (container.scope !== 'character') return true;
    return !item.origin || (item.origin.scope === 'character' && item.origin.id === container.id);
  };

  return {
    listItems: items.asReadonly(),
    loading: computed(() => inFlight() && loadedKey() !== currentKey()),
    error: error.asReadonly(),
    canManage,
    reload,
  };
}
