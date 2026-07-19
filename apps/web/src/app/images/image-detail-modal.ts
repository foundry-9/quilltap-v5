import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
  untracked,
} from '@angular/core';

import { CoreClient } from '../core/core-client';
import { apiUrl } from '../core/api-url';
import { DeletedImagePlaceholder } from './deleted-image-placeholder';
import { ImageActions } from './image-actions';
import { ImageMetadata } from './image-metadata';
import { applyImageNavigation } from './image-navigation';
import {
  fetchCharacterGalleryLinks,
  type CharacterGalleryLink,
  type DetailCharacter,
  type ImageData,
} from './images.api';

/**
 * The deep image-detail modal — a port of v4
 * `components/images/image-detail/ImageDetailModal.tsx` (145 lines) plus its
 * `useImageActions` hook (229 lines): the z-[60] overlay layered ABOVE the
 * z-50 gallery, with the nav arrows/toolbar (`qt-image-actions`) and the
 * character-album panel (`qt-image-metadata`) fed by the parallel
 * `characterList` + `imageInfoGet` load (v4 `:43-46`).
 *
 * The album actions branch on the `ImageData` `linkId`-vs-`id` split
 * (`useImageActions.ts:46-48`): a vault photo saves `{linkId}` (the server
 * re-links from the existing blob), an images-v2 file saves `{fileId}`.
 *
 * v4 quirk carried faithfully: the hook's `internalCharacters` roster is a
 * `useState(characters)` snapshot taken at MOUNT (before the roster loads), so
 * the add-to-album toast's name lookup always misses and falls back to
 * `'Character'` (`useImageActions.ts:34`, `:61`, `:67`), and the Avatar badge
 * does not move after Set Avatar until the panel reloads. Reproduced, not
 * repaired — a repair would be silent divergence.
 *
 * v4's toasts render as the transient inline notice (the `image-modal.ts`
 * divergence — the SPA has no toast service).
 */
@Component({
  selector: 'qt-image-detail-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [DeletedImagePlaceholder, ImageActions, ImageMetadata],
  template: `
    <!-- v4 ImageDetailModal.tsx:87 — z-[60], one layer above the z-50 gallery. -->
    <div
      class="fixed inset-0 z-[60] flex items-center justify-center qt-bg-overlay backdrop-blur-sm p-4"
      (click)="closeModal.emit()"
    >
      <qt-image-actions
        [onPrev]="onPrev()"
        [onNext]="onNext()"
        [savingToGallery]="savingToGallery()"
        (download)="handleDownload()"
        (copyImage)="handleCopyToClipboard()"
        (saveToGallery)="handleSaveToGallery()"
        (closeModal)="closeModal.emit()"
      />

      <!-- Image container -->
      <div
        class="relative max-w-[90vw] max-h-[90vh] flex flex-col items-center justify-center gap-4"
        (click)="$event.stopPropagation()"
      >
        @if (imageMissing()) {
          <qt-deleted-image-placeholder
            [imageId]="image().id"
            [filename]="image().filename"
            [width]="600"
            [height]="400"
            (cleanup)="closeModal.emit()"
          />
        } @else {
          <img
            [src]="imageSrc()"
            [alt]="image().filename"
            class="max-w-full max-h-[70vh] w-auto h-auto object-contain"
            (error)="imageMissing.set(true)"
          />
        }

        <!-- Character gallery panel -->
        @if (!imageMissing()) {
          <qt-image-metadata
            [imageId]="image().id"
            [characters]="characters()"
            [loadingEntities]="loadingEntities()"
            [characterGalleryLinks]="characterGalleryLinks()"
            [savingToGalleryFor]="savingToGalleryFor()"
            [settingAvatar]="settingAvatar()"
            (addToCharacterGallery)="addToCharacterGallery($event)"
            (removeFromCharacterGallery)="removeFromCharacterGallery($event)"
            (setAsAvatar)="setAsAvatar($event)"
          />
        }
      </div>

      @if (notice(); as n) {
        <div
          class="absolute top-4 left-1/2 -translate-x-1/2 qt-text-overlay text-sm qt-bg-overlay-caption px-3 py-1 rounded"
        >
          {{ n }}
        </div>
      }

      <!-- Filename at bottom -->
      <div
        class="absolute bottom-4 left-1/2 -translate-x-1/2 qt-text-overlay-muted text-sm qt-bg-overlay-caption px-3 py-1 rounded"
      >
        {{ image().filename }}
      </div>
    </div>
  `,
})
export class ImageDetailModal {
  private readonly core = inject(CoreClient);

