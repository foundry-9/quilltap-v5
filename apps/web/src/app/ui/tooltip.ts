import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  TemplateRef,
  afterRenderEffect,
  booleanAttribute,
  computed,
  effect,
  inject,
  input,
  signal,
  untracked,
  viewChild,
} from '@angular/core';
import { NgTemplateOutlet } from '@angular/common';

/**
 * A tooltip that Quilltap draws itself, rather than deferring to the native
 * `title` attribute (v4 `components/ui/Tooltip.tsx` at `0bd84139`).
 *
 * Native tooltips are an OS/Chromium widget: they demand a second of perfectly
 * still hovering, evaporate at the smallest twitch of the pointer, refuse to
 * return until you leave and re-enter, truncate long text, and cannot be read
 * at leisure. Under the desktop shell that unreliability is glaring. This
 * bubble is ours — themed, positioned, flipped away from viewport edges, and
 * optionally pinnable so a substantial note can be read (and selected) in peace.
 *
 * The trigger is wrapped in a layout-neutral anchor that carries the hover and
 * focus listeners; the wrapped control keeps its own `aria-label` (an icon-only
 * button still needs one — a tooltip is not an accessible name), and should
 * carry no `title`, or the native tooltip will double up on ours.
 *
 * Shape notes for the port:
 *  - v4 wraps the trigger in a `<span class="qt-tooltip-anchor">`; here the
 *    component HOST is the anchor (it takes the same class, whose
 *    `inline-flex items-center` also supplies the explicit display a custom
 *    element host otherwise lacks — the finding-#97/#105 class). v4's
 *    `anchorClassName` prop maps to plain `class=` on `<qt-tooltip>`, which
 *    Angular merges onto the host.
 *  - v4's `content` prop takes a ReactNode. Here a plain tip is the `content`
 *    string input and a structured one is the `contentTemplate` input
 *    (`TemplateRef`), rendered inside the bubble — the ConfirmationBadge is
 *    the structured consumer.
 *  - v4 portals the bubble to `document.body` with `createPortal`; here the
 *    bubble renders under `@if` and an `afterRenderEffect` reparents it (the
 *    `image-detail-modal.ts` idiom — a constructor-time reparent of an
 *    `@if`-mounted node is silently undone by the insertion that follows it).
 *  - v4's React `onFocus`/`onBlur` are delegated `focusin`/`focusout` (they
 *    fire when the wrapped control gains focus), so those are the host events
 *    bound here — a plain `focus` listener on the anchor would never fire.
 */

export type TooltipPlacement = 'top' | 'bottom';

/** px kept clear of every viewport edge */
const VIEWPORT_MARGIN = 8;
/** px between the trigger and the bubble */
const ANCHOR_GAP = 6;
/** grace period letting the pointer cross the gap into an interactive bubble */
const CLOSE_GRACE_MS = 120;

interface TooltipCoords {
  top: number;
  left: number;
  placement: TooltipPlacement;
}

