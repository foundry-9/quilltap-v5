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
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../core/core-client';
import type { SearchReplaceScopeWire } from '../../core/core-contract';
import { characterKeys, fetchCharacterList } from '../../screens/characters/characters.api';
import { Modal } from '../../ui/modal';
import { searchReplaceExecute, searchReplacePreview } from '../chat-admin.api';

/** v4 `SearchReplacePreview` (`search-replace/types.ts:33-38`). */
export interface SearchReplacePreviewCounts {
  messageMatches: number;
  memoryMatches: number;
  affectedChats: number;
  affectedMemories: number;
}

/** v4 `SearchReplaceResult` (`types.ts:43-48`). */
export interface SearchReplaceResultCounts {
  messagesUpdated: number;
  memoriesUpdated: number;
  chatsAffected: number;
  errors: string[];
}

/** v4 `WIZARD_STEPS` (`types.ts:60-66`) — the id order IS the navigation order. */
export const WIZARD_STEPS = [
  { id: 'scope', title: 'Select Scope' },
  { id: 'search', title: 'Search & Replace' },
  { id: 'confirm', title: 'Confirm Changes' },
  { id: 'processing', title: 'Processing' },
  { id: 'results', title: 'Results' },
] as const;

export type WizardStepId = (typeof WIZARD_STEPS)[number]['id'];

/** v4 debounces the preview by 300 ms (`useSearchReplace.ts:102-119`). */
const PREVIEW_DEBOUNCE_MS = 300;

/**
 * Search & Replace (v4 `components/tools/search-replace/**`, ~1,100 LOC across a
 * modal, a hook and five step components) — the Edit Content drawer's "Replace".
 *
 * v5 keeps the five steps but not the five files: the hook's state is signals on
 * this component, which is where v5 puts dialog state everywhere else.
 *
 * ## Ported exactly, and why
 *
 * - **The step indicator shows only the FIRST THREE steps** (`SearchReplaceModal.tsx:173`)
 *   — processing and results replace the whole body rather than advancing a bar.
 * - **"Next" on the confirm step EXECUTES** rather than advancing
 *   (`useSearchReplace.ts:288-292`), which is why the button reads "Replace All"
 *   there.
 * - **Going back clears the confirmation tick** (`:187`), so a changed search
 *   cannot inherit an old consent.
 * - **`canProceed` on the search step needs a preview with matches**
 *   (`:213-215`): a search that found nothing cannot be carried forward, even
 *   with text in the box.
 * - **The preview is debounced 300 ms** and refetched when the flags move, not
 *   only when the text does (`:102-119`).
 * - **A failed execute still lands on the results step** (`:270-274`) — it
 *   reports rather than stranding the operator mid-wizard.
 * - The dialog is sealed while executing (`:167-169`).
 *
 * ⚠ **The two include flags default TRUE on the server** (§1), so they are
 * always sent explicitly; an omission would read as "included" and mean the
 * opposite of an unticked box.
 */
