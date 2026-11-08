import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import type { KitState, ResolvedInventoryItem } from '../core/core-contract';
import { inventoryOriginLabel } from './inventory-container';
import { InventoryGlyph } from './inventory-glyph';
import { ItemActionMenu, type MenuEntryLike } from '../item-browser/item-action-menu';

/** v4 `InventoryRowActions` — what a row's ⋮ menu reaches. */
export interface InventoryRowActions {
  onEdit: (item: ResolvedInventoryItem) => void;
  onInstantiate: (item: ResolvedInventoryItem) => void;
  onDetach: (item: ResolvedInventoryItem) => void;
  onDuplicate: (item: ResolvedInventoryItem) => void;
  onMoveToFolder: (item: ResolvedInventoryItem) => void;
  /** Copy into another folder of the same tier (copyable objects only; v4 pin `:32`). */
  onCopyToFolder: (item: ResolvedInventoryItem) => void;
  onTransfer: (item: ResolvedInventoryItem, mode: 'move' | 'copy') => void;
  onToggleArchived: (item: ResolvedInventoryItem) => void;
  onDelete: (item: ResolvedInventoryItem) => void;
}

/** v4 `InventoryRowEquip` — present only in a chat with a character. */
export interface InventoryRowEquip {
  equipped: boolean;
  onEquip: (item: ResolvedInventoryItem) => void;
  onUnequip: (item: ResolvedInventoryItem) => void;
  /** A kit's state in the chat. */
  kitState?: KitState;
}

/**
 * One object in the Inventory browser — a port of v4
 * `components/inventory/inventory-item-row.tsx` at the pin `624d0d25c`
 * (`322a5488e`; `29ba4d3f8` onto the fixed menu; `b2b783b47` onto the shared
 * `ItemActionMenu`).
 *
 * The glyph (never the picture), the title with the name beside it when both
 * differ, and chips in v4's order: *Archetype*, *Instance of X*, *Kit · n*
 * (*Kit · all/some/none* in a chat), *Equipped*, *Default*, the tier an object
 * is borrowed from, *archived*, and a search hit's folder. In a chat an Equip
 * / Unequip button sits on the row (never on an archetype). The ⋮ menu holds
 * Edit, Unequip the kit, Instantiate… (archetypes), Detach from archetype…
 * (instances), Duplicate (only when copyable), Move to folder…, Copy to
 * folder… (only when copyable), Move / Copy to another inventory…, Archive /
 * Restore and Delete; a borrowed row keeps only the cross-tier Copy (and
 * Instantiate for an archetype).
 *
 * The menu is the shared `qt-item-action-menu`: FIXED to the viewport
 * (`ui/fixed-menu.ts`), closed by a scroll outside it, a resize, an outside
 * press, or Escape — swallowed on the CAPTURE phase so the dialog behind keeps
 * its own Escape (v4's `useClickOutside` with `escapeCapture: true`).
 */
@Component({
  selector: 'qt-inventory-item-row',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [InventoryGlyph, ItemActionMenu],
  template: `
    <div [attr.class]="rowClass()" data-testid="inventory-item-row">
      @if (selection() && manageable()) {
        <input
          type="checkbox"
          class="qt-checkbox"
          [checked]="selection()!.selected"
          (change)="selection()!.onToggle(item())"
          [attr.aria-label]="'Select ' + label()"
        />
      }
      <qt-inventory-glyph [item]="item()" />
      <div class="flex-1 min-w-0">
        <div class="text-sm text-foreground truncate" [attr.title]="label()">
          {{ label() }}
          @if (showName()) {
            <span class="qt-text-xs qt-text-secondary ml-1">({{ item().name }})</span>
          }
        </div>
        <div class="flex flex-wrap items-center gap-1 mt-0.5">
          @if (item().isArchetype) {
            <span class="qt-badge qt-badge-primary">Archetype</span>
          }
          @if (isInstance()) {
            <span
              class="qt-badge qt-badge-outline"
              [attr.title]="item().parentMissing ? 'Its archetype could not be found' : null"
              >{{ instanceChip() }}</span
            >
          }
          @if (item().kind === 'kit') {
            <span class="qt-badge qt-badge-info">{{ kitChip() }}</span>
          }
          @if (equip()?.equipped) {
            <span class="qt-badge qt-badge-success">Equipped</span>
          }
          @if (item().equippedByDefault && !item().isArchetype) {
            <span class="qt-badge qt-badge-secondary">Default</span>
          }
          @if (borrowed(); as b) {
            <span class="qt-badge qt-badge-wardrobe-shared">{{ b }}</span>
          }
          @if (item().archivedAt) {
            <span class="qt-badge qt-badge-secondary">archived</span>
          }
          @if (folderCrumb(); as crumb) {
            <span class="qt-text-xs qt-text-secondary" data-testid="inventory-folder-crumb">{{ crumb }}</span>
          }
        </div>
      </div>

      @if (equip(); as eq) {
        @if (!item().isArchetype && !item().archivedAt) {
          @if (item().kind === 'kit' || !eq.equipped) {
            <button
              type="button"
              class="qt-button-secondary qt-button-sm"
              [disabled]="!!selection()"
              (click)="eq.onEquip(item())"
            >
              Equip
            </button>
          } @else {
            <button
              type="button"
              class="qt-button-ghost qt-button-sm"
              [disabled]="!!selection()"
              (click)="eq.onUnequip(item())"
            >
              Unequip
            </button>
          }
        }
      }

      <qt-item-action-menu [disabled]="!!selection()" [entries]="entries()" />
    </div>
  `,
})
export class InventoryItemRow {
  readonly item = input.required<ResolvedInventoryItem>();
  /** The object lives in the tier being browsed. */
  readonly manageable = input(false);
  readonly actions = input.required<InventoryRowActions>();
  /** Present only when the browser is opened from a chat with a character. */
  readonly equip = input<InventoryRowEquip | undefined>(undefined);
  readonly selection = input<
    { selected: boolean; onToggle: (item: ResolvedInventoryItem) => void } | undefined
  >(undefined);
  /** A search hit's folder. */
  readonly folderCrumb = input<string | null>(null);

