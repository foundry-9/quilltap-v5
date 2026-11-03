/**
 * The card an absent character's name chip shows — v4 `MentionChip.tsx`'s
 * `MentionCard` (`9a88f862f`, `:36-60` at `236c04e35`) inside v4's interactive,
 * pinnable tooltip bubble.
 *
 * **Recorded divergence `MENTION_CARD_SHARED_POPOVER` (ruling R-2).** v4 wraps
 * every chip in its own `Tooltip interactive pinnable delay={250}`. A v5 chip is
 * a `<span>` inside a message's `[innerHTML]`, which no Angular component can
 * wrap, so ONE card per message is positioned under the active chip by
 * {@link import('./mention-chip-host').MentionChipHost} with the same delay,
 * grace and pin rules — and the tooltip's own bubble classes and top-first
 * placement (`ui/tooltip.ts`).
 *
 * @module chat/absent-mentions/mention-card
 */

import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import type { AbsentMentionCard } from '../../core/core-contract';

/** px kept clear of every viewport edge (`ui/tooltip.ts`). */
const VIEWPORT_MARGIN = 8;
/** px between the chip and the card (`ui/tooltip.ts`). */
const ANCHOR_GAP = 6;

@Component({
  selector: 'qt-mention-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div
      role="tooltip"
      aria-hidden="true"
      class="qt-tooltip"
      data-interactive="true"
      [attr.data-pinned]="pinned() ? 'true' : null"
      [attr.data-placement]="coords()?.placement ?? 'top'"
      [style.top.px]="coords()?.top ?? 0"
      [style.left.px]="coords()?.left ?? 0"
      [style.visibility]="coords() ? 'visible' : 'hidden'"
      (pointerenter)="pointerEnter.emit()"
      (pointerleave)="pointerLeave.emit()"
    >
      <div class="qt-tooltip-body">
        <div class="qt-tooltip-title">{{ card().name }}</div>
        @if (card().aliases.length > 0) {
          <div class="qt-tooltip-section">
            <div class="qt-tooltip-section-label">Also known as</div>
            <div>{{ card().aliases.join(', ') }}</div>
          </div>
        }
        @if (card().pronouns) {
          <div class="qt-tooltip-section">
            <div class="qt-tooltip-section-label">Pronouns</div>
            <div>{{ card().pronouns }}</div>
          </div>
        }
        @if (card().body) {
          <div class="qt-tooltip-section">
            <div class="qt-tooltip-quote">{{ card().body }}</div>
          </div>
        }
        <div class="qt-tooltip-hint">Not in this conversation · click to pin</div>
      </div>
    </div>
  `,
})
export class MentionCard {
  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef);

  readonly card = input.required<AbsentMentionCard>();
  readonly pinned = input(false);
  readonly pointerEnter = output<void>();
  readonly pointerLeave = output<void>();

  protected readonly coords = signal<{
    top: number;
    left: number;
    placement: 'top' | 'bottom';
  } | null>(null);

  /** The card's own element (the delegation tests "is the pointer in the card?"). */
  get element(): HTMLElement {
    return this.host.nativeElement;
  }

  /** Place the card above the chip (below when it would not fit) — `ui/tooltip.ts`'s math. */
  positionUnder(anchor: Element): void {
    const bubble = this.host.nativeElement.firstElementChild as HTMLElement | null;
    if (!bubble) return;
    const a = anchor.getBoundingClientRect();
    const b = bubble.getBoundingClientRect();
    const side: 'top' | 'bottom' =
      a.top - b.height - ANCHOR_GAP < VIEWPORT_MARGIN ? 'bottom' : 'top';
    const top = side === 'top' ? a.top - b.height - ANCHOR_GAP : a.bottom + ANCHOR_GAP;
    const centred = a.left + a.width / 2 - b.width / 2;
    const rightmost = Math.max(VIEWPORT_MARGIN, window.innerWidth - b.width - VIEWPORT_MARGIN);
    const left = Math.min(Math.max(centred, VIEWPORT_MARGIN), rightmost);
    this.coords.set({ top, left, placement: side });
  }
}
