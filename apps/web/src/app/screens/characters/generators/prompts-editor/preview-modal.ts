import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

import type { CharacterSystemPrompt } from '../../../../core/core-contract';
import { Modal } from '../../../../ui/modal';
import { TemplateDisplay } from '../../view/template-display';

/**
 * The system-prompt preview modal — v4 `components/characters/system-prompts-
 * editor/PreviewModal.tsx` (80 lines): the prompt name as the dialog title
 * (v4's own `isDefault` badge JSX at `:22-31` is dead code — it builds a
 * `title` node with the badge but the `<BaseModal>` call passes `prompt.name`
 * directly, not that node; not replicated) and the content rendered with the
 * `{{char}}`/`{{user}}` highlighter, matching the read-only detail page's own
 * prompt display (`view/tabs/system-prompts-tab.ts`) rather than v4's
 * ReactMarkdown — v5 has no chat-independent Markdown renderer to reuse here.
 * `TemplateDisplay` is K4's file (`view/template-display.ts`): imported, not
 * edited, per the work order.
 */
@Component({
  selector: 'qt-character-prompt-preview-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, TemplateDisplay],
  template: `
    <qt-modal [title]="prompt().name" maxWidth="2xl" (close)="close.emit()">
      <div
        class="p-4 border qt-border-default rounded-lg qt-bg-muted/30 max-h-[60vh] overflow-y-auto"
      >
        <pre
          class="whitespace-pre-wrap break-words text-sm text-foreground"
        ><code><qt-template-display
              [content]="prompt().content"
              [characterName]="characterName()"
            /></code></pre>
      </div>

      <div qt-modal-footer class="flex justify-end gap-3">
        <button type="button" class="qt-button-secondary" (click)="close.emit()">Close</button>
        <button type="button" class="qt-button-primary" (click)="onEdit()">Edit</button>
      </div>
    </qt-modal>
  `,
})
export class CharacterPromptPreviewModal {
  readonly prompt = input.required<CharacterSystemPrompt>();
  readonly characterName = input<string>('Character');
  readonly close = output<void>();
  readonly edit = output<CharacterSystemPrompt>();

  protected onEdit(): void {
    this.edit.emit(this.prompt());
    this.close.emit();
  }
}