  /** v4 `:103-106`. */
  protected readonly label = computed(() => this.item().title || this.item().name);
  protected readonly showName = computed(() => {
    const item = this.item();
    return !!item.title && item.title.trim().toLowerCase() !== item.name.trim().toLowerCase();
  });
  protected readonly borrowed = computed(() => (this.manageable() ? null : inventoryOriginLabel(this.item().origin)));
  protected readonly isInstance = computed(() => !!this.item().parentId);
  protected readonly rowClass = computed(
    () => `qt-card py-2 px-3 flex items-center gap-2 ${this.item().archivedAt ? 'opacity-60' : ''}`,
  );
  protected readonly instanceChip = computed(() =>
    this.item().parentMissing ? 'Instance (archetype missing)' : `Instance of ${this.item().parentName ?? 'an archetype'}`,
  );
  protected readonly kitChip = computed(() => {
    const state = this.equip()?.kitState;
    return `Kit${state ? ` · ${state}` : ` · ${this.item().itemIds.length}`}`;
  });

  /** v4 pin `:121-137` — the row's ⋮ entries in v4's order, the holes dropped by the menu. */
  protected readonly entries = computed<readonly MenuEntryLike[]>(() => {
    const item = this.item();
    const manageable = this.manageable();
    const actions = this.actions();
    const equip = this.equip();
    const isInstance = this.isInstance();
    const run = (fn: (i: ResolvedInventoryItem) => void) => () => fn(item);
    return [
      manageable && { key: 'edit', label: 'Edit', onSelect: run(actions.onEdit) },
      !!equip &&
        item.kind === 'kit' &&
        !!equip.kitState &&
        equip.kitState !== 'none' && { key: 'unequip-kit', label: 'Unequip the kit', onSelect: run(equip.onUnequip) },
      item.isArchetype &&
        !item.archivedAt && { key: 'instantiate', label: 'Instantiate…', onSelect: run(actions.onInstantiate) },
      manageable && isInstance && { key: 'detach', label: 'Detach from archetype…', onSelect: run(actions.onDetach) },
      manageable && item.canCopy && { key: 'duplicate', label: 'Duplicate', onSelect: run(actions.onDuplicate) },
      manageable && { key: 'move-to-folder', label: 'Move to folder…', onSelect: run(actions.onMoveToFolder) },
      manageable &&
        item.canCopy && { key: 'copy-to-folder', label: 'Copy to folder…', onSelect: run(actions.onCopyToFolder) },
      manageable && {
        key: 'transfer-move',
        label: 'Move to another inventory…',
        onSelect: () => actions.onTransfer(item, 'move'),
      },
      item.canCopy && {
        key: 'transfer-copy',
        label: 'Copy to another inventory…',
        onSelect: () => actions.onTransfer(item, 'copy'),
      },
      manageable && {
        key: 'archive',
        label: item.archivedAt ? 'Restore' : 'Archive',
        onSelect: run(actions.onToggleArchived),
      },
      manageable && { key: 'delete', label: 'Delete', destructive: true, onSelect: run(actions.onDelete) },
    ];
  });
}
