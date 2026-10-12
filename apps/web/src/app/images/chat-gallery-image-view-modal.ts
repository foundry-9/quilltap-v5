import { ChangeDetectionStrategy, Component, computed, inject, input, output } from '@angular/core';

import { downloadGalleryEntry } from '../core/download-utils';
import { copyImageToClipboard } from '../core/clipboard-utils';
import type { ChatGalleryEntry, ChatGallerySource } from '../core/core-contract';
import { Icon } from '../ui/icon';
import { ToastService } from '../ui/toast.service';
import {
  FullScreenImageAction,
  FullScreenImageBottomRight,
  FullScreenImageCaption,
  FullScreenImageViewer,
} from './full-screen-image-viewer';

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
 *
 * `01a83539d` (P4.D277) moved the view onto the shared
 * `FullScreenImageViewer` frame (`images/full-screen-image-viewer.ts`): its
 * own miss flag, keyboard hook and overlay markup are gone, and no
 * user-facing string moved. Four behaviours did, with the frame: (1) the
 * overlay is portalled to `document.body`; (2) the dialog is named by the
 * entry's filename (`aria-label`); (3) Close stays visible while the image is
 * missing (only the actions and the delete hide); (4) a miss clears when
 * prev / next changes the picture. The keys are always live here — v4 passes
 * no `keyboardActive`, so one Escape under an open save dialog closes this
 * view too (Tier 3 item 21, ported as-is).
 */
@Component({
  selector: 'qt-chat-gallery-image-view-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, FullScreenImageViewer, FullScreenImageCaption, FullScreenImageBottomRight],
  template: `
    <!-- 01a83539d — the shared frame (z-[60], one layer above the z-50 gallery). -->
    <qt-full-screen-image-viewer
      [src]="entry().url"
      [alt]="entry().filename"
      [imageId]="entry().id"
      [filename]="entry().filename"
      [onPrev]="onPrev()"
      [onNext]="onNext()"
      [actions]="actions()"
      (closeModal)="closeModal.emit()"
    >
      <!-- Delete — only where the chat itself owns the record. A portrait
           belongs to its character; a kept or inline picture belongs to an
           album or a vault; the background presently on the wall is what the
           chat is showing. -->
      @if (entry().deletable) {
        <button
          qtFullScreenBottomRight
          type="button"
          class="p-2 qt-bg-destructive/80 hover:qt-bg-destructive rounded-full qt-text-overlay transition-colors cursor-pointer"
          title="Delete image permanently"
          aria-label="Delete image permanently"
          (click)="$event.stopPropagation(); handleDeleteClick()"
        >
          <qt-icon name="trash" class="w-6 h-6" />
        </button>
      }

      <!-- Filename and provenance — the frame's caption. -->
      <ng-container qtFullScreenCaption>
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
      </ng-container>
    </qt-full-screen-image-viewer>
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

  /**
   * The three actions before Close (the `ChatGalleryImageViewModal.tsx` hunk
   * at `01a83539d`), a `computed` so the array is stable per render.
   */
  protected readonly actions = computed<FullScreenImageAction[]>(() => [
    { icon: 'bookmark', label: 'Save to a photo album', onClick: () => this.save.emit() },
    { icon: 'download', label: 'Download', onClick: () => this.handleDownload() },
    { icon: 'copy', label: 'Copy to clipboard', onClick: () => void this.handleCopyToClipboard() },
  ]);

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
