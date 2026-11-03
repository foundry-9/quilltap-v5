/**
 * The "Pronoun-aware" checkbox the Wardrobe lists and outfit composers share —
 * a port of v4 `components/wardrobe/PronounAwareToggle.tsx` (`eb90d9ed8`).
 * Renders nothing when the character has no standard pronouns (nothing could
 * be hidden). Shows how many items the filter is holding back while it is on.
 *
 * @module wardrobe/pronoun-aware-toggle
 */

import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import type { PronounGuidanceFilter } from './pronoun-guidance-filter';

@Component({
  selector: 'qt-pronoun-aware-toggle',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (filter().applies()) {
      <label
        class="flex items-center gap-2 qt-text-xs qt-text-secondary"
        title="Offer only pieces whose pronoun guidance suits this character (pieces without guidance are always offered)"
      >
        <input
          type="checkbox"
          class="qt-checkbox"
          [checked]="filter().aware()"
          (change)="filter().setAware($any($event.target).checked)"
        />
        Pronoun-aware
        @if (filter().aware() && heldBack() > 0) {
          <span>({{ heldBack() }} hidden)</span>
        }
      </label>
    }
  `,
})
export class PronounAwareToggle {
  readonly filter = input.required<PronounGuidanceFilter>();
  /** The pool the count is taken over. */
  readonly items = input.required<readonly { pronounGuidance?: readonly string[] | null }[]>();

  protected readonly heldBack = computed(() => this.filter().countHeldBack(this.items()));
}
