import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
  untracked,
} from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../../core/core-client';
import { apiUrl } from '../../../../core/api-url';
import { triggerBlobDownload } from '../../../../core/download-utils';
import { Icon } from '../../../../ui/icon';
import { normalizeAvatarSrc } from '../../../../ui/avatar-stack';
import { ImageDetailModal } from '../../../../images/image-detail-modal';
import type { ImageData } from '../../../../images/images.api';
import {
  avatarRollAction,
  characterKeys,
  deleteAvatarRoll,
  fetchAvatarRolls,
  toAvatarRoll,
  type AvatarRoll,
} from '../../characters.api';
import { ToastService } from '../../../../ui/toast.service';

/**
 * The plates the house has already developed for this character — v4
 * `components/images/embedded-gallery/AvatarRollsSection.tsx` +
 * `hooks/useAvatarRolls.ts` (`4dcbe0d21`), one component here because v5's
 * gallery tab has no hook layer to split them across.
 *
 * Carrying v4's own why: "A separate section rather than more tiles in the
 * album, because these are a different kind of thing: the album is what
 * someone chose to keep, and this is the avatar cache's working stock — one
 * image per configuration of outfit, provider, profile and model, reused so
 * the same sitting is never paid for twice. Every action the album offers
 * works here, plus one the album has no use for: keeping a plate *into* the
 * album." And on the default: "Collapsed by default. A character in long
 * service accumulates a great many of these, and they should not push the
 * album off the page."
 *
 * TWO recorded divergences, both structural rather than behavioural:
 *
 *  - **The tile markup is a copy, not a shared component.** v4 extends its
 *    `GalleryImage` with four optional props (`onSaveToAlbum`, `isInAlbum`,
 *    `isBusy`, `deleteTitle`) and reuses it through `GalleryGrid`. v5's
 *    gallery tab never factored its tile out — it is an inline template — so
 *    the section carries its own copy with the SAME class strings. The order
 *    anticipated this and asked for the divergence to be recorded.
 *  - **No spinner branches.** v4's tile spins when `settingAvatar`/
 *    `deletingImage` name the tile, but the rolls section passes BOTH as
 *    `null` (`AvatarRollsSection.tsx:152-153`), so `isUpdating` and
 *    `isDeletingImage` are false for every roll and the spinner arms are
 *    unreachable here. The busy state is carried entirely by `busyRollId`.
 *
 * The `<img>`'s own `console.warn` ("Image failed to load in gallery") is
 * `GalleryImage`'s, which v5's gallery tab has never carried; the section
 * mirrors the tab rather than reintroducing it alone. `handleRollError`'s
 * warn — this hunk's own — IS carried.
 */
