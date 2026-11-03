/**
 * The floating dialog — a port of v4 `components/ui/FloatingDialog.tsx`
 * (`99463fc56` + `dff76ebef`'s dialog semantics, at `236c04e35`): a floating,
 * draggable, resizable panel that persists its position and size to
 * localStorage.
 *
 * By default it sits in the panel layer, beneath modals. `topmost` lifts it
 * into the floating layer above every modal (see `floating-layer.ts`): it then
 * belongs to no modal, so using it never dismisses the one beneath.
 *
 * It is a non-modal dialog: the page behind it stays usable, so it is announced
 * with `aria-modal="false"` and Tab is not trapped. Focus moves in on open and
 * returns to the opener on close. Escape closes it when pressed inside it, or
 * anywhere while no modal dialog is open (with one open, Escape is the modal's).
 *
 * **Scoped (P4.D359 ruling R-1 / the round's R-K):** the Brahma Console is its
 * only host this round. The Inform / compose-mail / insert-announcement / Help
 * dialogs stay `qt-modal` (the `status-log.md` "No `FloatingDialog`" ruling is
 * overturned for the console alone).
 *
 * **Recorded divergence `FLOATING_DIALOG_IN_PLACE`:** v4 portals the dialog
 * into `document.body`; v5 renders it IN PLACE inside its host (v5's
 * convention — no portal). The element is `position: fixed`, so where the host
 * sits does not move it. v4's `useId` is a module counter here
 * (`qt-floating-dialog-title-<n>`), and `floatingLayerProps` a bound attribute.
 *
 * The body and the header actions are projected (`<ng-content>` and
 * `<ng-content select="[qt-floating-dialog-actions]">`). Projected content is
 * created by the HOST whether or not the dialog renders, so a host's panel —
 * and its state — outlives the dialog being hidden (v4's
 * `BrahmaConsoleDialog` keeps the same promise by staying mounted).
 *
 * @module ui/floating-dialog
 */

import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  Injector,
  afterNextRender,
  booleanAttribute,
  effect,
  inject,
  input,
  output,
  signal,
  untracked,
  viewChild,
} from '@angular/core';

import { Icon } from './icon';

