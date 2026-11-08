import { computed, inject, signal, type Signal } from '@angular/core';

import { ToastService } from '../ui/toast.service';
import {
  OWN_TIER,
  childFolderRows,
  tierLabel,
  tierRows,
  type BrowserViewItem,
  type FolderRowView,
  type TierRowView,
} from './folder-view';
import { folderName, isSameFolder, isUnderFolder, joinFolder, parentFolder } from './item-folders';

/**
 * The state every folder-of-items browser keeps (the wardrobe's, the
 * Inventory's, the Scriptorium's), stated once — a port of v4
 * `components/item-browser/useFolderBrowserState.ts` at the pin `624d0d25c`
 * (NEW at `b2b783b47`): where in the tree the browser is (a tier — the
 * container's own items, or one borrowed tier — and a folder within it),
 * select mode and its selection, the inline new-folder draft, and what the
 * view shows there (`item-browser/folder-view`).
 *
 * v4's invariant, carried: "Each piece of view state remembers the container
 * (or view) it was set in and reads as its starting value anywhere else — so
 * a new container opens at its root with nothing selected, and leaving a
 * folder or toggling search drops the selection, without an effect to reset
 * them." Here each piece is a `{key, value}` signal read through a
 * `computed` — no `effect` resets anything (L-2).
 *
 * {@link createItemFolderHandlers} is the other half: New folder / Rename /
 * Delete over an injected {@link ItemFolderOps} port, with the toasts and the
 * delete confirmation worded by the caller.
 *
 * @module item-browser/folder-browser-state
 */

type Item = BrowserViewItem & { id: string };

/** A signal, or any getter (v5's inputs and computeds are both). */
type Read<T> = () => T;

export interface FolderBrowserStateOptions<T extends Item> {
  /** The browsed container's key; `''` when none is chosen. */
  containerKey: Read<string>;
  /** Every item in the view's pool, filters aside — what counts are judged by. */
  items: Read<readonly T[]>;
  /** The items the browser's own filters let through, in display order. */
  filtered: Read<readonly T[]>;
  /** The own tier's server folder tree (empty folders included). */
  tree: Read<ReadonlyArray<{ path: string; itemCount: number }>>;
  /** The view tier an item sits in: `OWN_TIER`, or a borrowed tier's key. */
  tierOf: Read<(item: T) => string>;
  /** A character's merged view offers borrowed tiers as tier rows. */
  characterView: Read<boolean>;
  /** "Show shared" — off, a borrowed tier reads as the own tier's root. */
  showShared: Read<boolean>;
  /** A search is active: it looks everywhere and folder rows step aside. */
  searching: Read<boolean>;
}

export interface FolderBrowserState<T extends Item> {
  /** A search is active (echoed from the options). */
  readonly searching: Signal<boolean>;
  readonly currentTier: Signal<string>;
  readonly currentFolder: Signal<string>;
  readonly inOwnTier: Signal<boolean>;
  /** Folders are managed only in the container being browsed, never a borrowed tier. */
  readonly managed: Signal<boolean>;
  navigate(tier: string, folder: string): void;
  /** The label of the borrowed tier being browsed; null in the own tier. */
  readonly currentTierLabel: Signal<string | null>;

  readonly selectMode: Signal<boolean>;
  /** Turn select mode on or off; either way the selection empties. */
  setSelectMode(on: boolean): void;
  readonly selectedIds: Signal<ReadonlySet<string>>;
  readonly selectedItems: Signal<T[]>;
  toggleSelected(item: T): void;
  clearSelection(): void;

  readonly newFolder: {
    readonly open: Signal<boolean>;
    readonly name: Signal<string>;
    setName(name: string): void;
    close(): void;
  };

  /** Everything in the tier being browsed, filters aside. */
  readonly tierItems: Signal<T[]>;
  readonly folderRows: Signal<FolderRowView[]>;
  readonly sharedTierRows: Signal<TierRowView[]>;
  readonly visible: Signal<T[]>;
  /** Nothing at all in this folder (as opposed to nothing matching the filters). */
  readonly folderIsEmpty: Signal<boolean>;
}

const EMPTY_SELECTION: ReadonlySet<string> = new Set();

