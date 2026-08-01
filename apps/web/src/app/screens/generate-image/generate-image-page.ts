import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  OnInit,
  computed,
  inject,
  signal,
  viewChild,
} from '@angular/core';
import { RouterLink } from '@angular/router';

import { CoreClient } from '../../core/core-client';
import { ImageProfilePicker } from '../../images/image-profile-picker';
import { ImageModal } from '../../images/image-modal';
import { fileUrl } from '../../images/image-urls';
import { Icon } from '../../ui/icon';
import { fetchCharacterList } from '../characters/characters.api';
import { insertPlaceholderAt, type PlaceholderInsert } from './placeholder-insert';
import { ToastService } from '../../ui/toast.service';

/** One generated image (v4 `GenerateImageView` `GeneratedImage`, `:27-34`). */
export interface GeneratedImage {
  id: string;
  filename: string;
  filepath: string;
  mimeType: string;
  generationPrompt?: string;
  generationRevisedPrompt?: string;
}

/** One insertable character (v4 `EntityOption`, `:21-25`). */
interface EntityOption {
  id: string;
  name: string;
  type: 'character';
}

/**
 * The standalone Generate Image screen — a port of v4
 * `app/generate-image/GenerateImageView.tsx`. Image generation outside any
 * conversation: pick a profile, compose a prompt (with `{{Character}}`
 * placeholder inserts), choose how many, and generate.
 *
 * Three v4 behaviors this port carries deliberately, because they read as bugs
 * if you don't know they're intentional:
 *
 * - **No auto-selected profile.** `selectedProfileId` starts null (v4 `:37`) and
 *   the picker leads with "No image generation"; `[Default]` is a label, not a
 *   preselection. Generate stays disabled until the user picks (v4 `:282`).
 * - **The gallery is in-memory only** (v4 `:41`). Navigate away and it is gone —
 *   there is no localStorage anywhere in this flow and no server-side "save"
 *   from this screen. Generated images do persist server-side on their own (the
 *   Lantern Backgrounds `tool/` mount), with no album association and — because
 *   this screen sends no `chatId` — no notification.
 * - **The whole user-facing parameter surface is prompt + count.** `size`,
 *   `quality`, `style`, `aspectRatio` and `negativePrompt` live only in the API
 *   schema and the profile's own defaults, merged server-side. v5's
 *   `imageProfileGenerate` narrowing (`imageProfileId`/`prompt`/`chatId`/`count`)
 *   therefore covers this screen exactly. No cost display, in v4 or here.
 *
 * Every outcome is v4's own toast, with v4's copy
 * (`GenerateImageView.tsx:103-164`).
 *
 * **One byte-path divergence from v4, deliberate.** v4 renders, previews and
 * downloads straight from `image.filepath` because in v4 that string is a public
 * servable path. In v5 `filepath` is a STORE-RELATIVE path (`chat/pic.png`) and
 * is not fetchable; bytes are served id-keyed through
 * `GET /api/v1/files/{id}`. Every image URL here therefore goes through
 * {@link fileUrl}, which is also what routes it at the right origin under Tauri's
 * one-origin shell — the same substitution `chat/message-row.ts` makes.
 *
 * **Deferred (loud, named): the workspace-tab mount.** v4's `page.tsx:11`
 * calls `redirectToWorkspaceTab('generate-image')` before rendering this view;
 * the tabbed workspace shell is `p4.9j`'s whole scope and is NOT ported here.
 */
