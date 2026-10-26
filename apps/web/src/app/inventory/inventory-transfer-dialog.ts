import { ChangeDetectionStrategy, Component, computed, inject, input, output, signal } from '@angular/core';

import { CoreClient } from '../core/core-client';
import type { ResolvedInventoryItem } from '../core/core-contract';
import { Modal } from '../ui/modal';
import { ToastService } from '../ui/toast.service';
import {
  GENERAL_INVENTORY_CONTAINER,
  GENERAL_INVENTORY_NAME,
  decodeInventoryContainer,
  encodeInventoryContainer,
  type InventoryContainer,
} from './inventory-container';
import type { InventoryItemActions } from './inventory-item-actions';
import { describeInventoryRefusal } from './inventory-refusal';
import { transferInventoryItem } from './inventory.api';

/** v4 `InventoryCatalogue` (`InventoryTransferDialog.tsx:25-29`) — the select's three LIST reads. */
export interface InventoryCatalogue {
  characters: Array<{ id: string; name: string }>;
  projects: Array<{ id: string; name: string }>;
  groups: Array<{ id: string; name: string }>;
}

/** One `<option>` of a catalogue select. */
interface CatalogueOption {
  value: string;
  name: string;
}

/**
 * The catalogue as the destination selects list it (v4 `:96-110` / the
 * instantiate dialog's `:115-138`): characters, General, projects, groups —
 * each group present only when it has a member (General always), and the
 * container `omit` names left out (the transfer's source).
 */
export function catalogueGroups(
  catalogue: InventoryCatalogue,
  omit: string | null = null,
): Array<{ label: string; options: CatalogueOption[] }> {
  const option = (container: InventoryContainer, name: string): CatalogueOption[] => {
    const value = encodeInventoryContainer(container);
    return value === omit ? [] : [{ value, name }];
  };
  return [
    ...(catalogue.characters.length > 0
      ? [{ label: 'Characters', options: catalogue.characters.flatMap((c) => option({ scope: 'character', id: c.id }, c.name)) }]
      : []),
    { label: 'General', options: option(GENERAL_INVENTORY_CONTAINER, GENERAL_INVENTORY_NAME) },
    ...(catalogue.projects.length > 0
      ? [{ label: 'Projects', options: catalogue.projects.flatMap((p) => option({ scope: 'project', id: p.id }, p.name)) }]
      : []),
    ...(catalogue.groups.length > 0
      ? [{ label: 'Groups', options: catalogue.groups.flatMap((g) => option({ scope: 'group', id: g.id }, g.name)) }]
      : []),
  ];
}

/**
 * Move or copy one Inventory object to another tier — a port of v4
 * `components/inventory/InventoryTransferDialog.tsx` (`322a5488e`). A move
 * keeps the object's id (every chat that carries it, every kit that names it,
 * still does); a copy is a new object, offered only when the object can be
 * copied. The folder path comes along. The source tier is not offered.
 */
@Component({
  selector: 'qt-inventory-transfer-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  host: { '(document:keydown.escape)': 'onEscape()' },
  template: `
    <qt-modal
      [title]="verb() + ' &quot;' + label() + '&quot; to another inventory'"
      maxWidth="md"
      overlayClass="z-[70]"
      [closeOnBackdrop]="!working()"
      (close)="closed.emit()"
    >
      <p class="qt-text-xs qt-text-secondary mb-2">{{ note() }}</p>
      <label class="text-sm text-foreground" for="inventory-transfer-target">Destination</label>
      <select
        id="inventory-transfer-target"
        class="qt-select w-full mt-1"
        [value]="target()"
        (change)="target.set($any($event.target).value)"
      >
        <option value="" disabled>Choose an inventory</option>
        @for (group of groups(); track group.label) {
          <optgroup [label]="group.label">
            @for (option of group.options; track option.value) {
              <option [value]="option.value">{{ option.name }}</option>
            }
          </optgroup>
        }
      </select>

      <div qt-modal-footer class="flex justify-end gap-2 w-full">
        <button type="button" class="qt-button-secondary qt-button-sm" (click)="closed.emit()" [disabled]="working()">
          Cancel
        </button>
        <button type="button" class="qt-button-primary qt-button-sm" (click)="submit()" [disabled]="!target() || working()">
          {{ working() ? (verb() === 'Move' ? 'Moving' : 'Copying') + '…' : verb() }}
        </button>
      </div>
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
  protected readonly groups = computed(() => catalogueGroups(this.catalogue(), encodeInventoryContainer(this.from())));
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

  protected onEscape(): void {
    if (!this.working()) this.closed.emit();
  }
}
