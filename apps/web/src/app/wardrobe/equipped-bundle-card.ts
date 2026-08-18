import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import type { WardrobeItemDto } from '../core/core-contract';
import type { EquippedBundle } from './equipped-slots';
import { WARDROBE_SLOT_META } from './slot-meta';

/**
 * Renders a multi-slot composite ("bundle") as a single card above the slot
 * rows (v4 `components/wardrobe/equipped-bundle-card.tsx`), replacing the
 * one-chip-per-slot duplication so a four-slot composite shows up once.
 *
 * Presentational — the parent decides whether Take off / Break apart commit
 * via the equip API (Live outfit) or mutate staged state (Outfit Builder),
 * and whether to hide the action row entirely.
 */
@Component({
  selector: 'qt-equipped-bundle-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="qt-card py-2 px-3">
      <div class="flex items-start justify-between gap-2 mb-1">
        <div class="min-w-0">
          <div class="flex items-center gap-2 flex-wrap">
            <span class="text-sm font-medium text-foreground break-words">{{ title() }}</span>
            <span class="qt-text-xs qt-text-secondary">· bundle</span>
            @if (!bundle().allOccupied) {
              <span class="qt-badge qt-badge-warning">partially worn</span>
            }
          </div>
          <div class="flex flex-wrap gap-1 mt-1">
            @for (slot of bundle().occupiedSlots; track slot) {
              <span class="qt-badge uppercase" [class]="slotMeta[slot].badgeClass">
                {{ slotMeta[slot].label }}
              </span>
            }
          </div>
        </div>
        @if (showActions()) {
          <div class="flex items-center gap-1 flex-shrink-0">
            <button
              type="button"
              (click)="breakApart.emit(bundle())"
              class="qt-button-ghost qt-button-sm"
              title="Replace this bundle with its individual items"
            >
              Break apart
            </button>
            <button
              type="button"
              (click)="takeOff.emit(bundle())"
              class="qt-button-ghost qt-button-sm"
              title="Take this bundle off"
            >
              Take off bundle
            </button>
          </div>
        }
      </div>
    </div>
  `,
})
export class EquippedBundleCard {
  readonly bundle = input.required<EquippedBundle>();
  /** Lookup for resolving the composite's title. */
  readonly itemsById = input.required<Map<string, WardrobeItemDto>>();
  /** When false, hide Take off / Break apart (v4 `showActions`, default true). */
  readonly showActions = input(true);

  readonly takeOff = output<EquippedBundle>();
  readonly breakApart = output<EquippedBundle>();

  protected readonly slotMeta = WARDROBE_SLOT_META;

  /** v4 `:51-52`. */
  protected readonly title = computed(
    () => this.itemsById().get(this.bundle().compositeId)?.title ?? 'Unknown bundle',
  );
}