/** v4 `useFolderBrowserState`. Pure signals — no injection context needed. */
export function createFolderBrowserState<T extends Item>(opts: FolderBrowserStateOptions<T>): FolderBrowserState<T> {
  // v4 seeds each piece with the FIRST render's key and a starting value (the
  // own root, select mode off, nothing selected). Seeding with null reads as
  // that same starting value in every container, so nothing here reads an
  // input before it is bound.
  const view = signal<{ key: string; tier: string; folder: string } | null>(null);
  const selectModeState = signal<{ key: string; on: boolean } | null>(null);
  const newFolderState = signal({ key: '', name: '' });

  const searching = computed(() => opts.searching());
  // Hiding shared tiers while inside one reads as the container's root.
  const viewIsCurrent = computed(() => {
    const v = view();
    return v !== null && v.key === opts.containerKey() && (opts.showShared() || v.tier === OWN_TIER);
  });
  const currentTier = computed(() => (viewIsCurrent() ? view()!.tier : OWN_TIER));
  const currentFolder = computed(() => (viewIsCurrent() ? view()!.folder : ''));
  const inOwnTier = computed(() => currentTier() === OWN_TIER);
  const managed = computed(() => inOwnTier() && opts.containerKey() !== '');
  const navigate = (tier: string, folder: string): void => view.set({ key: opts.containerKey(), tier, folder });

  const viewKey = computed(() => `${opts.containerKey()}|${currentTier()}|${currentFolder()}|${searching()}`);
  const selection = signal<{ key: string; ids: ReadonlySet<string> } | null>(null);
  const selectedIds = computed(() => {
    const s = selection();
    return s !== null && s.key === viewKey() ? s.ids : EMPTY_SELECTION;
  });
  const clearSelection = (): void => selection.set({ key: viewKey(), ids: new Set() });
  const selectMode = computed(() => {
    const s = selectModeState();
    return s !== null && s.key === opts.containerKey() && s.on;
  });
  const setSelectMode = (on: boolean): void => {
    selectModeState.set({ key: opts.containerKey(), on });
    selection.set({ key: viewKey(), ids: new Set() });
  };
  const toggleSelected = (item: T): void => {
    const key = viewKey();
    const prev = selection();
    const next = new Set(prev !== null && prev.key === key ? prev.ids : []);
    if (next.has(item.id)) next.delete(item.id);
    else next.add(item.id);
    selection.set({ key, ids: next });
  };
  const selectedItems = computed(() => {
    const ids = selectedIds();
    return opts.items().filter((i) => ids.has(i.id));
  });

  const newFolderOpen = computed(() => newFolderState().key === viewKey());
  const newFolder = {
    open: newFolderOpen,
    name: computed(() => (newFolderOpen() ? newFolderState().name : '')),
    setName: (name: string): void => newFolderState.set({ key: viewKey(), name }),
    close: (): void => newFolderState.set({ key: '', name: '' }),
  };

  const tierItems = computed(() => {
    const tierOf = opts.tierOf();
    const tier = currentTier();
    return opts.items().filter((i) => tierOf(i) === tier);
  });
  const folderRows = computed(() =>
    searching() ? [] : childFolderRows(currentFolder(), tierItems(), managed() ? opts.tree() : []),
  );
  const sharedTierRows = computed(() =>
    opts.characterView() && inOwnTier() && !searching() && currentFolder() === '' && opts.showShared()
      ? tierRows(opts.items(), opts.tierOf())
      : [],
  );
  const visible = computed(() => {
    const filtered = opts.filtered();
    if (searching()) return [...filtered];
    const tierOf = opts.tierOf();
    const tier = currentTier();
    const folder = currentFolder();
    return filtered.filter((i) => tierOf(i) === tier && isSameFolder(i.folder ?? '', folder));
  });
  const currentTierLabel = computed(() => {
    if (inOwnTier()) return null;
    const tierOf = opts.tierOf();
    const tier = currentTier();
    const sample = opts.items().find((i) => tierOf(i) === tier);
    return sample ? tierLabel(sample.origin) : 'Shared';
  });
  const folderIsEmpty = computed(
    () =>
      !searching() &&
      folderRows().length === 0 &&
      sharedTierRows().length === 0 &&
      !tierItems().some((i) => isSameFolder(i.folder ?? '', currentFolder())),
  );

  return {
    searching,
    currentTier,
    currentFolder,
    inOwnTier,
    managed,
    navigate,
    currentTierLabel,
    selectMode,
    setSelectMode,
    selectedIds,
    selectedItems,
    toggleSelected,
    clearSelection,
    newFolder,
    tierItems,
    folderRows,
    sharedTierRows,
    visible,
    folderIsEmpty,
  };
}

