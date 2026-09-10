import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  afterNextRender,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import { injectChatGallery } from '../chat/chat-gallery.api';
import { CoreClient } from '../core/core-client';
import { apiUrl } from '../core/api-url';
import type { CharacterPhoto, ChatGalleryEntry, ChatGallerySource } from '../core/core-contract';
import { downloadGalleryEntry } from '../core/download-utils';
import { Icon } from '../ui/icon';
import { ToastService } from '../ui/toast.service';
import { ChatGalleryImageViewModal } from './chat-gallery-image-view-modal';
import { DeletedImagePlaceholder } from './deleted-image-placeholder';
import { ImageDetailModal } from './image-detail-modal';
import { applyImageNavigation } from './image-navigation';
import type { ImageData } from './images.api';
import { SaveImageDialog } from './save-image-dialog';

/** v4 `THUMBNAIL_SIZES` / `DEFAULT_THUMBNAIL_INDEX` (120px). */
const THUMBNAIL_SIZES = [80, 100, 120, 150, 180, 200];
const DEFAULT_THUMBNAIL_INDEX = 2;

/** Chip labels, in the order the chips are shown (v4 `SOURCE_LABELS`). */
const SOURCE_LABELS: Record<ChatGallerySource, string> = {
  'story-background': 'Backgrounds',
  avatar: 'Avatars',
  portrait: 'Portraits',
  generated: 'Generated',
  attachment: 'Attached',
  kept: 'Kept',
  inline: 'Inline',
};

/** v4 `SOURCE_ORDER` = `CHAT_GALLERY_SOURCES`'s chip order. */
const SOURCE_ORDER: readonly ChatGallerySource[] = [
  'story-background',
  'avatar',
  'portrait',
  'generated',
  'attachment',
  'kept',
  'inline',
];

/** `'all'` is the resting state; a chip narrows to one source (v4 `SourceFilter`). */
type SourceFilter = ChatGallerySource | 'all';

/**
 * v4's gallery modes (`PhotoGalleryModal.tsx:58-78`, a discriminated union).
 * `'chat'` reads the {@link injectChatGallery} query (P4.D176); `'character'`
 * / `'user-character'` keep their pre-existing `characterPhotoList` read —
 * Tier 3 of the round order: "port the strings and the mode switch only if
 * v5's modal already carries a character mode" — it does, so it stays.
 */
export type PhotoGalleryMode = 'chat' | 'character' | 'user-character';

/** One album-mode photo (character / user-character), pre-P4.D176 shape. */
interface AlbumImage {
  id: string;
  linkId?: string;
  filename: string;
  filepath: string;
  url?: string;
  mimeType: string;
  size: number;
  createdAt: string;
}

/**
 * The photo-gallery modal — a full rewrite of v4
 * `components/images/PhotoGalleryModal.tsx` (579 lines at `78b381a96`,
 * P4.D176) over the `chatGallery` query (§C.3): v4's filter chips (rendered
 * only with ≥2 non-zero sources), the `current` badge, the Save / Download /
 * Delete hover actions (Delete double-guarded on `deletable && idKind ===
 * 'file'`), and the detail-view routing (chat → {@link
 * ChatGalleryImageViewModal}, character/user-character → {@link
 * ImageDetailModal}).
 *
 * Portaled to `document.body` (v4 `:575-578`, bug-99's `.qt-workspace`
 * stacking-context rule — the `afterNextRender` body-reparent idiom
 * `image-detail-modal.ts` established; a constructor-time reparent is
 * silently undone under `@if`).
 */
