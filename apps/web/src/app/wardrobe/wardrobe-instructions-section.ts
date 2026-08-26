import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  signal,
  untracked,
} from '@angular/core';

import { CoreClient } from '../core/core-client';
import { MarkdownField } from '../editor/markdown-field';
import { Icon } from '../ui/icon';
import { PROMPT_FIELD_HINTS } from '../ui/prompt-field-hints';
import { PromptFieldLabel } from '../ui/prompt-field-label';
import { ToastService } from '../ui/toast.service';
import {
  encodeWardrobeContainer,
  type WardrobeContainer,
} from './wardrobe-container';
import { loadWardrobeInstructions, saveWardrobeInstructions } from './wardrobe-instructions.api';

/**
 * The dressing-instructions editor (v4
 * `components/wardrobe/WardrobeInstructionsSection.tsx`, NEW at `b86bb1a5`).
 *
 * A collapsible section under the wardrobe dialog's container selector — and,
 * on the Aurora page, under the Wardrobe tab's "Open wardrobe for …" button —
 * that edits the browsed container's optional `Wardrobe/instructions.md`: the
 * second-person guidance consulted when a character chooses their own opening
 * outfit ("Let character choose"). Every container tier gets one; at outfit
 * time the nearest tier's copy wins and the search stops there.
 *
 * Collapsed by default so the item grid keeps the stage; the summary row notes
 * whether the browsed container has instructions on file.
 *
 * ## Two recorded mechanism divergences
 *
 * **The remount key is not load-bearing here.** v4's Lexical editor reads
 * `value` only at mount, so it bumps a `remountKey` on every fetched load or
 * the async text would never appear. v5's `qt-markdown-field` feeds
 * `RichEditor`, which absorbs an external `value` change whenever it differs
 * from what it last emitted (`rich-editor.ts:139-142`) — the fetched text lands
 * without any re-key. v4's composite key is transcribed anyway (it is also what
 * re-seeds the field on a CONTAINER switch, which is behavior, not a
 * workaround), and it costs one absorbed re-init per load.
 *
 * **`container = null` renders nothing** (v4 `:53`, after its hooks). In
 * Angular there are no hook-ordering rules to satisfy, so the whole template is
 * simply gated — the load effect refuses a null container the way v4's `reload`
 * does (`:55-59`), including resetting `fetched` to false, which is the only
 * way that flag ever goes back.
 */
@Component({
  selector: 'qt-wardrobe-instructions-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { class: 'block' },
  imports: [Icon, MarkdownField, PromptFieldLabel],
  template: `
    @if (container(); as c) {
      <div class="border qt-border-default rounded-lg mb-3">
        <button
          type="button"
          class="w-full flex items-center gap-2 px-3 py-2 text-sm qt-text-secondary hover:text-foreground"
          [attr.aria-expanded]="expanded()"
          (click)="expanded.set(!expanded())"
        >
          <qt-icon name="chevron-down" [class]="chevronClass()" />
          <span class="font-medium">Dressing Instructions</span>
          <span class="ml-auto text-xs">{{ statusLabel() }}</span>
        </button>
        @if (expanded()) {
          <div class="px-3 pb-3">
            <qt-prompt-field-label [hint]="hint" optional />
            <qt-markdown-field
              ariaLabel="Dressing instructions"
              minHeight="8rem"
              [value]="draft()"
              [disabled]="loading() || saving()"
              [recordKey]="remountKey()"
              (contentChange)="draft.set($event)"
            />
            <div class="flex justify-end mt-2">
              <button
                type="button"
                class="qt-button qt-button-primary qt-button-sm"
                [disabled]="!dirty() || saving() || loading()"
                (click)="handleSave()"
              >
                {{ saving() ? 'Saving…' : 'Save Instructions' }}
              </button>
            </div>
          </div>
        }
      </div>
    }
  `,
})
export class WardrobeInstructionsSection {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  /** The container being browsed; `null` renders nothing (v4 `:53`). */
  readonly container = input.required<WardrobeContainer | null>();

  protected readonly hint = PROMPT_FIELD_HINTS.wardrobeInstructions;

  /** v4's hook state (`use-wardrobe-instructions.ts:45-48`). */
  private readonly instructions = signal<string | null>(null);
  protected readonly loading = signal(false);
  /** True once at least one load has COMPLETED for the current container. */
  private readonly fetched = signal(false);
  protected readonly saving = signal(false);

