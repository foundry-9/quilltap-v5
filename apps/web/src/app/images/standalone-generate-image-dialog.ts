import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  OnInit,
  computed,
  inject,
  input,
  output,
  signal,
  viewChild,
} from '@angular/core';

import { CoreClient } from '../core/core-client';
import type { ParticipantDetail } from '../core/core-contract';
import { Modal } from '../ui/modal';
import { ImageProfilePicker } from './image-profile-picker';
import { fetchCharacterList } from '../screens/characters/characters.api';
import { insertPlaceholderAt } from '../screens/generate-image/placeholder-insert';
import type { GeneratedImage } from './generate-image-dialog';

/** One insertable character (v4 `useEntitySearch` `EntityOption`, `:9-13`). */
interface EntityOption {
  id: string;
  name: string;
}

/**
 * The standalone in-chat Generate Image dialog — a port of v4
 * `components/chat/StandaloneGenerateImageDialog.tsx`. Unlike the chat-profile
 * dialog it lets the user pick ANY image profile explicitly, offers the chat's
 * participants as one-click `{{placeholder}}` inserts alongside a search over
 * every character, and generates 1–4 images at a time. The generated images are
 * attached to the next message, and the conversation records a `generate_image`
 * tool result.
 *
 * **Do not conflate this with `generate-image-dialog.ts`** (v4
 * `GenerateImageDialog`), which takes an injected `imageProfileId`, shows no
 * picker, and hardcodes `count: 1`.
 *
 * Two string/shape details differ from the `/generate-image` screen and are
 * transcribed rather than harmonized, because v4 differs:
 *
 * - v4 sends the prompt **untrimmed** here (`:84`); the screen trims (`:116`).
 * - The empty-result message is "No images generated" (`:112`); the screen says
 *   "No images **were** generated" (`:140`). The error precedence is likewise
 *   reversed — `message || error` here (`:92`), `error || message` there
 *   (`:129`).
 *
 * v4's toasts become an inline notice (the `image-modal.ts` precedent).
 */