@Component({
  selector: 'qt-photo-gallery-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, ChatGalleryImageViewModal, DeletedImagePlaceholder, ImageDetailModal, SaveImageDialog],
  template: `
    <!-- v4 :480 — the gallery overlay is z-50; detail modals sit at z-[60]. -->
    <div
      class="fixed inset-0 z-50 flex items-center justify-center qt-bg-overlay backdrop-blur-sm p-4"
      (click)="close.emit()"
    >
      <div
        class="qt-dialog flex flex-col max-h-[90vh] max-w-[90vw]"
        style="min-width: 300px"
        (click)="$event.stopPropagation()"
      >
        <div class="qt-dialog-header border-b qt-border-default">
          <h2 class="qt-heading-4 text-foreground">{{ title() }}</h2>
          <div class="flex items-center gap-2">
            <button
              type="button"
              class="p-2 qt-text-secondary hover:text-foreground disabled:opacity-30 disabled:cursor-not-allowed"
              title="Smaller thumbnails"
              [disabled]="sizeIndex() === 0"
              (click)="handleZoomOut()"
            >
              <qt-icon name="zoom-out" class="w-5 h-5" />
            </button>
            <button
              type="button"
              class="p-2 qt-text-secondary hover:text-foreground disabled:opacity-30 disabled:cursor-not-allowed"
              title="Larger thumbnails"
              [disabled]="sizeIndex() === maxSizeIndex"
              (click)="handleZoomIn()"
            >
              <qt-icon name="zoom-in" class="w-5 h-5" />
            </button>
            <button
              type="button"
              class="p-2 qt-text-secondary hover:text-foreground"
              title="Close"
              (click)="close.emit()"
            >
              <qt-icon name="close" class="w-5 h-5" />
            </button>
          </div>
        </div>

        <!-- v4 :291-324 — the filter chips: "one kind of picture is not a
             filter — it is the whole gallery" (< 2 non-zero sources → none). -->
        @if (mode() === 'chat' && chips().length >= 2) {
          <div class="qt-tab-group px-4 pt-3" role="group" aria-label="Filter by where the picture came from">
            <button
              type="button"
              [class]="'qt-tab' + (sourceFilter() === 'all' ? ' qt-tab-active' : '')"
              [attr.aria-pressed]="sourceFilter() === 'all'"
              (click)="sourceFilter.set('all')"
            >
              All ({{ galleryTotal() }})
            </button>
            @for (source of chips(); track source) {
              <button
                type="button"
                [class]="'qt-tab' + (sourceFilter() === source ? ' qt-tab-active' : '')"
                [attr.aria-pressed]="sourceFilter() === source"
                (click)="sourceFilter.set(source)"
              >
                {{ sourceLabel(source) }} ({{ galleryCounts()[source] }})
              </button>
            }
          </div>
        }

        <div class="flex-1 overflow-y-auto p-4">
          @if (loading()) {
            <div class="flex items-center justify-center py-12">
              <p class="qt-text-secondary">Loading images...</p>
            </div>
          } @else if (itemCount() === 0) {
            <div class="flex items-center justify-center py-12">
              <p class="qt-text-secondary">{{ emptyStateText() }}</p>
            </div>
          } @else {
            <div class="flex flex-wrap gap-2 justify-center" [style.max-width.px]="containerWidth()">
              @if (mode() === 'chat') {
                @for (entry of filteredEntries(); track entry.id; let index = $index) {
                  @if (missingImages().has(entry.id)) {
                    <div
                      class="relative rounded overflow-hidden"
                      [style.width.px]="thumbnailSize()"
                      [style.height.px]="thumbnailSize()"
                    >
                      <qt-deleted-image-placeholder
                        [imageId]="entry.id"
                        [filename]="entry.filename"
                        (cleanup)="reload()"
                        styleClass="w-full h-full absolute inset-0 !p-2"
                      />
                    </div>
                  } @else {
                    <div
                      class="relative group rounded overflow-hidden"
                      [style.width.px]="thumbnailSize()"
                      [style.height.px]="thumbnailSize()"
                    >
                      <button
                        type="button"
                        class="relative w-full h-full overflow-hidden rounded hover:ring-2 hover:ring-ring focus:ring-2 focus:ring-ring focus:outline-none transition-all"
                        [title]="entry.filename"
                        (click)="selectedIndex.set(index)"
                      >
                        <img
                          [src]="entry.url"
                          [alt]="entry.filename"
                          class="w-full h-full object-cover"
                          (error)="markMissing(entry.id)"
                        />
                        @if (entry.isCurrent) {
                          <span
                            class="absolute top-1 left-1 qt-bg-success qt-text-on-success text-xs px-1.5 py-0.5 rounded font-medium"
                          >
                            current
                          </span>
                        }
                      </button>

                      <div class="absolute bottom-1 right-1 flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                        <button
                          type="button"
                          class="p-1.5 rounded-full qt-shadow-md qt-bg-card qt-text-secondary hover:qt-bg-primary hover:qt-text-on-primary transition-colors"
                          title="Save to a photo album"
                          aria-label="Save to a photo album"
                          (click)="$event.stopPropagation(); saveTargetId.set(entry.id)"
                        >
                          <qt-icon name="bookmark" class="w-4 h-4" />
                        </button>
                        <button
                          type="button"
                          class="p-1.5 rounded-full qt-shadow-md qt-bg-card qt-text-secondary hover:qt-bg-primary hover:qt-text-on-primary transition-colors"
                          title="Download image"
                          aria-label="Download image"
                          (click)="$event.stopPropagation(); handleDownload(entry)"
                        >
                          <qt-icon name="download" class="w-4 h-4" />
                        </button>
                        @if (entry.deletable) {
                          <button
                            type="button"
                            class="p-1.5 rounded-full qt-shadow-md qt-bg-card qt-text-secondary hover:qt-bg-destructive hover:qt-text-on-destructive transition-colors"
                            title="Delete image"
                            aria-label="Delete image"
                            (click)="$event.stopPropagation(); handleDeleteEntry(entry)"
                          >
                            <qt-icon name="trash" class="w-4 h-4" />
                          </button>
                        }
                      </div>
                    </div>
                  }
                }
              } @else {
                @for (image of albumImages(); track image.id; let index = $index) {
                  @if (missingImages().has(image.id)) {
                    <div
                      class="relative rounded overflow-hidden hover:ring-2 hover:ring-ring focus:ring-2 focus:ring-ring focus:outline-none transition-all"
                      [style.width.px]="thumbnailSize()"
                      [style.height.px]="thumbnailSize()"
                    >
                      <qt-deleted-image-placeholder
                        [imageId]="image.id"
                        [filename]="image.filename"
                        (cleanup)="loadAlbum()"
                        styleClass="w-full h-full absolute inset-0 !p-2"
                      />
                    </div>
                  } @else {
                    <button
                      type="button"
                      class="relative rounded overflow-hidden hover:ring-2 hover:ring-ring focus:ring-2 focus:ring-ring focus:outline-none transition-all"
                      [style.width.px]="thumbnailSize()"
                      [style.height.px]="thumbnailSize()"
                      (click)="selectedIndex.set(index)"
                    >
                      <img
                        [src]="albumImageSrc(image)"
                        [alt]="image.filename"
                        class="w-full h-full object-cover"
                        (error)="markMissing(image.id)"
                      />
                    </button>
                  }
                }
              }
            </div>
          }
        </div>
      </div>
    </div>

    <!-- The routing split (v4 :521-550): chat items → the chat viewer,
         character/user-character items → the deep detail modal. -->
    @if (selectedChatEntry(); as entry) {
      <qt-chat-gallery-image-view-modal
        [entry]="entry"
        [onPrev]="selectedIndex() > 0 ? boundPrev : undefined"
        [onNext]="selectedIndex() < itemCount() - 1 ? boundNext : undefined"
        (closeModal)="selectedIndex.set(-1)"
        (deleteFile)="handleDeleteEntry(entry)"
        (save)="saveTargetId.set(entry.id)"
        (jumpToMessage)="handleJumpToMessage($event)"
      />
    }
    @if (selectedAlbumImage(); as image) {
      <qt-image-detail-modal
        [image]="albumImageAsImageData(image)"
        [onPrev]="selectedIndex() > 0 ? boundPrev : undefined"
        [onNext]="selectedIndex() < itemCount() - 1 ? boundNext : undefined"
        (closeModal)="selectedIndex.set(-1)"
      />
    }

    @if (mode() === 'chat' && saveTarget(); as target) {
      <qt-save-image-dialog
        [chatId]="chatId()!"
        [target]="{ kind: 'chat', fileId: target.id }"
        [attachments]="[
          { id: target.id, filename: target.filename, filepath: target.url, mimeType: target.mimeType },
        ]"
        (close)="saveTargetId.set(null)"
        (saved)="handleSaved()"
      />
    }
  `,
})
export class PhotoGalleryModal {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly mode = input<PhotoGalleryMode>('chat');
  readonly chatId = input<string | undefined>(undefined);
  readonly characterId = input<string | undefined>(undefined);
  readonly characterName = input<string | undefined>(undefined);
  readonly userCharacterId = input<string | undefined>(undefined);
  readonly userCharacterName = input<string | undefined>(undefined);