@Component({
  selector: 'qt-tooltip',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [NgTemplateOutlet],
  host: {
    class: 'qt-tooltip-anchor',
    '(pointerenter)': 'openAfterDelay()',
    '(pointerleave)': 'onAnchorPointerLeave()',
    '(focusin)': 'openNow()',
    '(focusout)': 'onAnchorFocusOut()',
    '(click)': 'onAnchorClick()',
  },
  template: `
    <ng-content />
    @if (open() && hasContent()) {
      <div
        #bubble
        role="tooltip"
        aria-hidden="true"
        class="qt-tooltip"
        [class]="bubbleClassName()"
        [attr.data-interactive]="bubbleInteractive() ? 'true' : null"
        [attr.data-pinned]="pinned() ? 'true' : null"
        [attr.data-placement]="coords()?.placement ?? placement()"
        [style.top.px]="coords()?.top ?? 0"
        [style.left.px]="coords()?.left ?? 0"
        [style.visibility]="coords() ? 'visible' : 'hidden'"
        (pointerenter)="onBubblePointerEnter()"
        (pointerleave)="onBubblePointerLeave()"
      >
        @if (contentTemplate(); as tpl) {
          <ng-container [ngTemplateOutlet]="tpl" />
        } @else {
          {{ content() }}
        }
      </div>
    }
  `,
})
export class Tooltip {
  /** Bubble contents — a string for a plain tip (v4 `content` as string). */
  readonly content = input<string | null>(null);
  /** Bubble contents — a template for a structured tip (v4 `content` as nodes). */
  readonly contentTemplate = input<TemplateRef<unknown> | null>(null);
  /** Preferred side; flipped automatically when the viewport is tight. */
  readonly placement = input<TooltipPlacement>('top');
  /** ms of hover before the bubble appears. */
  readonly delay = input(200);
  /** Clicking the trigger pins the bubble open until dismissed. */
  readonly pinnable = input(false, { transform: booleanAttribute });
  /** Let the pointer enter the bubble (needed to scroll or select its text). */
  readonly interactive = input(false, { transform: booleanAttribute });
  /** Extra classes for the bubble (v4 `className`). */
  readonly bubbleClassName = input('');

  protected readonly open = signal(false);
  protected readonly pinned = signal(false);
  protected readonly coords = signal<TooltipCoords | null>(null);

  protected readonly bubbleInteractive = computed(() => this.interactive() || this.pinned());
  protected readonly hasContent = computed(
    () => this.content() != null || this.contentTemplate() != null,
  );

  private readonly bubbleRef = viewChild<ElementRef<HTMLDivElement>>('bubble');
  private readonly hostEl = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;

  private openTimer: ReturnType<typeof setTimeout> | null = null;
  private closeTimer: ReturnType<typeof setTimeout> | null = null;
  /** The bubble node currently living on document.body (see the effect below). */
  private portalledBubble: HTMLElement | null = null;

  constructor() {
    // Reparent the bubble to document.body and place it as soon as (and every
    // time) it renders — v4's createPortal + useLayoutEffect pair. Reading
    // `content`/`contentTemplate` here mirrors v4's `[open, position, content]`
    // deps: changed contents re-measure and re-place the bubble.
    //
    // The reparented node must also be removed BY HAND when the `@if` closes:
    // Angular tears the embedded view down against the container it created the
    // nodes in, so a node moved to document.body outlives its view (measured —
    // closed bubbles accumulated on the body; the image-detail-modal's explicit
    // `host.remove()` exists for the same reason).
    afterRenderEffect(() => {
      const bubble = this.bubbleRef()?.nativeElement ?? null;
      this.content();
      this.contentTemplate();
      if (this.portalledBubble && this.portalledBubble !== bubble) {
        this.portalledBubble.remove();
        this.portalledBubble = null;
      }
      if (!bubble || typeof document === 'undefined') return;
      if (bubble.parentElement !== document.body) {
        document.body.appendChild(bubble);
      }
      this.portalledBubble = bubble;
      untracked(() => this.position());
    });

    // Follow the anchor while the page moves under it; dismiss on Escape or on
    // a click elsewhere (only a pinned bubble survives long enough to need
    // that). All listeners removed on close (v4 Tooltip.tsx:156-185).
    effect((onCleanup) => {
      if (!this.open()) return;

      let frame = 0;
      const reposition = () => {
        if (frame) return;
        frame = requestAnimationFrame(() => {
          frame = 0;
          this.position();
        });
      };
      const onKeyDown = (event: KeyboardEvent) => {
        if (event.key === 'Escape') this.closeNow();
      };
      const onPointerDown = (event: Event) => {
        // Reads pinned OUTSIDE the reactive context, as v4 reads pinnedRef.
        if (!untracked(this.pinned)) return;
        const target = event.target as Node;
        if (this.hostEl.contains(target)) return;
        const bubble = untracked(this.bubbleRef)?.nativeElement;
        if (bubble?.contains(target)) return;
        this.closeNow();
      };

      window.addEventListener('scroll', reposition, true);
      window.addEventListener('resize', reposition);
      document.addEventListener('keydown', onKeyDown);
      document.addEventListener('pointerdown', onPointerDown, true);
      onCleanup(() => {
        if (frame) cancelAnimationFrame(frame);
        window.removeEventListener('scroll', reposition, true);
        window.removeEventListener('resize', reposition);
        document.removeEventListener('keydown', onKeyDown);
        document.removeEventListener('pointerdown', onPointerDown, true);
      });
    });

    inject(DestroyRef).onDestroy(() => {
      this.clearTimers();
      this.portalledBubble?.remove();
      this.portalledBubble = null;
    });
  }

