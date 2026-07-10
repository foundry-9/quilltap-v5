import { ChangeDetectionStrategy, Component, computed, inject, input } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../../core/core-client';
import type { CharacterPhoto } from '../../../../core/core-contract';
import { Icon } from '../../../../ui/icon';
import { normalizeAvatarSrc } from '../../../../ui/avatar-stack';
import { EmptyState } from '../../../../ui/empty-state';
import { LoadingState } from '../../../../ui/loading-state';
import { characterKeys, fetchCharacterPhotos } from '../../characters.api';

/**
 * The Photo Gallery tab (v4 `components/CharacterGallery.tsx` →
 * `EmbeddedPhotoGallery`): a minimal grid over `characterPhotoList`, delete
 * via `characterPhotoRemove`. Upload (the v4 drag-and-drop / paste flow) is
 * deferred — noted inline.
 */
@Component({
  selector: 'qt-character-gallery-tab',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, EmptyState, LoadingState],
  template: `
    <div class="space-y-4">
      <div class="flex items-center justify-between flex-wrap gap-2">
        <div>
          <h2 class="qt-heading-4 text-foreground">Photo Gallery</h2>
          <p class="qt-text-small">Portraits saved for this character.</p>
        </div>
        <button
          type="button"
          disabled
          class="qt-button-secondary"
          title="Uploading new photos is not yet available in this vertical"
        >
          <qt-icon name="upload" class="w-4 h-4" />
          Upload Photo
        </button>
      </div>

      @if (photosQuery.isPending()) {
        <qt-loading-state message="Loading photos..." />
      } @else if (photos().length === 0) {
        <qt-empty-state title="No photos yet" description="Portraits added to this character will appear here." variant="dashed" />
      } @else {
        <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4">
          @for (photo of photos(); track photo.id) {
            <div class="relative overflow-hidden rounded-lg border qt-border-default qt-bg-muted" style="aspect-ratio: 4/5">
              <img [src]="srcFor(photo)" [alt]="'Character photo'" class="h-full w-full object-cover" />
              <button
                type="button"
                class="absolute top-1 right-1 rounded-full qt-bg-card/90 p-1.5 qt-text-secondary hover:text-destructive"
                title="Delete this photo"
                (click)="remove(photo)"
              >
                <qt-icon name="trash" class="w-4 h-4" />
              </button>
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

  protected srcFor(photo: CharacterPhoto): string | null {
    return photo.url || normalizeAvatarSrc(photo.filepath);
  }

  protected async remove(photo: CharacterPhoto): Promise<void> {
    const linkId = photo.linkId ?? photo.id;
    await this.core.dispatchData({ type: 'characterPhotoRemove', characterId: this.characterId(), linkId });
    await this.queryClient.invalidateQueries({ queryKey: characterKeys.photos(this.characterId()) });
  }
}