  readonly close = output<void>();
  /** Chat mode only (v4 `ChatModals.tsx` — the host reacts to a hard delete). */
  readonly imageDeleted = output<string>();
  /** Scroll the transcript to a message (v4 `onJumpToMessage`, chat mode only). */
  readonly jumpToMessage = output<string>();

  protected readonly maxSizeIndex = THUMBNAIL_SIZES.length - 1;
  protected readonly sizeIndex = signal(DEFAULT_THUMBNAIL_INDEX);
  protected readonly thumbnailSize = computed(() => THUMBNAIL_SIZES[this.sizeIndex()]);
  /** v4 `selectedIndex` — `-1` means no detail modal is open. */
  protected readonly selectedIndex = signal(-1);
  protected readonly missingImages = signal<ReadonlySet<string>>(new Set());
  protected readonly sourceFilter = signal<SourceFilter>('all');
  protected readonly saveTargetId = signal<string | null>(null);

  // --- chat mode: the shared chatGallery query (P4.D176, §C.3) ---
  private readonly gallery = injectChatGallery(
    () => this.chatId(),
    { enabled: () => this.mode() === 'chat' },
  );
  protected readonly galleryTotal = this.gallery.total;
  protected readonly galleryCounts = this.gallery.counts;

  protected readonly filteredEntries = computed<ChatGalleryEntry[]>(() => {
    const filter = this.sourceFilter();
    const entries = this.gallery.entries();
    return filter === 'all' ? entries : entries.filter((e) => e.source === filter);
  });

