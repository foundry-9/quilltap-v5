import { ChangeDetectionStrategy, Component, inject, output, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';

import { CoreClient } from '../../core/core-client';
import { Modal } from '../../ui/modal';
import { createGroup } from './groups.api';
import { ToastService } from '../../ui/toast.service';

/**
 * The Create Group dialog (v4 `AuroraView.tsx` inline dialog): name* + optional
 * description. On success it emits the new group's id (the caller navigates into
 * the editor). Failures surface an inline alert with v4's fallback microcopy.
 */
@Component({
  selector: 'qt-group-create-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [FormsModule, Modal],
  template: `
    <qt-modal title="Create Group" maxWidth="md" (close)="close.emit()">
      <form id="qt-group-create-form" (ngSubmit)="submit()">
        <div class="mb-4">
          <label class="mb-2 block text-sm font-medium text-foreground" for="qt-group-name">
            Group Name *
          </label>
          <input
            id="qt-group-name"
            name="name"
            type="text"
            [(ngModel)]="name"
            placeholder="e.g., Adventuring Party"
            class="w-full px-4 py-2 rounded-lg border qt-border-default bg-transparent text-foreground text-sm"
            required
          />
        </div>
        <div class="mb-2">
          <label class="mb-2 block text-sm font-medium text-foreground" for="qt-group-description">
            Description (optional)
          </label>
          <textarea
            id="qt-group-description"
            name="description"
            [(ngModel)]="description"
            placeholder="Optional description of this group"
            rows="3"
            class="w-full px-4 py-2 rounded-lg border qt-border-default bg-transparent text-foreground text-sm resize-none"
          ></textarea>
        </div>
      </form>

      <div qt-modal-footer class="flex gap-2 justify-end">
        <button
          type="button"
          class="inline-flex items-center rounded-lg border qt-border-default qt-bg-card px-4 py-2 text-sm qt-text-primary qt-shadow-sm hover:qt-bg-muted disabled:cursor-not-allowed disabled:opacity-60"
          [disabled]="saving()"
          (click)="close.emit()"
        >
          Cancel
        </button>
        <button
          type="submit"
          form="qt-group-create-form"
          class="inline-flex items-center rounded-lg bg-primary px-4 py-2 text-sm font-semibold text-primary-foreground shadow hover:qt-bg-primary/90 disabled:cursor-not-allowed disabled:opacity-60"
          [disabled]="saving() || !name().trim()"
        >
          {{ saving() ? 'Creating...' : 'Create' }}
        </button>
      </div>
    </qt-modal>
  `,
})
export class GroupCreateDialog {
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
      const group = await createGroup(this.core, name, this.description() || null);
      this.toasts.showSuccess('Group created successfully!');
      this.created.emit(group.id);
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to create group');
    } finally {
      this.saving.set(false);
    }
  }
}
