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
} from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../core/core-client';
import type { ChatFileDto, CharacterPhoto } from '../core/core-contract';
import { apiUrl } from '../core/api-url';
import { Icon } from '../ui/icon';
import { ChatGalleryImageViewModal, type ChatGalleryFile } from './chat-gallery-image-view-modal';
import { DeletedImagePlaceholder } from './deleted-image-placeholder';
import { ImageDetailModal } from './image-detail-modal';
import { applyImageNavigation } from './image-navigation';
import type { ImageData } from './images.api';
import { fileUrl, thumbnailUrl } from './image-urls';

/** v4 `PhotoGalleryModal` THUMBNAIL_SIZES / DEFAULT_THUMBNAIL_INDEX (120px). */
const THUMBNAIL_SIZES = [80, 100, 120, 150, 180, 200];
const DEFAULT_THUMBNAIL_INDEX = 2;

/**
 * v4's gallery modes (`PhotoGalleryModal.tsx:40-67`, a discriminated union).
 * Only `'chat'` has a live host (`ChatModals.tsx` always passes it — the
 * round's scope correction 2); `'character'` / `'user-character'` are ported
 * faithfully so the type is honest, with NO host invented for them.
 */
export type PhotoGalleryMode = 'chat' | 'character' | 'user-character';

/** v4 `GalleryItem` (`PhotoGalleryModal.tsx:69-71`) — the routing split's key. */
type GalleryItem =
  | { kind: 'chat'; data: ChatFileDto }
  | { kind: 'image'; data: ImageData };

/**
 * The photo-gallery modal — the full port of v4
 * `components/images/PhotoGalleryModal.tsx` (364 lines): the z-50 grid
 * overlay whose thumbnail click opens a z-[60] detail modal, routed by item
 * kind (`:338-351`): `kind: 'chat'` → `ChatGalleryImageViewModal` (hard
 * delete + album toggles), `kind: 'image'` → `ImageDetailModal`.
 *
 * The two subtlest invariants, both pinned by specs:
 * - **Nested-Escape suppression** (`:170-174`): the gallery handles Escape
 *   ONLY while no detail modal is open (`handleEscape: selectedIndex === -1`)
 *   — without it one press closes both layers.
 * - **The conditional-`undefined` arrow ends** (`:343-344`/`:358-359`): the
 *   prev/next callbacks are handed down as `undefined` at the ends of the
 *   list, which is how the arrows disappear — no separate disabled state.
 */
@Component({
  selector: 'qt-photo-gallery-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, ChatGalleryImageViewModal, DeletedImagePlaceholder, ImageDetailModal],
  template: `
    <!-- v4 :299 — the gallery overlay is z-50; detail modals sit at z-[60]. -->
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

        <div class="flex-1 overflow-y-auto p-4">
          @if (itemsQuery.isPending()) {
            <div class="flex items-center justify-center py-12">
              <p class="qt-text-secondary">Loading images...</p>
            </div>
          } @else if (items().length === 0) {
            <div class="flex items-center justify-center py-12">
              <p class="qt-text-secondary">{{ emptyStateText() }}</p>
            </div>
          } @else {
            <div
              class="flex flex-wrap gap-2 justify-center"
              [style.max-width.px]="containerWidth()"
            >
              @for (item of items(); track item.data.id; let index = $index) {
                <!-- v4 :258-265 — a div container for missing images (the
                     placeholder holds a button), a button for valid ones. -->
                @if (missingImages().has(item.data.id)) {
                  <div
                    class="relative rounded overflow-hidden hover:ring-2 hover:ring-ring focus:ring-2 focus:ring-ring focus:outline-none transition-all"
                    [style.width.px]="thumbnailSize()"
                    [style.height.px]="thumbnailSize()"
                  >
                    <qt-deleted-image-placeholder
                      [imageId]="item.data.id"
                      [filename]="item.data.filename"
                      styleClass="w-full h-full absolute inset-0 !p-2"
                      (cleanup)="reload()"
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
                      [src]="thumbSrc(item)"
                      [alt]="item.data.filename"
                      class="w-full h-full object-cover"
                      (error)="markMissing(item.data.id)"
                    />
                  </button>
                }
              }
            </div>
          }
        </div>
      </div>
    </div>

    <!-- The routing split (v4 :338-361): chat items → the chat viewer,
         image items → the deep detail modal. -->
    @if (selectedChatItem(); as item) {
      <qt-chat-gallery-image-view-modal
        [file]="chatFileFor(item)"
        [onPrev]="selectedIndex() > 0 ? boundPrev : undefined"
        [onNext]="selectedIndex() < items().length - 1 ? boundNext : undefined"
        [characterId]="mode() === 'chat' ? characterId() : undefined"
        [characterName]="mode() === 'chat' ? characterName() : undefined"
        [userCharacterId]="mode() === 'chat' ? userCharacterId() : undefined"
        [userCharacterName]="mode() === 'chat' ? userCharacterName() : undefined"
        (closeModal)="selectedIndex.set(-1)"
        (deleteFile)="handleDeleteChatFile(item.data.id)"
      />
    }
    @if (selectedImageItem(); as item) {
      <qt-image-detail-modal
        [image]="item.data"
        [onPrev]="selectedIndex() > 0 ? boundPrev : undefined"
        [onNext]="selectedIndex() < items().length - 1 ? boundNext : undefined"
        (closeModal)="selectedIndex.set(-1)"
      />
    }
  `,
})
export class PhotoGalleryModal {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();

