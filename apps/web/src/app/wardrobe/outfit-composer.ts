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
import { OutfitQuickPick } from './outfit-quick-pick';

/**
 * Renders an equipped outfit (live or staged) as an outfit pull-down, then
 * bundle cards, then slot rows, with picker controls for adding/removing items
 * (v4 `components/wardrobe/outfit-composer.tsx`). Controlled — the parent owns
 * the slots state and provides callbacks for mutation; bundle actions (Take off
 * / Break apart) can be hidden via `showBundleActions=false`.
 *
 * The pool is split by kind (v4 `aec86a613`): composed outfits (composites)
 * live in the `Wear an outfit…` pull-down at the top, garments in the per-slot
 * pickers. A three-slot bundle used to appear in three separate slot menus and
 * bury the garments actually meant for the slot.
 */
@Component({
  selector: 'qt-outfit-composer',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [EquippedBundleCard, EquippedSlotRow, OutfitQuickPick],
  template: `
    <div class="space-y-2 mb-3">
      <qt-outfit-quick-pick [items]="items()" (wear)="wearOutfit($event)" />
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

  /**
   * v4 `:81-84` — the pull-down reuses the EXISTING equip callback; `aec86a613`
   * added no equip path. Every consumer of `addToSlot` applies the flag-driven
   * rule across *every* slot the item covers (`wearItemIntoSlots`), so the
   * `slot` argument names where the gesture started rather than where the item
   * lands. v4 writes `outfit.types[0]!` — a composite with an empty `types`
   * would hand `undefined` through; v5 keeps the same shape rather than
   * inventing a fallback, because the value is never read as a destination.
   */
  protected wearOutfit(outfit: WardrobeItemDto): void {
    this.addToSlot.emit({ slot: outfit.types[0]!, itemId: outfit.id });
  }

  protected readonly grouped = computed(() => groupEquippedSlots(this.slots(), this.items()));
  protected readonly itemsById = computed(() => new Map(this.items().map((i) => [i.id, i])));
}
