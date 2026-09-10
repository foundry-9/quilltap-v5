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

import { downloadGalleryEntry } from '../core/download-utils';
import { copyImageToClipboard } from '../core/clipboard-utils';
import type { ChatGalleryEntry, ChatGallerySource } from '../core/core-contract';
import { Icon } from '../ui/icon';
import { ToastService } from '../ui/toast.service';
import { DeletedImagePlaceholder } from './deleted-image-placeholder';
import { applyImageNavigation } from './image-navigation';

/** v4 `SOURCE_PHRASE` — how each source describes itself in the provenance line. */
const SOURCE_PHRASE: Record<ChatGallerySource, string> = {
  'story-background': 'Story background',
  avatar: 'Avatar, repainted during this conversation',
  portrait: 'Standing portrait',
  generated: 'Generated in this conversation',
  attachment: 'Attached beneath a message',
  kept: 'Brought out of a photo album',
  inline: 'Woven into the prose',
};

/**
 * v4 `formatDay` (`ChatGalleryImageViewModal.tsx:51-57`): `null` for a
 * placeholder date (portraits and inline references can carry one) rather
 * than a nonsense day in the provenance line.
 */
function formatDay(iso: string): string | null {
  const ms = new Date(iso).getTime();
  if (!Number.isFinite(ms) || ms <= 0) return null;
  return new Date(ms).toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
}

/**
 * The chat-gallery detail view — a full rewrite of v4
 * `components/chat/ChatGalleryImageViewModal.tsx` (261 lines at `78b381a96`,
 * P4.D176).
 *
 * v4's module doc, verbatim: it used to carry two hard-wired album buttons
 * that posted to the first character's and the first user-character's photo
 * routes — a shortcut from before `SaveImageDialog` existed. That could reach
 * two albums out of the several a chat can see, had no caption or duplicate
 * notice, and handed a `doc_mount_file_links` id to a route that only
 * understands a `files.id`. Save now opens the SAME dialog the message
 * toolbar's bookmark opens (`onSave`, host-wired to
 * `SaveImageDialog`'s `target: {kind:'chat', fileId}`), and this view keeps
 * only what it is for — looking, copying, downloading, walking the roll, and
 * retiring a picture the chat itself owns.
 *
 * The v5 PREDECESSOR of this file (before P4.D176) carried those two album
 * buttons — a v5-only divergence from an already-superseded v4 shape; this
 * rewrite deletes them along with v4.
 */
@Component({
  selector: 'qt-chat-gallery-image-view-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, DeletedImagePlaceholder],
  template: `
    <!-- v4 :107-112 — z-[60], one layer above the z-50 gallery. -->
    <div
      class="fixed inset-0 z-[60] flex items-center justify-center qt-bg-overlay backdrop-blur-sm"
      role="dialog"
      aria-modal="true"
      (click)="closeModal.emit()"
    >
      <!-- Navigation buttons (v4 :114-138) -->
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

      <!-- Top right control buttons (v4 :140-188, hidden while missing) -->
      @if (!imageMissing()) {
        <div class="absolute top-4 right-4 flex gap-2 z-10">
          <button
            type="button"
            class="p-2 qt-bg-overlay-btn hover:qt-bg-overlay-btn rounded-full qt-text-overlay transition-colors cursor-pointer"
            title="Save to a photo album"
            aria-label="Save to a photo album"
            (click)="$event.stopPropagation(); save.emit()"
          >
            <qt-icon name="bookmark" class="w-6 h-6" />
          </button>
          <button
            type="button"
            class="p-2 qt-bg-overlay-btn hover:qt-bg-overlay-btn rounded-full qt-text-overlay transition-colors cursor-pointer"
            title="Download"
            aria-label="Download"
            (click)="$event.stopPropagation(); handleDownload()"
          >
            <qt-icon name="download" class="w-6 h-6" />
          </button>
          <button
            type="button"
            class="p-2 qt-bg-overlay-btn hover:qt-bg-overlay-btn rounded-full qt-text-overlay transition-colors cursor-pointer"
            title="Copy to clipboard"
            aria-label="Copy to clipboard"
            (click)="$event.stopPropagation(); handleCopyToClipboard()"
          >
            <qt-icon name="copy" class="w-6 h-6" />
          </button>
          <button
            type="button"
            class="p-2 qt-bg-overlay-btn hover:qt-bg-overlay-btn rounded-full qt-text-overlay transition-colors cursor-pointer"
            title="Close (Escape)"
            aria-label="Close"
            (click)="$event.stopPropagation(); closeModal.emit()"
          >
            <qt-icon name="close" class="w-6 h-6" />
          </button>
        </div>
      }

      <!-- Delete — only where the chat itself owns the record (v4 :190-208). -->
      @if (!imageMissing() && entry().deletable) {
        <div class="absolute bottom-4 right-4 z-10">
          <button
            type="button"
            class="p-2 qt-bg-destructive/80 hover:qt-bg-destructive rounded-full qt-text-overlay transition-colors cursor-pointer"
            title="Delete image permanently"
            aria-label="Delete image permanently"
            (click)="$event.stopPropagation(); handleDeleteClick()"
          >
            <qt-icon name="trash" class="w-6 h-6" />
          </button>
        </div>
      }

      <!-- Image container (v4 :210-231) -->
      <div
        class="relative max-w-[90vw] max-h-[90vh] flex items-center justify-center"
        (click)="$event.stopPropagation()"
      >
        @if (imageMissing()) {
          <qt-deleted-image-placeholder
            [imageId]="entry().id"
            [filename]="entry().filename"
            [width]="600"
            [height]="400"
            (cleanup)="closeModal.emit()"
          />
        } @else {
          <img
            [src]="entry().url"
            [alt]="entry().filename"
            class="max-w-full max-h-[90vh] w-auto h-auto object-contain"
            (error)="imageMissing.set(true)"
          />
        }
      </div>

      <!-- Filename and provenance at the bottom (v4 :233-258) -->
      <div
        class="absolute bottom-4 left-1/2 -translate-x-1/2 qt-text-overlay-muted text-sm qt-bg-overlay-caption px-3 py-1 rounded text-center"
        (click)="$event.stopPropagation()"
      >
        <div>{{ entry().filename }}</div>
        <div class="text-xs opacity-80">
          {{ provenance() }}
          @if (linkCount() > 0) {
            · [{{ linkCount() }} link{{ linkCount() === 1 ? '' : 's' }}]
          }
          @if (entry().messageId; as messageId) {
            ·
            <button
              type="button"
              class="qt-link underline"
              (click)="$event.stopPropagation(); jumpToMessage.emit(messageId)"
            >
              Jump to message
            </button>
          }
        </div>
      </div>
    </div>
  `,
})
export class ChatGalleryImageViewModal {
  private readonly toasts = inject(ToastService);

