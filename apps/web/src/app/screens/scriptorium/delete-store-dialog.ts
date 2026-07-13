import { ChangeDetectionStrategy, Component, output } from '@angular/core';

import { Modal } from '../../ui/modal';

/** The Delete Document Store confirm (v4 `DeleteDocumentStoreDialog.tsx`). */
@Component({
  selector: 'qt-delete-store-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal title="Delete Document Store" maxWidth="md" (close)="close.emit()">
      <p class="qt-text-small">
        Are you sure you want to delete this document store? All indexed files, text chunks, and
        embeddings will be permanently removed. The original files on disk will not be affected.
      </p>
      <div qt-modal-footer class="flex gap-2 justify-end">
        <button type="button" class="qt-button-secondary" (click)="close.emit()">Cancel</button>
        <button type="button" class="qt-button-destructive" (click)="confirm.emit()">Delete</button>
      </div>
    </qt-modal>
  `,
})
export class DeleteStoreDialog {
  readonly close = output<void>();
  readonly confirm = output<void>();
}
