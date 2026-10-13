/**
 * Row actions for a wardrobe list — star, archive, draw a picture, delete,
 * duplicate (into the same folder) — plus the open/closed state of the
 * overlays they summon (the item editor, the move/copy dialog, the full-screen
 * picture, the Move-to-folder picker). A port of v4
 * `components/wardrobe/hooks/useWardrobeItemActions.ts` at `4a38222e6`, WITH
 * `97f0d8cb8`'s picker state (bug 229, R-O).
 *
 * Every item is addressed through `containerForListedItem` when the browsed
 * wardrobe is a character's merged view (so a borrowed group or project item
 * is addressed through its own wardrobe, never through General), and through
 * the browsed container otherwise.
 *
 * **The picker lives here, beside the other overlays, so `overlayOpen` sees
 * it** (bug 229). In v4 the dialog's click-outside handler treated a click
 * inside the (portalled) picker as a click outside the dialog and closed both;
 * v5's vector is the dialog's document-level Escape gate, which reads
 * `overlayOpen` — a picker the gate could not see would let one Escape inside
 * it close the whole wardrobe.
 *
 * **Recorded mechanism divergences.**
 *  - (R-J) v4's one `useMutation` refreshes by invalidating
 *    `queryKeys.wardrobe.lists` / `folderLists` / `outfit(chatId)`; here
 *    {@link WardrobeItemActions.refresh} calls the host's reload (the lists
 *    and, in chat, the worn outfit) and bumps {@link
 *    WardrobeItemActions.refreshTick}, which every mounted browser watches to
 *    re-read its folder tree. The folder picker does not watch it: it reads
 *    its tree when it opens on a container and re-reads after its own
 *    **New folder** settles. v4's folder mutations invalidate the
 *    lists and the folders but not the outfit — {@link
 *    WardrobeItemActions.refreshLists} is that narrower refresh.
 *  - The item writes go through `wardrobe.api.ts`'s routed helpers with the
 *    item's `addressOf` home as the container, the dispatch twin of v4's
 *    `wardrobeItemUrl(home, id)`.
 *  - v4's imperative `showConfirmation` arrives as the host's `confirm`
 *    (`window.confirm` in the dialog — the dialog's recorded divergence).
 *
 * Plain signals — no injection context needed.
 *
 * @module wardrobe/wardrobe-item-actions
 */

import { computed, signal, type Signal } from '@angular/core';

import type { CoreClient } from '../core/core-client';
import type { WardrobeItemDto } from '../core/core-contract';
import type { ToastService } from '../ui/toast.service';
import { nextCopyTitle } from './equipped-slots';
import { generateWardrobeItemImage } from './item-images.api';
import { containerForListedItem, type WardrobeContainer } from './wardrobe-container';
import {
  deleteWardrobeItem,
  duplicateWardrobeItem,
  setWardrobeItemArchived,
  toggleItemDefault,
} from './wardrobe.api';

/** v4 `EditorRequest`. */
export type EditorRequest =
  | { kind: 'edit'; item: WardrobeItemDto }
  | {
      kind: 'create';
      mode: 'single' | 'bundle';
      componentItemIds: string[];
      /** The folder being browsed — where the editor files the new item by default. */
      folder?: string;
    };

/** v4 `TransferRequest`. */
export interface TransferRequest {
  item: WardrobeItemDto;
  mode: 'move' | 'copy';
}

/** v4 `97f0d8cb8` `FolderPickerRequest` — the items the picker files. */
export interface FolderPickerRequest {
  items: WardrobeItemDto[];
}

/** v4 `ImageViewRequest`. */
export interface ImageViewRequest {
  item: WardrobeItemDto;
  container: WardrobeContainer;
}

export interface WardrobeItemActionsOptions {
  core: CoreClient;
  toasts: Pick<ToastService, 'showError' | 'showSuccess'>;
  /** The wardrobe being browsed (null while nothing is selected). */
  container: () => WardrobeContainer | null;
  /** The list on display — Duplicate picks a title not already in it. */
  listItems: () => readonly WardrobeItemDto[];
  /** Ask the operator to confirm (v4 `requestConfirmation`). */
  confirm: (message: string) => boolean | Promise<boolean>;
  /** Re-read the lists on display (v4's `wardrobe.lists` invalidation). */
  reloadLists: () => Promise<void>;
  /** In chat: re-read the worn outfit (v4's `wardrobe.outfit(chatId)` invalidation). */
  reloadOutfit?: () => Promise<void>;
}

