import { ChangeDetectionStrategy, Component, computed, inject, input, output, signal } from '@angular/core';

import { apiUrl } from '../core/api-url';
import { copyImageToClipboard } from '../core/clipboard-utils';
import type { MessageAttachment } from '../core/core-contract';
import { downloadImageUrl } from '../core/download-utils';
import {
  FullScreenImageAction,
  FullScreenImageCaption,
  FullScreenImageViewer,
} from '../images/full-screen-image-viewer';
import { SaveImageDialog, type SaveImageTarget } from '../images/save-image-dialog';
import { ToastService } from '../ui/toast.service';
import type { InventoryContainer } from './inventory-container';

/**
 * `Brass Compass` → `Brass_Compass.webp` (v4 `inventory-image-viewer.tsx:30-33`).
 * Module-private in v4; exported so the spec can pin it (R-15).
 */
export function pictureFilename(title: string): string {
  const stem = title.replace(/[^a-zA-Z0-9]+/g, '_').replace(/^_+|_+$/g, '');
  return `${stem || 'inventory_object'}.webp`;
}

/**
 * An Inventory object's picture, full screen — a port of v4
 * `components/inventory/inventory-image-viewer.tsx` (`322a5488e`): the shared
 * full-screen frame with Save (to any document store's `photos/`, through the
 * save dialog's `inventory` door), Download and Copy, as the wardrobe's viewer
 * has. Opened from the editor's Pictures section, where Previous / Next walk
 * the pictures. It sits at `z-[90]`; the save dialog rides at `z-[100]`, and
 * while it is up the viewer leaves the keyboard alone.
 *
 * v5 shape (the wardrobe viewer's): mounted under the host's `@if`; `closed`
 * is v4's `onClose`. v4's `guard(label, fn, done)` is spelled out per action,
 * with v4's `console.error` line and `Failed to <label> image` toast.
 */
@Component({
  selector: 'qt-inventory-image-viewer',
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
      zIndexClass="z-[90]"
      [actions]="actions"
      (closeModal)="closed.emit()"
    >
      <ng-container qtFullScreenCaption>
        <div>{{ itemTitle() }}</div>
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
export class InventoryImageViewer {
  protected readonly toasts = inject(ToastService);

  readonly itemId = input.required<string>();
  readonly itemTitle = input.required<string>();
  readonly container = input.required<InventoryContainer>();
  readonly fileId = input.required<string>();
  readonly onPrev = input<(() => void) | undefined>(undefined);
  readonly onNext = input<(() => void) | undefined>(undefined);
  readonly closed = output<void>();

  protected readonly saving = signal(false);
  protected readonly src = computed(() => apiUrl(`/api/v1/files/${this.fileId()}`));
  protected readonly filename = computed(() => pictureFilename(this.itemTitle()));

  /** v4 `:62-66` — stable for the component's life (the callbacks read signals). */
  protected readonly actions: FullScreenImageAction[] = [
    { icon: 'bookmark', label: 'Save to a document store', onClick: () => this.saving.set(true) },
    { icon: 'download', label: 'Download', onClick: () => void this.guard('download', async () => downloadImageUrl(this.src(), this.filename())) },
    {
      icon: 'copy',
      label: 'Copy to clipboard',
      onClick: () =>
        void this.guard('copy', async () => {
          await copyImageToClipboard(this.src());
        }, 'Image copied to clipboard'),
    },
  ];

  protected readonly saveTarget = computed<SaveImageTarget>(() => ({
    kind: 'inventory',
    itemId: this.itemId(),
    container: this.container(),
    fileId: this.fileId(),
  }));

  protected readonly attachments = computed<MessageAttachment[]>(() => [
    { id: this.fileId(), filename: this.filename(), filepath: this.src(), mimeType: 'image/webp' },
  ]);

  /** v4 `guard` (`:39-47`). */
  private async guard(label: string, fn: () => Promise<void>, done?: string): Promise<void> {
    try {
      await fn();
      if (done) this.toasts.showSuccess(done);
    } catch (error) {
      console.error(`Failed to ${label} inventory picture:`, {
        error: error instanceof Error ? error.message : String(error),
      });
      this.toasts.showError(`Failed to ${label} image`);
    }
  }
}
