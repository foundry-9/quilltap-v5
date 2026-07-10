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
import { FormActions } from '../../../ui/form-actions';
import { Modal } from '../../../ui/modal';

/** The editable create/edit form for one system prompt. */
export interface PromptFormData {
  name: string;
  content: string;
  isDefault: boolean;
}

export const INITIAL_PROMPT_FORM_DATA: PromptFormData = { name: '', content: '', isDefault: false };

/**
 * Create/edit modal for one character system prompt (v4
 * `components/characters/system-prompts-editor/PromptModal.tsx`): name,
 * markdown content (plain textarea this round), and a "set as default"
 * checkbox.
 */
@Component({
  selector: 'qt-prompt-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, FormActions],
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
          <label class="qt-label">Content *</label>
          <p class="qt-text-xs mt-1 mb-2">
            Supports Markdown formatting. Use {{ '{{char}}' }} and {{ '{{user}}' }} for
            character/user name substitution.
          </p>
          <textarea
            class="qt-input"
            rows="10"
            aria-label="System prompt content"
            [value]="form().content"
            (input)="setField('content', $any($event.target).value)"
          ></textarea>
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

  protected readonly editing = computed(() => !!this.editingPrompt());
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
