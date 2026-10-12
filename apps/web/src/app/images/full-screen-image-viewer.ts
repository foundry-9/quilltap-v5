import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  Directive,
  ElementRef,
  afterNextRender,
  computed,
  contentChild,
  effect,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import { Icon, type IconName } from '../ui/icon';
import { DeletedImagePlaceholder } from './deleted-image-placeholder';
import { applyImageNavigation } from './image-navigation';

/** v4 `FullScreenImageAction` (`components/images/FullScreenImageViewer.tsx:25-31`). */
export interface FullScreenImageAction {
  icon: IconName;
  /** Tooltip and accessible name. */
  label: string;
  onClick: () => void;
  disabled?: boolean;
}

/**
 * Marks the content projected as the bottom-centre caption (v4 `caption`).
 * The caption box renders only when such content is projected — v4 renders it
 * only when `caption` is given (`:156-163`).
 */
@Directive({ selector: '[qtFullScreenCaption]' })
export class FullScreenImageCaption {}

/**
 * Marks the content projected into the bottom-right slot (v4 `bottomRight`,
 * conventionally a destructive action). Its wrapper renders only when such
 * content is projected AND the image is not missing (`:136`).
 */
@Directive({ selector: '[qtFullScreenBottomRight]' })
export class FullScreenImageBottomRight {}

// The frame's class strings bind through `[attr.class]`, not `[class]`:
// Angular's `[class]` string binding re-orders the tokens, and these are v4's
// strings byte for byte.

/** v4 `:59-60`. The `disabled:opacity-50` token is new versus the old chat viewer. */
const ROUND_BUTTON =
  'p-2 qt-bg-overlay-btn hover:qt-bg-overlay-btn rounded-full qt-text-overlay transition-colors cursor-pointer disabled:opacity-50';
/** v4 `:61-62` (plus `left-4` / `right-4`). */
const ARROW_BUTTON =
  'absolute top-1/2 -translate-y-1/2 p-3 qt-bg-overlay-btn hover:qt-bg-overlay-btn rounded-full qt-text-overlay transition-colors z-10 cursor-pointer';

/**
 * The shared full-screen picture frame — a port of v4
 * `components/images/FullScreenImageViewer.tsx` (NEW in `01a83539d`, read at
 * that pin; P4.D277).
 *
 * v4's module doc, carried: the frame every "look at this picture properly"
 * view shares — a blurred overlay over the whole window, the picture as large
 * as it will go, round buttons top-right, optional previous/next arrows, a
 * caption at the foot, and an optional destructive button bottom-right.
 *
 * The frame knows nothing about where a picture came from. Callers supply the
 * buttons (Save, Download, Copy, …) and the caption; the frame supplies
 * Close, Escape / arrow-key handling, body-scroll lock, and the swap to
 * `DeletedImagePlaceholder` when the bytes fail to load.
 *
 * It renders through a portal to `document.body`: inside the tabbed workspace
 * `.qt-workspace` is an isolated stacking context, and a `fixed inset-0`
 * child of it is trapped under the sticky toolbar (bug 99).
 *
 * v5 shape (R-B / R-E): the frame reparents its OWN host to `document.body`
 * after the first render (the `image-detail-modal.ts` idiom — every host
 * mounts the frame under an `@if`, so a constructor-time reparent would be
 * undone by the view insertion), and removes it on destroy. v4's `isOpen` has
 * no counterpart — a mounted frame is open. `actions` is a data input;
 * `onPrev` / `onNext` are conditionally-`undefined` function inputs; the
 * caption and bottom-right are content-projection slots marked by
 * {@link FullScreenImageCaption} / {@link FullScreenImageBottomRight};
 * `keyboardActive` (default true) and `zIndexClass` (default `z-[60]`, the
 * dialog layer) as v4; `closeModal` is v4's `onClose`.
 */
