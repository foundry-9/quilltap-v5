import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../core/core-client';
import type { AlbumKind, AlbumOption, MessageAttachment } from '../core/core-contract';
import { Modal } from '../ui/modal';
import { fileUrl } from './image-urls';

const ALBUM_KIND_LABEL: Record<AlbumKind, string> = {
  character: 'Character',
  project: 'Project',
  'document-store': 'Document Store',
  general: 'Quilltap General',
};

const ALBUM_KIND_ORDER: AlbumKind[] = ['character', 'project', 'document-store', 'general'];

interface AlbumGroup {
  kind: AlbumKind;
  label: string;
  items: AlbumOption[];
}

/**
 * SaveImageDialog — the operator "save this attached image" picker (a port of v4
 * `app/salon/[id]/components/SaveImageDialog.tsx`). Opened from the message
 * action bar's Save button, it loads the chat's candidate albums
 * (`chatPhotoAlbums`), lets the operator pick an image (when the message carries
 * more than one), an album (grouped by kind), and an optional caption, then saves
 * via `messageSaveImage`. Mirrors the LLM `keep_image` save path server-side.
 */
@Component({
  selector: 'qt-save-image-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal title="Save image to album" maxWidth="lg" (close)="close.emit()">
      <div class="space-y-4">
        @if (imageAttachments().length > 1) {
          <div>
            <label class="qt-label mb-2 block">Image</label>
            <div class="flex gap-2 flex-wrap">
              @for (att of imageAttachments(); track att.id) {
                <button
                  type="button"
                  class="qt-button qt-chat-attachment-button"
                  [class.ring-2]="att.id === selectedAttachmentId()"
                  [class.ring-offset-1]="att.id === selectedAttachmentId()"
                  [title]="att.filename"
                  (click)="setSelectedAttachment(att.id)"
                >
                  <img
                    [src]="preview(att)"
                    [alt]="att.filename"
                    width="64"
                    height="64"
                    class="qt-chat-attachment-image"
                  />
                </button>
              }
            </div>
          </div>
        }

        @if (selectedAttachment(); as att) {
          <div class="flex items-start gap-3">
            <img
              [src]="preview(att)"
              [alt]="att.filename"
              width="96"
              height="96"
              class="qt-chat-attachment-image"
            />
            <div class="text-sm opacity-80 break-all">{{ att.filename }}</div>
          </div>
        }

        <div>
          <label for="save-image-album" class="qt-label mb-2 block">Album</label>
          @if (albumsQuery.isPending()) {
            <div class="text-sm opacity-70">Loading albums…</div>
          } @else if (groupedAlbums().length === 0) {
            <div class="text-sm opacity-70">No photo albums are available for this chat.</div>
          } @else {
            <select
              id="save-image-album"
              class="qt-select w-full"
              (change)="selectedMountPointId.set($any($event.target).value)"
            >
              @for (group of groupedAlbums(); track group.kind) {
                <optgroup [label]="group.label">
                  @for (option of group.items; track option.mountPointId) {
                    <option
                      [value]="option.mountPointId"
                      [selected]="option.mountPointId === selectedMountPointId()"
                    >
                      {{ optionLabel(option) }}
                    </option>
                  }
                </optgroup>
              }
            </select>
          }
        </div>

        <div>
          <label for="save-image-caption" class="qt-label mb-2 block">
            Caption <span class="opacity-60">(optional)</span>
          </label>
          <input
            id="save-image-caption"
            type="text"
            class="qt-input w-full"
            placeholder="A short note to remember this image by"
            maxlength="200"
            [value]="caption()"
            (input)="caption.set($any($event.target).value)"
          />
        </div>

        @if (error(); as e) {
          <div class="qt-alert-error text-sm" role="alert">{{ e }}</div>
        }
      </div>

      <div qt-modal-footer class="flex justify-end gap-2">
        <button type="button" class="qt-button-secondary qt-button-sm" (click)="close.emit()">
          Cancel
        </button>
        <button
          type="button"
          class="qt-button-primary qt-button-sm"
          [disabled]="!canSave()"
          (click)="onSubmit()"
        >
          {{ submitting() ? 'Saving…' : 'Save image' }}
        </button>
      </div>
    </qt-modal>
  `,
})
export class SaveImageDialog {
  private readonly core = inject(CoreClient);

  readonly chatId = input.required<string>();
  readonly messageId = input.required<string>();
  readonly attachments = input.required<MessageAttachment[]>();
  /** The attachment id pre-selected from the toolbar click (v4 `initialAttachmentId`). */
  readonly initialAttachmentId = input<string | null>(null);

  readonly close = output<void>();
  readonly saved = output<{ mountPoint: string; relativePath: string }>();

  protected readonly caption = signal('');
  protected readonly submitting = signal(false);
  protected readonly error = signal<string | null>(null);
  protected readonly selectedMountPointId = signal('');

  protected readonly imageAttachments = computed(() =>
    this.attachments().filter((a) => a.mimeType.startsWith('image/')),
  );

  private readonly attachmentOverride = signal<string | null>(null);
  protected readonly selectedAttachmentId = computed(
    () =>
      this.attachmentOverride() ??
      this.initialAttachmentId() ??
      this.imageAttachments()[0]?.id ??
      '',
  );

  protected readonly selectedAttachment = computed(
    () =>
      this.imageAttachments().find((a) => a.id === this.selectedAttachmentId()) ??
      this.imageAttachments()[0] ??
      null,
  );

  protected readonly albumsQuery = injectQuery(() => ({
    queryKey: ['chatPhotoAlbums', this.chatId()],
    queryFn: async (): Promise<AlbumOption[]> => {
      const data = await this.core.dispatchData({ type: 'chatPhotoAlbums', chatId: this.chatId() });
      const albums = (data['albums'] as AlbumOption[]) ?? [];
      // Seed the default selection (v4 picks isDefault, else the first).
      const preferred = albums.find((a) => a.isDefault) ?? albums[0];
      if (preferred && !this.selectedMountPointId()) {
        this.selectedMountPointId.set(preferred.mountPointId);
      }
      return albums;
    },
  }));

  protected readonly groupedAlbums = computed<AlbumGroup[]>(() => {
    const albums = this.albumsQuery.data() ?? [];
    return ALBUM_KIND_ORDER.flatMap((kind) => {
      const items = albums.filter((a) => a.kind === kind);
      return items.length ? [{ kind, label: ALBUM_KIND_LABEL[kind], items }] : [];
    });
  });

  protected readonly canSave = computed(
    () =>
      !this.submitting() &&
      !this.albumsQuery.isPending() &&
      !!this.selectedAttachment() &&
      !!this.selectedMountPointId(),
  );

  protected preview(att: MessageAttachment): string {
    return fileUrl(att.id);
  }

  protected optionLabel(option: AlbumOption): string {
    return option.kind === 'character' && option.isUserCharacter ? `${option.name} (you)` : option.name;
  }

  protected setSelectedAttachment(id: string): void {
    this.attachmentOverride.set(id);
  }

  protected async onSubmit(): Promise<void> {
    const att = this.selectedAttachment();
    const mountPointId = this.selectedMountPointId();
    if (!att || !mountPointId) return;
    this.submitting.set(true);
    this.error.set(null);
    try {
      const resp = await this.core.dispatch({
        type: 'messageSaveImage',
        chatId: this.chatId(),
        messageId: this.messageId(),
        fileId: att.id,
        mountPointId,
        caption: this.caption().trim() ? this.caption().trim() : undefined,
      });
      if (resp.type === 'error') throw new Error(resp.data.message);
      const body = (resp.data ?? {}) as {
        mountPoint?: string;
        relativePath?: string;
        data?: { mountPoint?: string; relativePath?: string };
      };
      this.saved.emit({
        mountPoint: body.data?.mountPoint ?? body.mountPoint ?? '',
        relativePath: body.data?.relativePath ?? body.relativePath ?? '',
      });
      this.close.emit();
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : String(err));
    } finally {
      this.submitting.set(false);
    }
  }
}
