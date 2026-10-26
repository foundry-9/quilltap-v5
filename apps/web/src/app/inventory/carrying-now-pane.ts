import { ChangeDetectionStrategy, Component, computed, input, model, output } from '@angular/core';

import type { CarriedEntry, ResolvedInventoryItem } from '../core/core-contract';
import { Modal } from '../ui/modal';
import { inventoryDisplayName } from './inventory-display';
import { InventoryGlyph, InventoryPicture } from './inventory-glyph';

/**
 * *Carrying now* — what one cast member carries in this chat, beside the
 * Inventory browser when the dialog is opened from a chat. A port of v4
 * `components/inventory/CarryingNowPane.tsx` (`322a5488e`, `102a23648`'s
 * lifted picker state): each object with its glyph and an Unequip button (a
 * `silent` note when it has no description), the kits within reach with their
 * all / some / none state (Equip / Unequip the whole kit), *Take up…* (a
 * picker over everything the character can reach that is not an archetype, a
 * kit, archived or already carried — pictures shown) and *Reset to defaults*.
 *
 * `picking` is a two-way binding so the dialog's Escape and outside-click
 * gates read it (v4 `onPickingChange`); the picker's Cancel, ✕, backdrop and
 * Escape close ONLY the picker.
 */
@Component({
  selector: 'qt-carrying-now-pane',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, InventoryGlyph, InventoryPicture],
  host: { '(document:keydown.escape)': 'onEscape()' },
  template: `
    <section class="flex flex-col min-h-0 gap-2" data-testid="inventory-carrying-now">
      <div class="flex items-center justify-between gap-2">
        <h3 class="text-sm text-foreground">Carrying now · {{ characterName() }} in this chat</h3>
        <div class="flex items-center gap-1">
          <button type="button" class="qt-button-secondary qt-button-sm" (click)="picking.set(true)" [disabled]="busy()">
            Take up…
          </button>
          <button type="button" class="qt-button-ghost qt-button-sm" (click)="reset.emit()" [disabled]="busy()">
            Reset to defaults
          </button>
        </div>
      </div>
      @if (carried().equipped.length === 0) {
        <p class="qt-text-xs qt-text-secondary px-1">Empty-handed. Equip something from the list, or take it up here.</p>
      } @else {
        <ul class="space-y-1" aria-label="Carried objects">
          @for (item of carried().equipped; track item.id) {
            <li class="qt-card py-1.5 px-2 flex items-center gap-2">
              <qt-inventory-glyph [item]="item" size="sm" />
              <span class="flex-1 min-w-0 text-sm truncate">{{ displayName(item) }}</span>
              @if (!item.description) {
                <span class="qt-text-xs qt-text-secondary" title="No description: the model is told nothing about it">silent</span>
              }
              <button type="button" class="qt-button-ghost qt-button-sm" [disabled]="busy()" (click)="unequip.emit([item.id])">
                Unequip
              </button>
            </li>
          }
        </ul>
      }
      @if (kits().length > 0) {
        <div class="space-y-1">
          <h4 class="qt-text-xs qt-text-secondary px-1">Kits</h4>
          <ul class="space-y-1" aria-label="Kits">
            @for (entry of kits(); track entry.kit.id) {
              <li class="qt-card py-1.5 px-2 flex items-center gap-2">
                <qt-inventory-glyph [item]="entry.kit" size="sm" />
                <span class="flex-1 min-w-0 text-sm truncate">{{ displayName(entry.kit) }}</span>
                <span class="qt-badge qt-badge-info">{{ entry.state }}</span>
                @if (entry.state !== 'all') {
                  <button type="button" class="qt-button-secondary qt-button-sm" [disabled]="busy()" (click)="equip.emit([entry.kit.id])">
                    Equip
                  </button>
                }
                @if (entry.state !== 'none') {
                  <button type="button" class="qt-button-ghost qt-button-sm" [disabled]="busy()" (click)="unequip.emit([entry.kit.id])">
                    Unequip
                  </button>
                }
              </li>
            }
          </ul>
        </div>
      }
      @if (picking()) {
        <qt-modal [title]="'Take up — ' + characterName()" maxWidth="lg" overlayClass="z-[70]" (close)="picking.set(false)">
          @if (choices().length === 0) {
            <p class="text-sm qt-text-secondary">Nothing within reach that is not already carried.</p>
          } @else {
            <ul role="listbox" aria-label="Objects within reach" class="space-y-1 max-h-[55vh] overflow-y-auto">
              @for (item of choices(); track item.id) {
                <li>
                  <button
                    type="button"
                    role="option"
                    aria-selected="false"
                    class="w-full flex items-center gap-2 px-2 py-1 rounded text-left hover:qt-bg-muted"
                    (click)="pick(item)"
                  >
                    <qt-inventory-picture [item]="item" />
                    <span class="flex-1 min-w-0">
                      <span class="block text-sm text-foreground truncate">{{ displayName(item) }}</span>
                      @if (item.description) {
                        <span class="block qt-text-xs qt-text-secondary truncate">{{ item.description }}</span>
                      }
                    </span>
                  </button>
                </li>
              }
            </ul>
          }
          <div qt-modal-footer class="flex items-center justify-end w-full">
            <button type="button" (click)="picking.set(false)" class="qt-button-secondary qt-button-sm">Cancel</button>
          </div>
        </qt-modal>
      }
    </section>
  `,
})
export class CarryingNowPane {
  readonly characterName = input.required<string>();
  readonly carried = input.required<CarriedEntry>();
  /** Everything the character can reach (for kits' names and the picker). */
  readonly pool = input<ResolvedInventoryItem[]>([]);
  readonly busy = input(false);
  /** The Take up… picker's open state, held by the dialog (v4 `picking` / `onPickingChange`). */
  readonly picking = model(false);
  readonly equip = output<string[]>();
  readonly unequip = output<string[]>();
  readonly reset = output<void>();

  protected readonly displayName = inventoryDisplayName;

  private readonly byId = computed(() => new Map(this.pool().map((i) => [i.id, i])));
  private readonly carriedIds = computed(() => new Set(this.carried().equipped.map((i) => i.id)));
  protected readonly kits = computed(() =>
    Object.entries(this.carried().kits)
      .map(([id, state]) => ({ kit: this.byId().get(id), state }))
      .filter((k): k is { kit: ResolvedInventoryItem; state: typeof k.state } => !!k.kit),
  );
  protected readonly choices = computed(() =>
    this.pool().filter((i) => !i.isArchetype && !i.archivedAt && i.kind !== 'kit' && !this.carriedIds().has(i.id)),
  );

  protected pick(item: ResolvedInventoryItem): void {
    this.picking.set(false);
    this.equip.emit([item.id]);
  }

  /** v4 `BaseModal` `closeOnEscape` on the picker — the dialog's own gate stands down meanwhile. */
  protected onEscape(): void {
    if (this.picking()) this.picking.set(false);
  }
}
