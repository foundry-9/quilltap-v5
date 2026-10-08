import { NgTemplateOutlet } from '@angular/common';
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
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../core/core-client';
import type { ImageProfileDto, WardrobeItemDto } from '../../core/core-contract';
import { chatSettingsKeys, fetchChatSettings } from '../../screens/settings/chat/chat-settings.api';
import { fetchImageProfiles, imageProfileKeys } from '../../screens/settings/images/image-profiles.api';
import { Icon } from '../../ui/icon';
import { ToastService } from '../../ui/toast.service';
import {
  deleteWardrobeItemImage,
  generateWardrobeItemImage,
  listWardrobeItemImages,
  setCurrentWardrobeItemImage,
  uploadWardrobeItemImage,
  WardrobeImageRequestError,
  wardrobeImageThumbnailUrl,
  wardrobeImageUrl,
  type WardrobeItemImageGenerateResponse,
  type WardrobeItemImagesResponse,
  type WardrobeItemImageSummary,
} from '../item-images.api';
import type { WardrobeContainer } from '../wardrobe-container';

/** Types accepted by Upload — the same set as Import from image (v4 `:50-52`). */
export const WARDROBE_IMAGE_ACCEPT = 'image/jpeg,image/png,image/webp,image/gif';
const ACCEPTED_TYPES = new Set(WARDROBE_IMAGE_ACCEPT.split(','));
/** Upload ceiling, checked here before the bytes travel (the route checks again). */
export const WARDROBE_IMAGE_MAX_BYTES = 10 * 1024 * 1024;

/** v4 `:80-84` — the picker's option label. */
function profileLabel(p: ImageProfileDto): string {
  return `${p.name}${p.isDefault ? ' (default)' : ''} — ${p.provider}/${p.modelName}${
    p.isDangerousCompatible ? ' · uncensored' : ''
  }`;
}

/**
 * The live half of the Picture section (v4 `ActiveImageSection`, `:111-487`):
 * mounted only once there is an item and its home container, so create mode
 * makes no request at all.
 *
 * **Recorded mechanism divergences.** v4's history is a TanStack query keyed
 * `queryKeys.wardrobe.images`; v5 holds it in a signal and re-reads after
 * every change (there is no wardrobe query cache to share — the list is
 * signal-held, survey §7.5). The profiles and the designated profile ride the
 * SAME TanStack keys the Settings cards use (`imageProfileKeys.list()`,
 * `chatSettingsKeys.all`), so they dedupe with them. v4's `showConfirmation`
 * is `window.confirm`, the wardrobe dialog's idiom throughout. v4's
 * `queryKeys.wardrobe.all` invalidate after a change is the `imageChanged`
 * output — the dialog reloads its list on it.
 */
