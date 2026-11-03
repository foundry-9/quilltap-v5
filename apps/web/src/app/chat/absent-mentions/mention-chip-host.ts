/**
 * The name chips' behaviour on a rendered message — v4 `MentionChip.tsx`'s
 * `MentionChip` + `MentionText` + `withMentionChips` (`9a88f862f`, at
 * `236c04e35`), as ONE directive on `qt-message-content[mentions]`.
 *
 * It owns the `mentions` binding (the message's absent mentions, from
 * {@link import('./absent-mentions.api').injectAbsentMentions}). After every
 * render of the message it marks the rendered DOM
 * ({@link markMentionsInElement}) — on the `qt-message-rendered` event
 * `message-content.ts`'s fenced hook dispatches once its own post-passes ran,
 * and again whenever the mentions change. Then, DELEGATED on the host:
 *
 *  - hover opens the card after v4's `delay` (250 ms); focus opens it at once
 *    (`ui/tooltip.ts`'s focus path); the pointer may cross into the card;
 *  - a click — or Enter / Space on the focused chip (WCAG R8: a
 *    `role="button"` span acts on the keyboard as on a click) — pins it, and
 *    again unpins it;
 *  - a pinned card stays until a click outside it, or Escape; neither fires for
 *    an interaction inside the topmost floating layer (the Brahma Console).
 *
 * ONE card per message ({@link MentionCard} — `MENTION_CARD_SHARED_POPOVER`).
 *
 * @module chat/absent-mentions/mention-chip-host
 */

import {
  DestroyRef,
  Directive,
  ElementRef,
  Injector,
  ViewContainerRef,
  afterNextRender,
  effect,
  inject,
  input,
  untracked,
  type ComponentRef,
} from '@angular/core';

import type { AbsentMention } from '../../core/core-contract';
import { isInFloatingLayer } from '../../ui/floating-layer';
import { MentionCard } from './mention-card';
import { MENTION_CHIP_CLASS, markMentionsInElement } from './mention-markers';

/** v4 `MentionChip` — `<Tooltip … delay={250}>`. */
const OPEN_DELAY_MS = 250;
/** `ui/tooltip.ts` — the grace letting the pointer cross into the card. */
const CLOSE_GRACE_MS = 120;

/** The event `message-content.ts`'s P4.D359 fence dispatches after each render. */
export const MESSAGE_RENDERED_EVENT = 'qt-message-rendered';

