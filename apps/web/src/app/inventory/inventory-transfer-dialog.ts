import { ChangeDetectionStrategy, Component, computed, inject, input, output, signal } from '@angular/core';

import { CoreClient } from '../core/core-client';
import type { ResolvedInventoryItem } from '../core/core-contract';
import { Modal } from '../ui/modal';
import { ToastService } from '../ui/toast.service';
import type { CatalogueEntry } from '../item-browser/container-catalogue';
import { ContainerSelect } from '../item-browser/container-select';
import { ItemTransferFooter } from '../item-browser/item-transfer-footer';
import {
  GENERAL_INVENTORY_NAME,
  decodeInventoryContainer,
  encodeInventoryContainer,
  type InventoryContainer,
} from './inventory-container';
import type { InventoryItemActions } from './inventory-item-actions';
import { describeInventoryRefusal } from './inventory-refusal';
import { transferInventoryItem } from './inventory.api';

/**
 * Where an object can go: the shared container catalogue's three lists (v4
 * pin `InventoryTransferDialog.tsx:30` —
 * `Pick<ContainerCatalogue, 'characters' | 'projects' | 'groups'>`).
 */
export interface InventoryCatalogue {
  characters: ReadonlyArray<CatalogueEntry>;
  projects: ReadonlyArray<CatalogueEntry>;
  groups: ReadonlyArray<CatalogueEntry>;
}

/**
 * Move or copy one Inventory object to another tier — a port of v4
 * `components/inventory/InventoryTransferDialog.tsx` (`322a5488e`). A move
 * keeps the object's id (every chat that carries it, every kit that names it,
 * still does); a copy is a new object, offered only when the object can be
 * copied. The folder path comes along. The source tier is not offered.
 * At the pin `624d0d25c` the destination is the shared `qt-container-select`
 * over the container catalogue and the footer is `qt-item-transfer-footer`
 * (v4 `b2b783b47` / `600c2906f`).
 */
@Component({
  selector: 'qt-inventory-transfer-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ContainerSelect, ItemTransferFooter, Modal],
  template: `
    <qt-modal
      [title]="verb() + ' &quot;' + label() + '&quot; to another inventory'"
      maxWidth="md"
      overlayClass="z-[70]"
      [closeOnBackdrop]="!working()"
      [closeOnEscape]="!working()"
      (close)="closed.emit()"
    >
      <p class="qt-text-xs qt-text-secondary mb-2">{{ note() }}</p>
      <label class="text-sm text-foreground" id="inventory-transfer-target-label" for="inventory-transfer-target">Destination</label>
      <qt-container-select
        id="inventory-transfer-target"
        labelledBy="inventory-transfer-target-label"
        selectClass="qt-select w-full mt-1"
        [value]="target()"
        (valueChange)="target.set($event)"
        [catalogue]="catalogue()"
        [generalName]="generalName"
        placeholder="Choose an inventory"
        [exclude]="exclude()"
      />

      <qt-item-transfer-footer
        qt-modal-footer
        [mode]="mode()"
        [working]="working()"
        [submitDisabled]="!target()"
        (cancelled)="closed.emit()"
        (submitted)="submit()"
      />
    </qt-modal>
  `,
})
export class InventoryTransferDialog {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly item = input.required<ResolvedInventoryItem>();
  readonly mode = input.required<'move' | 'copy'>();
  readonly from = input.required<InventoryContainer>();
  readonly catalogue = input.required<InventoryCatalogue>();
  readonly actions = input.required<InventoryItemActions>();
  readonly closed = output<void>();

  protected readonly target = signal('');
  protected readonly working = signal(false);
  protected readonly verb = computed(() => (this.mode() === 'move' ? 'Move' : 'Copy'));
  protected readonly label = computed(() => this.item().title || this.item().name);
  protected readonly generalName = GENERAL_INVENTORY_NAME;
  /** v4 pin `exclude={[fromKey]}` — the source tier is not offered. */
  protected readonly exclude = computed(() => [encodeInventoryContainer(this.from())]);
  /** v4 `:88-93`. */
  protected readonly note = computed(
    () =>
      (this.mode() === 'move'
        ? 'The object keeps its identity: chats that carry it, and kits that name it, still do. Its folder comes along.'
        : 'The copy is a new object with its own identity, filed in the same folder at the destination.') +
      (this.item().parentId ? ' An instance stays an instance of its archetype wherever it goes.' : ''),
  );

  /** v4 `submit` (`:63-77`). */
  protected async submit(): Promise<void> {
    const to = decodeInventoryContainer(this.target());
    if (!to) return;
    const mode = this.mode();
    const label = this.label();
    this.working.set(true);
    try {
      await transferInventoryItem(this.core, { action: mode, itemId: this.item().id, from: this.from(), to });
      this.toasts.showSuccess(mode === 'move' ? `"${label}" has changed hands` : `A copy of "${label}" is on its way`);
      this.closed.emit();
    } catch (error) {
      this.toasts.showError(describeInventoryRefusal(error, `The object would not ${mode}`));
    } finally {
      this.working.set(false);
      void this.actions().refresh();
    }
  }
}
