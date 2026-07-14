import { ChangeDetectionStrategy, Component, computed, input, signal } from '@angular/core';

import { thumbnailUrl } from '../../images/image-urls';
import { getFileIcon, supportsThumbnail } from './file-model';

/**
 * A file thumbnail (v4 `components/files/FileThumbnail.tsx`, legacy mode). For
 * the five thumbnailable image types it lazy-loads the server's cached WebP
 * thumbnail (`/api/v1/files/{id}?action=thumbnail&size=`); every other type
 * renders the emoji fallback icon. A load failure falls back to the icon too.
 *
 * v4's IntersectionObserver + exponential-retry machinery is a performance
 * refinement over the same route; the port relies on the browser's native
 * `loading="lazy"` and the thumbnail route's own on-demand regeneration.
 */
@Component({
  selector: 'qt-file-thumbnail',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (canShowThumbnail() && status() !== 'error') {
      <div
        class="relative overflow-hidden qt-bg-muted"
        [class]="className()"
        [style.width.px]="size()"
        [style.height.px]="size()"
      >
        @if (status() === 'loading') {
          <div class="absolute inset-0 animate-pulse qt-bg-muted-foreground/20"></div>
        }
        <img
          [src]="url()"
          [alt]="alt()"
          class="w-full h-full object-cover transition-opacity duration-200"
          [class.opacity-100]="status() === 'loaded'"
          [class.opacity-0]="status() !== 'loaded'"
          loading="lazy"
          (load)="status.set('loaded')"
          (error)="status.set('error')"
        />
      </div>
    } @else {
      <div
        class="flex items-center justify-center qt-bg-muted text-4xl"
        [class]="className()"
        [style.width.px]="size()"
        [style.height.px]="size()"
      >
        {{ icon() }}
      </div>
    }
  `,
})
export class FileThumbnail {
  readonly fileId = input.required<string>();
  readonly mimeType = input.required<string>();
  readonly alt = input('');
  readonly size = input(150);
  readonly className = input('');

  protected readonly status = signal<'loading' | 'loaded' | 'error'>('loading');

  protected readonly canShowThumbnail = computed(() => supportsThumbnail(this.mimeType()));
  protected readonly icon = computed(() => getFileIcon(this.mimeType()));
  protected readonly url = computed(() => thumbnailUrl(this.fileId(), this.size()));
}
