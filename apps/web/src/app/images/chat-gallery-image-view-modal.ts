import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  effect,
  inject,
  input,
  output,
  signal,
  untracked,
} from '@angular/core';

import { CoreClient } from '../core/core-client';
import { Icon } from '../ui/icon';
import { DeletedImagePlaceholder } from './deleted-image-placeholder';
import { applyImageNavigation } from './image-navigation';
import { fetchCharacterGalleryLinks } from './images.api';
import { ToastService } from '../ui/toast.service';

/** The chat-file slice the modal reads (v4 `ChatGalleryImageViewModal.tsx:13-22`). */
export interface ChatGalleryFile {
  id: string;
  filename: string;
  /** The displayable byte URL (v5 serves it off the id-keyed file route). */
  url: string;
}

/**
 * The chat-item detail modal — a port of v4
 * `components/chat/ChatGalleryImageViewModal.tsx` (361 lines): the z-[60]
 * viewer `PhotoGalleryModal` routes `kind: 'chat'` items to
 * (`PhotoGalleryModal.tsx:338-351`), with the HARD chat-file delete and the
 * two album toggle buttons whose already-saved state comes from
 * `imageInfoGet` (`:49-90` — the load-bearing read this lane added; without
 * it these buttons were write-only).
 *
 * `onPrev`/`onNext` are the conditionally-`undefined` function inputs (the
 * arrow idiom); keyboard rides `applyImageNavigation`.
 */