export interface WardrobeItemActions {
  // overlay state
  readonly editor: Signal<EditorRequest | null>;
  readonly transfer: Signal<TransferRequest | null>;
  readonly imageView: Signal<ImageViewRequest | null>;
  readonly folderPicker: Signal<FolderPickerRequest | null>;
  /** True while any of this store's overlays is open (bug 229 counts the picker). */
  readonly overlayOpen: Signal<boolean>;
  // row state
  readonly updatingDefaultId: Signal<string | null>;
  readonly generatingImageIds: Signal<ReadonlySet<string>>;
  /** Bumped by every refresh; browsers re-read their folder tree on it. */
  readonly refreshTick: Signal<number>;
  // handlers
  toggleDefault(item: WardrobeItemDto): Promise<void>;
  toggleArchived(item: WardrobeItemDto): Promise<void>;
  remove(item: WardrobeItemDto): Promise<void>;
  duplicate(item: WardrobeItemDto): Promise<void>;
  generateImage(item: WardrobeItemDto): Promise<void>;
  openImage(item: WardrobeItemDto): void;
  openEditor(item: WardrobeItemDto): void;
  openCreate(mode: 'single' | 'bundle', componentItemIds?: string[], opts?: { folder?: string }): void;
  closeEditor(): void;
  move(item: WardrobeItemDto): void;
  copy(item: WardrobeItemDto): void;
  closeTransfer(): void;
  closeImage(): void;
  openFolderPicker(items: WardrobeItemDto[]): void;
  closeFolderPicker(): void;
  /** The container an item's routes live under, from this view. */
  addressOf(item: WardrobeItemDto): WardrobeContainer | null;
  /**
   * Re-read every wardrobe list and folder tree (an item's folder may be new),
   * and in chat the worn snapshot.
   */
  refresh(): Promise<void>;
  /** The lists and folder trees only — what v4's folder mutations invalidate. */
  refreshLists(): Promise<void>;
  /** Ask the operator to confirm (the folder delete uses it too). */
  confirm(message: string): Promise<boolean>;
}

