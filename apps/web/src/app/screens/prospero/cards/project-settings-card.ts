import { ChangeDetectionStrategy, Component, input, output, signal } from '@angular/core';

import type { ProjectDetail } from '../../../core/core-contract';
import { CollapsibleCard } from '../../../ui/collapsible-card';
import { StateEditorModal } from '../state-editor-modal';
import type { ProjectEditForm } from './project-header';

/**
 * The project Settings card (v4 `SettingsCard.tsx`, full-width): a Project
 * Instructions editor with an explicit Save and a Project State row opening the
 * {@link StateEditorModal} JSON editor.
 *
 * DIVERGENCE (recorded): v4 uses the `MarkdownLexicalEditor` (a Lexical tree);
 * v5 ships a plain `<textarea>` here (the Lexical-equivalent editor is a
 * deferred vertical). The instructions bytes still round-trip exactly.
 */
@Component({
  selector: 'qt-project-settings-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [CollapsibleCard, StateEditorModal],
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
          <label class="qt-text-label block mb-2" for="qt-project-instructions">
            Project Instructions
          </label>
          <p class="qt-text-xs qt-text-secondary mb-2">
            These instructions are included in system prompts for all project chats.
          </p>
          <textarea
            id="qt-project-instructions"
            [value]="form().instructions"
            rows="8"
            aria-label="Project instructions"
            class="qt-textarea w-full"
            (input)="onInstructions($event)"
          ></textarea>
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
        [projectId]="project().id"
        [projectName]="project().name"
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

  protected readonly stateOpen = signal(false);

  protected onInstructions(event: Event): void {
    this.formChange.emit({ instructions: (event.target as HTMLTextAreaElement).value });
  }
}
