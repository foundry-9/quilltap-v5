import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  signal,
  untracked,
} from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../../core/core-client';
import type { CharacterPhoto } from '../../../../core/core-contract';
import { apiUrl } from '../../../../core/api-url';
import { Icon } from '../../../../ui/icon';
import { normalizeAvatarSrc } from '../../../../ui/avatar-stack';
import { LoadingState } from '../../../../ui/loading-state';
import { ImageDetailModal } from '../../../../images/image-detail-modal';
import type { ImageData } from '../../../../images/images.api';
import {
  characterKeys,
  fetchCharacter,
  fetchCharacterPhotos,
  uploadCharacterPhoto,
} from '../../characters.api';
import { ToastService } from '../../../../ui/toast.service';

/** v4 `EmbeddedPhotoGallery` THUMBNAIL_SIZES / DEFAULT_THUMBNAIL_INDEX (120px). */
const THUMBNAIL_SIZES = [80, 100, 120, 150, 180, 200];
const DEFAULT_THUMBNAIL_INDEX = 2;

/**
 * The Photo Gallery tab at `EmbeddedPhotoGallery` parity (v4
 * `components/images/embedded-gallery/EmbeddedPhotoGallery.tsx` +
 * `GalleryControls` + `GalleryGrid` + `GalleryImage` + `useGalleryData`):
 * the count + upload + Clear-Avatar + zoom header, the tile grid with the
 * Avatar ring/badge and the hover set-avatar / confirm-double-click delete
 * overlay, missing-image detection, and the deep `ImageDetailModal` opened
 * per tile — the ONLY host that passes `onAvatarSet`
 * (`EmbeddedPhotoGallery.tsx:189-195`), and the one that sets
 * `linkId: selectedImage.id` (`:175`) so the modal's save-to-album takes the
 * link-relink branch.
 *
 * The character's `defaultImageId`/`name` ride the shared
 * `characterKeys.detail` query (the detail screen already holds it in cache),
 * standing in for v4's `currentAvatarId`/`entityName` props — the tab stays
 * self-contained so the host screen is untouched.
 *
 * Upload keeps the lane-C multipart route (the byte leg dispatch can't
 * express), and reports through the toast, as v4's `useGalleryData` does.
 */
