import { ChangeDetectionStrategy, Component, input, output, signal } from '@angular/core';

import type { ProjectDetail } from '../../../core/core-contract';
import { MarkdownField } from '../../../editor/markdown-field';
import { StateEditorModal } from '../../../shared/state/state-editor-modal';
import { CollapsibleCard } from '../../../ui/collapsible-card';
import { PROMPT_FIELD_HINTS } from '../../../ui/prompt-field-hints';
import { PromptFieldLabel } from '../../../ui/prompt-field-label';
import type { ProjectEditForm } from './project-header';

/**
 * The project Settings card (v4 `SettingsCard.tsx`, full-width): a Project
 * Instructions markdown field (v4 `:68`, its `minHeight="14rem"` and
 * `remountKey={project.id}`) with an explicit Save, and a Project State row
 * opening the shared {@link StateEditorModal} JSON editor (entityType project).
 *
 * NOTE: v4 renders this same instructions editor in TWO places — this card and
 * `SettingsTab.tsx:35`, which passes `minHeight="10rem"`. v5 has only the card,
 * so it takes the card's `14rem`.
 *
 * The header is the shared `qt-prompt-field-label` over
 * `PROMPT_FIELD_HINTS.projectInstructions` (v4 `a6870c5a`, `SettingsTab.tsx`
 * — no `optional` on this one), which replaces v5's older hand-rolled helper
 * sentence.
 */
@Component({
  selector: 'qt-project-settings-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [CollapsibleCard, StateEditorModal, MarkdownField, PromptFieldLabel],
  template: `
    <qt-collapsible-card
      title="Project Settings"
      description="Instructions & project state"
      icon="settings"
      [defaultOpen]="defaultOpen()"
      class="col-span-full block"
    >
      <div class="space-y-4">
        <div>
          <qt-prompt-field-label [hint]="hints.projectInstructions" />
          <qt-markdown-field
            ariaLabel="Project instructions"
            minHeight="14rem"
            [recordKey]="project().id"
            [value]="form().instructions"
            (contentChange)="formChange.emit({ instructions: $event })"
          />
          <div class="mt-2 flex justify-end">
            <button type="button" class="qt-button qt-button-primary" (click)="save.emit()">
              Save
            </button>
          </div>
        </div>

        <div class="flex items-center justify-between p-3 rounded-lg qt-border qt-bg-surface">
          <div>
            <h4 class="qt-label text-foreground">Project State</h4>
            <p class="qt-text-xs qt-text-secondary">
              Persistent JSON data for games, inventory, and session tracking.
            </p>
          </div>
          <button
            type="button"
            class="qt-button qt-button-secondary qt-button-sm"
            (click)="stateOpen.set(true)"
          >
            View/Edit
          </button>
        </div>
      </div>
    </qt-collapsible-card>

    @if (stateOpen()) {
      <qt-state-editor-modal
        entityType="project"
        [entityId]="project().id"
        [entityName]="project().name"
        (close)="stateOpen.set(false)"
      />
    }
  `,
})
export class ProjectSettingsCard {
  readonly project = input.required<ProjectDetail>();
  readonly form = input.required<ProjectEditForm>();
  readonly defaultOpen = input(false);

  readonly formChange = output<Partial<ProjectEditForm>>();
  readonly save = output<void>();

  protected readonly hints = PROMPT_FIELD_HINTS;

  protected readonly stateOpen = signal(false);
}
