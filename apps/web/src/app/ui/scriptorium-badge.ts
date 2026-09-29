import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import { Icon } from './icon';
import { Tooltip } from './tooltip';

/** The three Scriptorium render states carried on every chat summary. */
export type ScriptoriumStatus = 'none' | 'rendered' | 'embedded';

/**
 * The Scriptorium status badge (v4 `components/chat/ChatCard.tsx:250-275`): a
 * three-state pill (not-rendered / rendered / rendered-and-embedded) that, when
 * clicked, queues an on-demand conversation render with a full re-embed. The
 * colour + title logic was previously duplicated read-only on the character
 * Conversations card; lifted here so both card sites share it and gain the
 * click-to-re-render affordance.
 *
 * Presentational only — the click preventDefault/stopPropagations (the badge
 * lives inside the card's `<a>`) and emits `render`; the host card owns the
 * dispatch, toast, and queue nudge.
 *
 * The three strings are v4's `SCRIPTORIUM_TOOLTIPS` at `f7f3d7bf0`
 * (`components/chat/ChatCard.tsx:98-103`, reworded there), carried through the
 * in-app `qt-tooltip` and repeated as the `aria-label`; the button keeps no
 * native `title` (it would double up on the bubble).
 */
@Component({
  selector: 'qt-scriptorium-badge',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, Tooltip],
  template: `
    <qt-tooltip [content]="badgeTitle()">
      <button
        type="button"
        class="chat-card__badge inline-flex items-center gap-1 rounded-full px-2 py-0.5 qt-body-sm font-semibold flex-shrink-0 transition-colors cursor-pointer"
        [class]="badgeClass()"
        [attr.aria-label]="badgeTitle()"
        [disabled]="busy()"
        (click)="onClick($event)"
      >
        <qt-icon name="file" class="w-3 h-3" />
      </button>
    </qt-tooltip>
  `,
})
export class ScriptoriumBadge {
  readonly status = input.required<ScriptoriumStatus>();
  /** Reflect an in-flight dispatch (the host card sets this while rendering). */
  readonly busy = input(false);
  readonly render = output<void>();

  protected readonly badgeClass = computed(() => {
    switch (this.status()) {
      case 'embedded':
        return 'qt-bg-success/10 qt-text-success hover:qt-bg-success/20';
      case 'rendered':
        return 'qt-bg-warning/10 qt-text-warning hover:qt-bg-warning/20';
      default:
        return 'qt-bg-destructive/10 qt-text-destructive hover:qt-bg-destructive/20';
    }
  });

  protected readonly badgeTitle = computed(() => {
    switch (this.status()) {
      case 'embedded':
        return 'Scriptorium: transcribed and indexed, every word findable — click to re-render';
      case 'rendered':
        return 'Scriptorium: transcribed, the indexing still under way — click to re-render';
      default:
        return 'Scriptorium: not yet transcribed — click to render and index';
    }
  });

  protected onClick(event: Event): void {
    event.preventDefault();
    event.stopPropagation();
    this.render.emit();
  }
}
