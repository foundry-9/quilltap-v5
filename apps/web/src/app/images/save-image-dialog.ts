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
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../core/core-client';
import { CoreDispatchError } from '../core/core-contract';
import type { AlbumKind, AlbumOption, MessageAttachment } from '../core/core-contract';
import { Modal } from '../ui/modal';
import { listWardrobeSaveTargets, saveWardrobeImageToStore } from '../wardrobe/item-images.api';
import type { WardrobeContainer } from '../wardrobe/wardrobe-container';
import { listInventorySaveTargets, saveInventoryImageToStore } from '../inventory/inventory.api';
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
 * Which door the dialog was opened from, and therefore where it reads its
 * albums and which verb it saves through (v4 `SaveImageDialog.tsx:45-53`
 * `SaveImageTarget` at `01a83539d`). `fileId` is the image selected when it
 * opened; the in-dialog picker can move it when a message carries several.
 * `chatId` rides the two chat targets (it was a separate input before
 * `01a83539d`).
 */
export type SaveImageTarget =
  | { kind: 'message'; chatId: string; messageId: string; fileId: string }
  | { kind: 'chat'; chatId: string; fileId: string }
  | { kind: 'wardrobe'; itemId: string; container: WardrobeContainer; fileId: string }
  // P4.D330 — v4 `4fbdad186` (`:55`): the Inventory picture viewer's door.
  | { kind: 'inventory'; itemId: string; container: WardrobeContainer; fileId: string };

/**
 * SaveImageDialog — the operator "save this attached image" picker (a port of
 * v4 `app/salon/[id]/components/SaveImageDialog.tsx`, P4.D176; read again at
 * `01a83539d` for P4.D277).
 *
 * v4's module doc, carried: opens from three doors — the per-message Save
 * Image toolbar button, the chat gallery's Save, and the wardrobe picture
 * viewer's Save. For the two Salon doors it fetches the chat's candidate photo
 * albums (chat participants' vaults, the project album, linked document
 * stores, Quilltap General); the wardrobe viewer belongs to no chat and is
 * offered every document store. It then saves the chosen album through
 * whichever save-image verb matches the door it came from.
 *
 * The doors differ only in the verb they dispatch. The message route's guard —
 * *is this image attached to this message* — is a real invariant there, and
 * half the gallery has no message at all (a Lantern backdrop posted with
 * alerts off, a participant's standing portrait), so the gallery dispatches a
 * chat-scoped twin whose guard is *is this image in this chat's gallery*
 * (`chatSaveGalleryImage`). The wardrobe door dispatches the item's images
 * verb (`wardrobeItemImageSaveToStore`), whose guard is *is this one of the
 * item's own pictures*. Everything the reader sees is identical, bar the empty
 * sentence.
 *
 * Mirrors the LLM `keep_image` save path under the hood.
 *
 * **Recorded mechanism divergences.** v4 fetches URLs; v5 rides verbs (album
 * reads: `chatPhotoAlbums` for both chat doors, `wardrobeItemImageSaveTargets`
 * for the wardrobe door — each under its own TanStack key). v4's
 * `Failed to load photo albums (<status>)` line has no counterpart (the
 * dispatch error carries no status; a failed read renders the empty sentence —
 * P4.D277 Tier 3 item 17). And (R-C) v5's `qt-modal` renders in place with no
 * Escape, so this dialog ports v4 `BaseModal`'s two behaviours LOCALLY for all
 * three doors: it reparents its own host to `document.body` (so it paints
 * above a body-portalled full-screen viewer, and escapes the item editor's
 * `transform`ed ancestor), and it closes on a document Escape (v4
 * `closeOnEscape`, default true).
 */