// ---- folder handlers -------------------------------------------------------

/** The folder writes a browser's own tier accepts. */
export interface ItemFolderOps {
  create: (path: string) => Promise<{ name: string }>;
  rename: (path: string, name: string) => Promise<{ name: string }>;
  remove: (path: string) => Promise<{ movedItems: number }>;
}

/** How a browser words its folder toasts and confirmation. */
export interface ItemFolderWording {
  /** The noun its items are counted in, e.g. `item`, `object`. */
  noun: string;
  /** Where a deleted folder's contents go at the top, e.g. `the top of the wardrobe`. */
  rootPhrase: string;
  created: (name: string) => string;
  /** The toast after a delete that moved `count` (already pluralised) up. */
  movedUp: (count: string) => string;
  /** Override the delete confirmation (the wardrobe mentions archived pieces). */
  deleteMessage?: (args: { name: string; count: number; parentLabel: string }) => string;
}

export interface ItemFolderHandlersOptions<T extends Item> {
  state: FolderBrowserState<T>;
  /** Null while no container is chosen — every handler is then a no-op. */
  ops: () => ItemFolderOps | null;
  wording: ItemFolderWording;
  confirm: (message: string) => Promise<boolean>;
  /** A refusal's message for a toast. */
  describeError: (error: unknown, fallback: string) => string;
}

export interface ItemFolderHandlers {
  submitNewFolder: () => Promise<void>;
  renameFolder: (path: string, name: string) => Promise<boolean>;
  deleteFolder: (path: string) => Promise<void>;
}

/** v4 `pluralize`. */
export const pluralize = (n: number, word: string): string => `${n} ${word}${n === 1 ? '' : 's'}`;

/**
 * v4 `useItemFolderHandlers`. Call in an injection context (the toasts are
 * `ToastService`'s). Each handler reads the view it was called from BEFORE
 * any await — v4's handlers close over one render's values.
 */
export function createItemFolderHandlers<T extends Item>(opts: ItemFolderHandlersOptions<T>): ItemFolderHandlers {
  const toasts = inject(ToastService);
  const { state, wording, confirm, describeError } = opts;

  const submitNewFolder = async (): Promise<void> => {
    const name = state.newFolder.name().trim();
    const ops = opts.ops();
    if (!name || !ops) return;
    const currentFolder = state.currentFolder();
    try {
      const folder = await ops.create(joinFolder(currentFolder, name));
      toasts.showSuccess(wording.created(folder.name));
      state.newFolder.close();
    } catch (error) {
      toasts.showError(describeError(error, 'The folder would not be made'));
    }
  };

  const renameFolder = async (path: string, name: string): Promise<boolean> => {
    const ops = opts.ops();
    if (!ops) return false;
    try {
      const folder = await ops.rename(path, name);
      toasts.showSuccess(`Renamed to "${folder.name}"`);
      return true;
    } catch (error) {
      toasts.showError(describeError(error, 'The folder would not be renamed'));
      return false;
    }
  };

  const deleteFolder = async (path: string): Promise<void> => {
    const ops = opts.ops();
    if (!ops) return;
    const currentFolder = state.currentFolder();
    const currentTier = state.currentTier();
    const parent = parentFolder(path);
    const count = state.tierItems().filter((i) => isUnderFolder(i.folder ?? '', path)).length;
    const parentLabel = parent ? `"${folderName(parent)}"` : wording.rootPhrase;
    const message = wording.deleteMessage
      ? wording.deleteMessage({ name: folderName(path), count, parentLabel })
      : count > 0
        ? `Send its ${pluralize(count, wording.noun)} up to ${parentLabel}? Only the folder itself goes.`
        : `Remove the folder "${folderName(path)}"?`;
    if (!(await confirm(message))) return;
    try {
      const result = await ops.remove(path);
      toasts.showSuccess(
        result.movedItems > 0 ? wording.movedUp(pluralize(result.movedItems, wording.noun)) : 'Folder removed',
      );
      if (isUnderFolder(currentFolder, path)) state.navigate(currentTier, parent);
    } catch (error) {
      toasts.showError(describeError(error, 'The folder would not be removed'));
    }
  };

  return { submitNewFolder, renameFolder, deleteFolder };
}