  /** v4's union discriminant; the live host always passes (or defaults to) `'chat'`. */
  readonly mode = input<PhotoGalleryMode>('chat');
  readonly chatId = input<string | undefined>(undefined);
  readonly characterId = input<string | undefined>(undefined);
  readonly characterName = input<string | undefined>(undefined);
  readonly userCharacterId = input<string | undefined>(undefined);
  readonly userCharacterName = input<string | undefined>(undefined);

  readonly close = output<void>();
  /** Chat mode only in v4 (`ChatModals.tsx:169-184`) — the host reacts to a hard delete. */
  readonly imageDeleted = output<string>();

  protected readonly maxSizeIndex = THUMBNAIL_SIZES.length - 1;
  protected readonly sizeIndex = signal(DEFAULT_THUMBNAIL_INDEX);
  protected readonly thumbnailSize = computed(() => THUMBNAIL_SIZES[this.sizeIndex()]);
  /** v4 `selectedIndex` (`:80`) — `-1` means no detail modal is open. */
  protected readonly selectedIndex = signal(-1);
  /** v4 `missingImages` (`:81`) — per-item broken-thumbnail state. */
  protected readonly missingImages = signal<ReadonlySet<string>>(new Set());

  /** v4 `:91-96`. */
  protected readonly title = computed(() =>
    this.mode() === 'chat'
      ? 'Chat Photos'
      : this.mode() === 'character'
        ? `${this.characterName()}'s Photos`
        : `${this.userCharacterName()}'s Photos`,
  );
  /** v4 `:98-103` (both character arms share the string). */
  protected readonly emptyStateText = computed(() =>
    this.mode() === 'chat' ? 'No photos in this chat' : "No photos in this character's album",
  );

  /** v4 `loadItems` (`:105-151`) — chat files or the character photo roll. */
  protected readonly itemsQuery = injectQuery(() => ({
    queryKey: [
      'photoGalleryItems',
      this.mode(),
      this.chatId() ?? this.characterId() ?? this.userCharacterId() ?? '',
    ],
    queryFn: async (): Promise<GalleryItem[]> => {
      if (this.mode() === 'chat') {
        const chatId = this.chatId();
        if (!chatId) return [];
        const data = await this.core.dispatchData({ type: 'chatFilesList', chatId });
        const files = (data['files'] as ChatFileDto[]) ?? [];
        return files
          .filter((f) => f.mimeType.startsWith('image/'))
          .map((data) => ({ kind: 'chat' as const, data }));
      }
      const targetId = this.mode() === 'character' ? this.characterId() : this.userCharacterId();
      if (!targetId) return [];
      // v4 `:122` — `?limit=200`.
      const data = await this.core.dispatchData({
        type: 'characterPhotoList',
        characterId: targetId,
        limit: 200,
      });
      const entries = (data['entries'] as CharacterPhoto[]) ?? [];
      // v4 `:130-143` — the entry → GalleryImage mapping; `tags: []` is
      // hardcoded (image tag editing has no UI in v4 — scope correction 1).
      return entries.map((entry) => ({
        kind: 'image' as const,
        data: {
          id: entry.linkId,
          linkId: entry.linkId,
          filename: entry.fileName,
          filepath: entry.blobUrl,
          url: apiUrl(entry.blobUrl),
          mimeType: entry.mimeType || 'image/webp',
          size: entry.fileSizeBytes || 0,
          createdAt: entry.keptAt,
          tags: [],
        } satisfies ImageData,
      }));
    },
  }));