  private clearTimers(): void {
    if (this.openTimer) {
      clearTimeout(this.openTimer);
      this.openTimer = null;
    }
    if (this.closeTimer) {
      clearTimeout(this.closeTimer);
      this.closeTimer = null;
    }
  }

  protected openNow(): void {
    this.clearTimers();
    this.open.set(true);
  }

  protected openAfterDelay(): void {
    this.clearTimers();
    this.openTimer = setTimeout(() => this.open.set(true), this.delay());
  }

  private closeNow(): void {
    this.clearTimers();
    this.pinned.set(false);
    this.open.set(false);
    this.coords.set(null);
  }

  /** Close unless the pointer arrives in the bubble first, or it is pinned. */
  private closeSoon(): void {
    this.clearTimers();
    this.closeTimer = setTimeout(() => {
      if (this.pinned()) return;
      this.open.set(false);
      this.coords.set(null);
    }, CLOSE_GRACE_MS);
  }

  protected onAnchorPointerLeave(): void {
    if (!this.pinned()) this.closeSoon();
  }

  protected onAnchorFocusOut(): void {
    if (!this.pinned()) this.closeNow();
  }

  protected onAnchorClick(): void {
    if (!this.pinnable()) return;
    this.clearTimers();
    const next = !this.pinned();
    this.pinned.set(next);
    this.open.set(next);
    if (!next) this.coords.set(null);
  }

  protected onBubblePointerEnter(): void {
    if (this.bubbleInteractive()) this.clearTimers();
  }

  protected onBubblePointerLeave(): void {
    if (this.bubbleInteractive() && !this.pinned()) this.closeSoon();
  }

  /** Measure the anchor and the (already rendered) bubble, then place it. */
  private position(): void {
    const bubble = this.bubbleRef()?.nativeElement;
    if (!bubble) return;

    const a = this.hostEl.getBoundingClientRect();
    const b = bubble.getBoundingClientRect();

    let side = this.placement();
    if (side === 'top' && a.top - b.height - ANCHOR_GAP < VIEWPORT_MARGIN) {
      side = 'bottom';
    } else if (
      side === 'bottom' &&
      a.bottom + b.height + ANCHOR_GAP > window.innerHeight - VIEWPORT_MARGIN
    ) {
      side = 'top';
    }

    const top = side === 'top' ? a.top - b.height - ANCHOR_GAP : a.bottom + ANCHOR_GAP;
    const centred = a.left + a.width / 2 - b.width / 2;
    const rightmost = Math.max(VIEWPORT_MARGIN, window.innerWidth - b.width - VIEWPORT_MARGIN);
    const left = Math.min(Math.max(centred, VIEWPORT_MARGIN), rightmost);

    // Identity-stable, as v4's setCoords is — a same-values write under the
    // rAF reposition loop would otherwise re-render every frame.
    const prev = this.coords();
    if (prev && prev.top === top && prev.left === left && prev.placement === side) return;
    this.coords.set({ top, left, placement: side });
  }
}