  readonly image = input.required<ImageData>();
  /** Conditionally `undefined` at the ends of the host's list (the arrow idiom). */
  readonly onPrev = input<(() => void) | undefined>(undefined);
  readonly onNext = input<(() => void) | undefined>(undefined);

  readonly closeModal = output<void>();
  /** v4 `onAvatarSet` — only the embedded-gallery host passes it. */
  readonly avatarSet = output<void>();

  protected readonly characters = signal<DetailCharacter[]>([]);
  protected readonly loadingEntities = signal(true);
  protected readonly imageMissing = signal(false);
  protected readonly characterGalleryLinks = signal<CharacterGalleryLink[]>([]);
  protected readonly savingToGalleryFor = signal<ReadonlySet<string>>(new Set());
  protected readonly settingAvatar = signal<ReadonlySet<string>>(new Set());
  protected readonly savingToGallery = signal(false);
  protected readonly notice = signal<string | null>(null);

  /**
   * The v4 hook's mount-time roster snapshot (`useImageActions.ts:34`) — stays
   * empty unless `setAsAvatar` maps it, so the add toast's name lookup misses
   * (the documented quirk).
   */
  private readonly internalCharacters = signal<DetailCharacter[]>([]);

  /** v4 `:82-83` — url over filepath, leading slash guaranteed. */
  protected readonly imageSrc = computed(() => {
    const filepath = this.image().url || this.image().filepath;
    if (/^[a-z][a-z0-9+.-]*:/i.test(filepath)) {
      return filepath; // already absolute (a resolved byte-route URL)
    }
    return apiUrl(filepath.startsWith('/') ? filepath : `/${filepath}`);
  });

  constructor() {
    // v4 `:38-70` — (re)load the roster + gallery links per image id. The
    // gallery keeps this modal mounted across prev/next, so the load re-runs
    // when the image input changes (React's `[isOpen, image.id]` deps).
    effect(() => {
      const id = this.image().id;
      untracked(() => void this.loadEntities(id));
    });

    // v4 `:72-78` — keyboard navigation (Escape, arrow keys), re-applied when
    // the conditionally-undefined callbacks change (the hook's dep-change
    // re-run), disposed on destroy.
    let dispose: (() => void) | null = null;
    effect(() => {
      const onPrev = this.onPrev();
      const onNext = this.onNext();
      dispose?.();
      dispose = applyImageNavigation({
        isOpen: true,
        onClose: () => this.closeModal.emit(),
        onPrev,
        onNext,
      });
    });
    inject(DestroyRef).onDestroy(() => dispose?.());
  }

  private async loadEntities(imageId: string): Promise<void> {
    this.loadingEntities.set(true);
    try {
      // v4 `:43-46` — the two reads run in PARALLEL.
      const [chars, links] = await Promise.all([
        this.core
          .dispatchData({ type: 'characterList' })
          .then((data) => (data['characters'] as DetailCharacter[]) ?? [])
          .catch(() => null),
        fetchCharacterGalleryLinks(this.core, imageId),
      ]);
      if (chars) {
        this.characters.set(chars);
      }
      if (links) {
        this.characterGalleryLinks.set(links);
      }
    } finally {
      this.loadingEntities.set(false);
    }
  }

  // ── the useImageActions port ─────────────────────────────────────────────

  /** v4 `useImageActions.ts:41-85`. */
  protected async addToCharacterGallery(characterId: string): Promise<void> {
    this.mutateSet(this.savingToGalleryFor, (s) => s.add(characterId));
    try {
      // The `ImageData` split (`:46-48`): vault photos re-link by linkId;
      // images-v2 files save by fileId.
      const linkId = this.image().linkId;
      const resp = await this.core.dispatch(
        linkId
          ? { type: 'characterPhotoSaveById', characterId, linkId }
          : { type: 'characterPhotoSaveById', characterId, fileId: this.image().id },
      );
      if (resp.type === 'error') {
        throw new Error(resp.data.message || 'Failed to save to photo album');
      }
      const data = resp.data as { linkId?: string };
      // The mount-time snapshot lookup — misses by design (the v4 quirk).
      const character = this.internalCharacters().find((c) => c.id === characterId);
      this.characterGalleryLinks.update((prev) => [
        ...prev,
        {
          characterId,
          characterName: character?.name ?? 'Character',
          linkId: data.linkId ?? '',
        },
      ]);
      this.flash(`Saved to ${character?.name ?? 'character'}'s photo album`);
    } catch (err) {
      this.flash(err instanceof Error ? err.message : 'Failed to save to photo album');
    } finally {
      this.mutateSet(this.savingToGalleryFor, (s) => s.delete(characterId));
    }
  }

