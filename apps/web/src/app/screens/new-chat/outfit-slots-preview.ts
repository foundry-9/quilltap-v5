import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import type { OutfitPreviewEntry, OutfitPreviewSlots } from '../../core/core-contract';
import { WARDROBE_SLOT_META, WARDROBE_SLOT_TYPES } from '../../wardrobe/slot-meta';

interface SlotRow {
  key: keyof OutfitPreviewSlots;
  label: string;
  badge: string;
  entries: OutfitPreviewEntry[];
}

const SLOTS: { key: keyof OutfitPreviewSlots; label: string; badge: string }[] =
  WARDROBE_SLOT_TYPES.map((key) => ({
    key,
    label: WARDROBE_SLOT_META[key].label,
    badge: WARDROBE_SLOT_META[key].badgeClass,
  }));

/**
 * Read-only per-slot render of a decided outfit (v4 `OutfitSlotsPreview`).
 * Presentational — the Green Room uses it to show what an LLM-run character
 * chose to wear.
 *
 * `?? []` in {@link OutfitSlotsPreview.rows} is v4's forward-compat guard
 * (`OutfitSlotsPreview.tsx:27`): a frame minted before a slot existed renders
 * that slot empty instead of crashing. Every slot gets a card, empty or not —
 * v4's client renders the vacancy ("nothing") for all five, `reportWhenEmpty`
 * being a `lib/`-side narration rule, not a UI one.
 */
@Component({
  selector: 'qt-outfit-slots-preview',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
      @for (slot of rows(); track slot.key) {
        <div class="qt-card p-2">
          <div class="qt-text-secondary mb-1 text-xs uppercase tracking-wide">{{ slot.label }}</div>
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
