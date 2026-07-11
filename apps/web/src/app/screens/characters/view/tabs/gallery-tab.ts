import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../../core/core-client';
import type { CharacterPhoto } from '../../../../core/core-contract';
import { Icon } from '../../../../ui/icon';
import { normalizeAvatarSrc } from '../../../../ui/avatar-stack';
import { EmptyState } from '../../../../ui/empty-state';
import { ErrorAlert } from '../../../../ui/error-alert';
import { LoadingState } from '../../../../ui/loading-state';
import { characterKeys, fetchCharacterPhotos, uploadCharacterPhoto } from '../../characters.api';

/**
 * The Photo Gallery tab (v4 `components/CharacterGallery.tsx` →
 * `EmbeddedPhotoGallery`): a minimal grid over `characterPhotoList`, delete
 * via `characterPhotoRemove`, and an **Upload Photo** affordance that multipart-
 * POSTs to lane C's `POST /api/v1/characters/{id}/photos` web route (the byte leg
 * dispatch can't carry). Upload failures surface the v4 400 keyword message.
 */
@Component({
  selector: 'qt-character-gallery-tab',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, EmptyState, ErrorAlert, LoadingState],
  template: `
    <div class="space-y-4">
      <div class="flex items-center justify-between flex-wrap gap-2">
        <div>
          <h2 class="qt-heading-4 text-foreground">Photo Gallery</h2>
          <p class="qt-text-small">Portraits saved for this character.</p>
        </div>
        <button
          type="button"
          class="qt-button-secondary"
          [disabled]="uploading()"
          title="Upload a new portrait for this character"
          (click)="fileInput.click()"
        >
          <qt-icon name="upload" class="w-4 h-4" />
          {{ uploading() ? 'Uploading...' : 'Upload Photo' }}
        </button>
        <input
          #fileInput
          type="file"
          accept="image/*"
          class="hidden"
          aria-label="Upload photo"
          (change)="onPick($any($event.target))"
        />
      </div>

      @if (uploadError(); as msg) {
        <qt-error-alert [message]="msg" />
      }

      @if (photosQuery.isPending()) {
        <qt-loading-state message="Loading photos..." />
      } @else if (photos().length === 0) {
        <qt-empty-state
          title="No photos yet"
          description="Portraits added to this character will appear here."
          variant="dashed"
        />
      } @else {
        <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4">
          @for (photo of photos(); track photo.linkId) {
            <div
              class="relative overflow-hidden rounded-lg border qt-border-default qt-bg-muted"
              style="aspect-ratio: 4/5"
            >
              <img
                [src]="srcFor(photo)"
                [alt]="photo.caption || 'Character photo'"
                [title]="photo.caption || null"
                class="h-full w-full object-cover"
              />
              <button
                type="button"
                class="absolute top-1 right-1 rounded-full qt-bg-card/90 p-1.5 qt-text-secondary hover:text-destructive"
                title="Delete this photo"
                (click)="remove(photo)"
              >
                <qt-icon name="trash" class="w-4 h-4" />
              </button>
              @if (photo.caption) {
                <p
                  class="absolute inset-x-0 bottom-0 truncate qt-bg-card/80 px-2 py-1 qt-text-xs qt-text-secondary"
                >
                  {{ photo.caption }}
                </p>
              }
            </div>
          }
        </div>
      }
    </div>
  `,
})
export class CharacterGalleryTab {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();

  readonly characterId = input.required<string>();

  protected readonly photosQuery = injectQuery(() => ({
    queryKey: characterKeys.photos(this.characterId()),
    queryFn: (): Promise<CharacterPhoto[]> => fetchCharacterPhotos(this.core, this.characterId()),
  }));

  protected readonly photos = computed(() => this.photosQuery.data() ?? []);
  protected readonly uploading = signal(false);
  protected readonly uploadError = signal<string | null>(null);

  protected srcFor(photo: CharacterPhoto): string | null {
    return normalizeAvatarSrc(photo.blobUrl);
  }

  protected async onPick(input: HTMLInputElement): Promise<void> {
    const file = input.files?.[0];
    // Reset the input so re-picking the same file re-fires change.
    input.value = '';
    if (!file) {
      return;
    }
    this.uploading.set(true);
    this.uploadError.set(null);
    try {
      await uploadCharacterPhoto(this.characterId(), file);
      await this.queryClient.invalidateQueries({
        queryKey: characterKeys.photos(this.characterId()),
      });
    } catch (err) {
      this.uploadError.set(err instanceof Error ? err.message : 'Failed to upload photo');
    } finally {
      this.uploading.set(false);
    }
  }

  protected async remove(photo: CharacterPhoto): Promise<void> {
    await this.core.dispatchData({
      type: 'characterPhotoRemove',
      characterId: this.characterId(),
      linkId: photo.linkId,
    });
    await this.queryClient.invalidateQueries({
      queryKey: characterKeys.photos(this.characterId()),
    });
  }
}
