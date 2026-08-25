import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import type { CharacterGalleryLink, DetailCharacter } from './images.api';

/**
 * The character-album panel under the detail image — a port of v4
 * `components/images/image-detail/ImageMetadata.tsx` (150 lines):
 *
 * - **"In Photo Albums"** — the characters whose vault gallery holds a copy of
 *   this image (derived from `characterGalleryLinks`), each with an Avatar
 *   badge (when `character.defaultImageId === imageId`) or a Set Avatar
 *   button, plus a per-character remove (`×`).
 * - **"Save to photo album"** — a select over the characters NOT yet holding
 *   the image; picking one emits the add.
 *
 * Both lists sort by `name.localeCompare` (v4 `:46-47`). The select keeps
 * v4's controlled `value=""` reset by writing the DOM value back to `''`
 * after each pick — no `[value]` binding (the standing dogfood-#6 select
 * rule).
 */
@Component({
  selector: 'qt-image-metadata',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="qt-panel qt-bg-overlay-medium backdrop-blur-sm w-full max-w-2xl">
      <div class="flex flex-col gap-4">
        @if (loadingEntities()) {
          <p class="qt-text-secondary text-sm">Loading characters...</p>
        }

        <!-- Section 1: Characters whose gallery contains this photo (v4 :59-110) -->
        @if (!loadingEntities() && galleryCharacters().length > 0) {
          <div>
            <h3 class="text-foreground font-semibold mb-3">In Photo Albums</h3>
            <div class="flex flex-col gap-2">
              @for (character of galleryCharacters(); track character.id) {
                <div class="flex items-center gap-2 px-3 py-2 qt-bg-muted/50 rounded">
                  <span class="flex-1 text-foreground font-medium text-sm">
                    {{ character.name }}
                  </span>
                  @if (character.defaultImageId === imageId()) {
                    <span class="text-xs bg-success qt-text-on-success px-2 py-0.5 rounded">
                      Avatar
                    </span>
                  } @else {
                    <button
                      type="button"
                      class="qt-button-success qt-button-sm"
                      title="Set as avatar"
                      [disabled]="isSettingAvatar(character.id)"
                      (click)="$event.stopPropagation(); setAsAvatar.emit(character.id)"
                    >
                      {{ isSettingAvatar(character.id) ? '...' : 'Set Avatar' }}
                    </button>
                  }
                  <button
                    type="button"
                    class="w-6 h-6 flex items-center justify-center qt-text-secondary hover:qt-text-destructive hover:qt-bg-destructive/20 rounded transition-colors disabled:opacity-50"
                    title="Remove from photo album"
                    [disabled]="savingToGalleryFor().has(character.id)"
                    (click)="$event.stopPropagation(); removeFromCharacterGallery.emit(character.id)"
                  >
                    {{ savingToGalleryFor().has(character.id) ? '...' : '×' }}
                  </button>
                </div>
              }
            </div>
          </div>
        }

        <!-- Section 2: Save to character photo album (v4 :112-141) -->
        @if (!loadingEntities() && availableCharacters().length > 0) {
          <div>
            <label for="add-to-character-gallery" class="block text-foreground font-semibold mb-2">
              Save to photo album
            </label>
            <select
              id="add-to-character-gallery"
              class="qt-select w-full"
              (change)="onSelectCharacter($any($event.target))"
            >
              <option value="" disabled selected>Select a character...</option>
              @for (character of availableCharacters(); track character.id) {
                <option [value]="character.id">{{ character.name }}</option>
              }
            </select>
          </div>
        }

        <!-- No characters available message (v4 :143-146) -->
        @if (!loadingEntities() && characters().length === 0) {
          <p class="qt-text-secondary text-sm">No characters available.</p>
        }
      </div>
    </div>
  `,
})
export class ImageMetadata {
  readonly imageId = input.required<string>();
  readonly characters = input.required<DetailCharacter[]>();
  readonly loadingEntities = input.required<boolean>();
  readonly characterGalleryLinks = input.required<CharacterGalleryLink[]>();
  /** Character ids with an in-flight add/remove (v4 `savingToGalleryFor`). */
  readonly savingToGalleryFor = input.required<ReadonlySet<string>>();
  /** `character-{id}-avatar` keys with an in-flight avatar set (v4 `settingAvatar`). */
  readonly settingAvatar = input.required<ReadonlySet<string>>();

  readonly addToCharacterGallery = output<string>();
  readonly removeFromCharacterGallery = output<string>();
  /** v4 emits `('character', entityId)`; EntityType is only `'character'`. */
  readonly setAsAvatar = output<string>();

  private readonly inGalleryCharacterIds = computed(
    () => new Set(this.characterGalleryLinks().map((l) => l.characterId)),
  );

  /** v4 `:34-50` — the in-gallery/available split, each name-sorted. */
  protected readonly galleryCharacters = computed(() => {
    const inIds = this.inGalleryCharacterIds();
    return this.characters()
      .filter((c) => inIds.has(c.id))
      .sort((a, b) => a.name.localeCompare(b.name));
  });
  protected readonly availableCharacters = computed(() => {
    const inIds = this.inGalleryCharacterIds();
    return this.characters()
      .filter((c) => !inIds.has(c.id))
      .sort((a, b) => a.name.localeCompare(b.name));
  });

  protected isSettingAvatar(characterId: string): boolean {
    // v4 `:67` — the key shape the hook stamps.
    return this.settingAvatar().has(`character-${characterId}-avatar`);
  }

  protected onSelectCharacter(select: HTMLSelectElement): void {
    const value = select.value;
    // v4's controlled `value=""` — the select snaps back after each pick.
    select.value = '';
    if (value) {
      this.addToCharacterGallery.emit(value);
    }
  }
}