  protected readonly items = computed(() => this.itemsQuery.data() ?? []);
  protected readonly selectedItem = computed<GalleryItem | null>(() => {
    const index = this.selectedIndex();
    return index >= 0 ? (this.items()[index] ?? null) : null;
  });
  protected readonly selectedChatItem = computed(() => {
    const item = this.selectedItem();
    return item?.kind === 'chat' ? item : null;
  });
  protected readonly selectedImageItem = computed(() => {
    const item = this.selectedItem();
    return item?.kind === 'image' ? item : null;
  });

  /** v4 `:225-227` — the grid width caps at ~800px of tiles. */
  protected readonly containerWidth = computed(() => {
    const size = this.thumbnailSize();
    const maxColumns = Math.floor(800 / (size + 8)) || 1;
    const visibleColumns = Math.min(this.items().length || 1, maxColumns);
    return visibleColumns * (size + 8);
  });

  /** v4 `:188-194` — the host owns the index arithmetic, clamped at the ends. */
  protected readonly boundPrev = (): void => {
    this.selectedIndex.update((prev) => (prev > 0 ? prev - 1 : prev));
  };
  protected readonly boundNext = (): void => {
    this.selectedIndex.update((prev) =>
      prev < this.items().length - 1 ? prev + 1 : prev,
    );
  };

  constructor() {
    // v4 `:170-174` — the gallery's own Escape handler is SUPPRESSED while a
    // detail modal is open (`handleEscape: selectedIndex === -1`); the detail
    // modal's own navigation handles that press. Body scroll locks while
    // mounted (the host mounts this component only while open).
    let dispose: (() => void) | null = null;
    effect(() => {
      const suppressed = this.selectedIndex() !== -1;
      dispose?.();
      dispose = applyImageNavigation({
        isOpen: true,
        onClose: () => this.close.emit(),
        handleEscape: !suppressed,
      });
    });
    inject(DestroyRef).onDestroy(() => dispose?.());
  }

  protected handleZoomIn(): void {
    if (this.sizeIndex() < this.maxSizeIndex) {
      this.sizeIndex.update((prev) => prev + 1);
    }
  }
  protected handleZoomOut(): void {
    if (this.sizeIndex() > 0) {
      this.sizeIndex.update((prev) => prev - 1);
    }
  }

  protected thumbSrc(item: GalleryItem): string {
    // Vault photos come with their blob URL (v4 `:250`, `item.data.url ||
    // item.data.filepath`) — and so do `mountFile` entries, which the server
    // contributes from the Librarian announcement walk rather than the `files`
    // table. Their `id` is a `doc_mount_file_links` id, so the id-keyed
    // thumbnail route 404s on them (dogfood #48). Only genuine chat files —
    // uploads and generated images, both real `files` rows — take the v5
    // thumbnail path, which is itself a v5 divergence (v4 served thumbnails
    // straight off `filepath`; see `image-urls.ts`).
    if (item.kind !== 'chat' || item.data.type === 'mountFile') {
      return item.data.url ?? item.data.filepath;
    }
    return thumbnailUrl(item.data.id, this.thumbnailSize());
  }

  protected chatFileFor(item: GalleryItem & { kind: 'chat' }): ChatGalleryFile {
    return {
      id: item.data.id,
      filename: item.data.filename,
      // Same split as `thumbSrc`: a `mountFile`'s id does not address the files
      // route, so the full-size view must use the server's own blob URL or the
      // click straight after the thumbnail 404s in turn (dogfood #48).
      url:
        item.data.type === 'mountFile'
          ? (item.data.url ?? item.data.filepath)
          : fileUrl(item.data.id),
    };
  }

  protected markMissing(id: string): void {
    this.missingImages.update((prev) => new Set(prev).add(id));
  }

  /** v4 `:157-158` / the placeholder's `onCleanup: loadItems` (`:278`). */
  protected reload(): void {
    void this.itemsQuery.refetch();
  }

  /** v4 `handleDeleteChatFile` (`:196-217`) — the hard chat-file delete. */
  protected async handleDeleteChatFile(fileId: string): Promise<void> {
    try {
      const resp = await this.core.dispatch({ type: 'chatFileDelete', fileId });
      if (resp.type === 'error') {
        throw new Error(resp.data.message || 'Failed to delete image');
      }
      this.selectedIndex.set(-1);
      await this.itemsQuery.refetch();
      const chatId = this.chatId();
      if (chatId) {
        await this.queryClient.invalidateQueries({ queryKey: ['chatFilesList', chatId] });
      }
      if (this.mode() === 'chat') {
        this.imageDeleted.emit(fileId);
      }
    } catch {
      // v4 toasts the failure; the grid keeps its item.
    }
  }
}