@Component({
  selector: 'qt-standalone-generate-image-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, ImageProfilePicker],
  template: `
    <qt-modal title="Generate Image" maxWidth="3xl" (close)="close.emit()">
      <div class="space-y-4">
        <!-- Image profile picker (v4 :153-162) -->
        <div class="mb-4">
          <label class="block text-sm qt-text-primary mb-2">Image Profile</label>
          <qt-image-profile-picker
            [value]="selectedProfileId()"
            [disabled]="generating()"
            (changed)="selectedProfileId.set($event)"
          />
        </div>

        <div class="flex gap-4">
          <!-- Left: quick inserts (v4 :164-209) -->
          <div class="w-48 flex-shrink-0 space-y-2">
            <div class="qt-text-small font-medium mb-3">Quick Insert</div>

            <!-- The user character inserts {{me}}, labelled with its name (v4 :172-181) -->
            @if (userCharacter(); as me) {
              <button
                type="button"
                (click)="insertPlaceholder('me')"
                [disabled]="generating()"
                class="w-full px-3 py-2 text-left text-sm qt-bg-primary/10 hover:qt-bg-primary/20 text-primary rounded border qt-border-primary/30 transition-colors"
              >
                <div class="font-medium">{{ me.name }}</div>
                <div class="text-xs opacity-75">{{ '{{me}}' }}</div>
              </button>
            }

            <!-- Each LLM-controlled character inserts its own name (v4 :183-196) -->
            @for (c of characterParticipants(); track c.id) {
              <button
                type="button"
                (click)="insertPlaceholder(c.name)"
                [disabled]="generating()"
                class="w-full px-3 py-2 text-left text-sm bg-accent hover:bg-accent/80 text-accent-foreground rounded border border-accent transition-colors"
              >
                <div class="font-medium truncate">{{ c.name }}</div>
                <div class="text-xs opacity-75">{{ '{{' + c.name + '}}' }}</div>
              </button>
            }

            <!-- The all-characters search (v4 EntitySearchDropdown) -->
            <div class="relative pt-4" #dropdownRef>
              <button
                type="button"
                (click)="dropdownOpen.set(!dropdownOpen())"
                [disabled]="generating()"
                class="w-full px-3 py-2 text-sm text-foreground qt-bg-muted hover:qt-bg-muted/80 rounded border border-input transition-colors flex items-center justify-between"
              >
                <span>Other Characters...</span>
              </button>

              @if (dropdownOpen()) {
                <div
                  class="absolute top-full left-0 right-0 mt-1 bg-background border qt-border-default rounded-lg qt-shadow-lg max-h-64 overflow-hidden flex flex-col z-10"
                >
                  <div class="p-2 border-b qt-border-default">
                    <input
                      type="text"
                      placeholder="Search..."
                      [value]="searchTerm()"
                      (input)="searchTerm.set($any($event.target).value)"
                      class="qt-input"
                    />
                  </div>
                  <div class="overflow-y-auto">
                    @for (entity of filteredEntities(); track entity.id) {
                      <button
                        type="button"
                        (click)="onEntitySelect(entity)"
                        class="w-full px-3 py-2 text-left text-sm qt-hover-accent flex items-center gap-2"
                      >
                        <span class="px-1.5 py-0.5 text-xs rounded bg-accent text-accent-foreground"
                          >C</span
                        >
                        <span class="text-foreground">{{ entity.name }}</span>
                      </button>
                    }
                    @if (filteredEntities().length === 0) {
                      <div class="px-3 py-4 qt-text-small text-center">No matches found</div>
                    }
                  </div>
                </div>
              }
            </div>
          </div>

          <!-- Right: the prompt (v4 :211-228) -->
          <div class="flex-1">
            <label for="standalone-image-prompt" class="block text-sm qt-text-primary mb-2">
              Image Prompt
            </label>
            <textarea
              #promptRef
              id="standalone-image-prompt"
              [value]="prompt()"
              (input)="prompt.set($any($event.target).value)"
              [disabled]="generating()"
              class="qt-textarea h-64"
              placeholder="Describe the image you want to generate. Use {{ '{{placeholders}}' }} for characters and personas."
            ></textarea>
            <div class="mt-2 qt-text-xs">
              Click buttons on the left or type {{ '{{name}}' }} to insert placeholders
            </div>
          </div>
        </div>

        @if (error(); as e) {
          <div class="qt-alert-error text-sm" role="alert">{{ e }}</div>
        }
      </div>

      <!-- Footer (v4 :232-280) -->
      <div qt-modal-footer class="flex items-center justify-between">
        <div class="flex items-center gap-2">
          <label for="standalone-image-count" class="text-sm qt-text-secondary">Images:</label>
          <select
            id="standalone-image-count"
            (change)="imageCount.set(+$any($event.target).value)"
            [disabled]="generating()"
            class="qt-input w-16"
          >
            @for (n of counts; track n) {
              <option [value]="n" [selected]="n === imageCount()">{{ n }}</option>
            }
          </select>
        </div>
        <div class="flex gap-3">
          <button
            type="button"
            class="qt-button qt-button-secondary"
            [disabled]="generating()"
            (click)="close.emit()"
          >
            Cancel
          </button>
          <button
            type="button"
            class="qt-button qt-button-primary"
            [disabled]="!canGenerate()"
            (click)="onGenerate()"
          >
            {{ generating() ? 'Generating...' : 'Generate Image' }}
          </button>
        </div>
      </div>
    </qt-modal>
  `,
})
export class StandaloneGenerateImageDialog implements OnInit {
  private readonly core = inject(CoreClient);

  readonly chatId = input.required<string>();
  readonly participants = input<ParticipantDetail[]>([]);

  readonly close = output<void>();
  /** v4 `onImagesGenerated` (`:25`) — the images plus the expanded prompt. */
  readonly generated = output<{ images: GeneratedImage[]; prompt: string }>();

  private readonly promptRef = viewChild<ElementRef<HTMLTextAreaElement>>('promptRef');
  private readonly dropdownRef = viewChild<ElementRef<HTMLElement>>('dropdownRef');

  /** v4 `:245-248` — a fixed 1..4. */
  protected readonly counts = [1, 2, 3, 4] as const;

  protected readonly selectedProfileId = signal<string | null>(null);
  protected readonly prompt = signal('');
  protected readonly imageCount = signal(1);
  protected readonly generating = signal(false);
  protected readonly error = signal<string | null>(null);
  protected readonly allEntities = signal<EntityOption[]>([]);
  protected readonly searchTerm = signal('');
  protected readonly dropdownOpen = signal(false);