@Component({
  selector: 'qt-generate-image-page',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, Icon, ImageProfilePicker, ImageModal],
  template: `
    <div class="qt-page-container max-w-4xl mx-auto">
      <!-- Header (v4 :171-176) -->
      <div class="flex items-center justify-between mb-6">
        <h1 class="qt-heading-2 text-foreground">Generate Image</h1>
        <a routerLink="/" class="text-sm qt-text-secondary hover:text-foreground">
          &larr; Back to Home
        </a>
      </div>

      <!-- Generation form (v4 :179-302) -->
      <div class="qt-card p-6 mb-6">
        <div class="space-y-4">
          <div>
            <label class="block qt-label text-foreground mb-2">Image Profile</label>
            <!-- v4 :186-189 passes value/onChange ONLY — no character sort here. -->
            <qt-image-profile-picker
              [value]="selectedProfileId()"
              (changed)="selectedProfileId.set($event)"
            />
          </div>

          <div>
            <label for="generate-image-prompt" class="block qt-label text-foreground mb-2">
              Prompt
            </label>
            <div class="relative">
              <textarea
                #promptRef
                id="generate-image-prompt"
                [value]="prompt()"
                (input)="prompt.set($any($event.target).value)"
                placeholder="Describe the image you want to generate... Use {{ '{{' }}CharacterName{{
                  '}}'
                }} to include character descriptions."
                class="w-full h-32 px-3 py-2 border qt-border-default rounded-lg qt-bg-card text-foreground resize-none focus:outline-none focus:ring-2 focus:ring-ring"
              ></textarea>

              <!-- Character placeholder insert widget (v4 :207-256) -->
              <div class="relative mt-2" #dropdownRef>
                <div class="flex items-center gap-2">
                  <input
                    type="text"
                    [value]="searchTerm()"
                    (input)="onSearchInput($any($event.target).value)"
                    (focus)="dropdownOpen.set(true)"
                    placeholder="Search characters to insert..."
                    class="flex-1 px-3 py-2 text-sm border qt-border-default rounded-lg qt-bg-card text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                  />
                  <button
                    type="button"
                    (click)="insertPlaceholder('me')"
                    class="qt-button qt-button-secondary qt-button-sm"
                    title="Insert {{ '{{' }}me{{ '}}' }} placeholder"
                  >
                    me
                  </button>
                  <button
                    type="button"
                    (click)="insertPlaceholder('char')"
                    class="qt-button qt-button-secondary qt-button-sm"
                    title="Insert {{ '{{' }}char{{ '}}' }} placeholder"
                  >
                    char
                  </button>
                </div>

                <!-- v4 :239-255: open, and only when something matched. -->
                @if (dropdownOpen() && visibleEntities().length > 0) {
                  <div
                    class="absolute z-10 w-full mt-1 max-h-48 overflow-y-auto qt-bg-card border qt-border-default rounded-lg qt-shadow-lg"
                  >
                    @for (entity of visibleEntities(); track entity.id) {
                      <button
                        type="button"
                        (click)="insertPlaceholder(entity.name)"
                        class="w-full px-3 py-2 text-left text-sm hover:qt-bg-muted transition-colors flex items-center gap-2"
                      >
                        <span class="text-xs px-1.5 py-0.5 qt-bg-muted rounded">{{
                          entity.type
                        }}</span>
                        <span>{{ entity.name }}</span>
                      </button>
                    }
                  </div>
                }
              </div>
            </div>
          </div>

          <!-- Image count (v4 :261-276) — a static 1-4 list, so [value] is safe. -->
          <div>
            <label for="generate-image-count" class="block qt-label text-foreground mb-2">
              Number of Images
            </label>
            <select
              id="generate-image-count"
              (change)="imageCount.set(+$any($event.target).value)"
              class="px-3 py-2 border qt-border-default rounded-lg qt-bg-card text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
            >
              @for (n of counts; track n) {
                <option [value]="n" [selected]="n === imageCount()">
                  {{ n }} image{{ n > 1 ? 's' : '' }}
                </option>
              }
            </select>
          </div>

          <!-- Generate (v4 :279-300) -->
          <div class="pt-2">
            <button
              type="button"
              (click)="onGenerate()"
              [disabled]="!canGenerate()"
              class="qt-button qt-button-primary w-full sm:w-auto disabled:opacity-50 disabled:cursor-not-allowed"
            >
              @if (generating()) {
                Generating...
              } @else {
                <qt-icon name="image" class="w-4 h-4 mr-2" />
                Generate Image
              }
            </button>
          </div>
        </div>
      </div>

      <!-- Results gallery (v4 :305-345) -->
      @if (generatedImages().length > 0) {
        <div class="qt-card p-6">
          <h2 class="qt-heading-4 text-foreground mb-4">
            Generated Images ({{ generatedImages().length }})
          </h2>
          <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
            @for (image of generatedImages(); track image.id) {
              <div class="relative group">
                <img
                  [src]="srcFor(image)"
                  [alt]="image.generationPrompt || 'Generated image'"
                  class="w-full aspect-square object-cover rounded-lg border qt-border-default"
                />
                <div
                  class="absolute inset-0 qt-bg-overlay-medium opacity-0 group-hover:opacity-100 transition-opacity rounded-lg flex items-center justify-center gap-2"
                >
                  <button
                    type="button"
                    (click)="onDownload(image)"
                    class="p-2 qt-bg-overlay-btn hover:qt-bg-overlay-btn rounded-full transition-colors"
                    title="Download"
                  >
                    <qt-icon name="download" class="w-5 h-5 qt-text-overlay" />
                  </button>
                  <button
                    type="button"
                    (click)="previewImage.set({ src: srcFor(image), filename: image.filename })"
                    class="p-2 qt-bg-overlay-btn hover:qt-bg-overlay-btn rounded-full transition-colors"
                    title="View image"
                  >
                    <qt-icon name="eye" class="w-5 h-5 qt-text-overlay" />
                  </button>
                </div>
                <!-- Prompt tooltip (v4 :336-339): gated on generationPrompt,
                     but PREFERS the revised prompt when the provider sent one. -->
                @if (image.generationPrompt) {
                  <div
                    class="absolute bottom-0 left-0 right-0 p-2 qt-bg-overlay-medium qt-text-overlay text-xs rounded-b-lg opacity-0 group-hover:opacity-100 transition-opacity line-clamp-2"
                  >
                    {{ image.generationRevisedPrompt || image.generationPrompt }}
                  </div>
                }
              </div>
            }
          </div>
        </div>
      }

      <!-- Preview lightbox (v4 :348-355) -->
      @if (previewImage(); as preview) {
        <qt-image-modal
          [src]="preview.src"
          [filename]="preview.filename"
          (close)="previewImage.set(null)"
        />
      }

      <!-- Empty state (v4 :358-364) -->
      @if (generatedImages().length === 0) {
        <div class="qt-card p-8 text-center qt-text-secondary">
          <qt-icon name="image" class="w-16 h-16 mx-auto mb-4 opacity-50" />
          <p class="qt-text-section">No images generated yet</p>
          <p class="text-sm mt-1">
            Enter a prompt and click Generate to create your first image
          </p>
        </div>
      }
    </div>
  `,
})
export class GenerateImagePage implements OnInit {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  private readonly promptRef = viewChild<ElementRef<HTMLTextAreaElement>>('promptRef');
  private readonly dropdownRef = viewChild<ElementRef<HTMLElement>>('dropdownRef');

