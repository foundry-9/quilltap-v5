import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import { Icon } from '../../ui/icon';

/**
 * Stand-in for an image THUMBNAIL while the Salon's images are hidden — v4
 * `HiddenImageTile` (`components/quick-hide/images-hidden-context.tsx:33-46`,
 * `e3937d7aa`). Fills its parent box, so it drops into the same sized
 * container the `<img>` occupied; the host generates no box of its own
 * (`display: contents`) so v4's `w-full h-full` resolves against that parent.
 *
 * Not to be confused with its two neighbours: `qt-hidden-placeholder`
 * (`quick-hide/hidden-placeholder.ts`) is the whole-screen "Hidden" card for a
 * quick-hidden TAG's detail page, and `hiddenInlineImageHtml`
 * (`hidden-inline-image.ts`) is the inline stand-in for an image embedded in
 * message prose.
 */
@Component({
  selector: 'qt-hidden-image-tile',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  host: { style: 'display: contents' },
  template: `<span
    class="w-full h-full flex items-center justify-center qt-bg-muted qt-text-secondary"
    [attr.title]="text()"
    role="img"
    [attr.aria-label]="text()"
  >
    <qt-icon name="eye-off" class="w-6 h-6" />
  </span>`,
})
export class HiddenImageTile {
  /** What the image was (v4 passes the attachment's filename). */
  readonly label = input<string | undefined>(undefined);

  /** v4: `label ? \`Image hidden: ${label}\` : 'Image hidden'` — title AND aria-label. */
  protected readonly text = computed(() => {
    const label = this.label();
    return label ? `Image hidden: ${label}` : 'Image hidden';
  });
}