@Component({
  selector: 'qt-avatar-rolls-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { class: 'qt-avatar-rolls-section' },
  imports: [Icon, ImageDetailModal],
  template: `
    <!-- v4 :119 — nothing at all when the house has drawn nothing yet;
         "say nothing at all rather than adding an empty shelf to the page".
         There is no empty-state string (§R.6(9)). -->
    @if (loading() || total() > 0) {
      <section class="mt-8 border-t qt-border-default pt-6">
        <button
          type="button"
          class="flex w-full items-center gap-2 text-left"
          [attr.aria-expanded]="expanded()"
          (click)="expanded.set(!expanded())"
        >
          <qt-icon
            [name]="expanded() ? 'chevron-down' : 'chevron-right'"
            class="w-4 h-4 qt-text-secondary"
          />
          <span class="qt-text-label">Avatar Rolls</span>
          <span class="qt-text-label-xs">{{ countLabel() }}</span>
        </button>

        <p class="qt-text-label-xs mt-1 ml-6">
          Portraits the house has already developed for {{ entityName() }} — one plate per
          configuration of outfit, provider and model, kept so the same sitting need never be
          paid for twice. Keep one in the album, hang it as the portrait, take a copy away, or
          discard it and let the next sitting be drawn afresh.
        </p>

        @if (expanded()) {
          <div class="mt-4">
            @if (loading()) {
              <div class="flex items-center justify-center py-8">
                <div class="animate-spin rounded-full h-6 w-6 border-b-2 qt-border-primary"></div>
              </div>
            } @else {
              <!-- v4 GalleryGrid:36-40, at the album's own thumbnail size. -->
              <div
                class="grid gap-2"
                [style.grid-template-columns]="
                  'repeat(auto-fill, minmax(' + thumbnailSize() + 'px, 1fr))'
                "
              >
                @for (roll of rolls(); track roll.id; let index = $index) {
                  <div class="relative group">
                    <button
                      type="button"
                      class="relative aspect-square w-full overflow-hidden rounded-lg qt-bg-muted hover:ring-2 hover:ring-primary focus:outline-none focus:ring-2 focus:ring-ring transition-all {{
                        portraitRollId() === roll.id ? 'ring-2 ring-success' : ''
                      }}"
                      (click)="selectedIndex.set(index)"
                    >
                      @if (missingRolls().has(roll.id)) {
                        <div
                          class="absolute inset-0 flex items-center justify-center qt-text-secondary"
                        >
                          <qt-icon name="image" class="w-8 h-8" />
                        </div>
                      } @else {
                        <img
                          [src]="srcFor(roll)"
                          [alt]="roll.filename"
                          class="absolute inset-0 w-full h-full object-cover"
                          (error)="handleRollError(roll.id)"
                        />
                      }
                      @if (portraitRollId() === roll.id) {
                        <div
                          class="absolute top-1 left-1 qt-bg-success qt-text-on-success text-xs px-1.5 py-0.5 rounded font-medium"
                        >
                          Avatar
                        </div>
                      }
                    </button>

                    <div
                      class="absolute bottom-1 right-1 flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity"
                    >
                      @if (portraitRollId() !== roll.id) {
                        <button
                          type="button"
                          class="p-1.5 rounded-full qt-shadow-md qt-bg-card qt-text-secondary hover:qt-bg-success hover:qt-text-on-success transition-colors {{
                            isBusy(roll) ? 'opacity-50' : ''
                          }}"
                          title="Set as avatar"
                          [disabled]="isBusy(roll)"
                          (click)="$event.stopPropagation(); handleSetAvatar(roll)"
                        >
                          <qt-icon name="user" class="w-4 h-4" />
                        </button>
                      }
                      <!-- v4 GalleryImage:89-105 — the keep button, avatar
                           rolls only. Filled and disabled once kept. -->
                      <button
                        type="button"
                        class="p-1.5 rounded-full qt-shadow-md transition-colors {{
                          isInAlbum(roll)
                            ? 'qt-bg-success qt-text-on-success'
                            : 'qt-bg-card qt-text-secondary hover:qt-bg-primary hover:qt-text-on-primary'
                        }} {{ isBusy(roll) && !isInAlbum(roll) ? 'opacity-50' : '' }}"
                        [title]="keepLabel(roll)"
                        [attr.aria-label]="keepLabel(roll)"
                        [disabled]="isInAlbum(roll) || isBusy(roll)"
                        (click)="$event.stopPropagation(); handleSaveToAlbum(roll)"
                      >
                        <qt-icon name="bookmark" class="w-4 h-4" />
                      </button>
                      @if (!missingRolls().has(roll.id)) {
                        <button
                          type="button"
                          class="p-1.5 rounded-full qt-shadow-md qt-bg-card qt-text-secondary hover:qt-bg-primary hover:qt-text-on-primary transition-colors"
                          title="Download image"
                          aria-label="Download image"
                          (click)="$event.stopPropagation(); handleDownload(roll)"
                        >
                          <qt-icon name="download" class="w-4 h-4" />
                        </button>
                      }
                      @if (portraitRollId() !== roll.id || missingRolls().has(roll.id)) {
                        <button
                          type="button"
                          class="p-1.5 rounded-full qt-shadow-md transition-colors {{
                            confirmDelete() === roll.id
                              ? 'qt-bg-destructive qt-text-on-destructive'
                              : 'qt-bg-card qt-text-secondary hover:qt-bg-destructive hover:qt-text-on-destructive'
                          }} {{ isBusy(roll) ? 'opacity-50' : '' }}"
                          [title]="deleteLabel(roll)"
                          [disabled]="isBusy(roll)"
                          (click)="$event.stopPropagation(); handleDelete(roll)"
                        >
                          <qt-icon name="trash" class="w-4 h-4" />
                        </button>
                      }
                    </div>
                  </div>
                }
              </div>
            }
          </div>
        }

        <!-- v4 :171-202 — the SAME detail modal the album opens, with NO
             generation-prompt field: v4's ImageData has none (§R.6(8)). -->
        @if (selectedImage(); as image) {
          <qt-image-detail-modal
            [image]="image"
            [onPrev]="selectedIndex() > 0 ? boundPrev : undefined"
            [onNext]="selectedIndex() < rolls().length - 1 ? boundNext : undefined"
            (closeModal)="selectedIndex.set(-1)"
            (avatarSet)="refresh.emit()"
          />
        }
      </section>
    }
  `,
})
export class AvatarRollsSection {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  private readonly queryClient = injectQueryClient();