@Component({
  selector: 'qt-wardrobe-item-image-active',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, NgTemplateOutlet],
  template: `
    <section aria-label="Picture" data-testid="wardrobe-item-image-section">
      <span class="qt-label mb-1 block">Picture</span>

      @if (currentImage(); as current) {
        <div class="relative inline-block">
          <a [href]="fullUrl(current.fileId)" target="_blank" rel="noreferrer">
            <img
              [src]="fullUrl(current.fileId)"
              [alt]="'Picture of ' + item().title"
              class="max-h-[16rem] w-auto rounded border qt-border-default qt-bg-muted"
              data-testid="wardrobe-item-image-current"
            />
          </a>
          @if (generating()) {
            <div
              class="absolute inset-0 flex items-center justify-center rounded qt-bg-overlay-medium"
            >
              <div
                class="animate-spin rounded-full h-8 w-8 border-b-2 qt-border-primary"
                role="status"
                aria-label="Generating a picture"
              ></div>
            </div>
          }
        </div>
      } @else {
        <div
          class="flex flex-col items-center justify-center gap-3 rounded border-2 border-dashed qt-border-default px-4 py-6 text-center"
          style="min-height: 10rem"
        >
          @if (generating()) {
            <div
              class="animate-spin rounded-full h-8 w-8 border-b-2 qt-border-primary"
              role="status"
              aria-label="Generating a picture"
            ></div>
            <p class="qt-text-small qt-text-secondary italic">The artist is at the easel…</p>
          } @else {
            <p class="qt-text-small qt-text-secondary italic">
              {{ imagesLoading() ? 'Fetching the portfolio…' : 'No picture yet' }}
            </p>
          }
          <ng-container *ngTemplateOutlet="buttons" />
        </div>
      }

      @if (currentImage()) {
        <div class="mt-2 flex flex-wrap items-center justify-between gap-2">
          <ng-container *ngTemplateOutlet="buttons" />
          @if (caption(); as text) {
            <span class="qt-text-xs qt-text-secondary" data-testid="wardrobe-item-image-caption">{{
              text
            }}</span>
          }
        </div>
      }

      @if (refusalNotice(); as notice) {
        <div
          role="alert"
          class="mt-2 rounded border qt-border-warning/50 qt-bg-warning/10 px-3 py-2 qt-text-small qt-text-warning"
        >
          {{ notice }}
        </div>
      }

      @if (pickerOpen() && !noProfiles()) {
        <div class="mt-2 qt-card py-2 px-3 qt-bg-muted/30 flex flex-wrap items-center gap-2">
          <label [for]="'wardrobe-item-image-profile-' + item().id" class="text-sm qt-text-secondary">
            Image model
          </label>
          <select
            [id]="'wardrobe-item-image-profile-' + item().id"
            class="qt-select flex-1 min-w-[12rem]"
            (change)="pickedProfileId.set($any($event.target).value || null)"
          >
            @for (p of profiles(); track p.id) {
              <option [value]="p.id" [selected]="effectivePickedProfileId() === p.id">
                {{ profileLabel(p) }}{{ p.id === designatedProfileId() ? ' · designated' : '' }}
              </option>
            }
          </select>
          <button
            type="button"
            class="qt-button-primary qt-button-sm"
            [disabled]="busy() || !effectivePickedProfileId()"
            (click)="generate(effectivePickedProfileId())"
          >
            {{ generating() ? 'Generating…' : 'Generate with this' }}
          </button>
          <button type="button" class="qt-button-ghost qt-button-sm" (click)="pickerOpen.set(false)">
            Cancel
          </button>
        </div>
      }

      @if (images().length > 0) {
        <div class="mt-2">
          <span class="qt-text-xs qt-text-secondary block mb-1">History</span>
          <ul class="flex flex-wrap gap-2" aria-label="Picture history">
            @for (img of images(); track img.fileId) {
              <li
                class="group relative rounded overflow-hidden border-2"
                [class.qt-border-primary]="img.fileId === currentId()"
                [class.qt-border-default]="img.fileId !== currentId()"
                data-testid="wardrobe-item-image-history-entry"
              >
                <img
                  [src]="thumbUrl(img.fileId)"
                  [alt]="img.fileId === currentId() ? 'Current picture' : 'Earlier picture'"
                  [attr.title]="img.prompt ?? null"
                  class="block w-16 h-16 object-cover qt-bg-muted"
                  loading="lazy"
                />
                <div
                  class="absolute inset-0 flex flex-col items-stretch justify-end gap-0.5 p-0.5 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity"
                >
                  @if (img.fileId !== currentId()) {
                    <button
                      type="button"
                      class="qt-button-secondary qt-button-sm !px-1 !py-0 qt-text-xs"
                      [disabled]="busy()"
                      title="Make current"
                      (click)="makeCurrent(img.fileId)"
                    >
                      Make current
                    </button>
                  }
                  <button
                    type="button"
                    class="qt-button-secondary qt-button-sm !px-1 !py-0 qt-text-xs qt-text-destructive"
                    [disabled]="busy()"
                    title="Delete this picture"
                    (click)="remove(img.fileId)"
                  >
                    Delete
                  </button>
                </div>
              </li>
            }
          </ul>
        </div>
      }

      <ng-template #buttons>
        <div class="flex flex-wrap items-center gap-2">
          <div class="inline-flex">
            <button
              type="button"
              class="qt-button-secondary qt-button-sm rounded-r-none"
              [disabled]="busy() || noProfiles()"
              [title]="
                noProfiles()
                  ? 'No image profiles are configured'
                  : 'Paint it with the designated wardrobe profile'
              "
              (click)="generate(null)"
            >
              <qt-icon name="sparkles" class="w-4 h-4 mr-1" />
              {{ generating() ? 'Generating…' : 'Generate' }}
            </button>
            <button
              type="button"
              class="qt-button-secondary qt-button-sm rounded-l-none border-l qt-border-default px-2"
              [disabled]="busy() || noProfiles()"
              aria-label="Choose an image profile"
              [attr.aria-expanded]="pickerOpen()"
              title="Choose another image profile, just this once"
              (click)="pickerOpen.set(!pickerOpen())"
            >
              <qt-icon name="chevron-down" class="w-4 h-4" />
            </button>
          </div>
          <button
            type="button"
            class="qt-button-secondary qt-button-sm"
            [disabled]="busy()"
            title="Hang a picture of your own (JPEG, PNG, WebP or GIF; 10 MB at most)"
            (click)="fileInput.click()"
          >
            <qt-icon name="upload" class="w-4 h-4 mr-1" />
            {{ uploading() ? 'Uploading…' : 'Upload' }}
          </button>
          <input
            #fileInput
            type="file"
            [accept]="accept"
            class="hidden"
            data-testid="wardrobe-item-image-upload-input"
            aria-label="Upload a picture"
            (change)="onFileChosen($event)"
          />
        </div>
      </ng-template>
    </section>
  `,
})
export class WardrobeItemImageActive {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly item = input.required<WardrobeItemDto>();
  /** The item's home container — the one its edit route is addressed through. */
  readonly container = input.required<WardrobeContainer>();
  /** After the current picture changes, so the lists can refresh. */
  readonly imageChanged = output<void>();