@Component({
  selector: 'qt-search-replace-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal
      title="Search &amp; Replace"
      maxWidth="xl"
      [closeOnBackdrop]="!executing()"
      (close)="onClose()"
    >
      @if (step() !== 'processing' && step() !== 'results') {
        <div class="mb-6">
          <div class="flex items-center justify-between">
            @for (s of indicatorSteps; track s.id; let i = $index) {
              <div class="flex items-center flex-1">
                <div class="flex items-center">
                  <div
                    [class]="
                      'w-8 h-8 rounded-full flex items-center justify-center qt-label ' +
                      (i <= stepIndex()
                        ? 'bg-primary text-primary-foreground'
                        : 'qt-bg-muted qt-text-secondary')
                    "
                  >
                    {{ i < stepIndex() ? '✓' : i + 1 }}
                  </div>
                  <span
                    [class]="
                      'ml-2 text-sm hidden sm:inline ' +
                      (s.id === step() ? 'qt-text-primary font-medium' : 'qt-text-secondary')
                    "
                    >{{ s.title }}</span
                  >
                </div>
                @if (i < 2) {
                  <div
                    [class]="'flex-1 h-0.5 mx-4 ' + (i < stepIndex() ? 'bg-primary' : 'qt-bg-muted')"
                  ></div>
                }
              </div>
            }
          </div>
        </div>
      }

      @switch (step()) {
        @case ('scope') {
          <div class="space-y-6">
            <div>
              <h3 class="qt-text-section mb-2">Select Scope</h3>
              <p class="qt-text-secondary text-sm">Choose where to search and replace text.</p>
            </div>
            <div class="space-y-3">
              <label
                [class]="
                  'flex items-center p-4 rounded-lg border-2 cursor-pointer transition-colors ' +
                  (scopeType() === 'chat'
                    ? 'qt-border-primary qt-bg-primary/10'
                    : 'qt-border-default bg-background hover:qt-border-primary/50')
                "
              >
                <input
                  type="radio"
                  name="scopeType"
                  class="mr-3"
                  aria-label="Current Chat"
                  [checked]="scopeType() === 'chat'"
                  (change)="scopeType.set('chat')"
                />
                <div>
                  <div class="font-medium qt-text-primary">Current Chat</div>
                  <div class="text-sm qt-text-secondary">
                    {{ chatTitle() || 'This conversation only' }}
                  </div>
                </div>
              </label>

              <label
                [class]="
                  'flex items-center p-4 rounded-lg border-2 cursor-pointer transition-colors ' +
                  (scopeType() === 'character'
                    ? 'qt-border-primary qt-bg-primary/10'
                    : 'qt-border-default bg-background hover:qt-border-primary/50')
                "
              >
                <input
                  type="radio"
                  name="scopeType"
                  class="mr-3"
                  aria-label="All Chats for Character"
                  [checked]="scopeType() === 'character'"
                  (change)="scopeType.set('character')"
                />
                <div class="flex-1">
                  <div class="font-medium qt-text-primary">All Chats for Character</div>
                  <div class="text-sm qt-text-secondary">
                    Search across all conversations with a specific character
                  </div>
                </div>
              </label>

              @if (scopeType() === 'character') {
                <div class="ml-8 mt-2">
                  <select
                    class="qt-input w-full"
                    aria-label="Select a character"
                    [value]="characterId()"
                    (change)="characterId.set($any($event.target).value)"
                  >
                    <option value="">Select a character...</option>
                    @for (c of characters(); track c.id) {
                      <option [value]="c.id" [selected]="characterId() === c.id">
                        {{ c.name }}
                      </option>
                    }
                  </select>
                </div>
              }
            </div>
          </div>
        }

        @case ('search') {
          <div class="space-y-6">
            <div>
              <h3 class="qt-text-section mb-2">Search &amp; Replace</h3>
              <p class="qt-text-secondary text-sm">
                Enter the text you want to find and what to replace it with.
              </p>
            </div>

            <div>
              <label class="qt-label block mb-2" for="qt-sr-find">Find</label>
              <input
                id="qt-sr-find"
                type="text"
                class="qt-input w-full"
                placeholder="Text to search for..."
                [value]="searchText()"
                (input)="searchText.set($any($event.target).value)"
              />
            </div>

            <div>
              <label class="qt-label block mb-2" for="qt-sr-replace">Replace with</label>
              <input
                id="qt-sr-replace"
                type="text"
                class="qt-input w-full"
                placeholder="Replacement text (leave empty to delete)"
                [value]="replaceText()"
                (input)="replaceText.set($any($event.target).value)"
              />
            </div>

            <div class="space-y-3">
              <label class="flex items-center gap-2 cursor-pointer">
                <input
                  type="checkbox"
                  class="qt-checkbox"
                  aria-label="Include chat messages"
                  [checked]="includeMessages()"
                  (change)="includeMessages.set($any($event.target).checked)"
                />
                <span class="qt-text-primary">Include chat messages</span>
              </label>
              <label class="flex items-center gap-2 cursor-pointer">
                <input
                  type="checkbox"
                  class="qt-checkbox"
                  aria-label="Include memories"
                  [checked]="includeMemories()"
                  (change)="includeMemories.set($any($event.target).checked)"
                />
                <span class="qt-text-primary">Include memories</span>
              </label>
            </div>

            <div class="p-4 rounded-lg qt-bg-muted/50">
              <div class="font-medium qt-text-primary mb-2">Preview</div>
              @if (!searchText().trim()) {
                <p class="text-sm qt-text-secondary">Enter search text to see matches</p>
              } @else if (loadingPreview()) {
                <p class="text-sm qt-text-secondary">Searching...</p>
              } @else if (previewError(); as message) {
                <p class="text-sm qt-text-destructive">{{ message }}</p>
              } @else if (preview(); as counts) {
                <div class="space-y-2 text-sm">
                  @if (totalMatches() === 0) {
                    <p class="qt-text-secondary">No matches found</p>
                  } @else {
                    @if (includeMessages()) {
                      <div class="flex justify-between">
                        <span class="qt-text-secondary">Messages:</span>
                        <span class="qt-text-primary font-medium">
                          {{ counts.messageMatches }} matches in {{ counts.affectedChats }} chat{{
                            counts.affectedChats !== 1 ? 's' : ''
                          }}
                        </span>
                      </div>
                    }
                    @if (includeMemories()) {
                      <div class="flex justify-between">
                        <span class="qt-text-secondary">Memories:</span>
                        <span class="qt-text-primary font-medium">
                          {{ counts.memoryMatches }} match{{
                            counts.memoryMatches !== 1 ? 'es' : ''
                          }}
                        </span>
                      </div>
                    }
                    <div class="pt-2 border-t qt-border-default">
                      <div class="flex justify-between font-medium">
                        <span class="qt-text-primary">Total:</span>
                        <span class="text-primary"
                          >{{ totalMatches() }} match{{ totalMatches() !== 1 ? 'es' : '' }}</span
                        >
                      </div>
                    </div>
                  }
                </div>
              }
            </div>
          </div>
        }

        @case ('confirm') {
          <div class="space-y-6">
            <div>
              <h3 class="qt-text-section mb-2">Confirm Changes</h3>
              <p class="qt-text-secondary text-sm">Review the changes before proceeding.</p>
            </div>

            <div class="p-4 rounded-lg border qt-border-default bg-background">
              <div class="space-y-3">
                <div>
                  <div class="text-sm qt-text-secondary">Find:</div>
                  <div class="font-mono qt-text-primary qt-bg-muted px-2 py-1 rounded mt-1">
                    &quot;{{ searchText() }}&quot;
                  </div>
                </div>
                <div>
                  <div class="text-sm qt-text-secondary">Replace with:</div>
                  <div class="font-mono qt-text-primary qt-bg-muted px-2 py-1 rounded mt-1">
                    @if (replaceText()) {
                      &quot;{{ replaceText() }}&quot;
                    } @else {
                      <span class="italic qt-text-secondary">(delete)</span>
                    }
                  </div>
                </div>
              </div>
            </div>

            <div class="p-4 rounded-lg qt-bg-destructive/10 border qt-border-destructive/20">
              <div class="flex gap-3">
                <div class="qt-text-destructive text-xl">⚠️</div>
                <div>
                  <div class="font-medium qt-text-destructive mb-1">
                    This action cannot be undone
                  </div>
                  <p class="text-sm qt-text-destructive/80">
                    The changes will be applied immediately and permanently. Make sure you have
                    reviewed the search and replace text carefully.
                  </p>
                </div>
              </div>
            </div>

            <label class="flex items-start gap-3 cursor-pointer">
              <input
                type="checkbox"
                class="qt-checkbox mt-1"
                aria-label="I understand"
                [checked]="confirmed()"
                (change)="confirmed.set($any($event.target).checked)"
              />
              <span class="qt-text-primary">
                I understand that this action cannot be undone and want to proceed with the
                replacement.
              </span>
            </label>
          </div>
        }

        @case ('processing') {
          <div class="space-y-6 py-8 text-center">
            <h3 class="qt-text-section mb-2">Processing...</h3>
            <p class="qt-text-secondary text-sm">
              Please wait while changes are being applied.
            </p>
            <p class="text-xs qt-text-secondary">
              Do not close this window until the operation completes.
            </p>
          </div>
        }

        @case ('results') {
          <div class="space-y-6 py-4">
            @if (error(); as message) {
              <div class="flex flex-col items-center justify-center">
                <h3 class="qt-text-destructive qt-text-section mb-2">Operation Failed</h3>
                <p class="qt-text-secondary text-sm text-center max-w-md">{{ message }}</p>
              </div>
            } @else if (result(); as res) {
              <div class="flex flex-col items-center justify-center">
                <h3 class="qt-text-section mb-2">
                  {{ res.errors.length > 0 ? 'Completed with Warnings' : 'Operation Complete' }}
                </h3>
                <p class="qt-text-secondary text-sm text-center">
                  Successfully updated {{ totalUpdated() }} item{{
                    totalUpdated() !== 1 ? 's' : ''
                  }}.
                </p>
              </div>

              <div class="p-4 rounded-lg qt-bg-muted/50">
                <div class="space-y-2 text-sm">
                  @if (res.messagesUpdated > 0) {
                    <div class="flex justify-between">
                      <span class="qt-text-secondary">Messages updated:</span>
                      <span class="qt-text-primary font-medium">
                        {{ res.messagesUpdated }} in {{ res.chatsAffected }} chat{{
                          res.chatsAffected !== 1 ? 's' : ''
                        }}
                      </span>
                    </div>
                  }
                  @if (res.memoriesUpdated > 0) {
                    <div class="flex justify-between">
                      <span class="qt-text-secondary">Memories updated:</span>
                      <span class="qt-text-primary font-medium">{{ res.memoriesUpdated }}</span>
                    </div>
                  }
                  <div class="pt-2 border-t qt-border-default">
                    <div class="flex justify-between font-medium">
                      <span class="qt-text-primary">Total:</span>
                      <span class="text-primary">{{ totalUpdated() }} updated</span>
                    </div>
                  </div>
                </div>
              </div>

              @if (res.errors.length > 0) {
                <div class="p-4 rounded-lg qt-bg-destructive/10 border qt-border-destructive/20">
                  <div class="font-medium qt-text-destructive mb-2">
                    Warnings ({{ res.errors.length }})
                  </div>
                  <ul class="text-sm qt-text-destructive/80 space-y-1 list-disc list-inside">
                    @for (err of res.errors; track $index) {
                      <li>{{ err }}</li>
                    }
                  </ul>
                </div>
              }
            } @else {
              <div class="flex flex-col items-center justify-center">
                <p class="qt-text-secondary">No results available</p>
              </div>
            }
          </div>
        }
      }

      <div qt-modal-footer class="w-full">
        @if (step() === 'results') {
          <div class="flex justify-end">
            <button type="button" class="qt-button qt-button-primary" (click)="onClose()">
              Close
            </button>
          </div>
        } @else if (step() !== 'processing') {
          <div class="flex justify-between">
            <button
              type="button"
              class="qt-button qt-button-secondary"
              [disabled]="executing()"
              (click)="canGoBack() ? prev() : onClose()"
            >
              {{ canGoBack() ? 'Back' : 'Cancel' }}
            </button>
            <button
              type="button"
              class="qt-button qt-button-primary"
              [disabled]="!canProceed() || executing()"
              (click)="next()"
            >
              {{ step() === 'confirm' ? 'Replace All' : 'Next' }}
            </button>
          </div>
        }
      </div>
    </qt-modal>
  `,
})
export class SearchReplaceModal {
  private readonly core = inject(CoreClient);
  private readonly destroyRef = inject(DestroyRef);

  /** The chat this was opened from — v4's `currentChatId` AND its initial scope. */
  readonly chatId = input.required<string>();
  readonly chatTitle = input<string | null>(null);

  readonly completed = output<SearchReplaceResultCounts>();
  readonly close = output<void>();

  protected readonly indicatorSteps = WIZARD_STEPS.slice(0, 3);

  /**
   * v4 starts on `search` when an initial scope was supplied
   * (`useSearchReplace.ts:193-195`), and the Salon always supplies one
   * (`{type:'chat', chatId}`). The `scope` step is reachable by going Back.
   */
  protected readonly step = signal<WizardStepId>('search');
  protected readonly scopeType = signal<'chat' | 'character'>('chat');
  protected readonly characterId = signal('');
  protected readonly searchText = signal('');
  protected readonly replaceText = signal('');
  protected readonly includeMessages = signal(true);
  protected readonly includeMemories = signal(true);
  protected readonly confirmed = signal(false);
  protected readonly preview = signal<SearchReplacePreviewCounts | null>(null);
  protected readonly loadingPreview = signal(false);
  protected readonly previewError = signal<string | null>(null);
  protected readonly executing = signal(false);
  protected readonly result = signal<SearchReplaceResultCounts | null>(null);
  protected readonly error = signal<string | null>(null);

  private readonly charactersQuery = injectQuery(() => ({
    queryKey: characterKeys.list(),
    queryFn: () => fetchCharacterList(this.core),
    enabled: this.scopeType() === 'character',
  }));
  protected readonly characters = computed(() => this.charactersQuery.data() ?? []);

  protected readonly scope = computed<SearchReplaceScopeWire | null>(() => {
    if (this.scopeType() === 'chat') return { type: 'chat', chatId: this.chatId() };
    const id = this.characterId();
    return id ? { type: 'character', characterId: id } : null;
  });

  protected readonly stepIndex = computed(() =>
    WIZARD_STEPS.findIndex((s) => s.id === this.step()),
  );
  protected readonly totalMatches = computed(
    () => (this.preview()?.messageMatches ?? 0) + (this.preview()?.memoryMatches ?? 0),
  );
  protected readonly totalUpdated = computed(
    () => (this.result()?.messagesUpdated ?? 0) + (this.result()?.memoriesUpdated ?? 0),
  );

  /** v4 `canProceed` (`useSearchReplace.ts:209-227`). */
  protected readonly canProceed = computed(() => {
    switch (this.step()) {
      case 'scope':
        return this.scope() !== null;
      case 'search':
        return (
          this.searchText().trim().length > 0 &&
          this.preview() !== null &&
          this.totalMatches() > 0
        );
      case 'confirm':
        return this.confirmed();
      case 'results':
        return true;
      default:
        return false;
    }
  });

  /** v4 `canGoBack` (`:229-233`). */
  protected readonly canGoBack = computed(
    () => this.stepIndex() > 0 && this.step() !== 'processing' && this.step() !== 'results',
  );

  constructor() {
    // v4's debounced preview: refires on the flags as well as the text (`:102-119`).
    effect((onCleanup) => {
      const scope = this.scope();
      const text = this.searchText();
      const replace = this.replaceText();
      const messages = this.includeMessages();
      const memories = this.includeMemories();
      if (this.step() !== 'search' || !scope || !text.trim()) return;

      const timer = setTimeout(() => {
        void this.fetchPreview(scope, text, replace, messages, memories);
      }, PREVIEW_DEBOUNCE_MS);
      onCleanup(() => clearTimeout(timer));
    });
  }

  private async fetchPreview(
    scope: SearchReplaceScopeWire,
    searchText: string,
    replaceText: string,
    includeMessages: boolean,
    includeMemories: boolean,
  ): Promise<void> {
    this.loadingPreview.set(true);
    this.previewError.set(null);
    try {
      this.preview.set(
        await searchReplacePreview(this.core, {
          scope,
          searchText,
          replaceText,
          includeMessages,
          includeMemories,
        }),
      );
    } catch (err) {
      this.previewError.set(err instanceof Error ? err.message : 'Failed to fetch preview');
    } finally {
      this.loadingPreview.set(false);
    }
  }

  /** v4 `nextStep` (`:286-300`) — on `confirm` it EXECUTES rather than advancing. */
  protected next(): void {
    if (this.step() === 'confirm') {
      void this.execute();
      return;
    }
    const index = this.stepIndex();
    if (index < WIZARD_STEPS.length - 1) {
      this.step.set(WIZARD_STEPS[index + 1].id);
    }
  }

  /** v4 `prevStep` (`:302-310`) — going back drops the confirmation. */
  protected prev(): void {
    const index = this.stepIndex();
    if (index > 0) {
      this.step.set(WIZARD_STEPS[index - 1].id);
      this.confirmed.set(false);
    }
  }

  /** v4 `execute` (`:240-282`) — a failure still lands on `results`. */
  private async execute(): Promise<void> {
    const scope = this.scope();
    if (!scope || !this.searchText().trim()) return;

    this.executing.set(true);
    this.error.set(null);
    this.step.set('processing');
    try {
      const result = await searchReplaceExecute(this.core, {
        scope,
        searchText: this.searchText(),
        replaceText: this.replaceText(),
        includeMessages: this.includeMessages(),
        includeMemories: this.includeMemories(),
      });
      this.result.set(result);
      this.step.set('results');
      this.completed.emit(result);
    } catch (err) {
      this.error.set(
        err instanceof Error ? err.message : 'Failed to execute search/replace',
      );
      this.step.set('results');
    } finally {
      this.executing.set(false);
    }
  }

  /** v4 `handleClose` (`:120-124`) — sealed while executing. */
  protected onClose(): void {
    if (this.executing()) return;
    this.close.emit();
  }
}
