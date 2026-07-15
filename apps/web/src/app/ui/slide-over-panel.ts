import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  effect,
  inject,
  input,
  output,
  untracked,
  viewChild,
} from '@angular/core';

import { Icon } from './icon';

/**
 * A reusable slide-over panel that slides in from the right edge of the viewport
 * (v4 `components/ui/SlideOverPanel.tsx`). The LLM Inspector is its only consumer
 * today; v4 keeps it generic and so do we.
 *
 * ## Two v4 structures are load-bearing, not incidental
 *
 *  - **The scrim and panel are ALWAYS mounted**, carrying `data-open` (v4 :106,
 *    :115). Both `qt-slide-over-*` rules animate on that attribute
 *    (`_surfaces.css:1067-1102` — opacity for the scrim, `translateX(100%) → 0`
 *    for the panel), so an `@if` around the markup would swap the transition for
 *    a hard cut. `pointer-events: none` on the closed scrim is what keeps the
 *    always-mounted overlay from eating clicks.
 *  - **Focus is saved on open and restored on close** (v4 :39-50): the panel
 *    itself takes focus (it is `tabindex="-1"`) one frame after opening, and the
 *    previously-focused element gets it back on close. Without the restore, the
 *    toolbar button that opened the panel loses the caret on every close.
 *
 * PORTAL DIVERGENCE: v4 renders through `createPortal(…, document.body)` because
 * React composes the panel deep inside SalonView's tree. v5 renders inline — the
 * same choice {@link import('./modal').Modal} already made — because the CSS is
 * `position: fixed` and the host element adds no containing block. (A `transform`
 * or `filter` on an ancestor WOULD make `fixed` resolve against it; nothing in the
 * chat layout carries one.)
 *
 * ## CLOSED-STATE DIVERGENCE (deliberate — v4 has a defect here)
 *
 * v4 hard-codes `role="dialog"` + `aria-modal="true"` on the panel, and because
 * the panel is always mounted, EVERY Salon page permanently contains what
 * announces itself as an open modal dialog — one that is merely translated
 * off-screen. Two consequences, both real: assistive tech is told a modal owns
 * the page at all times, and the closed panel's controls (close, refresh, the
 * filter select) stay tab-reachable, so keyboard users tab into an invisible
 * panel.
 *
 * v5 therefore declares the dialog role ONLY while open, and marks the closed
 * panel `inert`. The markup, classes and `data-open` transitions are unchanged —
 * this is v4's INTENT (a dialog when there is a dialog) without the artifact, the
 * same call `message-row.ts` makes about v4's stray "0" token glyph. It is also
 * what keeps `getByRole('dialog')` honest for every other Salon spec.
 */
@Component({
  selector: 'qt-slide-over-panel',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <!-- Scrim backdrop (v4 :103-109). A click on the scrim ITSELF closes; the
         target check is what stops a click bubbling out of the panel from
         closing it. -->
    <div
      class="qt-slide-over-overlay"
      [attr.data-open]="isOpen()"
      aria-hidden="true"
      (click)="onScrimClick($event)"
    ></div>

    <div
      #panel
      class="qt-slide-over-panel"
      [class]="klass()"
      [attr.data-open]="isOpen()"
      [style.width]="width()"
      [attr.role]="isOpen() ? 'dialog' : null"
      [attr.aria-modal]="isOpen() ? 'true' : null"
      [attr.inert]="isOpen() ? null : ''"
      [attr.aria-label]="ariaLabel() || title()"
      tabindex="-1"
      (keydown)="onKeydown($event)"
    >
      <div class="qt-slide-over-header">
        <h2 class="text-base font-semibold qt-text truncate">{{ title() }}</h2>
        <div class="flex items-center gap-2">
          <ng-content select="[qt-slide-over-actions]"></ng-content>
          <button
            type="button"
            class="qt-text-secondary hover:text-foreground transition-colors p-1"
            aria-label="Close panel"
            (click)="close.emit()"
          >
            <qt-icon name="close" class="w-5 h-5" />
          </button>
        </div>
      </div>

      <ng-content></ng-content>
    </div>
  `,
})
export class SlideOverPanel {
  readonly isOpen = input.required<boolean>();
  readonly title = input.required<string>();
  /** v4's `width` prop and its default (`SlideOverPanel.tsx:30`). */
  readonly width = input('min(480px, 85vw)');
  /** v4's `className` — extra classes on the panel element. */
  readonly klass = input('', { alias: 'class' });
  /** v4's `ariaLabel`, falling back to the title (v4 :119). */
  readonly ariaLabel = input<string | undefined>(undefined);
  readonly close = output<void>();

  private readonly panel = viewChild<ElementRef<HTMLElement>>('panel');

  /**
   * v4's `headerActions` prop is a ReactNode slot; Angular's equivalent is a
   * named `ng-content`. Consumers project with `<div qt-slide-over-actions>`.
   */
  constructor() {
    // v4 :39-50 — save/restore focus around the open transition.
    let previousFocus: HTMLElement | null = null;
    effect(() => {
      const open = this.isOpen();
      untracked(() => {
        if (open) {
          previousFocus = document.activeElement as HTMLElement | null;
          // v4 uses requestAnimationFrame: the panel must be laid out before it
          // can take focus.
          requestAnimationFrame(() => this.panel()?.nativeElement.focus());
        } else if (previousFocus) {
          previousFocus.focus();
          previousFocus = null;
        }
      });
    });

    // Escape closes (v4 :53-65). The listener is DOCUMENT-level and attached only
    // while open — v4 `stopPropagation`s so an outer Escape handler (the Salon's
    // terminal-focus exit) does not also fire on the same key.
    const onDocumentKeydown = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        event.stopPropagation();
        this.close.emit();
      }
    };
    effect((onCleanup) => {
      if (!this.isOpen()) return;
      document.addEventListener('keydown', onDocumentKeydown);
      onCleanup(() => document.removeEventListener('keydown', onDocumentKeydown));
    });

    inject(DestroyRef).onDestroy(() => document.removeEventListener('keydown', onDocumentKeydown));
  }

  protected onScrimClick(event: MouseEvent): void {
    // v4 :68-72 — only a click on the scrim itself, never a bubbled one.
    if (event.target === event.currentTarget) {
      this.close.emit();
    }
  }

  /**
   * The focus trap (v4 :75-97): Tab off the last focusable wraps to the first,
   * Shift+Tab off the first wraps to the last. v4's selector list is carried
   * verbatim — it is what decides which controls are in the cycle.
   */
  protected onKeydown(event: KeyboardEvent): void {
    if (event.key !== 'Tab') return;
    const panel = this.panel()?.nativeElement;
    if (!panel) return;

    const focusable = panel.querySelectorAll<HTMLElement>(
      'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
    );
    if (focusable.length === 0) return;

    const first = focusable[0];
    const last = focusable[focusable.length - 1];

    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }
}