@Component({
  selector: 'qt-character-gallery-tab',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, LoadingState, ImageDetailModal],
  template: `
    <div class="space-y-4">
      <div class="flex items-center justify-between flex-wrap gap-2">
        <div>
          <h2 class="qt-heading-4 text-foreground">Photo Gallery</h2>
          <!-- v4 GalleryControls:31-33 — the count line. -->
          <p class="qt-text-small">
            {{ photos().length }} photo{{ photos().length !== 1 ? 's' : '' }}
          </p>
        </div>
        <div class="flex items-center gap-2">
          <button
            type="button"
            class="qt-button-secondary"
            [disabled]="uploading()"
            title="Upload a new portrait for this character"
            (click)="fileInput.click()"
          >
            <qt-icon name="upload" class="w-4 h-4" />
            {{ uploading() ? 'Uploading...' : 'Upload' }}
          </button>
          <!-- v4 GalleryControls:49-58 — only while an avatar is set. -->
          @if (currentAvatarId()) {
            <button
              type="button"
              class="px-3 py-1 qt-text-label-xs qt-text-destructive hover:qt-bg-destructive/10 rounded-md transition-colors"
              (click)="handleClearAvatar()"
            >
              Clear Avatar
            </button>
          }
          <!-- v4 GalleryControls:60-83 — the zoom cluster. -->
          <button
            type="button"
            class="p-1 qt-text-secondary hover:text-foreground disabled:opacity-50"
            title="Smaller thumbnails"
            [disabled]="sizeIndex() === 0"
            (click)="sizeIndex.set(sizeIndex() - 1)"
          >
            <qt-icon name="zoom-out" class="w-5 h-5" />
          </button>
          <span class="qt-text-label-xs w-12 text-center">{{ thumbnailSize() }}px</span>
          <button
            type="button"
            class="p-1 qt-text-secondary hover:text-foreground disabled:opacity-50"
            title="Larger thumbnails"
            [disabled]="sizeIndex() === maxSizeIndex"
            (click)="sizeIndex.set(sizeIndex() + 1)"
          >
            <qt-icon name="zoom-in" class="w-5 h-5" />
          </button>
        </div>
        <input
          #fileInput
          type="file"
          accept="image/*"
          class="hidden"
          aria-label="Upload photo"
          (change)="onPick($any($event.target))"
        />
      </div>


      @if (photosQuery.isPending()) {
        <qt-loading-state message="Loading photos..." />
      } @else if (photos().length === 0) {
        <!-- v4 GalleryEmpty:11-20 (verbatim copy, entityName interpolated). -->
        <div class="text-center py-12 border border-dashed qt-border-default rounded-lg">
          <qt-icon name="image" class="mx-auto h-12 w-12 qt-text-secondary" />
          <p class="mt-2 qt-text-small">No photos in {{ entityName() }}&rsquo;s album yet</p>
          <p class="qt-text-label-xs mt-1">
            Upload one with the button above, or have {{ entityName() }}
            <code>keep_image</code> a generated image to start the album
          </p>
        </div>
      } @else {
        <!-- v4 GalleryGrid:36-40. -->
        <div
          class="grid gap-2"
          [style.grid-template-columns]="'repeat(auto-fill, minmax(' + thumbnailSize() + 'px, 1fr))'"
        >
          @for (photo of photos(); track photo.linkId; let index = $index) {
            <div class="relative group">
              <!-- v4 GalleryImage:28-59 — the tile button; avatar ring. -->
              <button
                type="button"
                class="relative aspect-square w-full overflow-hidden rounded-lg qt-bg-muted hover:ring-2 hover:ring-primary focus:outline-none focus:ring-2 focus:ring-ring transition-all {{
                  currentAvatarId() === photo.linkId ? 'ring-2 ring-success' : ''
                }}"
                (click)="selectedIndex.set(index)"
              >
                @if (missingImages().has(photo.linkId)) {
                  <div class="absolute inset-0 flex items-center justify-center qt-text-secondary">
                    <qt-icon name="image" class="w-8 h-8" />
                  </div>
                } @else {
                  <img
                    [src]="srcFor(photo)"
                    [alt]="photo.fileName"
                    [title]="photo.caption || null"
                    class="absolute inset-0 w-full h-full object-cover"
                    (error)="markMissing(photo.linkId)"
                  />
                }
                @if (currentAvatarId() === photo.linkId) {
                  <div
                    class="absolute top-1 left-1 bg-success qt-text-success-foreground text-xs px-1.5 py-0.5 rounded font-medium"
                  >
                    Avatar
                  </div>
                }
              </button>

              <!-- v4 GalleryImage:62-108 — the hover action overlay. -->
              <div
                class="absolute bottom-1 right-1 flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity"
              >
                @if (currentAvatarId() !== photo.linkId) {
                  <button
                    type="button"
                    class="p-1.5 rounded-full qt-shadow-md qt-bg-card qt-text-secondary hover:bg-success hover:qt-text-success-foreground transition-colors"
                    title="Set as avatar"
                    [disabled]="settingAvatar() === photo.linkId"
                    (click)="$event.stopPropagation(); handleSetAvatar(photo)"
                  >
                    <qt-icon name="user" class="w-4 h-4" />
                  </button>
                }
                @if (currentAvatarId() !== photo.linkId || missingImages().has(photo.linkId)) {
                  <button
                    type="button"
                    class="p-1.5 rounded-full qt-shadow-md transition-colors {{
                      confirmDelete() === photo.linkId
                        ? 'bg-destructive qt-text-destructive-foreground'
                        : 'qt-bg-card qt-text-secondary hover:bg-destructive hover:qt-text-destructive-foreground'
                    }}"
                    [title]="
                      confirmDelete() === photo.linkId
                        ? 'Click again to confirm delete'
                        : 'Delete image'
                    "
                    [disabled]="deletingImage() === photo.linkId"
                    (click)="$event.stopPropagation(); handleDeleteClick(photo)"
                  >
                    <qt-icon name="trash" class="w-4 h-4" />
                  </button>
                }
              </div>
            </div>
          }
        </div>
      }
    </div>

    <!-- v4 EmbeddedPhotoGallery:170-196 — the deep detail modal; the ONLY
         host passing onAvatarSet, and linkId = the tile's own id, so the
         modal's save-to-album takes the LINK-RELINK branch. -->
    @if (selectedImage(); as image) {
      <qt-image-detail-modal
        [image]="image"
        [onPrev]="selectedIndex() > 0 ? boundPrev : undefined"
        [onNext]="selectedIndex() < photos().length - 1 ? boundNext : undefined"
        (closeModal)="selectedIndex.set(-1)"
        (avatarSet)="onAvatarSetFromModal()"
      />
    }
  `,
})
export class CharacterGalleryTab {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  private readonly queryClient = injectQueryClient();