export function createWardrobeItemActions(opts: WardrobeItemActionsOptions): WardrobeItemActions {
  const { core, toasts } = opts;
  const isCharacterScope = (): boolean => opts.container()?.scope === 'character';

  const editor = signal<EditorRequest | null>(null);
  const transfer = signal<TransferRequest | null>(null);
  const imageView = signal<ImageViewRequest | null>(null);
  const folderPicker = signal<FolderPickerRequest | null>(null);
  const updatingDefaultId = signal<string | null>(null);
  // Every item whose picture is being drawn right now. A set, not one id: two
  // rows may each have a commission out.
  const generatingImageIds = signal<ReadonlySet<string>>(new Set());
  const refreshTick = signal(0);

  const refreshLists = async (): Promise<void> => {
    refreshTick.update((n) => n + 1);
    await opts.reloadLists();
  };

  const refresh = async (): Promise<void> => {
    await Promise.all([refreshLists(), opts.reloadOutfit ? opts.reloadOutfit() : Promise.resolve()]);
  };

  const confirm = async (message: string): Promise<boolean> => Boolean(await opts.confirm(message));

  /**
   * Run one item write; toast and resolve false on failure, refresh on
   * success (v4 `send` over the mutation's `onSuccess: refresh`).
   */
  const send = async (write: () => Promise<unknown>, failure: string): Promise<boolean> => {
    try {
      await write();
    } catch (error) {
      toasts.showError(error instanceof Error && error.message ? error.message : failure);
      return false;
    }
    await refresh();
    return true;
  };

  const addressOf = (item: WardrobeItemDto): WardrobeContainer | null =>
    isCharacterScope() ? containerForListedItem(item) : opts.container();

  const toggleDefault = async (item: WardrobeItemDto): Promise<void> => {
    const home = addressOf(item);
    if (!home) return;
    updatingDefaultId.set(item.id);
    try {
      await send(() => toggleItemDefault(core, item, home), 'Failed to update item');
    } finally {
      updatingDefaultId.set(null);
    }
  };

  /**
   * Archive or restore one garment. Archiving hides it from the pickers and
   * the outfit-selection LLM's candidates; it does NOT strip it off anyone
   * already wearing it, and nobody may put it back on until it is restored.
   */
  const toggleArchived = async (item: WardrobeItemDto): Promise<void> => {
    const home = addressOf(item);
    if (!home) return;
    await send(() => setWardrobeItemArchived(core, item, home), 'Failed to update item');
  };

  const remove = async (item: WardrobeItemDto): Promise<void> => {
    const home = addressOf(item);
    if (!home) return;
    if (!(await confirm(`Delete "${item.title}"? This cannot be undone.`))) return;
    if (await send(() => deleteWardrobeItem(core, item, home), 'Failed to delete item')) {
      toasts.showSuccess(`Deleted "${item.title}"`);
    }
  };

  /**
   * Duplicate is offered only for manageable items, so the copy stays where
   * the original lives. Composite references are copied verbatim — member
   * items are not cloned. (The body's `folder` — v4's `draftToPayload({
   * ...draftFromItem(item), title })` — is the HANDOFF to the unifier,
   * §R.10(l): `duplicateWardrobeItem` routes through P4.D286's draft.)
   */
  const duplicate = async (item: WardrobeItemDto): Promise<void> => {
    const home = addressOf(item);
    if (!home) return;
    const title = nextCopyTitle(
      item.title,
      opts.listItems().map((i) => i.title),
    );
    console.debug('[useWardrobeItemActions] Duplicating wardrobe item', {
      sourceId: item.id,
      targetScope: home.scope,
      targetId: home.id,
      newTitle: title,
      componentItemIds: item.componentItemIds,
    });
    if (await send(() => duplicateWardrobeItem(core, home, item, title), 'Failed to duplicate item')) {
      toasts.showSuccess(`Duplicated "${item.title}"`);
    }
  };

  /** Draw a picture with the designated wardrobe profile (no picker — the editor has that). */
  const generateImage = async (item: WardrobeItemDto): Promise<void> => {
    const home = addressOf(item);
    if (!home || generatingImageIds().has(item.id)) return;
    generatingImageIds.update((prev) => new Set(prev).add(item.id));
    try {
      const result = await generateWardrobeItemImage(item.id, home);
      toasts.showSuccess(
        result.rerouted
          ? `A portrait of "${item.title}" is hung — drawn at the uncensored desk`
          : `A portrait of "${item.title}" is hung`,
      );
      // v4 also invalidates `queryKeys.wardrobe.images(item.id)`; v5 holds no
      // such cache — an open editor's Picture section refetches on its own.
      await refresh();
    } catch (error) {
      toasts.showError(error instanceof Error ? error.message : 'Failed to generate a picture');
    } finally {
      generatingImageIds.update((prev) => {
        const next = new Set(prev);
        next.delete(item.id);
        return next;
      });
    }
  };

  /** Open a row's current picture full screen, addressed through the wardrobe it hangs in. */
  const openImage = (item: WardrobeItemDto): void => {
    const home = addressOf(item);
    if (!home || !item.imageFileId) return;
    imageView.set({ item, container: home });
  };

  return {
    editor: editor.asReadonly(),
    transfer: transfer.asReadonly(),
    imageView: imageView.asReadonly(),
    folderPicker: folderPicker.asReadonly(),
    overlayOpen: computed(
      () =>
        editor() !== null ||
        transfer() !== null ||
        imageView() !== null ||
        folderPicker() !== null,
    ),
    updatingDefaultId: updatingDefaultId.asReadonly(),
    generatingImageIds: generatingImageIds.asReadonly(),
    refreshTick: refreshTick.asReadonly(),
    toggleDefault,
    toggleArchived,
    remove,
    duplicate,
    generateImage,
    openImage,
    openEditor: (item) => editor.set({ kind: 'edit', item }),
    openCreate: (mode, componentItemIds = [], o) =>
      editor.set({ kind: 'create', mode, componentItemIds, folder: o?.folder }),
    closeEditor: () => editor.set(null),
    move: (item) => transfer.set({ item, mode: 'move' }),
    copy: (item) => transfer.set({ item, mode: 'copy' }),
    closeTransfer: () => transfer.set(null),
    closeImage: () => imageView.set(null),
    openFolderPicker: (items) => {
      if (items.length > 0) folderPicker.set({ items });
    },
    closeFolderPicker: () => folderPicker.set(null),
    addressOf,
    refresh,
    refreshLists,
    confirm,
  };
}
