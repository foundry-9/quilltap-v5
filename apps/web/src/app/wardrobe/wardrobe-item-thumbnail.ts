import { NgTemplateOutlet } from '@angular/common';
import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import { wardrobeImageThumbnailUrl } from './item-images.api';

/**
 * A wardrobe item's current picture at a fixed square size — a port of v4
 * `components/wardrobe/wardrobe-item-thumbnail.tsx` (`7c8572869`, read at the
 * pin `f5e953a3f`). Renders NOTHING without a `fileId` (v4 returns `null`; an
 * Angular host element cannot leave the DOM, so it takes `display: contents`
 * and is empty — the image itself is the box the row lays out).
 *
 * Used by the list row (40 px, between the expander and the title) and the
 * pickers (28 px, the candidate button's first child).
 *
 * With `onOpen` the thumbnail is its own button (open the picture full screen
 * in `WardrobeImageViewer`). Leave it off where the thumbnail already sits
 * inside a button — the pickers — since buttons do not nest. (v4 `01a83539d`
 * `:14-16`; v5's gate is the {@link openable} input, since an Angular `output`
 * always exists — the `canGenerateImage` precedent, R-E.)
 */
@Component({
  selector: 'qt-wardrobe-item-thumbnail',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [NgTemplateOutlet],
  host: { style: 'display: contents' },
  template: `
    @if (src(); as url) {
      @if (openable()) {
        <button
          type="button"
          class="flex-shrink-0 rounded cursor-zoom-in hover:opacity-80 transition-opacity"
          [title]="openLabel()"
          [attr.aria-label]="openLabel()"
          data-testid="wardrobe-item-thumbnail-open"
          (click)="open.emit()"
        >
          <ng-container *ngTemplateOutlet="img; context: { $implicit: url }" />
        </button>
      } @else {
        <ng-container *ngTemplateOutlet="img; context: { $implicit: url }" />
      }
    }
    <ng-template #img let-url>
      <img
        [src]="url"
        [alt]="alt()"
        [width]="size()"
        [height]="size()"
        loading="lazy"
        data-testid="wardrobe-item-thumbnail"
        class="flex-shrink-0 rounded object-cover border qt-border-default qt-bg-muted"
        [style.width.px]="size()"
        [style.height.px]="size()"
      />
    </ng-template>
  `,
})
export class WardrobeItemThumbnail {
  /** The item's current picture (`WardrobeItemDto.imageFileId`). */
  readonly fileId = input<string | null | undefined>(null);
  /** Edge length in pixels. */
  readonly size = input.required<number>();
  /**
   * Alt text. Defaults to empty: beside the title the picture is decorative,
   * and an empty alt keeps the surrounding button's accessible name the title.
   */
  readonly alt = input('');
  /** Makes the thumbnail a button that opens the picture full screen (v4 `onOpen`). */
  readonly openable = input(false);
  /** Accessible name for the button form; defaults to "View picture full size". */
  readonly openLabel = input('View picture full size');

  /** v4 `onOpen` — emitted when the button form is clicked. */
  readonly open = output<void>();

  protected readonly src = computed(() => {
    const id = this.fileId();
    return id ? wardrobeImageThumbnailUrl(id) : null;
  });
}