  readonly characterId = input.required<string>();

  protected readonly maxSizeIndex = THUMBNAIL_SIZES.length - 1;
  protected readonly sizeIndex = signal(DEFAULT_THUMBNAIL_INDEX);
  protected readonly thumbnailSize = computed(() => THUMBNAIL_SIZES[this.sizeIndex()]);
  protected readonly selectedIndex = signal(-1);
  protected readonly missingImages = signal<ReadonlySet<string>>(new Set());
  protected readonly settingAvatar = signal<string | null>(null);
  protected readonly deletingImage = signal<string | null>(null);
  /** v4 `confirmDelete` — armed tile id; disarms after 3 s (`:47-52`). */
  protected readonly confirmDelete = signal<string | null>(null);
  protected readonly uploading = signal(false);

  protected readonly photosQuery = injectQuery(() => ({
    queryKey: characterKeys.photos(this.characterId()),
    queryFn: (): Promise<CharacterPhoto[]> => fetchCharacterPhotos(this.core, this.characterId()),
  }));
  protected readonly photos = computed(() => this.photosQuery.data() ?? []);

  /** The shared detail cache — v4's `currentAvatarId` / `entityName` props. */
  protected readonly characterQuery = injectQuery(() => ({
    queryKey: characterKeys.detail(this.characterId()),
    queryFn: () => fetchCharacter(this.core, this.characterId()),
  }));
  protected readonly currentAvatarId = computed(
    () => this.characterQuery.data()?.defaultImageId ?? null,
  );
  protected readonly entityName = computed(() => this.characterQuery.data()?.name || 'Character');

  protected readonly selectedImage = computed<ImageData | null>(() => {
    const index = this.selectedIndex();
    const photo = index >= 0 ? this.photos()[index] : undefined;
    if (!photo) return null;
    // v4 `:174-186` — the tile → ImageData mapping; `linkId: photo.linkId`
    // (the id IS a link id here) drives the modal's link-relink save branch,
    // and `tags: []` is hardcoded (scope correction 1).
    return {
      id: photo.linkId,
      linkId: photo.linkId,
      filename: photo.fileName,
      filepath: photo.blobUrl,
      url: apiUrl(photo.blobUrl),
      mimeType: photo.mimeType || 'image/webp',
      size: photo.fileSizeBytes,
      createdAt: photo.keptAt,
      tags: [],
    };
  });

  /** v4 `:54-60` — the host's clamped index arithmetic. */
  protected readonly boundPrev = (): void => {
    this.selectedIndex.update((prev) => (prev > 0 ? prev - 1 : prev));
  };
  protected readonly boundNext = (): void => {
    this.selectedIndex.update((prev) =>
      prev < this.photos().length - 1 ? prev + 1 : prev,
    );
  };

