import { ChangeDetectionStrategy, Component, inject, output, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';

import { CoreClient } from '../../core/core-client';
import { Modal } from '../../ui/modal';
import { createProject } from './projects.api';
import { ToastService } from '../../ui/toast.service';

/**
 * The Create Project dialog (v4 `CreateProjectDialog.tsx`): name* (max 100) +
 * optional description (max 2000). On success it emits the new project's id
 * (the list navigates into the detail). Failures surface an inline alert.
 */
@Component({
  selector: 'qt-project-create-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [FormsModule, Modal],
  template: `
    <qt-modal title="Create Project" maxWidth="md" (close)="close.emit()">
      <form id="qt-project-create-form" (ngSubmit)="submit()">
        <div class="mb-4">
          <label class="qt-label mb-2 block" for="qt-project-name">Name</label>
          <input
            id="qt-project-name"
            name="name"
            type="text"
            [(ngModel)]="name"
            maxlength="100"
            placeholder="My Project"
            class="qt-input"
            required
          />
        </div>
        <div>
          <label class="qt-label mb-2 block" for="qt-project-description">
            Description (optional)
          </label>
          <textarea
            id="qt-project-description"
            name="description"
            [(ngModel)]="description"
            maxlength="2000"
            rows="3"
            placeholder="What is this project about?"
            class="qt-textarea"
          ></textarea>
        </div>
      </form>

      <div qt-modal-footer class="flex gap-2 justify-end">
        <button
          type="button"
          class="qt-button-secondary"
          [disabled]="saving()"
          (click)="close.emit()"
        >
          Cancel
        </button>
        <button
          type="submit"
          form="qt-project-create-form"
          class="qt-button-primary"
          [disabled]="saving() || !name().trim()"
        >
          {{ saving() ? 'Creating...' : 'Create' }}
        </button>
      </div>
    </qt-modal>
  `,
})
export class ProjectCreateDialog {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly close = output<void>();
  readonly created = output<string>();

  protected readonly name = signal('');
  protected readonly description = signal('');
  protected readonly saving = signal(false);

  protected async submit(): Promise<void> {
    const name = this.name().trim();
    if (!name || this.saving()) {
      return;
    }
    this.saving.set(true);
    try {
      const project = await createProject(this.core, name, this.description() || null);
      this.toasts.showSuccess('Project created successfully!');
      this.created.emit(project.id);
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to create project');
    } finally {
      this.saving.set(false);
    }
  }
}
