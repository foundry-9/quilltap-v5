import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import type { OutfitPreviewEntry, OutfitPreviewSlots } from '../../core/core-contract';

interface SlotRow {
  key: keyof OutfitPreviewSlots;
  label: string;
  badge: string;
  entries: OutfitPreviewEntry[];
}

const SLOTS: { key: keyof OutfitPreviewSlots; label: string; badge: string }[] = [
  { key: 'top', label: 'Top', badge: 'qt-badge-wardrobe-top' },
  { key: 'bottom', label: 'Bottom', badge: 'qt-badge-wardrobe-bottom' },
  { key: 'footwear', label: 'Footwear', badge: 'qt-badge-wardrobe-footwear' },
  { key: 'accessories', label: 'Accessories', badge: 'qt-badge-wardrobe-accessories' },
];

/**
 * Read-only four-slot render of a decided outfit (v4 `OutfitSlotsPreview`).
 * Presentational — the Green Room uses it to show what an LLM-run character
 * chose to wear.
 */
@Component({
  selector: 'qt-outfit-slots-preview',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
      @for (slot of rows(); track slot.key) {
        <div class="qt-card p-2">
          <div class="qt-text-tertiary mb-1 text-xs uppercase tracking-wide">{{ slot.label }}</div>
          @if (slot.entries.length === 0) {
            <div class="qt-text-muted text-xs italic">nothing</div>
          } @else {
            <div class="flex flex-wrap gap-1">
              @for (e of slot.entries; track e.id) {
                <span [class]="'qt-badge ' + slot.badge + ' text-xs'">
                  {{ e.title }}{{ e.isComposite ? ' · set' : '' }}
                </span>
              }
            </div>
          }
        </div>
      }
    </div>
  `,
})
export class OutfitSlotsPreview {
  readonly slots = input.required<OutfitPreviewSlots>();

  protected readonly rows = computed<SlotRow[]>(() => {
    const s = this.slots();
    return SLOTS.map((slot) => ({ ...slot, entries: s[slot.key] ?? [] }));
  });
}