@Component({
  selector: 'qt-chat-gallery-image-view-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, DeletedImagePlaceholder],
  template: `
    <!-- v4 :208 — z-[60], one layer above the z-50 gallery. -->
    <div
      class="fixed inset-0 z-[60] flex items-center justify-center qt-bg-overlay backdrop-blur-sm"
      role="dialog"
      aria-modal="true"
      (click)="closeModal.emit()"
    >
      <!-- Navigation buttons (v4 :214-238) -->
      @if (onPrev(); as prev) {
        <button
          type="button"
          class="absolute left-4 top-1/2 -translate-y-1/2 p-3 qt-bg-overlay-btn hover:qt-bg-overlay-btn rounded-full qt-text-overlay transition-colors z-10 cursor-pointer"
          title="Previous image (Left Arrow)"
          (click)="$event.stopPropagation(); prev()"
        >
          <qt-icon name="chevron-left" class="w-8 h-8" />
        </button>
      }
      @if (onNext(); as next) {
        <button
          type="button"
          class="absolute right-4 top-1/2 -translate-y-1/2 p-3 qt-bg-overlay-btn hover:qt-bg-overlay-btn rounded-full qt-text-overlay transition-colors z-10 cursor-pointer"
          title="Next image (Right Arrow)"
          (click)="$event.stopPropagation(); next()"
        >
          <qt-icon name="chevron-right" class="w-8 h-8" />
        </button>
      }

      <!-- Top right control buttons (v4 :241-313, hidden while missing) -->
      @if (!imageMissing()) {
        <div class="absolute top-4 right-4 flex gap-2 z-10">
          @if (characterId(); as cid) {
            <button
              type="button"
              class="p-2 rounded-full qt-text-overlay transition-colors disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer {{
                isInCharacterGallery() ? 'bg-primary hover:qt-bg-primary/90' : 'qt-bg-overlay-btn hover:qt-bg-overlay-btn'
              }}"
              [disabled]="isSaving() || checkingGallery()"
              [title]="
                isInCharacterGallery()
                  ? 'Remove from ' + (characterName() || 'character') + '\\'s photo album'
                  : 'Save to ' + (characterName() || 'character') + '\\'s photo album'
              "
              (click)="$event.stopPropagation(); toggleCharacterGallery()"
            >
              <qt-icon name="user" class="w-6 h-6" />
            </button>
          }
          @if (userCharacterId(); as ucid) {
            <button
              type="button"
              class="p-2 rounded-full qt-text-overlay transition-colors disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer {{
                isInUserCharacterGallery() ? 'bg-primary hover:qt-bg-primary/90' : 'qt-bg-overlay-btn hover:qt-bg-overlay-btn'
              }}"
              [disabled]="isSaving() || checkingGallery()"
              [title]="
                isInUserCharacterGallery()
                  ? 'Remove from ' + (userCharacterName() || 'user character') + '\\'s photo album'
                  : 'Save to ' + (userCharacterName() || 'user character') + '\\'s photo album'
              "
              (click)="$event.stopPropagation(); toggleUserCharacterGallery()"
            >
              <qt-icon name="user" class="w-6 h-6" />
            </button>
          }
          <button
            type="button"
            class="p-2 qt-bg-overlay-btn hover:qt-bg-overlay-btn rounded-full qt-text-overlay transition-colors cursor-pointer"
            title="Download"
            (click)="$event.stopPropagation(); handleDownload()"
          >
            <qt-icon name="download" class="w-6 h-6" />
          </button>
          <button
            type="button"
            class="p-2 qt-bg-overlay-btn hover:qt-bg-overlay-btn rounded-full qt-text-overlay transition-colors cursor-pointer"
            title="Copy to clipboard"
            (click)="$event.stopPropagation(); handleCopyToClipboard()"
          >
            <qt-icon name="copy" class="w-6 h-6" />
          </button>
          <button
            type="button"
            class="p-2 qt-bg-overlay-btn hover:qt-bg-overlay-btn rounded-full qt-text-overlay transition-colors cursor-pointer"
            title="Close (Escape)"
            (click)="$event.stopPropagation(); closeModal.emit()"
          >
            <qt-icon name="close" class="w-6 h-6" />
          </button>
        </div>
      }

      <!-- Delete button - bottom right (v4 :316-329) -->
      @if (!imageMissing()) {
        <div class="absolute bottom-4 right-4 z-10">
          <button
            type="button"
            class="p-2 qt-bg-destructive/80 hover:qt-bg-destructive rounded-full qt-text-overlay transition-colors cursor-pointer"
            title="Delete image permanently"
            (click)="$event.stopPropagation(); handleDeleteClick()"
          >
            <qt-icon name="trash" class="w-6 h-6" />
          </button>
        </div>
      }

      <!-- Image container (v4 :332-353) -->
      <div
        class="relative max-w-[90vw] max-h-[90vh] flex items-center justify-center"
        (click)="$event.stopPropagation()"
      >
        @if (imageMissing()) {
          <qt-deleted-image-placeholder
            [imageId]="file().id"
            [filename]="file().filename"
            [width]="600"
            [height]="400"
            (cleanup)="closeModal.emit()"
          />
        } @else {
          <img
            [src]="file().url"
            [alt]="file().filename"
            class="max-w-full max-h-[90vh] w-auto h-auto object-contain"
            (error)="imageMissing.set(true)"
          />
        }
      </div>


      <!-- Filename at bottom (v4 :356-358) -->
      <div
        class="absolute bottom-4 left-1/2 -translate-x-1/2 qt-text-overlay-muted text-sm qt-bg-overlay-caption px-3 py-1 rounded"
      >
        {{ file().filename }}
      </div>
    </div>
  `,
})
export class ChatGalleryImageViewModal {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly file = input.required<ChatGalleryFile>();
  readonly onPrev = input<(() => void) | undefined>(undefined);
  readonly onNext = input<(() => void) | undefined>(undefined);
  readonly characterId = input<string | undefined>(undefined);
  readonly characterName = input<string | undefined>(undefined);
  readonly userCharacterId = input<string | undefined>(undefined);
  readonly userCharacterName = input<string | undefined>(undefined);

  readonly closeModal = output<void>();
  /** v4 `onDelete` — the HOST performs the hard chat-file delete. */
  readonly deleteFile = output<void>();

  protected readonly isInCharacterGallery = signal(false);
  protected readonly isInUserCharacterGallery = signal(false);
  protected readonly characterLinkId = signal<string | null>(null);
  protected readonly userCharacterLinkId = signal<string | null>(null);
  protected readonly isSaving = signal(false);
  protected readonly checkingGallery = signal(true);
  protected readonly imageMissing = signal(false);