@Component({
  selector: 'qt-save-image-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  // R-C — v4 BaseModal's `closeOnEscape` (default true; `BaseModal.tsx:97-100`).
  host: { '(document:keydown.escape)': 'close.emit()' },
  template: `
    <qt-modal
      title="Save image to album"
      maxWidth="lg"
      [overlayClass]="overlayClass()"
      (close)="close.emit()"
    >
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
            <div class="text-sm opacity-70">
              {{
                target().kind === 'wardrobe' || target().kind === 'inventory'
                  ? 'No document stores are available.'
                  : 'No photo albums are available for this chat.'
              }}
            </div>
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

  readonly target = input.required<SaveImageTarget>();
  /**
   * Candidate images for the in-dialog picker. The message door passes every
   * image attachment on the message (for the ribbon); the gallery door passes
   * the ONE entry it opened on (v4 `PhotoGalleryModal.tsx:558-565`).
   */
  readonly attachments = input.required<MessageAttachment[]>();
  /**
   * Extra overlay classes — a raised `z-[…]` when the dialog opens over
   * something that already sits above the ordinary dialog layer (v4 `:84-88`
   * `overlayClassName`, `01a83539d`).
   */
  readonly overlayClass = input('');

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
  /** v4 `:82-84` — `target.fileId || imageAttachments[0]?.id || ''`. */
  protected readonly selectedAttachmentId = computed(
    // v4 `:82-84` is `target.fileId || imageAttachments[0]?.id || ''` — an EMPTY
    // `fileId` is falsy and falls through to the first image (JS `||`, not `??`).
    () => this.attachmentOverride() ?? (this.target().fileId || this.imageAttachments()[0]?.id || ''),
  );

  protected readonly selectedAttachment = computed(
    () =>
      this.imageAttachments().find((a) => a.id === this.selectedAttachmentId()) ??
      this.imageAttachments()[0] ??
      null,
  );

  /**
   * The album list for the door (v4 `albumsUrlFor`, `:56-60`): the chat's
   * candidate albums for both chat doors, every document store for the
   * wardrobe door — each under its own key. The default selection is seeded
   * once from the list, whenever it ARRIVES — fetched or served from the cache
   * (v4 picks `isDefault`, else the first, in an effect over the fetched
   * albums on every mount). It used to be seeded inside the query function,
   * which TanStack does not run again for a cached key: a second open of the
   * dialog in one session then selected nothing and the browser showed the
   * first option (the `01a83539d` unification, §S.9's flipped P4.D277 beat).
   */
  protected readonly albumsQuery = injectQuery(() => {
    const target = this.target();
    return {
      queryKey:
        target.kind === 'wardrobe'
          ? ['wardrobeItemImageSaveTargets', target.itemId, target.container.scope, target.container.id]
          : target.kind === 'inventory'
            ? ['inventoryItemImageSaveTargets', target.itemId, target.container.scope, target.container.id]
            : ['chatPhotoAlbums', target.chatId],
      queryFn: async (): Promise<AlbumOption[]> => {
        const albums =
          target.kind === 'wardrobe'
            ? await listWardrobeSaveTargets(this.core, target.itemId, target.container)
            : target.kind === 'inventory'
              ? await listInventorySaveTargets(this.core, target.container, target.itemId)
              : (((await this.core.dispatchData({ type: 'chatPhotoAlbums', chatId: target.chatId }))[
                  'albums'
                ] as AlbumOption[]) ?? []);
        return albums;
      },
    };
  });

  private readonly seedDefaultAlbum = effect(() => {
    const albums = this.albumsQuery.data() ?? [];
    const preferred = albums.find((a) => a.isDefault) ?? albums[0];
    if (preferred && !this.selectedMountPointId()) {
      this.selectedMountPointId.set(preferred.mountPointId);
    }
  });

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

  constructor() {
    // R-C — v4 BaseModal's portal (`BaseModal.tsx:106`), ported locally: the
    // host reparents itself to `document.body` after the first render (the
    // `image-detail-modal.ts` idiom — every host mounts this dialog under an
    // `@if`, so a constructor-time reparent would be undone by the insertion).
    // The host moves with its content, so `qt-save-image-dialog` locators hold.
    const host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;
    inject(DestroyRef).onDestroy(() => host.remove());
    afterNextRender(() => {
      if (typeof document !== 'undefined') {
        document.body.appendChild(host);
      }
    });
  }

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
      const target = this.target();
      const caption = this.caption().trim() ? this.caption().trim() : undefined;
      let body: {
        mountPoint?: string;
        relativePath?: string;
        data?: { mountPoint?: string; relativePath?: string };
      };
      if (target.kind === 'wardrobe') {
        body = await saveWardrobeImageToStore(this.core, target.itemId, target.container, {
          fileId: att.id,
          mountPointId,
          caption,
        });
      } else if (target.kind === 'inventory') {
        body = await saveInventoryImageToStore(this.core, target.container, target.itemId, {
          fileId: att.id,
          mountPointId,
          caption,
        });
      } else {
        const resp =
          target.kind === 'message'
            ? await this.core.dispatch({
                type: 'messageSaveImage',
                chatId: target.chatId,
                messageId: target.messageId,
                fileId: att.id,
                mountPointId,
                caption,
              })
            : await this.core.dispatch({
                type: 'chatSaveGalleryImage',
                chatId: target.chatId,
                fileId: att.id,
                mountPointId,
                caption,
              });
        if (resp.type === 'error') throw new CoreDispatchError(resp.data);
        body = (resp.data ?? {}) as typeof body;
      }
      this.saved.emit({
        mountPoint: body.data?.mountPoint ?? body.mountPoint ?? '',
        relativePath: body.data?.relativePath ?? body.relativePath ?? '',
      });
      this.close.emit();
    } catch (err) {
      this.error.set(this.errorMessage(err));
    } finally {
      this.submitting.set(false);
    }
  }

  /**
   * v4 `:185-196` (at `01a83539d`) — the album already holds these bytes is an
   * ANSWER, not a failure, and deserves to be said in those words. v4 detects
   * it off the response directly — `res.status === 409 || body.code ===
   * 'ALREADY_SAVED'`, so ALL THREE doors (the chat and wardrobe legs' 409, and
   * the message leg's — a 400 carrying the same code until P4.D275 moves it to
   * the same 409, transparently) — and formats `body.keptAt` into the sentence
   * when the server sent one: `That picture is already in this album — it was
   * filed there on <toLocaleDateString()>.` The dispatch envelope carries
   * `code` and the `keptAt` rider on `CoreError` (the §3 unification review of
   * the `78b381a96` round: the riders used to be nested where no client could
   * read them, and this arm was gated on the chat leg alone).
   *
   * R-A (P4.D277, v4-faithful — a v4-filing candidate): the wardrobe door's
   * archived-vault refusal is a plain 409 conflict with NO `code` and NO
   * `keptAt`, so it too reads as the undated "already in this album" — as it
   * does in v4. Reachable only by archiving the character between the list
   * read and the save (save-targets never offers an archived vault).
   */
  private errorMessage(err: unknown): string {
    if (err instanceof CoreDispatchError) {
      if (err.kind === 'conflict' || err.code === 'ALREADY_SAVED') {
        const when = err.keptAt ? new Date(err.keptAt).toLocaleDateString() : null;
        return when
          ? `That picture is already in this album — it was filed there on ${when}.`
          : 'That picture is already in this album.';
      }
      return err.message;
    }
    return err instanceof Error ? err.message : String(err);
  }
}