  protected readonly accept = WARDROBE_IMAGE_ACCEPT;
  protected readonly profileLabel = profileLabel;

  protected readonly pickerOpen = signal(false);
  protected readonly pickedProfileId = signal<string | null>(null);
  protected readonly refusalNotice = signal<string | null>(null);
  /** The last generation's answer — the only place `profile.name` and
   *  `rerouted` are known, so the caption uses it while it is still current. */
  private readonly lastGeneration = signal<WardrobeItemImageGenerateResponse | null>(null);

  private readonly imagesData = signal<WardrobeItemImagesResponse | null>(null);
  protected readonly imagesLoading = signal(true);

  protected readonly generating = signal(false);
  protected readonly uploading = signal(false);
  private readonly changing = signal(false);
  private readonly deleting = signal(false);

  private readonly profilesQuery = injectQuery(() => ({
    queryKey: imageProfileKeys.list(),
    queryFn: () => fetchImageProfiles(this.core),
  }));
  private readonly settingsQuery = injectQuery(() => ({
    queryKey: chatSettingsKeys.all,
    queryFn: () => fetchChatSettings(this.core),
  }));

  protected readonly profiles = computed<ImageProfileDto[]>(() => this.profilesQuery.data() ?? []);
  /** The designated wardrobe profile, read off the chat-settings row (v4 `:75-78`). */
  protected readonly designatedProfileId = computed(
    () => this.settingsQuery.data()?.wardrobeImageSettings?.imageProfileId ?? null,
  );
  /** v4 `:149-155` — designated (when present), else the default, else the first. */
  private readonly preselectedProfileId = computed(() => {
    const profiles = this.profiles();
    const designated = this.designatedProfileId();
    if (designated && profiles.some((p) => p.id === designated)) return designated;
    return (profiles.find((p) => p.isDefault) ?? profiles[0])?.id ?? null;
  });
  /** Until the operator picks, the picker shows the preselection. */
  protected readonly effectivePickedProfileId = computed(
    () => this.pickedProfileId() ?? this.preselectedProfileId(),
  );

