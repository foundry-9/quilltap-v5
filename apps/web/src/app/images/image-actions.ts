import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

import { Icon } from '../ui/icon';

/**
 * The detail modal's nav arrows + top-right toolbar — a port of v4
 * `components/images/image-detail/ImageActions.tsx` (106 lines).
 *
 * `onPrev`/`onNext` are FUNCTION inputs, not booleans: v4's host passes the
 * callbacks conditionally-`undefined` at the ends of the list
 * (`PhotoGalleryModal.tsx:343-344`/`:358-359`), and the arrows render only
 * when the callback exists (`ImageActions.tsx:28`/`:41`) — there is no
 * separate disabled state. The idiom carries verbatim.
 */
@Component({
  selector: 'qt-image-actions',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <!-- Navigation buttons - left and right sides (v4 :27-52) -->
    @if (onPrev(); as prev) {
      <button
        type="button"
        class="absolute left-4 top-1/2 -translate-y-1/2 p-3 qt-bg-overlay-btn hover:qt-bg-overlay-btn rounded-full qt-text-overlay transition-colors z-10"
        title="Previous image (Left Arrow)"
        (click)="$event.stopPropagation(); prev()"
      >
        <qt-icon name="chevron-left" class="w-8 h-8" />
      </button>
    }
    @if (onNext(); as next) {
      <button
        type="button"
        class="absolute right-4 top-1/2 -translate-y-1/2 p-3 qt-bg-overlay-btn hover:qt-bg-overlay-btn rounded-full qt-text-overlay transition-colors z-10"
        title="Next image (Right Arrow)"
        (click)="$event.stopPropagation(); next()"
      >
        <qt-icon name="chevron-right" class="w-8 h-8" />
      </button>
    }

    <!-- Top right control buttons (v4 :54-103) -->
    <div class="absolute top-4 right-4 flex gap-2 z-10">
      <button
        type="button"
        class="p-2 qt-bg-overlay-btn hover:qt-bg-overlay-btn rounded-full qt-text-overlay transition-colors"
        title="Download"
        (click)="$event.stopPropagation(); download.emit()"
      >
        <qt-icon name="download" class="w-6 h-6" />
      </button>
      <button
        type="button"
        class="p-2 qt-bg-overlay-btn hover:qt-bg-overlay-btn rounded-full qt-text-overlay transition-colors"
        title="Copy to clipboard"
        (click)="$event.stopPropagation(); copyImage.emit()"
      >
        <qt-icon name="copy" class="w-6 h-6" />
      </button>
      <!-- Save to my gallery (v4 :79-91 — rendered only when the handler exists) -->
      @if (showSaveToGallery()) {
        <button
          type="button"
          class="p-2 qt-bg-overlay-btn hover:qt-bg-overlay-btn rounded-full qt-text-overlay transition-colors disabled:opacity-50"
          [disabled]="savingToGallery()"
          [title]="savingToGallery() ? 'Saving…' : 'Save to my gallery'"
          (click)="$event.stopPropagation(); onSaveClick()"
        >
          <qt-icon name="bookmark" class="w-6 h-6" />
        </button>
      }
      <button
        type="button"
        class="p-2 qt-bg-overlay-btn hover:qt-bg-overlay-btn rounded-full qt-text-overlay transition-colors"
        title="Close (Escape)"
        (click)="$event.stopPropagation(); closeModal.emit()"
      >
        <qt-icon name="close" class="w-6 h-6" />
      </button>
    </div>
  `,
})
export class ImageActions {
  /** Conditionally-`undefined` at the list ends — the arrow-visibility idiom. */
  readonly onPrev = input<(() => void) | undefined>(undefined);
  readonly onNext = input<(() => void) | undefined>(undefined);
  /** v4's optional `handleSaveToGallery` presence (`:79`). */
  readonly showSaveToGallery = input(true);
  readonly savingToGallery = input(false);

  readonly download = output<void>();
  readonly copyImage = output<void>();
  readonly saveToGallery = output<void>();
  readonly closeModal = output<void>();

  protected onSaveClick(): void {
    // v4 `:83` — the click is a no-op while saving (belt over the disabled attr).
    if (!this.savingToGallery()) {
      this.saveToGallery.emit();
    }
  }
}
