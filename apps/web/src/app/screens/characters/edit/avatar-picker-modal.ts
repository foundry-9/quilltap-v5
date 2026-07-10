import { ChangeDetectionStrategy, Component, inject, input, output, signal } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { CharacterPhoto } from '../../../core/core-contract';
import { Modal } from '../../../ui/modal';
import { characterKeys, fetchCharacterPhotos } from '../characters.api';

/**
 * A minimal avatar picker (v4 `components/images/avatar-selector.tsx`
 * subset): a grid over the character's photo gallery (`characterPhotoList`),
 * pick one and "Set as Avatar" (`characterAvatar {imageId}`), or "Clear
 * Avatar" (`imageId: null`). Uploading new photos into the gallery is a named
 * deferral (v4's `ImageUploadDialog` — out of scope this round).
 */
@Component({
  selector: 'qt-avatar-picker-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal title="Change Avatar" maxWidth="lg" (close)="close.emit()">
      @if (error()) {
        <div class="qt-alert-error mb-4">{{ error() }}</div>
      }

      @if (photosQuery.isPending()) {
        <div class="py-8 text-center qt-text-small">Loading photos...</div>
      } @else if (photos().length === 0) {
        <p class="qt-text-small py-4">
          No photos in this character&apos;s gallery yet. Upload photos from the character&apos;s
          Appearance tab to choose an avatar.
        </p>
      } @else {
        <div class="grid grid-cols-4 gap-3">
          @for (photo of photos(); track photo.id) {
            <button
              type="button"
              class="rounded-lg overflow-hidden border-2"
              [class]="selectedId() === photo.id ? 'qt-border-primary' : 'qt-border-default'"
              (click)="selectedId.set(photo.id)"
            >
              <img [src]="photo.url ?? photo.filepath" alt="" class="w-full h-full object-cover" />
            </button>
          }
        </div>
      }

      <div qt-modal-footer class="flex justify-between">
        <button
          type="button"
          class="qt-button-secondary"
          [disabled]="saving()"
          (click)="clearAvatar()"
        >
          Clear Avatar
        </button>
        <div class="flex gap-3">
          <button type="button" class="qt-button-secondary" (click)="close.emit()">Cancel</button>
          <button
            type="button"
            class="qt-button-primary"
            [disabled]="!selectedId() || saving()"
            (click)="setAvatar()"
          >
            {{ saving() ? 'Saving...' : 'Set as Avatar' }}
          </button>
        </div>
      </div>
    </qt-modal>
  `,
})
export class AvatarPickerModal {
  private readonly core = inject(CoreClient);

  readonly characterId = input.required<string>();
  readonly close = output<void>();
  readonly saved = output<void>();

  protected readonly selectedId = signal<string | null>(null);
  protected readonly saving = signal(false);
  protected readonly error = signal<string | null>(null);

  protected readonly photosQuery = injectQuery(() => ({
    queryKey: characterKeys.photos(this.characterId()),
    queryFn: (): Promise<CharacterPhoto[]> => fetchCharacterPhotos(this.core, this.characterId()),
  }));

  protected photos(): CharacterPhoto[] {
    return this.photosQuery.data() ?? [];
  }

  protected async setAvatar(): Promise<void> {
    const imageId = this.selectedId();
    if (!imageId) {
      return;
    }
    await this.applyAvatar(imageId);
  }

  protected async clearAvatar(): Promise<void> {
    await this.applyAvatar(null);
  }

  private async applyAvatar(imageId: string | null): Promise<void> {
    this.saving.set(true);
    this.error.set(null);
    try {
      await this.core.dispatchData({
        type: 'characterAvatar',
        characterId: this.characterId(),
        imageId,
      });
      this.saved.emit();
      this.close.emit();
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'Failed to set avatar');
    } finally {
      this.saving.set(false);
    }
  }
}
