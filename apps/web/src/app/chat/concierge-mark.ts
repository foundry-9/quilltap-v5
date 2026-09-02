import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import type { ConciergeState } from './concierge-state';
import {
  CONCIERGE_STATE_PRESENTATION,
  type ConciergeStateDescription,
  conciergeToneSuffix,
  describeConciergeState,
} from './concierge-state-presentation';
import { Tooltip } from '../ui/tooltip';

/**
 * The tooltip's contents — title, the full sentence, the classifier's
 * categories when there are any, and where to change the state. Exported so
 * the Salon header's badge can put the identical bubble on the identical
 * words (v4 `ConciergeMark.tsx`'s `ConciergeTooltipBody`).
 *
 * All six classes already exist in `_surfaces.css`. The host is
 * `display: contents` so this element never sits between the bubble and
 * `div.qt-tooltip-body` as a `display: inline` box (findings #97 / #105 /
 * #107); v4 has no wrapper here at all.
 */
@Component({
  selector: 'qt-concierge-tooltip-body',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { style: 'display: contents' },
  template: `
    <div class="qt-tooltip-body">
      <p class="qt-tooltip-title">{{ description().title }}</p>
      <p>{{ description().detail }}</p>
      @if (description().categories; as categories) {
        <div class="qt-tooltip-section">
          <p class="qt-tooltip-section-label">Categories</p>
          <p class="qt-tooltip-quote">{{ categories.join(', ') }}</p>
        </div>
      }
      <p class="qt-tooltip-hint">{{ description().hint }}</p>
    </div>
  `,
})
export class ConciergeTooltipBody {
  readonly description = input.required<ConciergeStateDescription>();
}

/**
 * The mark's class string, exactly as v4's `ConciergeMark` builds it.
 *
 * Pulled out of the component deliberately: Angular's `[class]` binding parses
 * its input into tokens and applies them through `classList`, so a class
 * emitted TWICE collapses in the DOM and v4's `expect(mark.className).toBe(…)`
 * assertion cannot see it. Testing the string itself restores the guard v4's
 * corpus intends — drop the empty-suffix check and this function starts
 * returning `'qt-concierge-mark qt-concierge-mark'`, which reddens.
 */
export function conciergeMarkClasses(state: ConciergeState, className = ''): string {
  // Danger is the base rule, so its suffix is empty — don't emit the base
  // class twice for it.
  const toneSuffix = conciergeToneSuffix(CONCIERGE_STATE_PRESENTATION[state].tone);
  return ['qt-concierge-mark', toneSuffix ? `qt-concierge-mark${toneSuffix}` : '', className]
    .filter(Boolean)
    .join(' ');
}

/**
 * ConciergeMark — the little asterisk that marks a chat's Concierge state on
 * every list in the house: the homepage's Recent Chats, the Salon list, a
 * character's Conversations, a Prospero project's chats (v4
 * `components/chat/ConciergeMark.tsx`, new at `c43d3b1b4`).
 *
 * It reads the derived four-state, never the raw danger label, so the three
 * states other than Monitored each get their own tone: red for the Concierge's
 * own verdict, grey for a chat you vouched safe, blue for a door you opened
 * yourself. Monitored is the default and wears nothing — the mark means
 * "something other than the default is in force," exactly as the Salon
 * header's pill does.
 *
 * The words come from the presentation table, so the mark, the pill and the
 * sidebar all say the same thing. The bubble is Quilltap's own {@link Tooltip}
 * rather than a native `title`: under the desktop shell the OS widget is
 * unreliable, which is why the Salon's message buttons already moved off it.
 *
 * Host note: `display: contents`, the ConfirmationBadge idiom — v4 renders no
 * wrapper element at all, so the tooltip anchor must be the list row's own
 * flex child. It also sidesteps the unstyled-custom-element `display: inline`
 * trap; the `qt-concierge-mark` CLASS on the span inside is a different thing
 * from this element's name, and has its own rule in `_chat.css`.
 */
@Component({
  selector: 'qt-concierge-mark',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Tooltip, ConciergeTooltipBody],
  host: { style: 'display: contents' },
  template: `
    @if (conciergeState() !== 'monitored') {
      <qt-tooltip [contentTemplate]="tip" placement="top">
        <!--
          Deliberately not focusable and not pinnable: the mark sits inside a
          router link, so a click must reach the link and navigate, and a
          focusable child of a link is worse than the tooltip gap it would
          close. Keyboard users get the aria-label; the sidebar's Chat section
          is the full-text home of the same words.
        -->
        <span role="img" [attr.aria-label]="spoken()" [class]="classes()">*</span>
      </qt-tooltip>
      <ng-template #tip>
        <qt-concierge-tooltip-body [description]="description()" />
      </ng-template>
    }
  `,
})
export class ConciergeMark {
  /** The derived four-state. Monitored renders nothing at all. */
  readonly conciergeState = input.required<ConciergeState>();
  /** The classifier's categories; surfaced on the bubble for Flagged only. */
  readonly dangerCategories = input<string[] | undefined>(undefined);
  /** Extra classes for the mark itself (sizing, flex behaviour). */
  readonly className = input('');

  protected readonly description = computed<ConciergeStateDescription>(() =>
    describeConciergeState(this.conciergeState(), this.dangerCategories()),
  );

  protected readonly spoken = computed(
    () => `Concierge: ${CONCIERGE_STATE_PRESENTATION[this.conciergeState()].label}`,
  );

  protected readonly classes = computed(() =>
    conciergeMarkClasses(this.conciergeState(), this.className()),
  );
}