  protected readonly images = computed<WardrobeItemImageSummary[]>(
    () => this.imagesData()?.images ?? [],
  );
  protected readonly currentId = computed(
    () => this.imagesData()?.current ?? this.item().imageFileId ?? null,
  );
  protected readonly currentImage = computed(
    () => this.images().find((i) => i.fileId === this.currentId()) ?? null,
  );
  private readonly isCatalogue = computed(() => this.container().scope !== 'character');

  protected readonly noProfiles = computed(() => this.profiles().length === 0);
  protected readonly busy = computed(
    () => this.generating() || this.uploading() || this.changing() || this.deleting(),
  );

  /** v4 `:262-278` — who drew the current picture, and whether it took the long way round. */
  protected readonly caption = computed(() => {
    const current = this.currentImage();
    if (!current) return null;
    const parts: string[] = [];
    const last = this.lastGeneration();
    if (last && last.current === current.fileId) {
      parts.push(`Drawn by ${last.profile.name}`);
      if (last.rerouted) parts.push('rerouted to the uncensored desk');
    } else if (current.source === 'GENERATED') {
      parts.push(current.model ? `Drawn by ${current.model}` : 'Drawn to order');
    } else if (current.source === 'IMPORTED') {
      parts.push('From the imported photograph');
    } else {
      parts.push('Hung by hand');
    }
    if (this.isCatalogue()) parts.push('catalogue shot');
    return parts.join(' · ');
  });

  constructor() {
    effect(() => {
      const item = this.item();
      const container = this.container();
      untracked(() => void this.loadImages(item.id, container));
    });
  }

  protected fullUrl(fileId: string): string {
    return wardrobeImageUrl(fileId);
  }

  protected thumbUrl(fileId: string): string {
    return wardrobeImageThumbnailUrl(fileId);
  }

  private async loadImages(itemId: string, container: WardrobeContainer): Promise<void> {
    try {
      const data = await listWardrobeItemImages(this.core, itemId, container);
      if (itemId !== this.item().id) return;
      this.imagesData.set(data);
    } catch {
      // A failed read leaves the frame empty — v4's query error renders the
      // same `No picture yet`; the buttons stay usable.
    } finally {
      if (itemId === this.item().id) this.imagesLoading.set(false);
    }
  }

  /** After any change to the picture set: refresh the history and tell the editor (v4 `:164-170`). */
  private async afterChange(): Promise<void> {
    await this.loadImages(this.item().id, this.container());
    this.imageChanged.emit();
  }

  protected async generate(imageProfileId: string | null): Promise<void> {
    if (this.busy()) return;
    this.refusalNotice.set(null);
    this.generating.set(true);
    try {
      const result = await generateWardrobeItemImage(this.item().id, this.container(), imageProfileId);
      this.lastGeneration.set(result);
      this.pickerOpen.set(false);
      this.toasts.showSuccess(
        result.rerouted
          ? 'The portrait is hung — drawn at the uncensored desk'
          : 'The portrait is hung',
      );
      await this.afterChange();
    } catch (error) {
      // v4 `:188-198` — a refusal that could not be rerouted names the artist
      // and opens the picker instead of toasting.
      if (
        error instanceof WardrobeImageRequestError &&
        error.status === 422 &&
        error.refusal?.refused !== false
      ) {
        const trail = error.refusal?.trail ?? [];
        const last = trail[trail.length - 1];
        const who = last?.profileName ?? 'The artist';
        const why = last?.detail ? ` (${last.detail})` : '';
        this.refusalNotice.set(`${who} declined to paint it${why}. Try another profile.`);
        this.pickerOpen.set(true);
        return;
      }
      this.toasts.showError(error instanceof Error ? error.message : 'Failed to generate a picture');
    } finally {
      this.generating.set(false);
    }
  }

