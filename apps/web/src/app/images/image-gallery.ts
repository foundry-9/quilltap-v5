import { ChangeDetectionStrategy, Component, inject, input, output, signal } from '@angular/core';

import { apiUrl } from '../core/api-url';
import { Icon } from '../ui/icon';
import { DeletedImagePlaceholder } from './deleted-image-placeholder';
import { ToastService } from '../ui/toast.service';
import type { ImageData } from './images.api';

/**
 * The generic image tile grid — a port of v4
 * `components/images/image-gallery.tsx` (187 lines): per-tile missing-image
 * detection (BOTH arms — `onError` AND the zero-dimension `onLoad` check,
 * `image-gallery.tsx:146-155`), the deleted-image placeholder swap, the
 * hover delete overlay (hidden for missing tiles, `:161`), and the optional
 * selection affordance.
 *
 * **Data-ownership divergence, loud:** v4 self-fetches
 * `GET /api/v1/images?tagType&tagId` (`:47-56`) — a LIST route v5 has not
 * ported (no `images` list verb exists; `avatar-picker.ts` notes the same
 * gap). Until that verb lands, this component takes `images`/`loading`/
 * `error` as INPUTS and emits `reloadImages` where v4 refetches; it has NO
 * v5 host yet (v4's two mount sites — the avatar selector and the AI wizard —
 * are unported), so the port exists for the deleted-handling family and its
 * case-for-case suite.
 *
 * The delete calls v4's `DELETE /api/v1/images/{id}` — in v5 the named loud
 * refusal (see `photos_routes::image_delete_not_available`).
 */
@Component({
  selector: 'qt-image-gallery',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, DeletedImagePlaceholder],
  template: `
    @if (loading()) {
      <div class="flex items-center justify-center py-12 {{ styleClass() }}">
        <div class="qt-text-secondary">Loading images...</div>
      </div>
    } @else if (error(); as err) {
      <div class="flex items-center justify-center py-12 {{ styleClass() }}">
        <div class="qt-text-destructive">Error: {{ err }}</div>
      </div>
    } @else if (images().length === 0) {
      <div class="flex items-center justify-center py-12 {{ styleClass() }}">
        <div class="qt-text-secondary">No images found</div>
      </div>
    } @else {
      <div class="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4 {{ styleClass() }}">
        @for (image of images(); track image.id) {
          <div
            class="qt-card relative group overflow-hidden transition-all p-0 {{
              selectedImageId() === image.id ? 'ring-2 ring-primary' : ''
            }} {{ onSelectImage() ? 'cursor-pointer' : '' }}"
            (click)="onSelectImage()?.(image)"
          >
            <div class="aspect-square relative qt-bg-muted">
              @if (missingImages().has(image.id)) {
                <qt-deleted-image-placeholder
                  [imageId]="image.id"
                  [filename]="image.filename"
                  styleClass="w-full h-full absolute inset-0"
                  (cleanup)="reloadImages.emit()"
                />
              } @else {
                <img
                  [src]="srcFor(image)"
                  [alt]="image.filename"
                  class="w-full h-full object-cover"
                  (error)="markMissing(image.id)"
                  (load)="checkDimensions($any($event.target), image.id)"
                />
              }
            </div>

            <!-- Overlay with actions (v4 :160-175 — hidden for missing tiles) -->
            @if (!missingImages().has(image.id)) {
              <div
                class="absolute inset-0 bg-black bg-opacity-0 group-hover:bg-opacity-50 transition-all opacity-0 group-hover:opacity-100"
              >
                <button
                  type="button"
                  class="absolute bottom-2 right-2 bg-destructive qt-text-destructive-foreground p-2 rounded-full hover:qt-bg-destructive/90 transition-colors"
                  title="Delete image"
                  aria-label="Delete image"
                  (click)="$event.stopPropagation(); handleDeleteImage(image.id)"
                >
                  <qt-icon name="trash" class="w-5 h-5" />
                </button>
              </div>
            }

            <!-- Selected indicator (v4 :177-182) -->
            @if (selectedImageId() === image.id && !missingImages().has(image.id)) {
              <div class="absolute top-2 right-2 bg-primary text-primary-foreground rounded-full p-1">
                <qt-icon name="check" class="w-4 h-4" />
              </div>
            }
          </div>
        }
      </div>
    }
  `,
})
export class ImageGallery {
  private readonly toasts = inject(ToastService);

  readonly images = input.required<ImageData[]>();
  readonly loading = input(false);
  readonly error = input<string | null>(null);
  /** v4 `onSelectImage?` — presence drives the pointer cursor and click. */
  readonly onSelectImage = input<((image: ImageData) => void) | undefined>(undefined);
  readonly selectedImageId = input<string | undefined>(undefined);
  /** v4 `className = ''`. */
  readonly styleClass = input('');

  /** Emitted where v4 refetches its list (`loadImages` — the host owns data). */
  readonly reloadImages = output<void>();

  protected readonly missingImages = signal<ReadonlySet<string>>(new Set());

  protected srcFor(image: ImageData): string {
    const raw = image.url || (image.filepath.startsWith('/') ? image.filepath : `/${image.filepath}`);
    return /^[a-z][a-z0-9+.-]*:/i.test(raw) ? raw : apiUrl(raw);
  }

  protected markMissing(id: string): void {
    this.missingImages.update((prev) => new Set(prev).add(id));
  }

  /** v4 `:149-155` — a "loaded" broken image reports zero natural dimensions. */
  protected checkDimensions(img: HTMLImageElement, id: string): void {
    if (img.naturalWidth === 0 || img.naturalHeight === 0) {
      this.markMissing(id);
    }
  }

  /** v4 `handleDeleteImage` (`:65-86`) — confirm, DELETE, then reload. */
  protected async handleDeleteImage(imageId: string): Promise<void> {
    if (!window.confirm('Permanently delete this image? This cannot be undone.')) {
      return;
    }
    try {
      const response = await fetch(apiUrl(`/api/v1/images/${imageId}`), { method: 'DELETE' });
      const data = (await response.json()) as { error?: string };
      if (!response.ok) {
        throw new Error(data.error || 'Failed to delete image');
      }
      this.reloadImages.emit();
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to delete image');
    }
  }
}
