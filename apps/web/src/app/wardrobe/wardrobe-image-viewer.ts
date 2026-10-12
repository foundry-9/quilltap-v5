import { ChangeDetectionStrategy, Component, computed, inject, input, output, signal } from '@angular/core';

import { copyImageToClipboard } from '../core/clipboard-utils';
import { CoreClient } from '../core/core-client';
import type { MessageAttachment } from '../core/core-contract';
import { downloadImageUrl } from '../core/download-utils';
import {
  FullScreenImageAction,
  FullScreenImageCaption,
  FullScreenImageViewer,
} from '../images/full-screen-image-viewer';
import { SaveImageDialog, type SaveImageTarget } from '../images/save-image-dialog';
import { ToastService } from '../ui/toast.service';
import { deleteWardrobeItemImage, wardrobeImageUrl } from './item-images.api';
import type { WardrobeContainer } from './wardrobe-container';

/**
 * `Blue Velvet Gown` → `Blue_Velvet_Gown.webp` — the stored pictures are WebP
 * (v4 `wardrobe-image-viewer.tsx:45-49` at `01a83539d`). Module-private in v4;
 * exported here so the spec can pin it (it has no oracle row — P4.D277 Tier 3
 * item 19).
 */
export function pictureFilename(title: string): string {
  const stem = title.replace(/[^a-zA-Z0-9]+/g, '_').replace(/^_+|_+$/g, '');
  return `${stem || 'wardrobe_item'}.webp`;
}

/**
 * A wardrobe item's picture, full screen — a port of v4
 * `components/wardrobe/wardrobe-image-viewer.tsx` (NEW in `01a83539d`, read at
 * that pin; P4.D277).
 *
 * v4's module doc, carried: the shared `FullScreenImageViewer` frame with three
 * buttons: Save (the Salon's `SaveImageDialog`, offered every document store,
 * filing a copy in the chosen store's `photos/`), Download, and Copy. Opened
 * from the row thumbnail and from the item editor's Picture section, where
 * Previous/Next walk the item's picture history.
 *
 * It sits at `z-[90]`, above the wardrobe dialog (`z-[60]`) and the item
 * editor (`z-[70]`/`z-[80]`); the save dialog it opens rides at `z-[100]`,
 * and while that dialog is up the viewer leaves the keyboard alone so Escape
 * closes only the dialog.
 *
 * When the picture's bytes are missing, the frame's Remove deletes it through
 * the images route's `delete-image` action rather than the generic image
 * delete, and refreshes the wardrobe queries (bug 194, `9ecebf986`).
 *
 * Design of record: docs/developer/features/complete/wardrobe-item-images.md
 *
 * v5 shape: mounted under an `@if` by its hosts (v4's `isOpen` has no
 * counterpart); `closed` is v4's `onClose`; `onPrev` / `onNext` are
 * conditionally-`undefined` function inputs (the frame's arrow idiom).
 * Download and Copy ride v5's `downloadImageUrl` / `copyImageToClipboard`
 * (R-I) inside v4's own try/catch shape — the copy helper THROWS when no
 * clipboard write method exists (`core/clipboard-utils.ts`), so v4's
 * `console.error` + error toast are reached as in v4. v4's
 * `invalidateQueries({ queryKey: queryKeys.wardrobe.all })` after the bug-194
 * removal has no v5 cache to name: the viewer emits {@link removed} and each
 * host answers with its own refresh (the recorded `imageChanged` idiom).
 *
 * @module wardrobe/wardrobe-image-viewer
 */