  readonly characterId = input.required<string>();
  readonly entityName = input.required<string>();
  readonly thumbnailSize = input.required<number>();

  /** v4 `onAvatarChange` — the ALBUM link id the server minted. */
  readonly avatarChange = output<string | null>();
  /** v4 `onRefresh` — the host re-reads whatever the mutation touched. */
  readonly refresh = output<void>();

  protected readonly expanded = signal(false);
  protected readonly selectedIndex = signal(-1);
  protected readonly confirmDelete = signal<string | null>(null);
  protected readonly missingRolls = signal<ReadonlySet<string>>(new Set());
  /** v4 `busyRollId` — the one tile with an action in flight, if any. */
  protected readonly busyRollId = signal<string | null>(null);

  protected readonly rollsQuery = injectQuery(() => ({
    queryKey: characterKeys.avatarRolls(this.characterId()),
    queryFn: () => fetchAvatarRolls(this.core, this.characterId()),
  }));

  protected readonly rolls = computed<AvatarRoll[]>(() =>
    (this.rollsQuery.data()?.entries ?? []).map(toAvatarRoll),
  );
  protected readonly total = computed(() => this.rollsQuery.data()?.total ?? 0);
  protected readonly loading = computed(() => this.rollsQuery.isPending());

  /** v4 `:131-133` — `…` while the count is unknown, else `N plate(s)`. */
  protected readonly countLabel = computed(() => {
    if (this.loading()) return '…';
    const total = this.total();
    return `${total} plate${total === 1 ? '' : 's'}`;
  });

  /**
   * v4 `:64` — "The grid badges whichever tile matches `currentAvatarId`. A
   * portrait pointer is an album link id, never a roll's file id, so hand the
   * grid the roll the server already resolved as the portrait rather than the
   * raw `defaultImageId` — which would match nothing here."
   */
  protected readonly portraitRollId = computed(
    () => this.rolls().find((r) => r.isPortrait)?.id,
  );

  private readonly albumMemberIds = computed(
    () => new Set(this.rolls().filter((r) => r.albumLinkId).map((r) => r.id)),
  );

  protected readonly selectedImage = computed<ImageData | null>(() => {
    const index = this.selectedIndex();
    const roll = index >= 0 ? this.rolls()[index] : undefined;
    return roll ? this.toImageData(roll) : null;
  });

  protected readonly boundPrev = (): void => {
    this.selectedIndex.update((prev) => (prev > 0 ? prev - 1 : prev));
  };
  protected readonly boundNext = (): void => {
    this.selectedIndex.update((prev) => (prev < this.rolls().length - 1 ? prev + 1 : prev));
  };

  constructor() {
    // v4 `:55-60` — an armed delete disarms after 3 s. The gallery tab's own
    // idiom, reused verbatim.
    effect((onCleanup) => {
      const armed = this.confirmDelete();
      if (armed) {
        const timer = setTimeout(() => untracked(() => this.confirmDelete.set(null)), 3000);
        onCleanup(() => clearTimeout(timer));
      }
    });
  }

