/**
 * The Inventory browser's row actions and the overlays they open — a port of
 * v4 `components/inventory/hooks/useInventoryItemActions.ts` (`322a5488e`,
 * with `102a23648`'s and `7263105c6`'s hunks): the editor, the folder picker,
 * the cross-tier transfer, the instantiate picker — plus the three warnings
 * the spec asks for:
 *
 *   - **Detach**: "This object will stop following *Magic bow*…";
 *   - **Delete an archetype in use**: the count and the holders, with
 *     *Detach all and delete* — or, when an archived character holds one,
 *     the list of them and no way through;
 *   - **Equip over the cap** (the dialog's gesture): which object and which cap.
 *
 * Every confirmation goes through `requestConfirmation` / `requestAlert` so
 * the dialog that hosts the browser can keep itself open while one is up.
 *
 * **Recorded mechanism divergences.**
 *  - (R-1) v4's one `useMutation` invalidates five key families in its
 *    `onSettled`; here every write settles through {@link InventoryItemActions.refresh},
 *    which reloads the host's list, folder tree and (in chat) the carried
 *    read — success or failure, as `onSettled` runs.
 *  - (R-4) `requestAlert` returns the CHOSEN LABEL (or null); the callers
 *    test it by label exactly as v4's do. The dialog binds it to the
 *    labelled-button alert (`ui/alert-modal.service.ts`, R-W).
 *  - (R-L) A refusal is a `CoreDispatchError` whose `code` / `details` stand
 *    for v4's `InventoryRequestError`'s.
 *
 * Plain signals — no injection context needed.
 *
 * @module inventory/inventory-item-actions
 */

import { computed, signal, type Signal } from '@angular/core';

import type { CoreClient } from '../core/core-client';
import { CoreDispatchError, type ResolvedInventoryItem } from '../core/core-contract';
import type { ToastService } from '../ui/toast.service';
import { GENERAL_INVENTORY_CONTAINER, containerOfOrigin, type InventoryContainer } from './inventory-container';
import { inventoryLabel } from './inventory-display';
import { describeInventoryRefusal, plural, refusalDetails } from './inventory-refusal';
import {
  deleteInventoryItem,
  detachInventoryItem,
  duplicateInventoryItem,
  updateInventoryItem,
} from './inventory.api';

export { describeInventoryRefusal } from './inventory-refusal';

/** v4 `InventoryEditorState`. */
export type InventoryEditorState =
  | { kind: 'create'; mode: 'item' | 'kit' | 'archetype'; folder: string }
  | { kind: 'edit'; item: ResolvedInventoryItem };

/** v4's `instantiateFor` — archetype-first, or destination-first. */
export type InventoryInstantiateRequest =
  | { archetype: ResolvedInventoryItem }
  | { destination: InventoryContainer; folder: string };

export interface InventoryItemActionsOptions {
  core: CoreClient;
  toasts: Pick<ToastService, 'showError' | 'showSuccess'>;
  /** The tier being browsed. */
  container: () => InventoryContainer | null;
  /** v4's `onSettled` invalidations — the host reloads its stores. */
  refresh: () => Promise<void>;
  requestConfirmation: (message: string) => Promise<boolean>;
  /** R-4: resolves to the chosen button's label, or null. */
  requestAlert: (message: string, buttons?: string[]) => Promise<string | null>;
}

export interface InventoryItemActions {
  readonly editor: Signal<InventoryEditorState | null>;
  readonly folderPicker: Signal<{ items: ResolvedInventoryItem[] } | null>;
  readonly transfer: Signal<{ item: ResolvedInventoryItem; mode: 'move' | 'copy' } | null>;
  readonly instantiateFor: Signal<InventoryInstantiateRequest | null>;
  /** True while any overlay this store opens is up (the dialog's Escape gate reads it). */
  readonly overlayOpen: Signal<boolean>;
  /** The tier an object lives in (a borrowed one is addressed at its own tier). */
  addressOf(item: ResolvedInventoryItem): InventoryContainer;
  /** Re-read everything a write may have changed (v4's `onSettled`). */
  refresh(): Promise<void>;
  requestConfirmation(message: string): Promise<boolean>;
  requestAlert(message: string, buttons?: string[]): Promise<string | null>;
  openCreate(mode: 'item' | 'kit' | 'archetype', folder?: string): void;
  openEditor(item: ResolvedInventoryItem): void;
  closeEditor(): void;
  openFolderPicker(items: ResolvedInventoryItem[]): void;
  closeFolderPicker(): void;
  openTransfer(item: ResolvedInventoryItem, mode: 'move' | 'copy'): void;
  closeTransfer(): void;
  openInstantiateArchetype(archetype: ResolvedInventoryItem): void;
  openInstantiateInto(destination: InventoryContainer, folder: string): void;
  closeInstantiate(): void;
  detach(item: ResolvedInventoryItem): Promise<void>;
  remove(item: ResolvedInventoryItem): Promise<void>;
  toggleArchived(item: ResolvedInventoryItem): Promise<void>;
  duplicate(item: ResolvedInventoryItem): Promise<void>;
}

/** v4 `:113-115` / `:136-138` — the archived holders, by name. */
function archivedNames(error: CoreDispatchError): string {
  return ((refusalDetails(error)['characters'] as Array<{ name?: string | null }> | undefined) ?? [])
    .map((c) => c.name ?? 'an archived character')
    .join(', ');
}

