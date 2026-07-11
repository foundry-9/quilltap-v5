import { ChangeDetectionStrategy, Component, output } from '@angular/core';

import { Modal } from '../../ui/modal';

/**
 * The Delete Project confirmation (v4 `DeleteProjectDialog.tsx`): a confirm
 * dialog whose copy makes the disassociation-not-deletion semantics explicit.
 */
@Component({
  selector: 'qt-project-delete-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal title="Delete Project" maxWidth="md" (close)="close.emit()">
      <p class="qt-text-small">
        Are you sure you want to delete this project? Chats and files will be disassociated but not
        deleted.
      </p>
      <div qt-modal-footer class="flex gap-2 justify-end">
        <button
          type="button"
          class="inline-flex items-center rounded-lg border qt-border-default qt-bg-card px-4 py-2 text-sm qt-text-primary qt-shadow-sm hover:qt-bg-muted"
          (click)="close.emit()"
        >
          Cancel
        </button>
        <button
          type="button"
          class="inline-flex items-center rounded-lg bg-destructive px-4 py-2 text-sm font-semibold qt-text-destructive-foreground shadow hover:qt-bg-destructive/90"
          (click)="confirm.emit()"
        >
          Delete
        </button>
      </div>
    </qt-modal>
  `,
})
export class ProjectDeleteDialog {
  readonly close = output<void>();
  readonly confirm = output<void>();
}