  protected isBusy(roll: AvatarRoll): boolean {
    return this.busyRollId() === roll.id;
  }

  protected isInAlbum(roll: AvatarRoll): boolean {
    return this.albumMemberIds().has(roll.id);
  }

  protected keepLabel(roll: AvatarRoll): string {
    return this.isInAlbum(roll) ? 'Already in the photo album' : 'Keep in the photo album';
  }

  /**
   * v4 `describeDelete` (`:103-113`) — "A destructive click should know what
   * it is destroying: the plate is discarded, a kept copy is not, and any
   * conversation showing it is unbound."
   */
  protected describeDelete(roll: AvatarRoll): string {
    const parts = ['Discard this plate'];
    if (roll.usedInChatCount > 0) {
      parts.push(
        `in use in ${roll.usedInChatCount} conversation${roll.usedInChatCount === 1 ? '' : 's'}`,
      );
    }
    if (roll.albumLinkId) {
      parts.push('the album copy stays');
    }
    return parts.length > 1 ? `${parts[0]} (${parts.slice(1).join('; ')})` : parts[0];
  }

  /** v4 GalleryImage's delete `title` — the confirming form wins. */
  protected deleteLabel(roll: AvatarRoll): string {
    return this.confirmDelete() === roll.id
      ? 'Click again to confirm delete'
      : this.describeDelete(roll);
  }

  protected srcFor(roll: AvatarRoll): string | null {
    return normalizeAvatarSrc(roll.filepath);
  }

  /** v4 `handleRollError` (`:90-93`). */
  protected handleRollError(rollId: string): void {
    this.missingRolls.update((prev) => new Set(prev).add(rollId));
    console.warn('Avatar roll failed to load', { rollId });
  }

  /**
   * v4 `:173-201` — the modal's `ImageData`. NO `linkId`: "A roll is an
   * images-v2 row, so no `linkId`: the modal's save-to-gallery actions post
   * `{ fileId }` and the server re-links from the blob the file already
   * names." NO prompt field either — v4's `ImageData` has none.
   *
   * `url` is `apiUrl`-wrapped where v4 leaves it undefined: v5's gallery tab
   * already does this for the same modal, so a Tauri webview off the qtap
   * origin can resolve the bytes.
   */
  private toImageData(roll: AvatarRoll): ImageData {
    return {
      id: roll.id,
      filename: roll.filename,
      filepath: roll.filepath,
      url: apiUrl(roll.filepath),
      mimeType: roll.mimeType ?? 'image/webp',
      size: roll.size,
      width: roll.width,
      height: roll.height,
      createdAt: roll.createdAt,
      tags: [],
    };
  }

  /**
   * v4 `invalidateAll` (`:82-88`) — three keys in parallel. "Every mutation
   * invalidates the album query too: promoting a roll to the portrait, and
   * keeping one, both add a photo to `photos/`." The `detail` key carries the
   * portrait pointer the header renders.
   */
  private async invalidateAll(): Promise<void> {
    const id = this.characterId();
    await Promise.all([
      this.queryClient.invalidateQueries({ queryKey: characterKeys.avatarRolls(id) }),
      this.queryClient.invalidateQueries({ queryKey: characterKeys.photos(id) }),
      this.queryClient.invalidateQueries({ queryKey: characterKeys.detail(id) }),
    ]);
  }

  /**
   * v4 `runAction` (`:95-120`). A refused action toasts the server's own
   * `error`, logs v4's four-key bag, and answers null so the caller says
   * nothing more.
   *
   * v4's `Request failed (<status>)` fallback has NO v5 counterpart: it fires
   * when a non-OK body carries no `error`, and the dispatch boundary has no
   * HTTP status to name — every `CoreDispatchError` carries the core's own
   * message, and a transport failure becomes a synthetic `internal` error
   * that carries one too.
   */
  private async runAction(
    roll: AvatarRoll,
    action: 'save-to-album' | 'set-avatar',
  ): Promise<{ linkId?: string; alreadyInAlbum?: boolean } | null> {
    this.busyRollId.set(roll.id);
    try {
      const body = await avatarRollAction(this.core, this.characterId(), roll.id, action);
      await this.invalidateAll();
      return body;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.toasts.showError(message);
      console.error('Avatar roll action failed', {
        action,
        characterId: this.characterId(),
        rollId: roll.id,
        error: message,
      });
      return null;
    } finally {
      this.busyRollId.set(null);
    }
  }

