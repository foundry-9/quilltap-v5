/**
 * One Inventory tier's folder tree, held as signals — v4 `useInventoryFolders`
 * (`lib/hooks/use-inventory.ts:115-131`, `322a5488e`) in the wardrobe's
 * `createWardrobeFoldersStore` idiom (R-1): empty until the read lands (for
 * each new container — the previous container's tree is never shown while the
 * next one loads), `loading` only for the FIRST read of a container, a late
 * answer for a container no longer asked about dropped. A failed read leaves
 * the tree empty — the browser then shows only the folders its objects imply,
 * as v4's `query.data ?? EMPTY_FOLDERS` does.
 *
 * @module inventory/inventory-folders-data
 */

import { signal, type Signal } from '@angular/core';

import type { CoreClient } from '../core/core-client';
import type { InventoryFolderDto } from '../core/core-contract';
import { inventoryTierKey, type InventoryContainer } from './inventory-container';
import { listInventoryFolders } from './inventory.api';

export interface InventoryFoldersStore {
  readonly folders: Signal<readonly InventoryFolderDto[]>;
  readonly loading: Signal<boolean>;
  /** Read `container`'s tree (null clears it — v4's `enabled: container !== null`). */
  load(container: InventoryContainer | null): Promise<void>;
  /** Re-read the container last asked about (v4's `folderLists` invalidation). */
  reload(): Promise<void>;
}

const EMPTY: readonly InventoryFolderDto[] = [];

export function createInventoryFoldersStore(core: CoreClient): InventoryFoldersStore {
  const folders = signal<readonly InventoryFolderDto[]>(EMPTY);
  const loading = signal(false);
  let current: InventoryContainer | null = null;
  let loadedKey = '';
  let seq = 0;

  const load = async (container: InventoryContainer | null): Promise<void> => {
    current = container;
    const ticket = ++seq;
    if (!container) {
      folders.set(EMPTY);
      loading.set(false);
      loadedKey = '';
      return;
    }
    const key = inventoryTierKey(container);
    if (key !== loadedKey) {
      loadedKey = '';
      folders.set(EMPTY);
      loading.set(true);
    }
    try {
      const next = await listInventoryFolders(core, container);
      if (ticket === seq) {
        folders.set(next);
        loadedKey = key;
      }
    } catch {
      if (ticket === seq) {
        folders.set(EMPTY);
        loadedKey = key;
      }
    } finally {
      if (ticket === seq) loading.set(false);
    }
  };

  return {
    folders: folders.asReadonly(),
    loading: loading.asReadonly(),
    load,
    reload: () => load(current),
  };
}
