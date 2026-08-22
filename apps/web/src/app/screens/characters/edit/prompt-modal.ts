import {
  ChangeDetectionStrategy,
  Component,
  OnInit,
  computed,
  input,
  output,
  signal,
} from '@angular/core';

import type { CharacterSystemPrompt } from '../../../core/core-contract';
import { MarkdownField } from '../../../editor/markdown-field';
import { FormActions } from '../../../ui/form-actions';
import { Modal } from '../../../ui/modal';
import { PROMPT_FIELD_HINTS } from '../../../ui/prompt-field-hints';
import { PromptFieldLabel } from '../../../ui/prompt-field-label';

/** The editable create/edit form for one system prompt. */
export interface PromptFormData {
  name: string;
  content: string;
  isDefault: boolean;
}

export const INITIAL_PROMPT_FORM_DATA: PromptFormData = { name: '', content: '', isDefault: false };

/**
 * Create/edit modal for one character system prompt (v4
 * `components/characters/system-prompts-editor/PromptModal.tsx`): name, the
 * markdown content field (v4 `:78`, its `minHeight="12rem"` and
 * `remountKey={editingPrompt?.id ?? 'new'}`), and a "set as default" checkbox.
 *
 * Content's header is the shared `qt-prompt-field-label` (v4 `a6870c5a`):
 * `label="Content"` `required` over the systemPrompt hint, with this surface's
 * Markdown/placeholder sentence appended to the shared helper.
 */
@Component({
  selector: 'qt-prompt-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, FormActions, MarkdownField, PromptFieldLabel],
  template: `
    <qt-modal
      [title]="editing() ? 'Edit Prompt' : 'Create Prompt'"
      maxWidth="2xl"
      (close)="close.emit()"
    >
      <div class="space-y-4">
        <div>
          <label class="qt-label">Name *</label>
          <input
            type="text"
            class="qt-input"
            placeholder="e.g., Romantic, Companion, Professional"
            [value]="form().name"
            (input)="setField('name', $any($event.target).value)"
          />
        </div>

        <div>
          <qt-prompt-field-label
            [hint]="hints.systemPrompt"
            label="Content"
            required
            [helper]="contentHelper"
          />
          <qt-markdown-field
            ariaLabel="System prompt content"
            minHeight="12rem"
            [recordKey]="recordKey()"
            [value]="form().content"
            (contentChange)="setField('content', $event)"
          />
        </div>

        <div class="flex items-center gap-2">
          <input
            type="checkbox"
            id="isDefault"
            class="qt-checkbox"
            [checked]="form().isDefault"
            (change)="setField('isDefault', $any($event.target).checked)"
          />
          <label for="isDefault" class="text-sm text-foreground">Set as default prompt</label>
        </div>
      </div>

      <div qt-modal-footer>
        <qt-form-actions
          [submitLabel]="saving() ? 'Saving...' : editing() ? 'Update' : 'Create'"
          [isLoading]="saving()"
          [isDisabled]="isDisabled()"
          (cancel)="close.emit()"
          (submit)="save.emit(form())"
        />
      </div>
    </qt-modal>
  `,
})
export class PromptModal implements OnInit {
  readonly editingPrompt = input<CharacterSystemPrompt | null>(null);
  readonly saving = input(false);
  readonly close = output<void>();
  readonly save = output<PromptFormData>();

  protected readonly form = signal<PromptFormData>(INITIAL_PROMPT_FORM_DATA);

  protected readonly hints = PROMPT_FIELD_HINTS;
  /**
   * v4 `PromptModal.tsx:74-79` — the shared systemPrompt helper with this
   * surface's own suffix appended. A TS constant, not a template literal in
   * the markup, so the `{{char}}` / `{{user}}` braces never reach Angular's
   * interpolation.
   */
  protected readonly contentHelper =
    `${PROMPT_FIELD_HINTS.systemPrompt.helper} Markdown is supported, and {{char}} / {{user}} substitute the character and user names.`;

  protected readonly editing = computed(() => !!this.editingPrompt());
  /** v4 `remountKey={editingPrompt?.id ?? 'new'}` (`PromptModal.tsx:81`). */
  protected readonly recordKey = computed(() => this.editingPrompt()?.id ?? 'new');
  protected readonly isDisabled = computed(
    () => !this.form().name.trim() || !this.form().content.trim(),
  );

  ngOnInit(): void {
    const p = this.editingPrompt();
    if (p) {
      this.form.set({ name: p.name, content: p.content, isDefault: p.isDefault });
    }
  }

  protected setField<K extends keyof PromptFormData>(key: K, value: PromptFormData[K]): void {
    this.form.update((f) => ({ ...f, [key]: value }));
  }
}
