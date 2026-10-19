import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../../core/core-client';
import { apiUrl } from '../../../../core/api-url';
import { triggerBlobDownload } from '../../../../core/download-utils';
import { Icon } from '../../../../ui/icon';
import { ImageDetailModal } from '../../../../images/image-detail-modal';
import type { ImageData } from '../../../../images/images.api';
import {
  characterKeys,
  fetchWardrobeImages,
  toWardrobeGalleryImage,
  type WardrobeGalleryImage,
} from '../../characters.api';
import { ToastService } from '../../../../ui/toast.service';

/**
 * The pictures drawn of the garments a character can reach — v4
 * `components/images/embedded-gallery/WardrobeImagesSection.tsx` +
 * `hooks/useWardrobeImages.ts` + `GalleryImage.tsx`'s caption arm
 * (`1cfc9b504`), one component here because v5's gallery tab has no hook
 * layer to split them across (the {@link AvatarRollsSection} precedent).
 *
 * Carrying v4's own why: "The pictures drawn of the garments a character can
 * reach — their own wardrobe, their groups' wardrobes and Quilltap General.
 * Read-only, and its own section for the same reason the avatar rolls are:
 * these belong to the wardrobe items, not to the character's album. Each tile
 * is captioned with the item it pictures and the wardrobe that item lives in.
 * Opening one offers the detail view's own save actions, so a picture can
 * still be kept in the album from there. Collapsed by default, and absent
 * entirely when there is nothing to show."
 *
 * Recorded divergences, as the rolls section records them:
 *
 *  - **The tile markup is a copy, not a shared component** (the P4.D188
 *    divergence): v4 renders its `GalleryGrid` / `GalleryImage` with
 *    `settingAvatar`, `deletingImage` and `confirmDelete` all `null`, no
 *    `currentAvatarId`, no `onSetAvatar` / `onDeleteImage` / `onSaveToAlbum`
 *    and a `captionFor` — so the tile here is the rolls tile MINUS the
 *    set-as-avatar, keep and delete buttons and the Avatar badge (no tile is
 *    ever the portrait), PLUS the caption, with the action overlay at
 *    `top-1` (v4 `GalleryImage.tsx:79`'s caption arm).
 *  - The `<img>`'s generic `console.warn` ("Image failed to load in gallery")
 *    is `GalleryImage`'s, which v5's gallery tab has never carried; mirrored
 *    from the rolls section. The hook's own `Wardrobe picture failed to load`
 *    IS carried.
 *
 * `isCurrentItemImage`, `generationPrompt`, `generationModel`, `itemId` reach
 * this component and are never rendered — v4 renders none of them at the pin.
 * The detail modal gets NO generation prompt (v4's `ImageData` has no prompt
 * field), whatever `help/character-gallery.md` says.
 *
 * Hidden on a failed read too (trap 9): TanStack's `isPending` is false on an
 * error and `total` is 0, so the shelf vanishes with no error text — v4's
 * `!isLoading && total === 0`.
 */
@Component({
  selector: 'qt-wardrobe-pictures-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { class: 'qt-wardrobe-pictures-section' },
  imports: [Icon, ImageDetailModal],
  template: `
    <!-- v4 :56 — absent entirely when there is nothing to show. -->
    @if (loading() || total() > 0) {
      <section class="mt-8 border-t qt-border-default pt-6">
        <button
          type="button"
          class="flex w-full items-center gap-2 text-left"
          [attr.aria-expanded]="expanded()"
          (click)="expanded.set(!expanded())"
        >
          <qt-icon
            [name]="expanded() ? 'chevron-down' : 'chevron-right'"
            class="w-4 h-4 qt-text-secondary"
          />
          <span class="qt-text-label">Wardrobe Pictures</span>
          <span class="qt-text-label-xs">{{ countLabel() }}</span>
        </button>

        <p class="qt-text-label-xs mt-1 ml-6">
          Every likeness the house has sketched of the garments within {{ entityName() }}&rsquo;s
          reach &mdash; from {{ entityName() }}&rsquo;s own wardrobe, the wardrobes of any company
          {{ entityName() }} keeps, and the Quilltap General collection. Each plate bears a little
          card naming the garment and the wardrobe it hangs in.
        </p>

        @if (expanded()) {
          <div class="mt-4">
            @if (loading()) {
              <div class="flex items-center justify-center py-8">
                <div class="animate-spin rounded-full h-6 w-6 border-b-2 qt-border-primary"></div>
              </div>
            } @else {
              <!-- v4 GalleryGrid:36-40, at the album's own thumbnail size. -->
              <div
                class="grid gap-2"
                [style.grid-template-columns]="
                  'repeat(auto-fill, minmax(' + thumbnailSize() + 'px, 1fr))'
                "
              >
                @for (image of images(); track image.id; let index = $index) {
                  <div class="relative group">
                    <button
                      type="button"
                      class="relative aspect-square w-full overflow-hidden rounded-lg qt-bg-muted hover:ring-2 hover:ring-primary focus:outline-none focus:ring-2 focus:ring-ring transition-all"
                      (click)="selectedIndex.set(index)"
                    >
                      @if (missingImages().has(image.id)) {
                        <div
                          class="absolute inset-0 flex items-center justify-center qt-text-secondary"
                        >
                          <qt-icon name="image" class="w-8 h-8" />
                        </div>
                      } @else {
                        <img
                          [src]="apiUrl(image.url)"
                          [alt]="image.filename"
                          class="absolute inset-0 w-full h-full object-cover"
                          (error)="handleImageError(image.id)"
                        />
                      }
                      <!-- v4 GalleryImage:59-68 — the source caption. -->
                      <div
                        [attr.class]="
                          'qt-gallery-caption absolute inset-x-0 bottom-0 px-1.5 pt-4 pb-1 text-left pointer-events-none'
                        "
                        [attr.title]="captionTitle(image) + ' — ' + image.locationLabel"
                      >
                        <div class="qt-gallery-caption-title truncate">
                          {{ captionTitle(image) }}
                        </div>
                        <div class="qt-gallery-caption-detail truncate">
                          {{ image.locationLabel }}
                        </div>
                      </div>
                    </button>

                    <div
                      [attr.class]="
                        'absolute top-1 right-1 flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity'
                      "
                    >
                      @if (!missingImages().has(image.id)) {
                        <button
                          type="button"
                          class="p-1.5 rounded-full qt-shadow-md qt-bg-card qt-text-secondary hover:qt-bg-primary hover:qt-text-on-primary transition-colors"
                          title="Download image"
                          aria-label="Download image"
                          (click)="$event.stopPropagation(); handleDownload(image)"
                        >
                          <qt-icon name="download" class="w-4 h-4" />
                        </button>
                      }
                    </div>
                  </div>
                }
              </div>
            }
          </div>
        }

        <!-- v4 :106-134 — the SHARED detail modal (not the wardrobe picture
             viewer), with no generation-prompt field. -->
        @if (selectedImage(); as image) {
          <qt-image-detail-modal
            [image]="image"
            [onPrev]="selectedIndex() > 0 ? boundPrev : undefined"
            [onNext]="selectedIndex() < images().length - 1 ? boundNext : undefined"
            (closeModal)="selectedIndex.set(-1)"
            (avatarSet)="refresh.emit()"
          />
        }
      </section>
    }
  `,
})
export class WardrobePicturesSection {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly characterId = input.required<string>();
  readonly entityName = input.required<string>();
  readonly thumbnailSize = input.required<number>();