@Directive({
  selector: 'qt-message-content[mentions]',
  host: {
    [`(${MESSAGE_RENDERED_EVENT})`]: 'mark()',
    '(mouseover)': 'onMouseOver($event)',
    '(mouseout)': 'onMouseOut($event)',
    '(focusin)': 'onFocusIn($event)',
    '(focusout)': 'onFocusOut($event)',
    '(click)': 'onClick($event)',
    '(keydown)': 'onKeyDown($event)',
  },
})
export class MentionChipHost {
  readonly mentions = input<readonly AbsentMention[] | null>(null);

  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef);
  private readonly vcr = inject(ViewContainerRef);
  private readonly injector = inject(Injector);

  private card: ComponentRef<MentionCard> | null = null;
  /** The chip the card is showing for. */
  private active: HTMLElement | null = null;
  private pinned = false;
  private openTimer: ReturnType<typeof setTimeout> | null = null;
  private closeTimer: ReturnType<typeof setTimeout> | null = null;

  constructor() {
    // The mentions arrive after the message has rendered (one read per chat):
    // mark once the next render settles.
    effect(() => {
      this.mentions();
      afterNextRender(() => this.mark(), { injector: this.injector });
    });
    inject(DestroyRef).onDestroy(() => this.close());
  }

  /** Mark the rendered message (idempotent — a previous pass's chips are unwrapped first). */
  mark(): void {
    const mentions = untracked(this.mentions) ?? [];
    markMentionsInElement(this.host.nativeElement, mentions);
    // The chip the card pointed at is gone with the old DOM.
    if (this.active && !this.host.nativeElement.contains(this.active)) this.close();
  }

  // --- delegation --------------------------------------------------------

  private chipOf(target: EventTarget | null): HTMLElement | null {
    if (!(target instanceof Element)) return null;
    const chip = target.closest(`.${MENTION_CHIP_CLASS}`) as HTMLElement | null;
    return chip && this.host.nativeElement.contains(chip) ? chip : null;
  }

  protected onMouseOver(event: MouseEvent): void {
    const chip = this.chipOf(event.target);
    if (!chip) return;
    this.clearClose();
    if (this.pinned || (this.active === chip && this.card)) return;
    this.clearOpen();
    this.openTimer = setTimeout(() => this.open(chip), OPEN_DELAY_MS);
  }

  protected onMouseOut(event: MouseEvent): void {
    const chip = this.chipOf(event.target);
    if (!chip) return;
    const to = event.relatedTarget;
    if (to instanceof Node && (chip.contains(to) || this.card?.instance.element.contains(to)))
      return;
    this.clearOpen();
    if (!this.pinned) this.closeSoon();
  }

  protected onFocusIn(event: FocusEvent): void {
    const chip = this.chipOf(event.target);
    if (!chip || this.pinned) return;
    this.clearOpen();
    this.open(chip);
  }

  protected onFocusOut(event: FocusEvent): void {
    if (!this.chipOf(event.target) || this.pinned) return;
    this.close();
  }

  protected onClick(event: MouseEvent): void {
    const chip = this.chipOf(event.target);
    if (chip) this.togglePin(chip);
  }

  /** WCAG R8 — Enter / Space on the focused chip does what a click does. */
  protected onKeyDown(event: KeyboardEvent): void {
    if (event.key !== 'Enter' && event.key !== ' ') return;
    const chip = this.chipOf(event.target);
    if (!chip) return;
    event.preventDefault();
    this.togglePin(chip);
  }

  // --- the card ----------------------------------------------------------

  private togglePin(chip: HTMLElement): void {
    this.clearOpen();
    if (this.pinned && this.active === chip) {
      this.close();
      return;
    }
    this.open(chip);
    this.setPinned(true);
  }

  private open(chip: HTMLElement): void {
    const index = Number(chip.getAttribute('data-mention'));
    const mention = (untracked(this.mentions) ?? [])[index];
    if (!mention) return;
    this.clearClose();
    if (!this.card) {
      this.card = this.vcr.createComponent(MentionCard);
      this.card.instance.pointerEnter.subscribe(() => this.clearClose());
      this.card.instance.pointerLeave.subscribe(() => {
        if (!this.pinned) this.closeSoon();
      });
      document.body.appendChild(this.card.location.nativeElement);
    }
    this.active = chip;
    this.card.setInput('card', mention.card);
    this.card.setInput('pinned', this.pinned);
    this.card.changeDetectorRef.detectChanges();
    this.card.instance.positionUnder(chip);
    this.card.changeDetectorRef.detectChanges();
  }

  private setPinned(pinned: boolean): void {
    if (this.pinned === pinned) return;
    this.pinned = pinned;
    this.card?.setInput('pinned', pinned);
    this.card?.changeDetectorRef.detectChanges();
    if (pinned) {
      document.addEventListener('pointerdown', this.onDocumentPointerDown, true);
      document.addEventListener('keydown', this.onDocumentKeyDown);
    } else {
      document.removeEventListener('pointerdown', this.onDocumentPointerDown, true);
      document.removeEventListener('keydown', this.onDocumentKeyDown);
    }
  }

  /** A pinned card closes on a click elsewhere (`ui/tooltip.ts`'s capture-phase idiom). */
  private readonly onDocumentPointerDown = (event: Event): void => {
    if (isInFloatingLayer(event.target)) return;
    const target = event.target as Node;
    if (this.active?.contains(target) || this.card?.instance.element.contains(target)) return;
    this.close();
  };

  /** …or on Escape. */
  private readonly onDocumentKeyDown = (event: KeyboardEvent): void => {
    if (isInFloatingLayer(event.target)) return;
    if (event.key === 'Escape') this.close();
  };

  private closeSoon(): void {
    this.clearClose();
    this.closeTimer = setTimeout(() => {
      if (!this.pinned) this.close();
    }, CLOSE_GRACE_MS);
  }

  private close(): void {
    this.clearOpen();
    this.clearClose();
    this.setPinned(false);
    this.active = null;
    if (this.card) {
      this.card.location.nativeElement.remove();
      this.card.destroy();
      this.card = null;
    }
  }

  private clearOpen(): void {
    if (this.openTimer !== null) clearTimeout(this.openTimer);
    this.openTimer = null;
  }

  private clearClose(): void {
    if (this.closeTimer !== null) clearTimeout(this.closeTimer);
    this.closeTimer = null;
  }
}
