import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

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
 */
@Component({
  selector: 'qt-wardrobe-item-thumbnail',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { style: 'display: contents' },
  template: `
    @if (src(); as url) {
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
    }
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

  protected readonly src = computed(() => {
    const id = this.fileId();
    return id ? wardrobeImageThumbnailUrl(id) : null;
  });
}