  /** v4 `:241-255`. */
  protected onFileChosen(event: Event): void {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    // Reset so choosing the same file again still fires a change.
    input.value = '';
    if (!file) return;
    if (!ACCEPTED_TYPES.has(file.type)) {
      this.toasts.showError('Only JPEG, PNG, WebP or GIF pictures may be hung');
      return;
    }
    if (file.size > WARDROBE_IMAGE_MAX_BYTES) {
      this.toasts.showError('That picture weighs more than 10 MB; the easel will not bear it');
      return;
    }
    void this.upload(file);
  }

  private async upload(file: File): Promise<void> {
    this.uploading.set(true);
    try {
      await uploadWardrobeItemImage(this.item().id, this.container(), file, 'uploaded');
      this.lastGeneration.set(null);
      this.refusalNotice.set(null);
      this.toasts.showSuccess('Picture hung');
      await this.afterChange();
    } catch (error) {
      this.toasts.showError(error instanceof Error ? error.message : 'Failed to upload the picture');
    } finally {
      this.uploading.set(false);
    }
  }

  protected async makeCurrent(fileId: string): Promise<void> {
    this.changing.set(true);
    try {
      await setCurrentWardrobeItemImage(this.core, this.item().id, this.container(), fileId);
      await this.afterChange();
    } catch (error) {
      this.toasts.showError(error instanceof Error ? error.message : 'Failed to change the picture');
    } finally {
      this.changing.set(false);
    }
  }

  /** v4 `:257-260` — confirm, then take it down. */
  protected async remove(fileId: string): Promise<void> {
    if (!window.confirm('Take this picture down for good? It cannot be rehung.')) return;
    this.deleting.set(true);
    try {
      await deleteWardrobeItemImage(this.core, this.item().id, this.container(), fileId);
      this.toasts.showSuccess('Picture taken down');
      await this.afterChange();
    } catch (error) {
      this.toasts.showError(error instanceof Error ? error.message : 'Failed to delete the picture');
    } finally {
      this.deleting.set(false);
    }
  }
}

/**
 * Picture — the wardrobe item editor's image section (v4 `7c8572869`
 * `components/wardrobe/wardrobe-item-editor/WardrobeItemImageSection.tsx`,
 * read whole at the pin `f5e953a3f`). Sits directly under Title. Shows the
 * item's current picture (or an empty dashed frame), a Generate button with a
 * profile picker, an Upload button, a caption naming who drew the current
 * picture, and a history strip of every picture the item has had, each of
 * which can be made current or taken down.
 *
 * In create mode there is no item id to hang a picture on, so the section is
 * present but inert — and makes no request. Labelled **Picture** (R-F), not
 * the "Image section" the help and commit prose say.
 */
@Component({
  selector: 'qt-wardrobe-item-image-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [WardrobeItemImageActive],
  template: `
    @if (item() && container()) {
      <qt-wardrobe-item-image-active
        [item]="item()!"
        [container]="container()!"
        (imageChanged)="imageChanged.emit()"
      />
    } @else {
      <section aria-label="Picture" data-testid="wardrobe-item-image-section">
        <span class="qt-label mb-1 block">Picture</span>
        <div
          class="flex items-center justify-center rounded border-2 border-dashed qt-border-default px-4 py-6 text-center"
          data-testid="wardrobe-item-image-inert"
        >
          <p class="qt-text-small qt-text-secondary italic">
            Save the item first; then it may sit for its portrait.
          </p>
        </div>
      </section>
    }
  `,
})
export class WardrobeItemImageSection {
  /** The item being edited; null in create mode (the section is inert). */
  readonly item = input<WardrobeItemDto | null>(null);
  /** The item's home container — the one its edit route is addressed through. */
  readonly container = input<WardrobeContainer | null>(null);
  /** After the current picture changes, so the lists can refresh. */
  readonly imageChanged = output<void>();
}