@Component({
  selector: 'qt-full-screen-image-viewer',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, DeletedImagePlaceholder],
  template: `
    <div
      [attr.class]="'fixed inset-0 ' + zIndexClass() + ' flex items-center justify-center qt-bg-overlay backdrop-blur-sm'"
      role="dialog"
      aria-modal="true"
      [attr.aria-label]="alt()"
      (click)="closeModal.emit()"
    >
      @if (onPrev(); as prev) {
        <button
          type="button"
          [attr.class]="arrowButton + ' left-4'"
          title="Previous image (Left Arrow)"
          (click)="$event.stopPropagation(); prev()"
        >
          <qt-icon name="chevron-left" class="w-8 h-8" />
        </button>
      }
      @if (onNext(); as next) {
        <button
          type="button"
          [attr.class]="arrowButton + ' right-4'"
          title="Next image (Right Arrow)"
          (click)="$event.stopPropagation(); next()"
        >
          <qt-icon name="chevron-right" class="w-8 h-8" />
        </button>
      }

      <div class="absolute top-4 right-4 flex gap-2 z-10">
        @if (!imageMissing()) {
          @for (action of actions(); track action.label) {
            <button
              type="button"
              [attr.class]="roundButton"
              [disabled]="!!action.disabled"
              [attr.title]="action.label"
              [attr.aria-label]="action.label"
              (click)="$event.stopPropagation(); action.onClick()"
            >
              <qt-icon [name]="action.icon" class="w-6 h-6" />
            </button>
          }
        }
        <button
          type="button"
          [attr.class]="roundButton"
          title="Close (Escape)"
          aria-label="Close"
          (click)="$event.stopPropagation(); closeModal.emit()"
        >
          <qt-icon name="close" class="w-6 h-6" />
        </button>
      </div>

      @if (!imageMissing() && bottomRight()) {
        <div class="absolute bottom-4 right-4 z-10" (click)="$event.stopPropagation()">
          <ng-content select="[qtFullScreenBottomRight]" />
        </div>
      }

      <div
        class="relative max-w-[90vw] max-h-[90vh] flex items-center justify-center"
        (click)="$event.stopPropagation()"
      >
        @if (imageMissing()) {
          <qt-deleted-image-placeholder
            [imageId]="imageId()"
            [filename]="filename()"
            [width]="600"
            [height]="400"
            (cleanup)="closeModal.emit()"
          />
        } @else {
          <img
            [src]="src()"
            [alt]="alt()"
            class="max-w-full max-h-[90vh] w-auto h-auto object-contain"
            (error)="missingSrc.set(src())"
          />
        }
      </div>

      @if (caption()) {
        <div
          class="absolute bottom-4 left-1/2 -translate-x-1/2 qt-text-overlay-muted text-sm qt-bg-overlay-caption px-3 py-1 rounded text-center"
          (click)="$event.stopPropagation()"
        >
          <ng-content select="[qtFullScreenCaption]" />
        </div>
      }
    </div>
  `,
})
export class FullScreenImageViewer {
  readonly src = input.required<string>();
  readonly alt = input.required<string>();
  /** Id handed to the missing-image placeholder's clean-up. */
  readonly imageId = input.required<string>();
  /** Filename shown by the missing-image placeholder. */
  readonly filename = input.required<string>();
  /** Buttons before Close, left to right. Hidden while the image is missing. */
  readonly actions = input<readonly FullScreenImageAction[]>([]);
  /** Conditionally-`undefined` at the list ends — the arrow-visibility idiom. */
  readonly onPrev = input<(() => void) | undefined>(undefined);
  readonly onNext = input<(() => void) | undefined>(undefined);
  /**
   * While false, keys are not handled — set it false while a dialog opened
   * from this viewer is up, so Escape closes that dialog and not the viewer.
   */
  readonly keyboardActive = input(true);
  /** Overlay z-index class. Defaults to `z-[60]`, the dialog layer. */
  readonly zIndexClass = input('z-[60]');

  /** v4 `onClose`. */
  readonly closeModal = output<void>();

  protected readonly roundButton = ROUND_BUTTON;
  protected readonly arrowButton = ARROW_BUTTON;

  protected readonly caption = contentChild(FullScreenImageCaption);
  protected readonly bottomRight = contentChild(FullScreenImageBottomRight);

  // v4 `:79-81` — keyed by `src`, so walking to the next picture clears a
  // previous miss.
  protected readonly missingSrc = signal<string | null>(null);
  protected readonly imageMissing = computed(() => this.missingSrc() === this.src());

  constructor() {
    // v4 `:83-89` — `useImageNavigation` with the keys gated on
    // `keyboardActive` (the arrow BUTTONS stay live), re-applied when the
    // callbacks or the gate change, disposed on destroy.
    let dispose: (() => void) | null = null;
    effect(() => {
      const active = this.keyboardActive();
      const onPrev = this.onPrev();
      const onNext = this.onNext();
      dispose?.();
      dispose = applyImageNavigation({
        isOpen: true,
        onClose: () => this.closeModal.emit(),
        onPrev: active ? onPrev : undefined,
        onNext: active ? onNext : undefined,
        handleEscape: active,
      });
    });
    inject(DestroyRef).onDestroy(() => dispose?.());

    // v4 `:98-166` — the portal to `document.body` (R-B; the
    // `image-detail-modal.ts` idiom and its `typeof document` guard).
    const host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;
    inject(DestroyRef).onDestroy(() => host.remove());
    afterNextRender(() => {
      if (typeof document !== 'undefined') {
        document.body.appendChild(host);
      }
    });
  }
}
