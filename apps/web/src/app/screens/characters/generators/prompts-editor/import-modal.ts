import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import { Modal } from '../../../../ui/modal';

/**
 * v4 `system-prompts-editor/types.ts` `PromptTemplate`.
 *
 * The three nullable fields are `?:` rather than v4's bare `| null` because the
 * wire OMITS a null column rather than sending `null` (see
 * `prompt-templates.api.ts`'s header). v4's own type is optimistic about this
 * and its template reads them truthily, for which absent and null are the same;
 * v5 states the truth in the type and renders identically.
 */
export interface PromptTemplate {
  id: string;
  name: string;
  content: string;
  description?: string | null;
  isBuiltIn: boolean;
  category?: string | null;
  modelHint?: string | null;
}

/**
 * The "Import from Template" modal — v4 `components/characters/system-
 * prompts-editor/ImportModal.tsx` (113 lines): built-in "Sample Prompts" and
 * the user's own templates, either imported by clicking its row.
 *
 * `templates` comes from `GET /api/v1/prompt-templates` — the
 * `promptTemplateList` verb, landed in P4.83 along with the lazy seeding of
 * v4's 21 built-in "Sample Prompts" (`prompt-templates.api.ts`). Both hosts
 * fetch on open, with v4's two DIFFERENT semantics: the editor ALWAYS refetches
 * (`useSystemPrompts.ts:265-268`), the new-character host opens first and
 * fetches only when the list is still empty (`NewCharacterView.tsx:93-108`).
 * The `p4.9k`-era "the catalogue is always empty" divergence this header used
 * to record is CLOSED; v4's own "No templates available" copy (`:104-108`) now
 * shows only when there genuinely are none.
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
