import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import type { MessageDto } from '../core/core-contract';
import { Tooltip } from '../ui/tooltip';

/**
 * A small, unobtrusive indicator on any Salon message that carries a resolved
 * answer-confirmation verdict (`confirmed` is not undefined). It reveals the
 * cheap-LLM discrepancy notes — and, on a revision, the pre-revision text — on
 * hover, and holds them open on a click. Metadata, not an alarm; kept quiet by
 * design. (v4 `ConfirmationBadge.tsx` at `0bd84139` — its post-tooltip form,
 * which is the FIRST form v5 has ever rendered: before this port only the
 * badge's CSS had been transcribed.)
 *
 * The verdict's notes are the longest thing in the action bar and the least
 * suited to a native `title`: too long to survive Chromium's truncation, and
 * gone at the first twitch of the pointer. It is a pinnable {@link Tooltip}
 * instead — hover to glance, click to keep it open and read (or select) the
 * whole of it.
 *
 * States:
 *   confirmed true  & !revised → "Vouched"    (consistent; no notes)
 *   confirmed true  &  revised → "Amended"    (rewritten; notes + original)
 *   confirmed false            → "Stood by"   (affirmed a flagged answer; notes)
 *   confirmed null             → "Unvetted"   (check could not run)
 *
 * Tri-state note: v5's chat GET omits SQL-NULL `confirmed` /
 * `confirmationChecked` / `confirmationRevised` exactly as v4's `?? undefined`
 * does (`api/salon.rs`'s confirmation family), so `confirmed === undefined`
 * means the same thing on both sides. A LIVE unvetted verdict reaches the
 * stream bubble as `confirmed: undefined` + `confirmationChecked: true` (the
 * mapper's `?? undefined` collapse) where v4's client holds `null` — the badge
 * lands in the same `unvetted` arm either way, so the collapse is
 * behavior-neutral.
 */
@Component({
  selector: 'qt-confirmation-badge',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Tooltip],
  // v4 renders no wrapper element at all — the Tooltip anchor is the flex
  // item. `display: contents` keeps this host layout-neutral the same way
  // (and sidesteps the unstyled-custom-element `display: inline` trap).
  host: { style: 'display: contents' },
  template: `
    @if (visible()) {
      <qt-tooltip [contentTemplate]="tip" [pinnable]="hasDetail()" [interactive]="hasDetail()">
        <button
          type="button"
          class="qt-confirmation-badge qt-text-xs"
          [attr.data-confirmation-state]="view().state"
          [attr.data-has-detail]="hasDetail() ? 'true' : null"
          [attr.aria-label]="spoken()"
        >
          <span aria-hidden="true" class="qt-confirmation-badge-glyph">{{ view().glyph }}</span>
          <span class="qt-confirmation-badge-label">{{ view().label }}</span>
        </button>
      </qt-tooltip>
      <ng-template #tip>
        <div class="qt-tooltip-body">
          <p class="qt-tooltip-title">{{ view().label }}</p>
          <p>{{ view().summary }}</p>
          @if (notes()) {
            <div class="qt-tooltip-section">
              <p class="qt-tooltip-section-label">What looked off</p>
              <p class="qt-tooltip-quote">{{ notes() }}</p>
            </div>
          }
          @if (original()) {
            <div class="qt-tooltip-section">
              <p class="qt-tooltip-section-label">Originally written</p>
              <p class="qt-tooltip-quote">{{ original() }}</p>
            </div>
          }
          @if (hasDetail()) {
            <p class="qt-tooltip-hint">Click the badge to pin this note; Esc dismisses it.</p>
          }
        </div>
      </ng-template>
    }
  `,
})
export class ConfirmationBadge {
  readonly message = input.required<MessageDto>();

  /**
   * Show whenever a check ran. `confirmed` is true/false/null live, but a
   * reloaded "unverified" (null) comes back as undefined from SQL NULL — so
   * `confirmationChecked` is what tells an unverified message from an
   * unchecked one after a refresh.
   */
  private readonly checked = computed(() => this.message().confirmationChecked === true);
  protected readonly visible = computed(() => {
    const m = this.message();
    return !(this.confirmedOf(m) === undefined && !this.checked());
  });

  protected readonly notes = computed(() => this.message().confirmationNotes ?? '');
  protected readonly original = computed(() => this.message().confirmationOriginalContent ?? '');
  protected readonly hasDetail = computed(() => Boolean(this.notes() || this.original()));

  /** The wire can carry null through the live patch; the DTO types it away. */
  private confirmedOf(m: MessageDto): boolean | null | undefined {
    return m.confirmed as boolean | null | undefined;
  }

  protected readonly view = computed(() => {
    const m = this.message();
    const confirmed = this.confirmedOf(m);
    const revised = m.confirmationRevised === true;
    if (confirmed === true && revised) {
      return {
        state: 'amended',
        glyph: '✎',
        label: 'Amended',
        summary: 'On reflection the author corrected this reply to match the record.',
      } as const;
    } else if (confirmed === true) {
      return {
        state: 'vouched',
        glyph: '✓',
        label: 'Vouched',
        summary:
          'Checked against what the character recalled and looked up this turn — no contradictions found.',
      } as const;
    } else if (confirmed === false) {
      return {
        state: 'stood-by',
        glyph: '!',
        label: 'Stood by',
        summary: 'The author was asked about apparent contradictions and stood by this reply unchanged.',
      } as const;
    }
    return {
      state: 'unvetted',
      glyph: '—',
      label: 'Unvetted',
      summary: 'This reply could not be checked — the verifier was unavailable or the check timed out.',
    } as const;
  });

  /**
   * Plain-text twin of the bubble, for assistive technology and for anyone who
   * reaches the badge by keyboard rather than pointer.
   */
  protected readonly spoken = computed(() => {
    const { label, summary } = this.view();
    const notes = this.notes();
    const original = this.original();
    return [
      `Answer confirmation: ${label}. ${summary}`,
      notes ? `What looked off: ${notes}` : '',
      original ? `Originally written: ${original}` : '',
    ]
      .filter(Boolean)
      .join(' ');
  });
}