  constructor() {
    // v4 `:47-52` — an armed delete disarms after 3 s.
    effect((onCleanup) => {
      const armed = this.confirmDelete();
      if (armed) {
        const timer = setTimeout(() => untracked(() => this.confirmDelete.set(null)), 3000);
        onCleanup(() => clearTimeout(timer));
      }
    });
  }

  protected srcFor(photo: CharacterPhoto): string | null {
    return normalizeAvatarSrc(photo.blobUrl);
  }

  protected markMissing(linkId: string): void {
    this.missingImages.update((prev) => new Set(prev).add(linkId));
  }

  /** v4 `useGalleryData.handleSetAvatar` (`:88-113`) via `characterAvatar`. */
  protected async handleSetAvatar(photo: CharacterPhoto): Promise<void> {
    this.settingAvatar.set(photo.linkId);
    try {
      const resp = await this.core.dispatch({
        type: 'characterAvatar',
        characterId: this.characterId(),
        imageId: photo.linkId,
      });
      if (resp.type === 'error') {
        throw new Error(resp.data.message || 'Failed to set avatar');
      }
      await this.refreshCharacter();
      this.toasts.showSuccess('Avatar updated!');
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to set avatar');
    } finally {
      this.settingAvatar.set(null);
    }
  }

  /** v4 `useGalleryData.handleClearAvatar` (`:115-140`) — `imageId: null`. */
  protected async handleClearAvatar(): Promise<void> {
    if (!this.currentAvatarId()) return;
    try {
      const resp = await this.core.dispatch({
        type: 'characterAvatar',
        characterId: this.characterId(),
        imageId: null,
      });
      if (resp.type === 'error') {
        throw new Error(resp.data.message || 'Failed to clear avatar');
      }
      await this.refreshCharacter();
      this.toasts.showSuccess('Avatar cleared!');
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to clear avatar');
    }
  }

  /** v4 `EmbeddedPhotoGallery.handleDeleteImageClick` (`:84-98`) — arm, then delete. */
  protected async handleDeleteClick(photo: CharacterPhoto): Promise<void> {
    if (this.confirmDelete() !== photo.linkId) {
      this.confirmDelete.set(photo.linkId);
      return;
    }
    this.deletingImage.set(photo.linkId);
    this.confirmDelete.set(null);
    try {
      const isCurrentAvatar = this.currentAvatarId() === photo.linkId;
      const resp = await this.core.dispatch({
        type: 'characterPhotoRemove',
        characterId: this.characterId(),
        linkId: photo.linkId,
      });
      if (resp.type === 'error') {
        throw new Error(resp.data.message || 'Failed to delete image');
      }
      // v4 `useGalleryData:158-162` — the server already nulled the avatar
      // reference when the deleted link was the current avatar.
      if (isCurrentAvatar) {
        await this.refreshCharacter();
      }
      await this.queryClient.invalidateQueries({
        queryKey: characterKeys.photos(this.characterId()),
      });
      this.toasts.showSuccess('Image deleted');
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to delete image');
    } finally {
      this.deletingImage.set(null);
    }
  }

  /** v4 `:189-195` — the modal set an avatar: refetch photos + the character. */
  protected async onAvatarSetFromModal(): Promise<void> {
    await this.queryClient.invalidateQueries({
      queryKey: characterKeys.photos(this.characterId()),
    });
    await this.refreshCharacter();
  }

  protected async onPick(input: HTMLInputElement): Promise<void> {
    const file = input.files?.[0];
    // Reset the input so re-picking the same file re-fires change.
    input.value = '';
    if (!file) {
      return;
    }
    this.uploading.set(true);
    try {
      await uploadCharacterPhoto(this.characterId(), file);
      this.toasts.showSuccess('Image uploaded');
      await this.queryClient.invalidateQueries({
        queryKey: characterKeys.photos(this.characterId()),
      });
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to upload image');
    } finally {
      this.uploading.set(false);
    }
  }

  private async refreshCharacter(): Promise<void> {
    await this.queryClient.invalidateQueries({
      queryKey: characterKeys.detail(this.characterId()),
    });
  }

}
