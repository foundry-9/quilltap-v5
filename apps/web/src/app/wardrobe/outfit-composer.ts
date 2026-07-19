import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import type { WardrobeItemDto, WardrobeSlotType } from '../core/core-contract';
import {
  groupEquippedSlots,
  WARDROBE_SLOT_TYPES,
  type EquippedBundle,
  type EquippedSlots,
} from './equipped-slots';
import { EquippedBundleCard } from './equipped-bundle-card';
import { EquippedSlotRow } from './equipped-slot-row';

/**
 * Renders an equipped outfit (live or staged) as bundle cards above slot rows,
 * with picker controls for adding/removing items (v4
 * `components/wardrobe/outfit-composer.tsx`). Controlled — the parent owns the
 * slots state and provides callbacks for mutation; bundle actions (Take off /
 * Break apart) can be hidden via `showBundleActions=false`.
 */
@Component({
  selector: 'qt-outfit-composer',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [EquippedBundleCard, EquippedSlotRow],
  template: `
    <div class="space-y-2 mb-3">
      @for (bundle of grouped().bundles; track bundle.compositeId) {
        <qt-equipped-bundle-card
          [bundle]="bundle"
          [itemsById]="itemsById()"
          [showActions]="showBundleActions()"
          (takeOff)="takeOffBundle.emit($event)"
          (breakApart)="breakApartBundle.emit($event)"
        />
      }
      @for (slot of slotTypes; track slot) {
        <qt-equipped-slot-row
          [slot]="slot"
          [equippedIds]="grouped().slotRemainders[slot]"
          [allItems]="items()"
          (add)="addToSlot.emit($event)"
          (remove)="removeFromSlot.emit($event)"
          (clear)="clearSlot.emit($event)"
        />
      }
    </div>
  `,
})
export class OutfitComposer {
  /** All wardrobe items available to the character (personal + archetypes). */
  readonly items = input.required<WardrobeItemDto[]>();
  /** Current equipped (or staged) slots. */
  readonly slots = input.required<EquippedSlots>();
  /** When true, bundle cards expose Take off / Break apart. */
  readonly showBundleActions = input.required<boolean>();

  readonly addToSlot = output<{ slot: WardrobeSlotType; itemId: string }>();
  readonly removeFromSlot = output<{ slot: WardrobeSlotType; itemId: string }>();
  readonly clearSlot = output<WardrobeSlotType>();
  readonly takeOffBundle = output<EquippedBundle>();
  readonly breakApartBundle = output<EquippedBundle>();

  protected readonly slotTypes = WARDROBE_SLOT_TYPES;

  protected readonly grouped = computed(() => groupEquippedSlots(this.slots(), this.items()));
  protected readonly itemsById = computed(
    () => new Map(this.items().map((i) => [i.id, i])),
  );
}