  /** v4 `useImageActions.ts:87-120`. */
  protected async removeFromCharacterGallery(characterId: string): Promise<void> {
    const link = this.characterGalleryLinks().find((l) => l.characterId === characterId);
    if (!link) return;
    this.mutateSet(this.savingToGalleryFor, (s) => s.add(characterId));
    try {
      const resp = await this.core.dispatch({
        type: 'characterPhotoRemove',
        characterId,
        linkId: link.linkId,
      });
      if (resp.type === 'error') {
        throw new Error(resp.data.message || 'Failed to remove from photo album');
      }
      this.characterGalleryLinks.update((prev) =>
        prev.filter((l) => l.characterId !== characterId),
      );
      this.flash(`Removed from ${link.characterName}'s photo album`);
    } catch (err) {
      this.flash(err instanceof Error ? err.message : 'Failed to remove from photo album');
    } finally {
      this.mutateSet(this.savingToGalleryFor, (s) => s.delete(characterId));
    }
  }

  /** v4 `useImageActions.ts:122-165` (`POST …?action=avatar` → `characterAvatar`). */
  protected async setAsAvatar(characterId: string): Promise<void> {
    const key = `character-${characterId}-avatar`;
    this.mutateSet(this.settingAvatar, (s) => s.add(key));
    try {
      const resp = await this.core.dispatch({
        type: 'characterAvatar',
        characterId,
        imageId: this.image().id,
      });
      if (resp.type === 'error') {
        throw new Error(resp.data.message || 'Failed to set avatar');
      }
      // v4 `:144-148` — updates the hook's INTERNAL snapshot only; the panel's
      // roster (and so the Avatar badge) stays put until the next load.
      this.internalCharacters.update((prev) =>
        prev.map((c) => (c.id === characterId ? { ...c, defaultImageId: this.image().id } : c)),
      );
      this.flash('Set as avatar for character');
      this.avatarSet.emit();
    } catch (err) {
      this.flash(err instanceof Error ? err.message : 'Failed to set avatar');
    } finally {
      this.mutateSet(this.settingAvatar, (s) => s.delete(key));
    }
  }

  /** v4 `useImageActions.ts:167-178`. */
  protected async handleDownload(): Promise<void> {
    try {
      const response = await fetch(this.imageSrc());
      const blob = await response.blob();
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = this.image().filename;
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(url);
    } catch {
      // v4 logs and stays silent (no toast on download failure).
    }
  }

  /** v4 `useImageActions.ts:180-200` — the user-gallery save (`photoGallerySave`). */
  protected async handleSaveToGallery(): Promise<void> {
    this.savingToGallery.set(true);
    try {
      const resp = await this.core.dispatch({
        type: 'photoGallerySave',
        fileId: this.image().id,
      });
      if (resp.type === 'error') {
        throw new Error(resp.data.message || 'Failed to save to gallery');
      }
      this.flash('Saved to your gallery');
    } catch (err) {
      this.flash(err instanceof Error ? err.message : 'Failed to save to gallery');
    } finally {
      this.savingToGallery.set(false);
    }
  }

  /** v4 `useImageActions.ts:202-213`. */
  protected async handleCopyToClipboard(): Promise<void> {
    try {
      const response = await fetch(this.imageSrc());
      const blob = await response.blob();
      await navigator.clipboard.write([new ClipboardItem({ [blob.type]: blob })]);
      this.flash('Image copied to clipboard');
    } catch {
      this.flash('Failed to copy image to clipboard');
    }
  }

  private mutateSet(
    target: typeof this.savingToGalleryFor,
    mutate: (s: Set<string>) => unknown,
  ): void {
    const next = new Set(target());
    mutate(next);
    target.set(next);
  }

  private flash(message: string): void {
    this.notice.set(message);
    setTimeout(() => this.notice.set(null), 2500);
  }
}