@Component({
  selector: 'qt-wardrobe-image-viewer',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [FullScreenImageViewer, FullScreenImageCaption, SaveImageDialog],
  template: `
    <qt-full-screen-image-viewer
      [src]="src()"
      [alt]="'Picture of ' + itemTitle()"
      [imageId]="fileId()"
      [filename]="filename()"
      [onPrev]="onPrev()"
      [onNext]="onNext()"
      [keyboardActive]="!saving()"
      [missingCleanup]="handleMissingCleanup"
      zIndexClass="z-[90]"
      [actions]="actions"
      (closeModal)="closed.emit()"
    >
      <ng-container qtFullScreenCaption>
        <div>{{ itemTitle() }}</div>
        @if (note(); as n) {
          <div class="text-xs opacity-80">{{ n }}</div>
        }
      </ng-container>
    </qt-full-screen-image-viewer>
    @if (saving()) {
      <qt-save-image-dialog
        [target]="saveTarget()"
        [attachments]="attachments()"
        overlayClass="z-[100]"
        (saved)="toasts.showSuccess('Saved to ' + $event.mountPoint)"
        (close)="saving.set(false)"
      />
    }
  `,
})
export class WardrobeImageViewer {
  protected readonly toasts = inject(ToastService);
  private readonly core = inject(CoreClient);

  readonly itemId = input.required<string>();
  readonly itemTitle = input.required<string>();
  /** The item's home container — the images route is addressed through it. */
  readonly container = input.required<WardrobeContainer>();
  /** The picture on show. */
  readonly fileId = input.required<string>();
  /** Optional caption line under the title (e.g. "Current picture"). */
  readonly note = input<string | null>(null);
  readonly onPrev = input<(() => void) | undefined>(undefined);
  readonly onNext = input<(() => void) | undefined>(undefined);

  /** v4 `onClose`. */
  readonly closed = output<void>();
  /** A missing picture was removed through the wardrobe route — the host
   *  refreshes the reads that might still show it (bug 194; fired before
   *  {@link closed}). */
  readonly removed = output<void>();

  protected readonly saving = signal(false);
  protected readonly src = computed(() => wardrobeImageUrl(this.fileId()));
  protected readonly filename = computed(() => pictureFilename(this.itemTitle()));

  /** v4 `:99-103` — stable for the component's life (the callbacks read signals). */
  protected readonly actions: FullScreenImageAction[] = [
    { icon: 'bookmark', label: 'Save to a document store', onClick: () => this.saving.set(true) },
    { icon: 'download', label: 'Download', onClick: () => this.handleDownload() },
    { icon: 'copy', label: 'Copy to clipboard', onClick: () => void this.handleCopy() },
  ];

  protected readonly saveTarget = computed<SaveImageTarget>(() => ({
    kind: 'wardrobe',
    itemId: this.itemId(),
    container: this.container(),
    fileId: this.fileId(),
  }));

  protected readonly attachments = computed<MessageAttachment[]>(() => [
    { id: this.fileId(), filename: this.filename(), filepath: this.src(), mimeType: 'image/webp' },
  ]);

  /**
   * The bytes are gone: remove the picture through the wardrobe's own route,
   * which drops the mount link and the `files` row and moves the item's
   * current pointer on — the generic image delete knows none of that (bug
   * 194) — then refresh every wardrobe read that might still show it (v4
   * `9ecebf986` `:73-82`). A throw reaches the placeholder's toast.
   */
  protected readonly handleMissingCleanup = async (): Promise<void> => {
    await deleteWardrobeItemImage(this.core, this.itemId(), this.container(), this.fileId());
    this.removed.emit();
  };

  /** v4 `:65-74`. */
  protected handleDownload(): void {
    try {
      downloadImageUrl(this.src(), this.filename());
    } catch (error) {
      console.error('Failed to download wardrobe picture:', {
        error: error instanceof Error ? error.message : String(error),
      });
      this.toasts.showError('Failed to download image');
    }
  }

  /** v4 `:76-86`. */
  protected async handleCopy(): Promise<void> {
    try {
      await copyImageToClipboard(this.src());
      this.toasts.showSuccess('Image copied to clipboard');
    } catch (error) {
      console.error('Failed to copy wardrobe picture:', {
        error: error instanceof Error ? error.message : String(error),
      });
      this.toasts.showError('Failed to copy image to clipboard');
    }
  }
}
