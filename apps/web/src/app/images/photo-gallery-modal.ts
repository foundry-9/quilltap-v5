import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../core/core-client';
import type { ChatFileDto } from '../core/core-contract';
import { EmptyState } from '../ui/empty-state';
import { Icon } from '../ui/icon';
import { LoadingState } from '../ui/loading-state';
import { Modal } from '../ui/modal';
import { ImageModal } from './image-modal';
import { fileUrl, thumbnailUrl } from './image-urls';

/** v4 `PhotoGalleryModal` THUMBNAIL_SIZES / DEFAULT_THUMBNAIL_INDEX (120px). */
const THUMBNAIL_SIZES = [80, 100, 120, 150, 180, 200];
const DEFAULT_THUMBNAIL_INDEX = 2;

/**
 * The in-chat photo gallery (v4 `components/images/PhotoGalleryModal.tsx`, chat
 * mode): a grid of the chat's image files (`chatFilesList`, filtered to image/*)
 * with a thumbnail-size control, opened from the conversation header. Clicking a
 * thumbnail opens the shared ImageModal lightbox (save-to-gallery / download /
 * copy / delete). v4's deep detail modals (ImageDetailModal /
 * ChatGalleryImageViewModal, tag editing, image-navigation) are a named
 * deferral — the lightbox covers view + save + delete.
 */
@Component({
  selector: 'qt-photo-gallery-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, Icon, EmptyState, LoadingState, ImageModal],
  template: `
    <qt-modal title="Chat Photos" maxWidth="3xl" (close)="close.emit()">
      <div class="space-y-4">
        <div class="flex items-center justify-end gap-2">
          <qt-icon name="image" class="w-4 h-4 qt-text-secondary" />
          <input
            type="range"
            min="0"
            [max]="maxSizeIndex"
            step="1"
            [value]="sizeIndex()"
            aria-label="Thumbnail size"
            (input)="sizeIndex.set(+$any($event.target).value)"
          />
        </div>

        @if (filesQuery.isPending()) {
          <qt-loading-state message="Loading photos..." />
        } @else if (images().length === 0) {
          <qt-empty-state
            title="No photos in this chat"
            description="Images shared or generated in this conversation will appear here."
            variant="dashed"
          />
        } @else {
          <div class="flex flex-wrap gap-2">
            @for (file of images(); track file.id) {
              <button
                type="button"
                class="qt-button qt-chat-attachment-button"
                [title]="file.filename"
                [attr.aria-label]="'View ' + file.filename"
                [style.width.px]="thumbnailSize()"
                [style.height.px]="thumbnailSize()"
                (click)="selected.set(file)"
              >
                <img
                  [src]="thumb(file)"
                  [alt]="file.filename"
                  class="w-full h-full object-cover"
                />
              </button>
            }
          </div>
        }
      </div>
    </qt-modal>

    @if (selected(); as file) {
      <qt-image-modal
        [src]="full(file)"
        [filename]="file.filename"
        [fileId]="file.id"
        [characterId]="characterId()"
        [characterName]="characterName()"
        [userCharacterId]="userCharacterId()"
        [userCharacterName]="userCharacterName()"
        (close)="selected.set(null)"
        (deleted)="onDeleted(file)"
      />
    }
  `,
})
export class PhotoGalleryModal {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();

  readonly chatId = input.required<string>();
  readonly characterId = input<string | undefined>(undefined);
  readonly characterName = input<string | undefined>(undefined);
  readonly userCharacterId = input<string | undefined>(undefined);
  readonly userCharacterName = input<string | undefined>(undefined);

  readonly close = output<void>();
  readonly imageDeleted = output<string>();

  protected readonly maxSizeIndex = THUMBNAIL_SIZES.length - 1;
  protected readonly sizeIndex = signal(DEFAULT_THUMBNAIL_INDEX);
  protected readonly thumbnailSize = computed(() => THUMBNAIL_SIZES[this.sizeIndex()]);
  protected readonly selected = signal<ChatFileDto | null>(null);

  protected readonly filesQuery = injectQuery(() => ({
    queryKey: ['chatFilesList', this.chatId()],
    queryFn: async (): Promise<ChatFileDto[]> => {
      const data = await this.core.dispatchData({ type: 'chatFilesList', chatId: this.chatId() });
      return (data['files'] as ChatFileDto[]) ?? [];
    },
  }));

  protected readonly images = computed(() =>
    (this.filesQuery.data() ?? []).filter((f) => f.mimeType.startsWith('image/')),
  );

  protected thumb(file: ChatFileDto): string {
    return thumbnailUrl(file.id, this.thumbnailSize());
  }

  protected full(file: ChatFileDto): string {
    return fileUrl(file.id);
  }

  protected async onDeleted(file: ChatFileDto): Promise<void> {
    this.selected.set(null);
    this.imageDeleted.emit(file.id);
    await this.queryClient.invalidateQueries({ queryKey: ['chatFilesList', this.chatId()] });
  }
}
