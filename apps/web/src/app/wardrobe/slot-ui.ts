import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import type { WardrobeSlotType } from '../core/core-contract';
import { WARDROBE_SLOT_META, WARDROBE_SLOT_TYPES } from './slot-meta';

/**
 * Slot chips and slot checkboxes — the one spelling of a wardrobe slot in the
 * wardrobe UI. Labels and colours come from `WARDROBE_SLOT_META`, so every row,
 * card, picker and form names a slot the same way. A port of v4
 * `components/wardrobe/slot-ui.tsx` (`9ecebf986`, read at the pin
 * `4a38222e6`).
 *
 * Both hosts take `display: contents`, so the markup lays out exactly as v4's
 * (an Angular host element cannot leave the DOM).
 *
 * @module wardrobe/slot-ui
 */

/** A slot's coloured chip, labelled with the slot's display name (v4
 *  `SlotBadge`, `:14-22`). The class string is bound whole through
 *  `[attr.class]` so its token order is v4's byte for byte. */
@Component({
  selector: 'qt-slot-badge',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { style: 'display: contents' },
  template: `<span [attr.class]="badgeClass()">{{ label() }}</span>`,
})
export class SlotBadge {
  readonly slot = input.required<WardrobeSlotType>();
  /** v4's `className` (no v4 caller passes one at the pin). */
  readonly extraClass = input<string | undefined>(undefined);

  protected readonly label = computed(() => WARDROBE_SLOT_META[this.slot()].label);
  protected readonly badgeClass = computed(() => {
    const extra = this.extraClass();
    return `qt-badge ${WARDROBE_SLOT_META[this.slot()].badgeClass}${extra ? ` ${extra}` : ''}`;
  });
}

/**
 * One checkbox per slot, in canonical order (v4 `SlotCheckboxGroup`,
 * `:24-69`). A locked slot shows checked and cannot be toggled (e.g. covered
 * by a component). `groupBlur` is v4's `onBlur` on the group — React's
 * `onBlur` bubbles, so it is the DOM's `focusout`.
 */
@Component({
  selector: 'qt-slot-checkbox-group',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { style: 'display: contents' },
  template: `
    <div class="flex flex-wrap gap-3" (focusout)="groupBlur.emit()">
      @for (slot of slotTypes; track slot) {
        <label
          [attr.class]="
            'inline-flex items-center gap-2 ' +
            (isLocked(slot) ? 'opacity-60 cursor-not-allowed' : 'cursor-pointer')
          "
          [attr.title]="isLocked(slot) ? (lockedTitle() ?? null) : null"
        >
          <input
            type="checkbox"
            class="qt-checkbox"
            [checked]="isLocked(slot) || value().includes(slot)"
            [disabled]="isLocked(slot)"
            (change)="toggle.emit(slot)"
          />
          <span class="text-sm text-foreground">{{ slotMeta[slot].label }}</span>
        </label>
      }
    </div>
  `,
})
export class SlotCheckboxGroup {
  /** Checked slots. */
  readonly value = input.required<readonly WardrobeSlotType[]>();
  /** Slots shown checked and not toggleable (e.g. covered by a component). */
  readonly locked = input<readonly WardrobeSlotType[]>([]);
  /** Tooltip on a locked slot. */
  readonly lockedTitle = input<string | undefined>(undefined);

  readonly toggle = output<WardrobeSlotType>();
  /** Fired when focus leaves the group (validation touch). */
  readonly groupBlur = output<void>();

  protected readonly slotTypes = WARDROBE_SLOT_TYPES;
  protected readonly slotMeta = WARDROBE_SLOT_META;

  protected isLocked(slot: WardrobeSlotType): boolean {
    return this.locked().includes(slot);
  }
}