  /** v4 `:270` — the count options are a fixed 1..4. */
  protected readonly counts = [1, 2, 3, 4] as const;

  protected readonly selectedProfileId = signal<string | null>(null);
  protected readonly prompt = signal('');
  protected readonly imageCount = signal(1);
  protected readonly generating = signal(false);
  protected readonly generatedImages = signal<GeneratedImage[]>([]);
  protected readonly allEntities = signal<EntityOption[]>([]);
  protected readonly searchTerm = signal('');
  protected readonly dropdownOpen = signal(false);
  protected readonly previewImage = signal<{ src: string; filename: string } | null>(null);

  /** v4 `:282`: disabled while generating, or without both a profile and a prompt. */
  protected readonly canGenerate = computed(
    () => !this.generating() && !!this.selectedProfileId() && this.prompt().trim().length > 0,
  );

  /** v4 `:95-97` filter, then the `:241` cap of ten rows. */
  protected readonly visibleEntities = computed<EntityOption[]>(() => {
    const term = this.searchTerm().toLowerCase();
    return this.allEntities()
      .filter((e) => e.name.toLowerCase().includes(term))
      .slice(0, 10);
  });

  ngOnInit(): void {
    void this.loadEntities();
    // v4 `:99` closes the dropdown on any outside click (`useClickOutside`).
    document.addEventListener('click', this.onDocumentClick, true);
  }