  constructor() {
    // v4 `:58-90` — the already-saved check over `imageInfoGet`, re-run when
    // the file (prev/next keeps this mounted) or either character changes.
    effect(() => {
      const fileId = this.file().id;
      const characterId = this.characterId();
      const userCharacterId = this.userCharacterId();
      untracked(() => void this.checkGalleryStatus(fileId, characterId, userCharacterId));
    });

    // v4 `:93-98` — keyboard navigation, re-applied on callback change.
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

  private async checkGalleryStatus(
    fileId: string,
    characterId: string | undefined,
    userCharacterId: string | undefined,
  ): Promise<void> {
    if (!fileId) return;
    this.checkingGallery.set(true);
    try {
      const links = await fetchCharacterGalleryLinks(this.core, fileId);
      if (links === null) return; // v4: a !ok response just stops checking
      if (characterId) {
        const match = links.find((l) => l.characterId === characterId);
        this.isInCharacterGallery.set(!!match);
        this.characterLinkId.set(match?.linkId ?? null);
      }
      if (userCharacterId) {
        const match = links.find((l) => l.characterId === userCharacterId);
        this.isInUserCharacterGallery.set(!!match);
        this.userCharacterLinkId.set(match?.linkId ?? null);
      }
    } finally {
      this.checkingGallery.set(false);
    }
  }

  /** v4 `:120-157`. */
  protected async toggleCharacterGallery(): Promise<void> {
    const characterId = this.characterId();
    if (!characterId || this.isSaving()) return;
    await this.toggle(
      characterId,
      this.characterName() || 'character',
      this.isInCharacterGallery,
      this.characterLinkId,
    );
  }

  /** v4 `:159-196`. */
  protected async toggleUserCharacterGallery(): Promise<void> {
    const userCharacterId = this.userCharacterId();
    if (!userCharacterId || this.isSaving()) return;
    await this.toggle(
      userCharacterId,
      this.userCharacterName() || 'user character',
      this.isInUserCharacterGallery,
      this.userCharacterLinkId,
    );
  }

  private async toggle(
    characterId: string,
    label: string,
    inGallery: typeof this.isInCharacterGallery,
    linkId: typeof this.characterLinkId,
  ): Promise<void> {
    this.isSaving.set(true);
    try {
      const currentLinkId = linkId();
      if (inGallery() && currentLinkId) {
        const resp = await this.core.dispatch({
          type: 'characterPhotoRemove',
          characterId,
          linkId: currentLinkId,
        });
        if (resp.type === 'error') {
          throw new Error(resp.data.message || 'Failed to remove from photo album');
        }
        inGallery.set(false);
        linkId.set(null);
        this.toasts.showSuccess(`Removed from ${label}'s photo album`);
      } else {
        const resp = await this.core.dispatch({
          type: 'characterPhotoSaveById',
          characterId,
          fileId: this.file().id,
        });
        if (resp.type === 'error') {
          throw new Error(resp.data.message || 'Failed to save to photo album');
        }
        const data = resp.data as { linkId?: string };
        inGallery.set(true);
        linkId.set(data.linkId ?? null);
        this.toasts.showSuccess(`Saved to ${label}'s photo album`);
      }
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to update photo album');
    } finally {
      this.isSaving.set(false);
    }
  }

  /** v4 `:110-118`. */
  protected async handleDownload(): Promise<void> {
    try {
      const response = await fetch(this.file().url);
      const blob = await response.blob();
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = this.file().filename;
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(url);
    } catch {
      // v4 logs and stays silent.
    }
  }

  /** v4 `:100-108`. */
  protected async handleCopyToClipboard(): Promise<void> {
    try {
      const response = await fetch(this.file().url);
      const blob = await response.blob();
      await navigator.clipboard.write([new ClipboardItem({ [blob.type]: blob })]);
      this.toasts.showSuccess('Image copied to clipboard');
    } catch {
      this.toasts.showError('Failed to copy image to clipboard');
    }
  }

  /** v4 `:198-202` — confirmation, then the HOST hard-deletes. */
  protected handleDeleteClick(): void {
    if (window.confirm('Permanently delete this photo? This cannot be undone.')) {
      this.deleteFile.emit();
    }
  }

}