  /** v4 `:293-295` — chip order, filtered to non-zero counts. */
  protected readonly chips = computed<ChatGallerySource[]>(() =>
    SOURCE_ORDER.filter((source) => (this.galleryCounts()[source] ?? 0) > 0),
  );

  // --- character / user-character mode: the pre-existing photo-roll read ---
  protected readonly albumImages = signal<AlbumImage[]>([]);
  protected readonly albumLoading = signal(true);

  protected readonly itemCount = computed(() =>
    this.mode() === 'chat' ? this.filteredEntries().length : this.albumImages().length,
  );
  protected readonly loading = computed(() =>
    this.mode() === 'chat' ? this.gallery.isLoading() : this.albumLoading(),
  );

  protected readonly selectedChatEntry = computed<ChatGalleryEntry | null>(() => {
    if (this.mode() !== 'chat') return null;
    const index = this.selectedIndex();
    return index >= 0 ? (this.filteredEntries()[index] ?? null) : null;
  });
  protected readonly selectedAlbumImage = computed<AlbumImage | null>(() => {
    if (this.mode() === 'chat') return null;
    const index = this.selectedIndex();
    return index >= 0 ? (this.albumImages()[index] ?? null) : null;
  });
  protected readonly saveTarget = computed<ChatGalleryEntry | null>(() => {
    const id = this.saveTargetId();
    if (!id) return null;
    return this.gallery.entries().find((e) => e.id === id) ?? null;
  });

  /** v4 `:151-156`. */
  protected readonly title = computed(() =>
    this.mode() === 'chat'
      ? 'Chat Photos'
      : this.mode() === 'character'
        ? `${this.characterName()}'s Photos`
        : `${this.userCharacterName()}'s Photos`,
  );
  /** v4 `:158-163`. */
  protected readonly emptyStateText = computed(() => {
    if (this.mode() !== 'chat') return "No photos in this character's album";
    const filter = this.sourceFilter();
    return filter === 'all' ? 'No photos in this chat' : `No ${this.sourceLabel(filter).toLowerCase()} in this chat`;
  });

  /** v4 `:286-288` — the grid width caps at ~800px of tiles. */
  protected readonly containerWidth = computed(() => {
    const size = this.thumbnailSize();
    const maxColumns = Math.floor(800 / (size + 8)) || 1;
    const visibleColumns = Math.min(this.itemCount() || 1, maxColumns);
    return visibleColumns * (size + 8);
  });

  /** v4 `:236-242` — the host owns the index arithmetic, clamped at the ends. */
  protected readonly boundPrev = (): void => {
    this.selectedIndex.update((prev) => (prev > 0 ? prev - 1 : prev));
  };
  protected readonly boundNext = (): void => {
    this.selectedIndex.update((prev) => (prev < this.itemCount() - 1 ? prev + 1 : prev));
  };

  constructor() {
    // v4 `:200-205` — load the album when the mode is character/user-character.
    effect(() => {
      const mode = this.mode();
      const characterId = this.characterId();
      const userCharacterId = this.userCharacterId();
      if (mode === 'chat') return;
      void this.loadAlbumFor(mode, characterId, userCharacterId);
    });

    // v4 `:217-222` — the gallery's own Escape handler is SUPPRESSED while a
    // detail modal is open (`handleEscape: selectedIndex === -1`); the detail
    // modal's own navigation handles that press.
    let dispose: (() => void) | null = null;
    effect(() => {
      const suppressed = this.selectedIndex() !== -1 || this.saveTargetId() !== null;
      dispose?.();
      dispose = applyImageNavigation({
        isOpen: true,
        onClose: () => this.close.emit(),
        handleEscape: !suppressed,
      });
    });
    inject(DestroyRef).onDestroy(() => dispose?.());

    // v4 `:575-578` — the portal to document.body (bug 99's `.qt-workspace`
    // stacking-context rule). Must run AFTER the first render: every host of
    // this modal mounts it under an `@if`, and an embedded view's root nodes
    // attach to the container only after the view is created.
    const host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;
    inject(DestroyRef).onDestroy(() => host.remove());
    afterNextRender(() => {
      if (typeof document !== 'undefined') {
        document.body.appendChild(host);
      }
    });
  }