  /** v4 `onRefresh` — the modal set the portrait; the host re-reads. */
  readonly refresh = output<void>();

  protected readonly apiUrl = apiUrl;
  protected readonly expanded = signal(false);
  protected readonly selectedIndex = signal(-1);
  protected readonly missingImages = signal<ReadonlySet<string>>(new Set());

  /** v4 `useWardrobeImages` `:69-76` — default options, no `enabled`. */
  protected readonly picturesQuery = injectQuery(() => ({
    queryKey: characterKeys.wardrobeImages(this.characterId()),
    queryFn: () => fetchWardrobeImages(this.core, this.characterId()),
  }));

  protected readonly images = computed<WardrobeGalleryImage[]>(() =>
    (this.picturesQuery.data()?.entries ?? []).map(toWardrobeGalleryImage),
  );
  protected readonly total = computed(() => this.picturesQuery.data()?.total ?? 0);
  protected readonly loading = computed(() => this.picturesQuery.isPending());

  /** v4 `:70-72` — `…` while the count is unknown, else `N picture(s)`. */
  protected readonly countLabel = computed(() => {
    if (this.loading()) return '…';
    const total = this.total();
    return `${total} picture${total === 1 ? '' : 's'}`;
  });

  protected readonly selectedImage = computed<ImageData | null>(() => {
    const index = this.selectedIndex();
    const image = index >= 0 ? this.images()[index] : undefined;
    return image ? this.toImageData(image) : null;
  });

  protected readonly boundPrev = (): void => {
    this.selectedIndex.update((prev) => (prev > 0 ? prev - 1 : prev));
  };
  protected readonly boundNext = (): void => {
    this.selectedIndex.update((prev) => (prev < this.images().length - 1 ? prev + 1 : prev));
  };

  /** v4 `captionFor` (`:43-49`) — the title half. */
  protected captionTitle(image: WardrobeGalleryImage): string {
    return image.itemArchived ? `${image.itemTitle} (archived)` : image.itemTitle;
  }

  /** v4 `handleImageError` (`useWardrobeImages.ts:80-83`). */
  protected handleImageError(imageId: string): void {
    this.missingImages.update((prev) => new Set(prev).add(imageId));
    console.warn('Wardrobe picture failed to load', { imageId });
  }

  /**
   * v4 `:106-122` — the modal's `ImageData`. "A `files` row, so no `linkId`:
   * the modal's save actions post `{ fileId }` and the server re-links from
   * the file's blob." `url` is `apiUrl`-wrapped where v4 passes the bare path
   * (the rolls section's reason: a Tauri webview off the qtap origin).
   */
  private toImageData(image: WardrobeGalleryImage): ImageData {
    return {
      id: image.id,
      filename: image.filename,
      filepath: image.fullUrl,
      url: apiUrl(image.fullUrl),
      mimeType: image.mimeType ?? 'image/webp',
      size: image.size,
      width: image.width,
      height: image.height,
      createdAt: image.createdAt,
      tags: [],
    };
  }

  /** v4 `downloadImage` (`useWardrobeImages.ts:85-99`) — the FULL-size bytes. */
  protected async handleDownload(image: WardrobeGalleryImage): Promise<void> {
    try {
      const res = await fetch(apiUrl(image.fullUrl));
      if (!res.ok) throw new Error(`Failed to fetch image (${res.status})`);
      const blob = await res.blob();
      triggerBlobDownload(blob, image.filename);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.toasts.showError('Failed to download image');
      console.error('Error downloading wardrobe picture', {
        characterId: this.characterId(),
        imageId: image.id,
        error: message,
      });
    }
  }
}