  /** v4's component state (`WardrobeInstructionsSection.tsx:35-40`). */
  protected readonly expanded = signal(false);
  protected readonly draft = signal('');
  private readonly loadCount = signal(0);

  private readonly containerKey = computed(() => {
    const c = this.container();
    return c ? encodeWardrobeContainer(c) : '';
  });

  /**
   * v4 `:79-82`, built as one string rather than a `[class.…]` binding: `class`
   * is an INPUT on `qt-icon` and its host is `display: contents`, so a class
   * token bound on the host lands on a box-less element where `transform` has
   * nothing to act on. The rotation has to ride the input, onto the span that
   * actually draws the glyph.
   */
  protected readonly chevronClass = computed(
    () =>
      `w-4 h-4 flex-shrink-0 transition-transform duration-200 ${this.expanded() ? '' : '-rotate-90'}`,
  );

  /** v4 `` `${containerKey}:${remountKey}` `` (`:106`). */
  protected readonly remountKey = computed(() => `${this.containerKey()}:${this.loadCount()}`);

  /** v4 `:56` — the summary row's right-hand note. U+2026 in "Consulting…". */
  protected readonly statusLabel = computed(() =>
    this.loading() ? 'Consulting…' : this.hasInstructions() ? 'On file' : 'None on file',
  );

  private readonly hasInstructions = computed(
    () => this.fetched() && this.instructions() !== null,
  );

  /**
   * v4 `:57`. The comparison is TRIMMED draft against UNTRIMMED stored — sound
   * only because the server stores the trimmed string, which is also what the
   * save echo carries back.
   */
  protected readonly dirty = computed(
    () => this.fetched() && (this.draft().trim() || '') !== (this.instructions() ?? ''),
  );

  constructor() {
    // v4's `reload` + the effect that fires it on every container change
    // (`:52-76`). A null container clears the state and resets `fetched`.
    effect(() => {
      const container = this.container();
      untracked(() => void this.reload(container));
    });

    // v4 `:42-48` — seed the draft from every completed load (and every
    // container switch), and bump the remount key with it.
    effect(() => {
      const fetched = this.fetched();
      const instructions = this.instructions();
      this.containerKey();
      if (!fetched) return;
      untracked(() => {
        this.draft.set(instructions ?? '');
        this.loadCount.update((k) => k + 1);
      });
    });
  }

  private async reload(container: WardrobeContainer | null): Promise<void> {
    if (!container) {
      this.instructions.set(null);
      this.fetched.set(false);
      return;
    }
    this.loading.set(true);
    try {
      this.instructions.set(await loadWardrobeInstructions(this.core, container));
    } catch (err) {
      // eslint-disable-next-line no-console
      console.warn('[WardrobeInstructionsSection] Failed to load dressing instructions', err);
      this.instructions.set(null);
    } finally {
      this.loading.set(false);
      this.fetched.set(true);
    }
  }

  /**
   * v4 `handleSave` (`:59-69`). The UNTRIMMED draft is sent when it holds
   * anything at all; a blank draft sends `null`, which clears the file. The
   * echo is adopted, so the next `dirty` read compares against what persisted.
   */
  protected async handleSave(): Promise<void> {
    const container = this.container();
    if (!container) return;
    const draft = this.draft();
    const clearing = draft.trim().length === 0;
    this.saving.set(true);
    try {
      this.instructions.set(
        await saveWardrobeInstructions(this.core, container, clearing ? null : draft),
      );
      // No draft re-seed here: adopting the echo changes `instructions`, and
      // the seeding effect above re-runs on it — exactly v4's effect deps
      // (`[fetched, instructions, containerKey]`), which is how v4's own save
      // replaces the untrimmed draft with the trimmed stored text.
      this.toasts.showSuccess(
        clearing ? 'Dressing instructions cleared' : 'Dressing instructions saved',
      );
    } catch (err) {
      // eslint-disable-next-line no-console
      console.warn('[WardrobeInstructionsSection] Failed to save dressing instructions', err);
      this.toasts.showError('Failed to save dressing instructions');
    } finally {
      this.saving.set(false);
    }
  }
}