  /** v4 `handleSaveToAlbum` (`:76-80`) + `saveRollToAlbum` (`:122-133`). */
  protected async handleSaveToAlbum(roll: AvatarRoll): Promise<void> {
    const result = await this.runAction(roll, 'save-to-album');
    if (result) {
      this.toasts.showSuccess(
        result.alreadyInAlbum ? 'Already in the album' : 'Kept in the photo album',
      );
    }
    this.refresh.emit();
  }

  /**
   * v4 `handleSetAvatar` (`:70-74`) + `setRollAsAvatar` (`:135-145`). A body
   * without a `linkId` bails SILENTLY — no toast, no avatar change.
   */
  protected async handleSetAvatar(roll: AvatarRoll): Promise<void> {
    const result = await this.runAction(roll, 'set-avatar');
    if (result?.linkId) {
      this.avatarChange.emit(result.linkId);
      this.toasts.showSuccess('Avatar updated!');
    }
    this.refresh.emit();
  }

  /** v4 `handleDownload` (`:82-85`) + `downloadRoll` (`:174-189`). */
  protected async handleDownload(roll: AvatarRoll): Promise<void> {
    const image = this.toImageData(roll);
    const src =
      image.url || (image.filepath.startsWith('/') ? image.filepath : `/${image.filepath}`);
    try {
      const res = await fetch(src);
      if (!res.ok) throw new Error(`Failed to fetch image (${res.status})`);
      const blob = await res.blob();
      triggerBlobDownload(blob, roll.filename);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.toasts.showError('Failed to download image');
      console.error('Error downloading avatar roll', {
        characterId: this.characterId(),
        rollId: roll.id,
        error: message,
      });
    }
  }

  /**
   * v4 `handleDelete` (`:87-99`) — first click arms, a second on the SAME
   * tile within 3 s discards. "The server clears every pointer at the plate
   * before the bytes go, so a portrait or a chat seat resolved to it needs
   * re-reading afterwards."
   */
  protected async handleDelete(roll: AvatarRoll): Promise<void> {
    if (this.confirmDelete() !== roll.id) {
      this.confirmDelete.set(roll.id);
      return;
    }
    this.confirmDelete.set(null);
    await this.deleteRoll(roll);
    this.selectedIndex.set(-1);
    this.refresh.emit();
  }

  /** v4 `deleteRoll` (`:147-171`). */
  private async deleteRoll(roll: AvatarRoll): Promise<void> {
    this.busyRollId.set(roll.id);
    try {
      const body = await deleteAvatarRoll(this.core, this.characterId(), roll.id);
      // v4 reaches the service through the REST edge, which turns the
      // service's `deleted: false` (a miss — the id names no roll of this
      // character) into `notFound('Avatar roll')`, and the client toasts that
      // 404's sentence. The dispatch verb answers the service's own shape,
      // so the miss is judged here, before anything is invalidated.
      if (!body?.deleted) {
        throw new Error('Avatar roll not found');
      }
      await this.invalidateAll();
      this.toasts.showSuccess(
        body?.keptInAlbum ? 'Roll discarded; the album copy stays' : 'Roll discarded',
      );
    } catch (error) {
      // v4 `:158` — `body?.error || 'Failed to delete the avatar roll'`.
      const message =
        (error instanceof Error && error.message) || 'Failed to delete the avatar roll';
      this.toasts.showError(message);
      console.error('Avatar roll delete failed', {
        characterId: this.characterId(),
        rollId: roll.id,
        error: message,
      });
    } finally {
      this.busyRollId.set(null);
    }
  }
}
