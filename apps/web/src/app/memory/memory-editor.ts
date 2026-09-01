import {
  ChangeDetectionStrategy,
  Component,
  OnInit,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import { CoreClient } from '../core/core-client';
import type { MemoryDto } from '../core/core-contract';
import { MarkdownField } from '../editor/markdown-field';
import { ErrorAlert } from '../ui/error-alert';
import { FormActions } from '../ui/form-actions';
import { Modal } from '../ui/modal';
import { createMemory, updateMemory } from './memory.api';
import { importanceLabel, importancePercent, keywordsFromString, keywordsToString } from './memory-format';
import { ToastService } from '../ui/toast.service';

/**
 * The create / edit memory modal (v4 `components/memory/memory-editor.tsx`):
 * Summary (required), Full Content (required), comma-separated Keywords, and an
 * importance range slider (0–1 step 0.1, labeled Low/Med/High + %). On save it
 * always sends `source:'MANUAL'` and does NOT send/edit `tags` or
 * `aboutCharacterId` (v4 fidelity). Create POSTs; edit PUTs (no re-embed).
 *
 * Content is edited through the shared `qt-markdown-field` (v4's
 * `MarkdownLexicalEditor`, "Designed for forms") — byte-faithful to the v4
 * composer dialect. The save/emit contract is unchanged: `content` is still a
 * plain markdown string bound in and out.
 */
@Component({
  selector: 'qt-memory-editor',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, FormActions, ErrorAlert, MarkdownField],
  template: `
    <qt-modal [title]="title()" maxWidth="2xl" (close)="close.emit()">
      <form class="space-y-4" (submit)="$event.preventDefault(); submit()">
        <div>
          <label for="summary" class="block qt-text-label mb-1">Summary *</label>
          <input
            id="summary"
            type="text"
            class="qt-input"
            placeholder="Brief summary of this memory"
            [value]="summary()"
            (input)="summary.set($any($event.target).value)"
          />
          <p class="mt-1 qt-text-xs">
            A short description that will be shown in lists and used for context injection.
          </p>
        </div>

        <div>
          <label class="block qt-text-label mb-1">Full Content *</label>
          <p class="mb-2 qt-text-xs">
            The full details of what this character should remember.
          </p>
          <qt-markdown-field
            ariaLabel="Memory content"
            minHeight="10rem"
            [value]="content()"
            (contentChange)="content.set($event)"
          />
        </div>

        <div>
          <label for="keywords" class="block qt-text-label mb-1">Keywords</label>
          <input
            id="keywords"
            type="text"
            class="qt-input"
            placeholder="keyword1, keyword2, keyword3"
            [value]="keywords()"
            (input)="keywords.set($any($event.target).value)"
          />
          <p class="mt-1 qt-text-xs">Comma-separated keywords for text-based search.</p>
        </div>

        <div>
          <label for="importance" class="block qt-text-label mb-1">
            Importance: {{ importanceText() }} ({{ importancePct() }}%)
          </label>
          <input
            id="importance"
            type="range"
            min="0"
            max="1"
            step="0.1"
            class="qt-range w-full"
            [value]="importance()"
            (input)="importance.set(+$any($event.target).value)"
          />
          <div class="flex justify-between qt-text-xs mt-1">
            <span>Low</span>
            <span>Medium</span>
            <span>High</span>
          </div>
          <p class="mt-1 qt-text-xs">
            Higher importance memories are prioritized when building context.
          </p>
        </div>

        @if (error()) {
          <qt-error-alert [message]="error()!" />
        }
      </form>

      <div qt-modal-footer>
        <qt-form-actions
          [submitLabel]="saving() ? 'Saving...' : submitLabel()"
          [isLoading]="saving()"
          [isDisabled]="!isValid()"
          (cancel)="close.emit()"
          (submit)="submit()"
        />
      </div>
    </qt-modal>
  `,
})
export class MemoryEditor implements OnInit {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly characterId = input.required<string>();
  /** The memory being edited, or `null` for create. */
  readonly memory = input<MemoryDto | null>(null);

  readonly close = output<void>();
  readonly saved = output<void>();

  protected readonly summary = signal('');
  protected readonly content = signal('');
  protected readonly keywords = signal('');
  protected readonly importance = signal(0.5);

  protected readonly saving = signal(false);
  protected readonly error = signal<string | null>(null);

  protected readonly isEditing = computed(() => !!this.memory());
  protected readonly title = computed(() => (this.isEditing() ? 'Edit Memory' : 'Add Memory'));
  protected readonly submitLabel = computed(() =>
    this.isEditing() ? 'Save Changes' : 'Create Memory',
  );
  protected readonly importanceText = computed(() => importanceLabel(this.importance()));
  protected readonly importancePct = computed(() => importancePercent(this.importance()));

  /** v4 marks summary + content `required`. */
  protected readonly isValid = computed(
    () => this.summary().trim().length > 0 && this.content().trim().length > 0,
  );

  ngOnInit(): void {
    const m = this.memory();
    if (m) {
      this.summary.set(m.summary ?? '');
      this.content.set(m.content ?? '');
      this.keywords.set(keywordsToString(m.keywords ?? []));
      this.importance.set(m.importance ?? 0.5);
    }
  }

  protected async submit(): Promise<void> {
    if (!this.isValid() || this.saving()) {
      return;
    }
    this.saving.set(true);
    this.error.set(null);
    const bag = {
      characterId: this.characterId(),
      content: this.content(),
      summary: this.summary(),
      keywords: keywordsFromString(this.keywords()),
      importance: this.importance(),
      source: 'MANUAL' as const,
    };
    try {
      const m = this.memory();
      if (m) {
        await updateMemory(this.core, m.id, bag);
      } else {
        await createMemory(this.core, bag);
      }
      // v4 `:86` — the sentence depends on which verb ran.
      this.toasts.showSuccess(m ? 'Memory updated' : 'Memory created');
      this.saved.emit();
      this.close.emit();
    } catch (err) {
      this.error.set(err instanceof Error && err.message ? err.message : 'Failed to save memory');
    } finally {
      this.saving.set(false);
    }
  }
}
