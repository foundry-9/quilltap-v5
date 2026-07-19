import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';
import { RouterLink } from '@angular/router';

import type { ProjectDetail } from '../../../core/core-contract';
import { Icon } from '../../../ui/icon';

export interface ProjectEditForm {
  name: string;
  description: string;
  instructions: string;
}

/**
 * The project detail header (v4 `ProjectDetailHeader.tsx`): a back link, the
 * swatch + emoji, the name/description with an inline Edit toggle (Save/Cancel),
 * and a **New Chat** link into `/salon/new?projectId=`. Presentational — the
 * detail owns the edit form and the save.
 */
@Component({
  selector: 'qt-project-header',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, Icon],
  template: `
    <div
      class="flex flex-wrap items-start justify-between gap-4 border-b qt-border-default/60 pb-6"
    >
      <div class="flex items-center gap-4">
        @if (inTab()) {
          <!-- Drilled inside the Prospero tab: back restores the list (v4 onBack). -->
          <button type="button" class="qt-text-primary hover:underline text-sm" (click)="back.emit()">
            &larr; Projects
          </button>
        } @else {
          <a routerLink="/prospero" class="qt-text-primary hover:underline text-sm"
            >&larr; Projects</a
          >
        }
        <div
          class="w-12 h-12 rounded-lg flex items-center justify-center text-xl"
          [style.backgroundColor]="project().color || 'var(--muted)'"
        >
          @if (project().icon) {
            {{ project().icon }}
          } @else {
            <qt-icon name="folder" class="w-6 h-6 qt-text-secondary" />
          }
        </div>
        <div>
          @if (editing()) {
            <input
              type="text"
              [value]="form().name"
              aria-label="Project name"
              class="qt-heading-2 bg-transparent border-b qt-border-primary focus:outline-none"
              (input)="patch('name', $event)"
            />
          } @else {
            <h1 class="qt-heading-2">{{ project().name }}</h1>
          }
          @if (editing()) {
            <input
              type="text"
              [value]="form().description"
              placeholder="Add a description..."
              aria-label="Project description"
              class="qt-text-small bg-transparent border-b qt-border-default focus:outline-none w-full"
              (input)="patch('description', $event)"
            />
          } @else if (project().description) {
            <p class="qt-text-small">{{ project().description }}</p>
          }
        </div>
      </div>
      <div class="flex gap-2">
        <a
          [routerLink]="['/salon/new']"
          [queryParams]="{ projectId: project().id }"
          class="inline-flex items-center gap-2 rounded-lg bg-success px-4 py-2 text-sm font-semibold qt-text-success-foreground shadow hover:qt-bg-success/90"
        >
          <qt-icon name="plus" class="w-4 h-4" />
          New Chat
        </a>
        @if (editing()) {
          <button
            type="button"
            class="inline-flex items-center rounded-lg border qt-border-default qt-bg-card px-4 py-2 text-sm qt-shadow-sm hover:qt-bg-muted"
            (click)="cancelEdit.emit()"
          >
            Cancel
          </button>
          <button
            type="button"
            class="inline-flex items-center rounded-lg bg-primary px-4 py-2 text-sm font-semibold text-primary-foreground shadow hover:qt-bg-primary/90"
            (click)="save.emit()"
          >
            Save
          </button>
        } @else {
          <button
            type="button"
            class="inline-flex items-center rounded-lg border qt-border-default qt-bg-card px-4 py-2 text-sm qt-shadow-sm hover:qt-bg-muted"
            (click)="editClick.emit()"
          >
            Edit
          </button>
        }
      </div>
    </div>
  `,
})
export class ProjectHeader {
  readonly project = input.required<ProjectDetail>();
  readonly editing = input(false);
  readonly form = input.required<ProjectEditForm>();
  /** Drilled inside the Prospero workspace tab ⇒ back emits instead of routing. */
  readonly inTab = input(false);

  readonly formChange = output<Partial<ProjectEditForm>>();
  readonly editClick = output<void>();
  readonly cancelEdit = output<void>();
  readonly save = output<void>();
  readonly back = output<void>();

  protected patch(key: keyof ProjectEditForm, event: Event): void {
    this.formChange.emit({ [key]: (event.target as HTMLInputElement).value });
  }
}
