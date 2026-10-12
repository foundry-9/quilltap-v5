/**
 * The wardrobe folder calls — a port of v4 `lib/hooks/use-wardrobe-folders.ts`
 * (`4a38222e6`): the folder tree of one container, and the four folder
 * mutations, each against a container named per call (a row's home may differ
 * from the browsed container in a character's merged view).
 *
 * **Recorded mechanism divergences.**
 *  - The five calls ride C2's scoped dispatch verbs (`wardrobeFoldersList`,
 *    `wardrobeFolderCreate`, `wardrobeFolderRename`, `wardrobeFolderDelete`,
 *    `wardrobeItemsMove` — R-D, one family carrying the container) where v4
 *    POSTs `wardrobeCollectionActionUrl(container, action)`. A refusal arrives
 *    as a `CoreDispatchError` whose `message` is the server's sentence — the
 *    `result.error` v4's `fetchJson` throws with.
 *  - v4's TanStack cache (`queryKeys.wardrobe.folders(tierKey)`, invalidated
 *    with the lists in a mutation's `onSettled`) is a small signal store here,
 *    {@link createWardrobeFoldersStore} (R-J: the SPA's wardrobe lists stay
 *    signals with an explicit refresh). The browser and the picker each hold
 *    one for the container they show; whoever runs a mutation reloads the
 *    tree and the lists afterwards, success or failure — v4's `onSettled`.
 *  - Every function takes the `CoreClient` first, the v5 data-layer idiom
 *    (`item-images.api.ts`); C3 §2's names and remaining arguments are v4's.
 *
 * @module wardrobe/wardrobe-folders.api
 */

import { signal, type Signal } from '@angular/core';

import type { CoreClient } from '../core/core-client';
import type { WardrobeFolderDto, WardrobeItemDto } from '../core/core-contract';
import type { WardrobeContainer } from './wardrobe-container';

export type { WardrobeFolderDto } from '../core/core-contract';

/** The container half of every folder verb: `containerId` absent for General. */
function containerFields(container: WardrobeContainer): {
  scope: WardrobeContainer['scope'];
  containerId?: string;
} {
  return container.id
    ? { scope: container.scope, containerId: container.id }
    : { scope: container.scope };
}

/**
 * The folder tree of `container` (v4 `useWardrobeFolders`'s query function:
 * `data?.folders ?? []`), sorted by path as the server answers it.
 */
export async function listWardrobeFolders(
  core: CoreClient,
  container: WardrobeContainer,
): Promise<WardrobeFolderDto[]> {
  const data = await core.dispatchData({ type: 'wardrobeFoldersList', ...containerFields(container) });
  return (data['folders'] as WardrobeFolderDto[] | undefined) ?? [];
}

/** v4 `createFolder(container, path)` → `data.folder`. */
export async function createWardrobeFolder(
  core: CoreClient,
  container: WardrobeContainer,
  path: string,
): Promise<WardrobeFolderDto> {
  const data = await core.dispatchData({
    type: 'wardrobeFolderCreate',
    ...containerFields(container),
    path,
  });
  return data['folder'] as WardrobeFolderDto;
}

/** v4 `renameFolder(container, path, name)` → `data.folder`. */
export async function renameWardrobeFolder(
  core: CoreClient,
  container: WardrobeContainer,
  path: string,
  name: string,
): Promise<WardrobeFolderDto> {
  const data = await core.dispatchData({
    type: 'wardrobeFolderRename',
    ...containerFields(container),
    path,
    name,
  });
  return data['folder'] as WardrobeFolderDto;
}

/**
 * v4 `deleteFolder(container, path)` — both counts read through
 * `Number(… ?? 0)`, so an absent key reads as nothing moved.
 */
export async function deleteWardrobeFolder(
  core: CoreClient,
  container: WardrobeContainer,
  path: string,
): Promise<{ movedItems: number; movedFolders: number }> {
  const data = await core.dispatchData({
    type: 'wardrobeFolderDelete',
    ...containerFields(container),
    path,
  });
  return {
    movedItems: Number(data['movedItems'] ?? 0),
    movedFolders: Number(data['movedFolders'] ?? 0),
  };
}

/** v4 `moveItems(container, itemIds, folder)` → `data.items ?? []`. */
export async function moveWardrobeItemsToFolder(
  core: CoreClient,
  container: WardrobeContainer,
  itemIds: string[],
  folder: string,
): Promise<WardrobeItemDto[]> {
  const data = await core.dispatchData({
    type: 'wardrobeItemsMove',
    ...containerFields(container),
    itemIds,
    folder,
  });
  return (data['items'] as WardrobeItemDto[] | undefined) ?? [];
}

/** One container's folder tree, held as signals (v4 `useWardrobeFolders`). */
export interface WardrobeFoldersStore {
  /** The tree; empty until the read lands, and empty when it fails. */
  readonly folders: Signal<readonly WardrobeFolderDto[]>;
  /**
   * v4's `isLoading`: true only while the FIRST read of the current container
   * is in flight — a refresh keeps the tree on display.
   */
  readonly loading: Signal<boolean>;
  /**
   * Read `container`'s tree (null clears it — v4's `enabled: container !==
   * null`). A late answer for a container no longer asked about is dropped.
   */
  load(container: WardrobeContainer | null): Promise<void>;
  /** Re-read the container last asked about. */
  refresh(): Promise<void>;
}

const EMPTY: readonly WardrobeFolderDto[] = [];

/**
 * v4 `useWardrobeFolders(container)` as a signal store. A failed read warns
 * with v4's line and leaves the tree empty — the browser then shows only the
 * folders its items imply, as v4's `query.data ?? EMPTY` does.
 */
export function createWardrobeFoldersStore(core: CoreClient): WardrobeFoldersStore {
  const folders = signal<readonly WardrobeFolderDto[]>(EMPTY);
  const loading = signal(false);
  let current: WardrobeContainer | null = null;
  /** The container whose tree is on display ('' before the first answer). */
  let loadedKey = '';
  let seq = 0;

  const load = async (container: WardrobeContainer | null): Promise<void> => {
    current = container;
    const ticket = ++seq;
    if (!container) {
      folders.set(EMPTY);
      loading.set(false);
      loadedKey = '';
      return;
    }
    const key = `${container.scope}:${container.id ?? ''}`;
    if (key !== loadedKey) loading.set(true);
    try {
      const next = await listWardrobeFolders(core, container);
      if (ticket === seq) {
        folders.set(next);
        loadedKey = key;
      }
    } catch (error) {
      if (ticket === seq) {
        folders.set(EMPTY);
        loadedKey = key;
      }
      console.warn('[useWardrobeFolders] Failed to load wardrobe folders', {
        tierKey: key,
        error,
      });
    } finally {
      if (ticket === seq) loading.set(false);
    }
  };

  return {
    folders: folders.asReadonly(),
    loading: loading.asReadonly(),
    load,
    refresh: () => load(current),
  };
}
