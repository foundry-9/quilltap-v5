import { ChangeDetectionStrategy, Component, computed, effect, inject, input, output, signal, untracked } from '@angular/core';

import { CoreClient } from '../core/core-client';
import type { ResolvedInventoryItem } from '../core/core-contract';
import { DestinationPickerDialog } from '../item-browser/destination-picker-dialog';
import { ToastService } from '../ui/toast.service';
import { folderCrumb, isSameFolder } from '../wardrobe/folders';
import type { InventoryContainer } from './inventory-container';
import { createInventoryFoldersStore } from './inventory-folders-data';
import type { InventoryItemActions } from './inventory-item-actions';
import { describeInventoryRefusal } from './inventory-refusal';
import { createInventoryFolder, fileInventoryItems } from './inventory.api';

/**
 * "Move to folder…" / "Copy to folder…" for Inventory objects — a port of v4
 * `components/inventory/InventoryFolderPickerDialog.tsx` (`322a5488e`; the
 * copy mode is `b2b783b47`'s, worded as the pin's `INVENTORY_PICKER_WORDING`):
 * the shared `qt-destination-picker-dialog` (v4's `ItemFolderPickerDialog`,
 * renamed `DestinationPickerDialog` by `c34986421`) over one tier's
 * `Inventory/` folders. The folder create and the filing each refresh the
 * host afterwards (v4's mutation `onSettled`).
 */
@Component({
  selector: 'qt-inventory-folder-picker-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [DestinationPickerDialog],
  template: `
    <qt-destination-picker-dialog
      [mode]="mode()"
      [subject]="subject()"
      [folders]="folders.folders()"
      [loading]="folders.loading()"
      rootLabel="Inventory (top level)"
      [createFolder]="createFolder"
      [currentFolder]="currentFolder()"
      [working]="working()"
      [note]="mode() === 'copy' ? copyNote : moveNote"
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
  /** v4 pin: Move to folder… or Copy to folder… (`b2b783b47`). */
  readonly mode = input<'move' | 'copy'>('move');
  readonly actions = input.required<InventoryItemActions>();
  readonly closed = output<void>();

  protected readonly folders = createInventoryFoldersStore(this.core);
  protected readonly working = signal(false);
  /** v4 pin `INVENTORY_PICKER_WORDING` (`InventoryBrowser.tsx:57-62`). */
  protected readonly moveNote = 'Folders are for your browsing; what anyone carries stays exactly as it was.';
  protected readonly copyNote = 'Each copy is a new object of its own; nobody carries it until it is equipped.';

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
    const copying = this.mode() === 'copy';
    const one = items.length === 1;
    this.working.set(true);
    try {
      const filed = await fileInventoryItems(this.core, this.mode(), this.container(), items.map((i) => i.id), folder);
      // v4 pin `ItemFolderPickerDialog.tsx` — the FILED row's folder (the server's casing).
      const where = folder ? `"${folderCrumb(filed[0]?.folder || folder)}"` : 'the top of the inventory';
      const rests = one ? 'sits' : 'sit';
      this.toasts.showSuccess(
        copying ? `${one ? 'A copy' : 'Copies'} of ${subject} now ${rests} in ${where}` : `${subject} now ${rests} in ${where}`,
      );
      this.closed.emit();
    } catch (error) {
      this.toasts.showError(describeInventoryRefusal(error, `The objects would not be ${copying ? 'copied' : 'moved'}`));
    } finally {
      this.working.set(false);
      void this.actions().refresh();
    }
  }
}