  readonly entry = input.required<ChatGalleryEntry>();
  /** Conditionally-`undefined` at the list ends — the arrow-visibility idiom. */
  readonly onPrev = input<(() => void) | undefined>(undefined);
  readonly onNext = input<(() => void) | undefined>(undefined);

  readonly closeModal = output<void>();
  /** The host performs the confirmed hard chat-file delete (v4 `onDelete`). */
  readonly deleteFile = output<void>();
  /** Open the shared album picker on this entry (v4 `onSave`). */
  readonly save = output<void>();
  /** Scroll the transcript to this entry's message (v4 `onJumpToMessage`). */
  readonly jumpToMessage = output<string>();

  protected readonly imageMissing = signal(false);

  /** v4 `:94-102` — the provenance line's four clauses, joined with ` · `. */
  protected readonly provenance = computed(() => {
    const entry = this.entry();
    const day = formatDay(entry.createdAt);
    return [
      SOURCE_PHRASE[entry.source],
      entry.characterName ?? null,
      day ? `painted ${day}` : null,
      entry.isCurrent ? 'current' : null,
    ]
      .filter((part): part is string => !!part)
      .join(' · ');
  });

  protected readonly linkCount = computed(() => this.entry().linkSummary?.count ?? 0);

  constructor() {
    // v4 `:71` — keyboard navigation, re-applied on callback change.
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

  /** v4 `:83-90` — the URL, not the bytes: `?download=1` names an `attachment`. */
  protected handleDownload(): void {
    try {
      downloadGalleryEntry(this.entry());
    } catch {
      this.toasts.showError('Failed to download image');
    }
  }

  /**
   * v4 `:73-81` — through `copyImageToClipboard` (v5's faithful twin in
   * `core/clipboard-utils.ts`), which converts a non-PNG blob to PNG first: a
   * bare `ClipboardItem({[blob.type]: blob})` throws for the WebP the host
   * pixel codec stores, so every copy used to fail (the §3 unification review
   * of the `78b381a96` round).
   */
  protected async handleCopyToClipboard(): Promise<void> {
    if (await copyImageToClipboard(this.entry().url)) {
      this.toasts.showSuccess('Image copied to clipboard');
    } else {
      this.toasts.showError('Failed to copy image to clipboard');
    }
  }

  /**
   * v4 `:195-197` — the bin calls `onDelete()` with NO confirmation of its own;
   * the single `showConfirmation(…)` lives in the host (`PhotoGalleryModal.tsx:
   * 258`, v5 `photo-gallery-modal.ts`'s `handleDeleteEntry`, which also carries
   * the `idKind` half of the guard). The §3 unification review of the
   * `78b381a96` round found this modal confirming TOO — two identical dialogs
   * for one delete.
   */
  protected handleDeleteClick(): void {
    this.deleteFile.emit();
  }
}