  /** v4 `:261` — needs a profile, a non-blank prompt, and no run in flight. */
  protected readonly canGenerate = computed(
    () => !this.generating() && !!this.selectedProfileId() && this.prompt().trim().length > 0,
  );

  /** v4 `:131` — the user-controlled participant, if it has a character. */
  protected readonly userCharacter = computed<{ id: string; name: string } | null>(() => {
    const p = this.participants().find((x) => x.controlledBy === 'user');
    return p?.character ? { id: p.character.id, name: p.character.name } : null;
  });

  /** v4 `:130` — CHARACTER participants that the user does NOT control. */
  protected readonly characterParticipants = computed<{ id: string; name: string }[]>(() =>
    this.participants()
      .filter((p) => p.type === 'CHARACTER' && p.controlledBy !== 'user' && p.character)
      .map((p) => ({ id: p.id, name: p.character!.name })),
  );

  /** v4 `useEntitySearch:36-38` — case-insensitive contains, uncapped here. */
  protected readonly filteredEntities = computed<EntityOption[]>(() => {
    const term = this.searchTerm().toLowerCase();
    return this.allEntities().filter((e) => e.name.toLowerCase().includes(term));
  });

  ngOnInit(): void {
    void this.loadEntities();
    document.addEventListener('click', this.onDocumentClick, true);
  }

  /** v4 `useEntitySearch:46-48` closes the dropdown on an outside click. */
  private readonly onDocumentClick = (event: MouseEvent): void => {
    const host = this.dropdownRef()?.nativeElement;
    if (host && !host.contains(event.target as Node)) this.dropdownOpen.set(false);
  };

  /** v4 `useEntitySearch:20-34` — every character, sorted by name. */
  private async loadEntities(): Promise<void> {
    try {
      const characters = await fetchCharacterList(this.core);
      const entities = characters.map((c) => ({ id: c.id, name: c.name }));
      entities.sort((a, b) => a.name.localeCompare(b.name));
      this.allEntities.set(entities);
    } catch {
      this.allEntities.set([]);
    }
  }

  /** v4 `useEntitySearch:40-44` — insert, then close and clear the search. */
  protected onEntitySelect(entity: EntityOption): void {
    this.insertPlaceholder(entity.name);
    this.dropdownOpen.set(false);
    this.searchTerm.set('');
  }

  /**
   * v4 `:43-62`. Note this copy does NOT clear the search term or close the
   * dropdown (the page's `:91-92` does) — that cleanup lives in
   * `handleEntitySelect` instead, so a quick-insert button leaves the dropdown
   * exactly as it was.
   */
  protected insertPlaceholder(text: string): void {
    const textarea = this.promptRef()?.nativeElement;
    if (!textarea) return;
    const result = insertPlaceholderAt(
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
  }

  /** v4 `:64-125`. */
  protected async onGenerate(): Promise<void> {
    const prompt = this.prompt();
    // v4 checks the prompt FIRST here (`:65`), the profile second (`:70`) —
    // the reverse of the /generate-image screen. The messages differ too.
    if (!prompt.trim()) {
      this.error.set('Please enter a prompt');
      return;
    }
    const imageProfileId = this.selectedProfileId();
    if (!imageProfileId) {
      this.error.set('Please select an image profile');
      return;
    }

    this.generating.set(true);
    this.error.set(null);
    try {
      const resp = await this.core.dispatch({
        type: 'imageProfileGenerate',
        imageProfileId,
        // v4 `:84` sends the prompt UNTRIMMED here.
        prompt,
        chatId: this.chatId(),
        count: this.imageCount(),
      });
      if (resp.type === 'error') {
        this.error.set(resp.data.message || 'Failed to generate image');
        return;
      }

      const body = (resp.data ?? {}) as {
        success?: boolean;
        data?: GeneratedImage[];
        expandedPrompt?: string;
      };
      if (body.success && body.data && body.data.length > 0) {
        // v4 `:102` prefers the server's expanded prompt for the tool result.
        const finalPrompt = body.expandedPrompt || prompt;
        this.prompt.set('');
        this.generated.emit({ images: body.data, prompt: finalPrompt });
        this.close.emit();
      } else {
        // v4 `:112` — note the wording differs from the screen's.
        this.error.set('No images generated');
      }
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'Failed to generate image');
    } finally {
      this.generating.set(false);
    }
  }
}