  protected handleZoomIn(): void {
    if (this.sizeIndex() < this.maxSizeIndex) this.sizeIndex.update((prev) => prev + 1);
  }
  protected handleZoomOut(): void {
    if (this.sizeIndex() > 0) this.sizeIndex.update((prev) => prev - 1);
  }

  protected sourceLabel(source: ChatGallerySource): string {
    return SOURCE_LABELS[source];
  }

  protected markMissing(id: string): void {
    this.missingImages.update((prev) => new Set(prev).add(id));
  }

  /** v4 `:157-158` / `loadItems`'s cleanup callback. */
  protected reload(): void {
    this.gallery.invalidate();
  }

  protected handleDownload(entry: ChatGalleryEntry): void {
    try {
      downloadGalleryEntry(entry);
    } catch {
      this.toasts.showError('Failed to download image');
    }
  }

  /** v4 `handleDeleteEntry` (`:253-277`) — double-guarded before the delete request. */
  protected async handleDeleteEntry(entry: ChatGalleryEntry): Promise<void> {
    if (!entry.deletable || entry.idKind !== 'file') return;
    if (!window.confirm('Permanently delete this photo? This cannot be undone.')) return;
    try {
      const resp = await this.core.dispatch({ type: 'chatFileDelete', fileId: entry.id });
      if (resp.type === 'error') throw new Error(resp.data.message || 'Failed to delete image');
      this.toasts.showSuccess('Image deleted');
      this.selectedIndex.set(-1);
      this.gallery.invalidate();
      this.imageDeleted.emit(entry.id);
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to delete image');
    }
  }

  protected handleSaved(): void {
    this.saveTargetId.set(null);
    this.gallery.invalidate();
  }

  /** v4 `ChatModals.tsx:168-172` — close BOTH modals, then let the tick pass. */
  protected handleJumpToMessage(messageId: string): void {
    this.selectedIndex.set(-1);
    this.jumpToMessage.emit(messageId);
    this.close.emit();
  }

  // --- character / user-character album (pre-P4.D176 read; unchanged) ---

  protected async loadAlbum(): Promise<void> {
    await this.loadAlbumFor(this.mode(), this.characterId(), this.userCharacterId());
  }

  private async loadAlbumFor(
    mode: PhotoGalleryMode,
    characterId: string | undefined,
    userCharacterId: string | undefined,
  ): Promise<void> {
    if (mode === 'chat') return;
    const targetId = mode === 'character' ? characterId : userCharacterId;
    if (!targetId) return;
    try {
      this.albumLoading.set(true);
      const data = await this.core.dispatchData({
        type: 'characterPhotoList',
        characterId: targetId,
        limit: 200,
      });
      const entries = (data['entries'] as CharacterPhoto[]) ?? [];
      this.albumImages.set(
        entries.map((entry) => ({
          id: entry.linkId,
          linkId: entry.linkId,
          filename: entry.fileName,
          filepath: entry.blobUrl,
          // Resolved once, here — v4 `:250` reads `item.data.url || item.data.filepath`
          // directly as the image `src`, and v5's `apiUrl` is the D14 raw-route rule.
          url: apiUrl(entry.blobUrl),
          mimeType: entry.mimeType || 'image/webp',
          size: entry.fileSizeBytes || 0,
          createdAt: entry.keptAt,
        })),
      );
    } catch (error) {
      this.toasts.showError(
        error instanceof Error ? error.message : 'Failed to load gallery items',
      );
    } finally {
      this.albumLoading.set(false);
    }
  }

  protected albumImageSrc(image: AlbumImage): string {
    const src = image.url || image.filepath;
    return src.startsWith('/') ? src : `/${src}`;
  }

  protected albumImageAsImageData(image: AlbumImage): ImageData {
    return {
      id: image.id,
      linkId: image.linkId,
      filename: image.filename,
      filepath: image.filepath,
      // Already resolved by `loadAlbumFor` — resolving twice would double-
      // prefix the Tauri cross-origin dev loop.
      url: image.url,
      mimeType: image.mimeType,
      size: image.size,
      createdAt: image.createdAt,
      tags: [],
    };
  }
}