  private readonly onDocumentClick = (event: MouseEvent): void => {
    const host = this.dropdownRef()?.nativeElement;
    if (host && !host.contains(event.target as Node)) this.dropdownOpen.set(false);
  };

  /** v4 `:49-69`: load every character, sorted by name, for the insert list. */
  private async loadEntities(): Promise<void> {
    try {
      const characters = await fetchCharacterList(this.core);
      const entities: EntityOption[] = characters.map((c) => ({
        id: c.id,
        name: c.name,
        type: 'character' as const,
      }));
      entities.sort((a, b) => a.name.localeCompare(b.name));
      this.allEntities.set(entities);
    } catch {
      // v4 leaves the list empty on a failed read — the quick-insert buttons
      // ({{me}}/{{char}}) still work, so the screen stays usable.
      this.allEntities.set([]);
    }
  }

  protected onSearchInput(value: string): void {
    // v4 `:212-215` opens the dropdown on every keystroke.
    this.searchTerm.set(value);
    this.dropdownOpen.set(true);
  }

  /** v4 `:71-93` — splice `{{text}}` over the selection, then restore the caret. */
  protected insertPlaceholder(text: string): void {
    const textarea = this.promptRef()?.nativeElement;
    if (!textarea) return;
    const result: PlaceholderInsert = insertPlaceholderAt(
      this.prompt(),
      text,
      textarea.selectionStart,
      textarea.selectionEnd,
    );
    this.prompt.set(result.value);
    setTimeout(() => {
      textarea.focus();
      textarea.setSelectionRange(result.caret, result.caret);
    }, 0);
    // v4 `:91-92` clears the search and closes the dropdown after an insert.
    this.searchTerm.set('');
    this.dropdownOpen.set(false);
  }

  /** v4 `:101-149`. */
  protected async onGenerate(): Promise<void> {
    const imageProfileId = this.selectedProfileId();
    if (!imageProfileId) {
      this.toasts.showError('Please select an image profile');
      return;
    }
    const prompt = this.prompt().trim();
    if (!prompt) {
      this.toasts.showError('Please enter a prompt');
      return;
    }

    this.generating.set(true);
    try {
      // v4 `:115-124` sends prompt + count and NO chatId — that omission is what
      // makes the run standalone (no chat notification, no album association).
      const resp = await this.core.dispatch({
        type: 'imageProfileGenerate',
        imageProfileId,
        prompt,
        count: this.imageCount(),
      });
      if (resp.type === 'error') throw new Error(resp.data.message);

      const body = (resp.data ?? {}) as { data?: GeneratedImage[] };
      const images = body.data ?? [];
      if (images.length > 0) {
        // v4 `:137` prepends, newest first.
        this.generatedImages.update((prev) => [...images, ...prev]);
        this.toasts.showSuccess(`Generated ${images.length} image${images.length > 1 ? 's' : ''}`);
      } else {
        // v4 `:140` — a 200 carrying nothing is still an error to the user.
        this.toasts.showError('No images were generated');
      }
    } catch (err) {
      this.toasts.showError(err instanceof Error && err.message ? err.message : 'Failed to generate image');
    } finally {
      this.generating.set(false);
    }
  }

  /** The servable URL for a generated image — see the byte-path note above. */
  protected srcFor(image: GeneratedImage): string {
    return fileUrl(image.id);
  }

  /**
   * v4 `:151-166` — a client-side blob download. There is no server "save" from
   * this screen; the bytes are fetched back and handed to an anchor.
   */
  protected async onDownload(image: GeneratedImage): Promise<void> {
    try {
      const res = await fetch(this.srcFor(image));
      const blob = await res.blob();
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = image.filename || 'generated-image.png';
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    } catch {
      this.toasts.showError('Failed to download image');
    }
  }
}