export interface DialogGeometry {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** v4 `:48-53` — `-1` means "center on first render". */
export const DEFAULT_GEOMETRY: DialogGeometry = {
  x: -1,
  y: -1,
  width: 420,
  height: 520,
};

/** v4 `:55-64` — the stored geometry over the defaults, else the initial over the defaults. */
export function loadGeometry(
  storageKey: string,
  initial?: Partial<DialogGeometry>,
): DialogGeometry {
  try {
    const stored = localStorage.getItem(storageKey);
    if (stored) {
      const parsed = JSON.parse(stored);
      return { ...DEFAULT_GEOMETRY, ...parsed };
    }
  } catch {
    /* ignore */
  }
  return { ...DEFAULT_GEOMETRY, ...initial };
}

/** v4 `:66-70`. */
export function saveGeometry(storageKey: string, geometry: DialogGeometry): void {
  try {
    localStorage.setItem(storageKey, JSON.stringify(geometry));
  } catch {
    /* ignore */
  }
}

/** v4 `:72-89` — clamp to the viewport, centring a `-1` axis. */
export function constrainToViewport(
  geo: DialogGeometry,
  minW: number,
  minH: number,
): DialogGeometry {
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const w = Math.max(minW, Math.min(geo.width, vw - 20));
  const h = Math.max(minH, Math.min(geo.height, vh - 20));
  let x = geo.x;
  let y = geo.y;

  // Center if -1
  if (x < 0) x = Math.max(10, (vw - w) / 2);
  if (y < 0) y = Math.max(10, (vh - h) / 2);

  // Constrain
  x = Math.max(0, Math.min(x, vw - 40));
  y = Math.max(0, Math.min(y, vh - 40));

  return { x, y, width: w, height: h };
}

/** Module counter behind each instance's title id (v4 `useId`). */
let titleCounter = 0;

@Component({
  selector: 'qt-floating-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    @if (open()) {
      <div
        #dialog
        class="qt-floating-dialog"
        [class.qt-floating-dialog-topmost]="topmost()"
        [attr.data-qt-floating-layer]="topmost() ? '' : null"
        role="dialog"
        aria-modal="false"
        [attr.aria-labelledby]="titleId"
        tabindex="-1"
        [style.left.px]="geometry().x"
        [style.top.px]="geometry().y"
        [style.width.px]="geometry().width"
        [style.height.px]="geometry().height"
        [style.min-width.px]="minWidth()"
        [style.min-height.px]="minHeight()"
        (keydown)="onInsideKeydown($event)"
      >
        <div class="qt-floating-dialog-header" (mousedown)="onHeaderMouseDown($event)">
          <div [id]="titleId" class="qt-floating-dialog-title">{{ title() }}</div>
          <div class="qt-floating-dialog-actions">
            <ng-content select="[qt-floating-dialog-actions]" />
            <button
              type="button"
              class="qt-floating-dialog-close"
              title="Close"
              aria-label="Close"
              (click)="close.emit()"
            >
              <qt-icon name="close" class="w-4 h-4" />
            </button>
          </div>
        </div>
        <div class="qt-floating-dialog-body">
          <ng-content />
        </div>
      </div>
    }
  `,
})
export class FloatingDialog {
  private readonly injector = inject(Injector);

  readonly open = input.required<boolean>();
  readonly title = input.required<string>();
  readonly storageKey = input('quilltap:help-dialog-geometry');
  readonly initialGeometry = input<Partial<DialogGeometry>>({});
  readonly minWidth = input(320);
  readonly minHeight = input(300);
  /** Float above every modal instead of beneath them. */
  readonly topmost = input(false, { transform: booleanAttribute });
  readonly close = output<void>();
  /** v4 `onGeometryChange`. */
  readonly geometryChange = output<DialogGeometry>();

  /** v4 `useId` — a per-instance id for `aria-labelledby`. */
  readonly titleId = `qt-floating-dialog-title-${++titleCounter}`;

  protected readonly geometry = signal<DialogGeometry>(DEFAULT_GEOMETRY);
  private seeded = false;
  private readonly dialogRef = viewChild<ElementRef<HTMLElement>>('dialog');
  private dragging = false;
  private dragOffset = { x: 0, y: 0 };

  constructor() {
    effect((onCleanup) => {
      if (!this.open()) return;
      untracked(() => this.openSession(onCleanup));
    });
  }

  /**
   * One open period: the document listeners, the resize observer, focus in on
   * a frame, and — when it ends (closed or destroyed) — focus back to the
   * opener (v4 `useDialogFocus` `:25-46`, the Tab trap NOT attached).
   */
  private openSession(onCleanup: (fn: () => void) => void): void {
    // The opener, captured before any focus moves.
    const opener = document.activeElement as HTMLElement | null;
    if (!this.seeded) {
      // v4 seeds its geometry state once, from the props it mounts with.
      this.seeded = true;
      this.geometry.set(
        constrainToViewport(
          loadGeometry(this.storageKey(), this.initialGeometry()),
          this.minWidth(),
          this.minHeight(),
        ),
      );
    }

    // v4 `:179-196` — Escape anywhere else closes it only while no modal
    // dialog is open (with one open, that Escape belongs to the modal).
    const onDocumentKeydown = (e: KeyboardEvent): void => {
      if (e.key !== 'Escape' || e.defaultPrevented) return;
      const el = this.dialogRef()?.nativeElement;
      if (el && el.contains(e.target as Node)) return;
      if (document.querySelector('[aria-modal="true"]')) return;
      this.close.emit();
    };
    // v4 `:131-154` — the header drag.
    const onMouseMove = (e: MouseEvent): void => {
      if (!this.dragging) return;
      this.updateGeometry({
        ...this.geometry(),
        x: e.clientX - this.dragOffset.x,
        y: e.clientY - this.dragOffset.y,
      });
    };
    const onMouseUp = (): void => {
      this.dragging = false;
    };
    document.addEventListener('keydown', onDocumentKeydown);
    document.addEventListener('mousemove', onMouseMove);
    document.addEventListener('mouseup', onMouseUp);

    let frame = 0;
    let observer: ResizeObserver | null = null;
    const ready = afterNextRender(
      () => {
        const el = this.dialogRef()?.nativeElement;
        if (!el) return;
        // v4 `:156-170` — CSS `resize: both` does the resizing; record it.
        if (typeof ResizeObserver !== 'undefined') {
          observer = new ResizeObserver((entries) => {
            for (const entry of entries) {
              const { width, height } = entry.contentRect;
              const geo = this.geometry();
              if (Math.abs(width - geo.width) > 2 || Math.abs(height - geo.height) > 2) {
                this.updateGeometry({ ...geo, width, height });
              }
            }
          });
          observer.observe(el);
        }
        // v4 `useDialogFocus` `:37-42` — focus in unless it already holds focus.
        frame = requestAnimationFrame(() => {
          if (!el.contains(document.activeElement)) el.focus();
        });
      },
      { injector: this.injector },
    );

    onCleanup(() => {
      ready.destroy();
      cancelAnimationFrame(frame);
      observer?.disconnect();
      this.dragging = false;
      document.removeEventListener('keydown', onDocumentKeydown);
      document.removeEventListener('mousemove', onMouseMove);
      document.removeEventListener('mouseup', onMouseUp);
      if (opener && opener.isConnected) opener.focus();
    });
  }

  /** v4 `:113-118` — constrain → set → save → notify. */
  private updateGeometry(next: DialogGeometry): void {
    const constrained = constrainToViewport(next, this.minWidth(), this.minHeight());
    this.geometry.set(constrained);
    saveGeometry(this.storageKey(), constrained);
    this.geometryChange.emit(constrained);
  }

  /**
   * v4 `:175-178` — Escape inside the dialog closes it; a topmost one swallows
   * the key so the modal beneath doesn't close too.
   */
  protected onInsideKeydown(e: KeyboardEvent): void {
    if (e.key !== 'Escape' || e.defaultPrevented) return;
    if (this.topmost()) e.stopPropagation();
    this.close.emit();
  }

  /** v4 `:121-129` — only the title bar drags, never its buttons. */
  protected onHeaderMouseDown(e: MouseEvent): void {
    if ((e.target as HTMLElement).closest('button')) return;
    e.preventDefault();
    this.dragging = true;
    const geo = this.geometry();
    this.dragOffset = { x: e.clientX - geo.x, y: e.clientY - geo.y };
  }
}
