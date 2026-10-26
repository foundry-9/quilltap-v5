import { ChangeDetectionStrategy, Component, computed, effect, inject, input, output, signal, untracked } from '@angular/core';

import { CoreClient } from '../core/core-client';
import type { ResolvedInventoryItem } from '../core/core-contract';
import { ItemFolderPickerDialog } from '../item-browser/item-folder-picker-dialog';
import { ToastService } from '../ui/toast.service';
import { folderCrumb, isSameFolder } from '../wardrobe/folders';
import type { InventoryContainer } from './inventory-container';
import { createInventoryFoldersStore } from './inventory-folders-data';
import type { InventoryItemActions } from './inventory-item-actions';
import { describeInventoryRefusal } from './inventory-refusal';
import { createInventoryFolder, fileInventoryItems } from './inventory.api';

/**
 * "Move to folder…" for Inventory objects — a port of v4
 * `components/inventory/InventoryFolderPickerDialog.tsx` (`322a5488e`): the
 * shared `qt-item-folder-picker-dialog` over one tier's `Inventory/` folders.
 * A move only (the Inventory files no copies by folder). The folder create
 * and the move each refresh the host afterwards (v4's mutation `onSettled`).
 */
@Component({
  selector: 'qt-inventory-folder-picker-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ItemFolderPickerDialog],
  template: `
    <qt-item-folder-picker-dialog
      mode="move"
      [subject]="subject()"
      [folders]="folders.folders()"
      [loading]="folders.loading()"
      rootLabel="Inventory (top level)"
      [createFolder]="createFolder"
      [currentFolder]="currentFolder()"
      [working]="working()"
      note="Folders are for your browsing; what anyone carries stays exactly as it was."
      (closed)="closed.emit()"
      (submitted)="submit($event)"
    />
  `,
})
export class InventoryFolderPickerDialog {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly container = input.required<InventoryContainer>();
  readonly items = input.required<ResolvedInventoryItem[]>();
  readonly actions = input.required<InventoryItemActions>();
  readonly closed = output<void>();

  protected readonly folders = createInventoryFoldersStore(this.core);
  protected readonly working = signal(false);

  /** v4 `:30-32`. */
  protected readonly currentFolder = computed(() => {
    const items = this.items();
    const first = items[0]?.folder ?? '';
    return items.every((i) => isSameFolder(i.folder ?? '', first)) ? first : null;
  });
  protected readonly subject = computed(() => {
    const items = this.items();
    return items.length === 1 ? `"${items[0].title || items[0].name}"` : `${items.length} objects`;
  });

  /** v4 `onCreateFolder` — the picker's tree and the host re-read afterwards. */
  protected readonly createFolder = async (path: string): Promise<{ path: string }> => {
    try {
      return await createInventoryFolder(this.core, this.container(), path);
    } finally {
      void this.folders.reload();
      void this.actions().refresh();
    }
  };

  constructor() {
    effect(() => {
      const container = this.container();
      untracked(() => void this.folders.load(container));
    });
  }

  /** v4 `onSubmit` (`:47-60`). */
  protected async submit(folder: string): Promise<void> {
    const items = this.items();
    const subject = this.subject();
    this.working.set(true);
    try {
      await fileInventoryItems(this.core, 'move', this.container(), items.map((i) => i.id), folder);
      this.toasts.showSuccess(
        `${subject} now ${items.length === 1 ? 'sits' : 'sit'} in ${folder ? `"${folderCrumb(folder)}"` : 'the top of the inventory'}`,
      );
      this.closed.emit();
    } catch (error) {
      this.toasts.showError(describeInventoryRefusal(error, 'The objects would not be moved'));
    } finally {
      this.working.set(false);
      void this.actions().refresh();
    }
  }
}
