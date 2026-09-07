import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import { Modal } from '../../../../ui/modal';

/** v4 `system-prompts-editor/types.ts` `PromptTemplate`. */
export interface PromptTemplate {
  id: string;
  name: string;
  content: string;
  description: string | null;
  isBuiltIn: boolean;
  category: string | null;
  modelHint: string | null;
}

/**
 * The "Import from Template" modal — v4 `components/characters/system-
 * prompts-editor/ImportModal.tsx` (113 lines): built-in "Sample Prompts" and
 * the user's own templates, either imported by clicking its row.
 *
 * ⚠ v4 sources `templates` from `GET /api/v1/prompt-templates`, a verb v5 has
 * no equivalent of — it rides no server family in this round's §B contract
 * (P4.9K1 ships the character trio, P4.9K2 the wizard pair; neither owns a
 * templates listing) and `core-contract.ts`/`characters.api.ts` are frozen
 * for this lane, so this lane cannot invent one (§B.6). The modal is
 * genuinely joined to the editor (both hosts open it; it renders v4's own
 * "No templates available" copy `:104-108`) rather than left as a disabled
 * button, but `templates` is always empty until a future round adds the
 * listing verb — recorded loud, not silent, in the lane's status-log entry.
 */
@Component({
  selector: 'qt-character-prompt-import-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal title="Import from Template" maxWidth="2xl" (close)="close.emit()">
      @if (loading()) {
        <div class="text-center py-8 qt-text-secondary">Loading templates...</div>
      } @else {
        <div class="space-y-6">
          @if (sampleTemplates().length > 0) {
            <div>
              <h4 class="text-sm qt-text-primary mb-3">Sample Prompts</h4>
              <div class="space-y-2 max-h-60 overflow-y-auto">
                @for (template of sampleTemplates(); track template.id) {
                  <button
                    type="button"
                    class="qt-button-ghost w-full p-3 text-left justify-start"
                    (click)="onImport(template)"
                  >
                    <div class="w-full">
                      <div class="flex items-center justify-between">
                        <span class="qt-text-primary">{{ template.name }}</span>
                        <div class="flex gap-1">
                          @if (template.category) {
                            <span class="qt-badge-secondary">{{ template.category }}</span>
                          }
                          @if (template.modelHint) {
                            <span class="qt-badge">{{ template.modelHint }}</span>
                          }
                        </div>
                      </div>
                      @if (template.description) {
                        <p class="qt-text-xs mt-1">{{ template.description }}</p>
                      }
                    </div>
                  </button>
                }
              </div>
            </div>
          }

          @if (userTemplates().length > 0) {
            <div>
              <h4 class="text-sm qt-text-primary mb-3">
                {{ sampleTemplates().length > 0 ? 'My Templates' : 'Templates' }}
              </h4>
              <div class="space-y-2 max-h-60 overflow-y-auto">
                @for (template of userTemplates(); track template.id) {
                  <button
                    type="button"
                    class="qt-button-ghost w-full p-3 text-left justify-start"
                    (click)="onImport(template)"
                  >
                    <div class="w-full">
                      <span class="qt-text-primary">{{ template.name }}</span>
                      @if (template.description) {
                        <p class="qt-text-xs mt-1">{{ template.description }}</p>
                      }
                    </div>
                  </button>
                }
              </div>
            </div>
          }

          @if (sampleTemplates().length === 0 && userTemplates().length === 0) {
            <p class="text-center qt-text-secondary py-4">
              No templates available. Create templates in Settings &gt; Prompts.
            </p>
          }
        </div>
      }
    </qt-modal>
  `,
})
export class CharacterPromptImportModal {
  readonly loading = input(false);
  readonly templates = input<PromptTemplate[]>([]);
  readonly close = output<void>();
  readonly importPrompt = output<{ content: string; suggestedName: string }>();

  protected readonly sampleTemplates = computed(() => this.templates().filter((t) => t.isBuiltIn));
  protected readonly userTemplates = computed(() => this.templates().filter((t) => !t.isBuiltIn));

  protected onImport(template: PromptTemplate): void {
    this.importPrompt.emit({ content: template.content, suggestedName: template.name });
  }
}