export function createInventoryItemActions(opts: InventoryItemActionsOptions): InventoryItemActions {
  const { core, toasts, requestConfirmation, requestAlert } = opts;
  const editor = signal<InventoryEditorState | null>(null);
  const folderPicker = signal<{ items: ResolvedInventoryItem[] } | null>(null);
  const transfer = signal<{ item: ResolvedInventoryItem; mode: 'move' | 'copy' } | null>(null);
  const instantiateFor = signal<InventoryInstantiateRequest | null>(null);

  /** v4 `:72-77`. */
  const addressOf = (item: ResolvedInventoryItem): InventoryContainer => {
    const container = opts.container();
    return container
      ? containerOfOrigin(item.origin, container)
      : containerOfOrigin(item.origin, GENERAL_INVENTORY_CONTAINER);
  };

  /** One write, refreshed afterwards whatever happened (v4's `onSettled`). */
  const settle = async <T>(run: () => Promise<T>): Promise<T> => {
    try {
      return await run();
    } finally {
      await opts.refresh();
    }
  };

  /** v4 `:79-93`. */
  const detach = async (item: ResolvedInventoryItem): Promise<void> => {
    const ok = await requestConfirmation(
      `This object will stop following "${item.parentName ?? 'its archetype'}". Its current values are kept; later edits to the archetype will not reach it.`,
    );
    if (!ok) return;
    try {
      await settle(() => detachInventoryItem(core, addressOf(item), item.id));
      toasts.showSuccess(`"${inventoryLabel(item)}" now stands on its own`);
    } catch (error) {
      toasts.showError(describeInventoryRefusal(error, 'The object would not detach'));
    }
  };

  /** v4 `:95-150`. */
  const remove = async (item: ResolvedInventoryItem): Promise<void> => {
    const at = addressOf(item);
    const label = inventoryLabel(item);
    try {
      if (!(await requestConfirmation(`Delete "${label}"? Its pictures go with it.`))) return;
      await settle(() => deleteInventoryItem(core, at, item.id));
      toasts.showSuccess(`"${label}" is gone from the table`);
    } catch (error) {
      if (!(error instanceof CoreDispatchError)) {
        toasts.showError(describeInventoryRefusal(error, 'The object would not be deleted'));
        return;
      }
      if (error.code === 'ARCHIVED_DEPENDENTS') {
        await requestAlert(
          `"${label}" cannot be deleted while archived characters hold instances of it: ${archivedNames(error)}. Their vaults are sealed; restore them first.`,
          ['Understood'],
        );
        return;
      }
      if (error.code !== 'ARCHETYPE_IN_USE') {
        toasts.showError(describeInventoryRefusal(error, 'The object would not be deleted'));
        return;
      }
      const details = refusalDetails(error);
      const count = Number(details['dependents'] ?? 0);
      const holders = ((details['holders'] as Array<{ ownerName?: string | null }> | undefined) ?? [])
        .map((h) => h.ownerName ?? 'somewhere')
        .join(', ');
      const choice = await requestAlert(
        `${plural(count, 'object')} ${count === 1 ? 'derives' : 'derive'} from "${label}" (held by ${holders}). Detach them all — each keeps its current values and stands on its own — and delete the archetype?`,
        ['Detach all and delete', 'Cancel'],
      );
      if (choice !== 'Detach all and delete') return;
      try {
        await settle(() => deleteInventoryItem(core, at, item.id, true));
        toasts.showSuccess(`"${label}" is gone; ${plural(count, 'object')} now stand on their own`);
      } catch (forcedError) {
        if (forcedError instanceof CoreDispatchError && forcedError.code === 'ARCHIVED_DEPENDENTS') {
          await requestAlert(
            `"${label}" cannot be deleted while archived characters hold instances of it: ${archivedNames(forcedError)}.`,
            ['Understood'],
          );
          return;
        }
        toasts.showError(describeInventoryRefusal(forcedError, 'The archetype would not be deleted'));
      }
    }
  };

  /** v4 `:152-162`. */
  const toggleArchived = async (item: ResolvedInventoryItem): Promise<void> => {
    try {
      await settle(() => updateInventoryItem(core, addressOf(item), item.id, { archived: !item.archivedAt }));
      toasts.showSuccess(
        item.archivedAt ? `"${inventoryLabel(item)}" is back on the shelf` : `"${inventoryLabel(item)}" is put away`,
      );
    } catch (error) {
      toasts.showError(describeInventoryRefusal(error, 'The object would not change'));
    }
  };

  /** v4 `:164-174`. */
  const duplicate = async (item: ResolvedInventoryItem): Promise<void> => {
    try {
      const copy = await settle(() => duplicateInventoryItem(core, addressOf(item), item.id));
      toasts.showSuccess(`"${inventoryLabel(copy)}" is laid beside the original`);
    } catch (error) {
      toasts.showError(describeInventoryRefusal(error, 'The object would not be duplicated'));
    }
  };

  return {
    editor: editor.asReadonly(),
    folderPicker: folderPicker.asReadonly(),
    transfer: transfer.asReadonly(),
    instantiateFor: instantiateFor.asReadonly(),
    overlayOpen: computed(
      () => editor() !== null || folderPicker() !== null || transfer() !== null || instantiateFor() !== null,
    ),
    addressOf,
    refresh: opts.refresh,
    requestConfirmation,
    requestAlert,
    openCreate: (mode, folder = '') => editor.set({ kind: 'create', mode, folder }),
    openEditor: (item) => editor.set({ kind: 'edit', item }),
    closeEditor: () => editor.set(null),
    openFolderPicker: (items) => folderPicker.set({ items }),
    closeFolderPicker: () => folderPicker.set(null),
    openTransfer: (item, mode) => transfer.set({ item, mode }),
    closeTransfer: () => transfer.set(null),
    openInstantiateArchetype: (archetype) => instantiateFor.set({ archetype }),
    openInstantiateInto: (destination, folder) => instantiateFor.set({ destination, folder }),
    closeInstantiate: () => instantiateFor.set(null),
    detach,
    remove,
    toggleArchived,
    duplicate,
  };
}
