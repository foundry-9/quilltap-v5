import { ChangeDetectionStrategy, Component, computed, input, output, signal } from '@angular/core';

import type { WardrobeItemDto, WardrobeSlotType } from '../core/core-contract';
import { WARDROBE_SLOT_META } from './slot-meta';

/**
 * One slot in the dialog's "Wearing now" column (v4
 * `components/wardrobe/equipped-slot-row.tsx`). Lists the items currently
 * occupying the slot (as removable chips) and exposes a "+" picker that adds
 * another item from the wardrobe. Composite items in the equipped state render
 * as a single chip with a `· bundle` note.
 *
 * The chip removes the **stored** id (which may be a composite) — leaf
 * expansion is purely for display labels.
 */
@Component({
  selector: 'qt-equipped-slot-row',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { '(document:keydown.escape)': 'onEscape($event)' },
  template: `
    <div class="qt-card py-2 px-3">
      <div class="flex items-center justify-between mb-1">
        <span class="qt-badge" [class]="slotMeta[slot()].badgeClass">{{ label() }}</span>
        <div class="flex items-center gap-1">
          <button
            type="button"
            (click)="pickerOpen.set(!pickerOpen())"
            class="qt-button-ghost qt-button-sm"
            [title]="'Wear something in ' + lowerLabel() + ' (fills every slot it covers)'"
          >
            +
          </button>
          @if (equippedIds().length > 0) {
            <button
              type="button"
              (click)="clear.emit(slot())"
              class="qt-button-ghost qt-button-sm qt-text-secondary"
              [title]="'Clear ' + lowerLabel()"
            >
              Clear
            </button>
          }
        </div>
      </div>

      @if (equippedItems().length === 0) {
        <div class="qt-text-xs qt-text-secondary italic">Empty</div>
      } @else {
        <div class="flex flex-wrap gap-1">
          @for (entry of equippedItems(); track entry.id) {
            <span
              class="inline-flex items-center gap-1 rounded-full qt-bg-muted border qt-border-default px-2 py-0.5 qt-text-xs"
            >
              @if (entry.item) {
                {{ entry.item.title }}
              } @else {
                <span class="qt-text-secondary italic">unknown</span>
              }
              @if (entry.isComposite) {
                <span class="qt-text-secondary">· bundle</span>
              }
              <button
                type="button"
                [attr.aria-label]="'Remove ' + (entry.item?.title ?? 'item')"
                (click)="remove.emit({ slot: slot(), itemId: entry.id })"
                class="qt-text-secondary hover:text-foreground"
              >
                ×
              </button>
            </span>
          }
        </div>
      }

      @if (pickerOpen()) {
        <div class="mt-2 rounded border qt-border-default qt-bg-muted/40 max-h-64 overflow-y-auto">
          <div class="p-2">
            <input
              type="search"
              [value]="search()"
              (input)="search.set($any($event.target).value)"
              [placeholder]="'Search ' + lowerLabel() + ' items…'"
              class="qt-input qt-input-sm w-full"
            />
          </div>
          @if (candidates().length === 0) {
            <div class="px-3 py-2 qt-text-xs qt-text-secondary">No matching items.</div>
          } @else {
            <ul class="divide-y qt-border-default">
              @for (c of candidates(); track c.id) {
                <li>
                  <button
                    type="button"
                    (click)="pickCandidate(c.id)"
                    class="flex w-full items-center justify-between gap-2 px-3 py-2 text-left hover:qt-bg-muted"
                  >
                    <span class="truncate text-sm text-foreground">{{ c.title }}</span>
                    <span class="qt-text-xs qt-text-secondary">
                      {{ c.types.join(', ')
                      }}{{ (c.componentItemIds ?? []).length > 0 ? ' · composite' : '' }}
                    </span>
                  </button>
                </li>
              }
            </ul>
          }
        </div>
      }
    </div>
  `,
})
export class EquippedSlotRow {
  readonly slot = input.required<WardrobeSlotType>();
  /** IDs currently in this slot (may include composites). */
  readonly equippedIds = input.required<string[]>();
  /** All wardrobe items for the selected character (incl. archetypes). */
  readonly allItems = input.required<WardrobeItemDto[]>();

  readonly add = output<{ slot: WardrobeSlotType; itemId: string }>();
  readonly remove = output<{ slot: WardrobeSlotType; itemId: string }>();
  readonly clear = output<WardrobeSlotType>();

  protected readonly slotMeta = WARDROBE_SLOT_META;
  protected readonly pickerOpen = signal(false);
  protected readonly search = signal('');

  protected readonly label = computed(() => WARDROBE_SLOT_META[this.slot()].label);
  protected readonly lowerLabel = computed(() =>
    WARDROBE_SLOT_META[this.slot()].label.toLowerCase(),
  );

  private readonly itemsById = computed(
    () => new Map(this.allItems().map((i) => [i.id, i])),
  );

  protected readonly equippedItems = computed(() =>
    this.equippedIds().map((id) => {
      const item = this.itemsById().get(id) ?? null;
      return { id, item, isComposite: item ? (item.componentItemIds ?? []).length > 0 : false };
    }),
  );

  /** v4 `:90-97` — same-slot, not-equipped, title-filtered candidates. */
  protected readonly candidates = computed(() => {
    const equipped = new Set(this.equippedIds());
    const term = this.search().trim().toLowerCase();
    return this.allItems()
      .filter((i) => i.types.includes(this.slot()))
      .filter((i) => !equipped.has(i.id))
      .filter((i) => (term ? i.title.toLowerCase().includes(term) : true));
  });

  protected pickCandidate(itemId: string): void {
    this.add.emit({ slot: this.slot(), itemId });
    this.pickerOpen.set(false);
    this.search.set('');
  }

  /** v4 `:69-81` — Escape closes the picker without dismissing the whole
   *  modal (capture + stopPropagation there; here we stop propagation when
   *  the picker is open). */
  protected onEscape(event: Event): void {
    if (!this.pickerOpen()) return;
    event.stopPropagation();
    event.preventDefault();
    this.pickerOpen.set(false);
    this.search.set('');
  }
}
