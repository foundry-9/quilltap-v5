import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import { Modal } from '../ui/modal';
import { formatBytes } from '../ui/format-bytes';
import type { ConflictResolution, FileConflictInfo } from './chat-files.api';

const CONFLICT_DESCRIPTION: Record<FileConflictInfo['conflictType'], string> = {
  filename: 'A file with this name already exists in the project.',
  content: 'This exact file already exists in the project (with a different name).',
  both: 'This exact file already exists with the same name.',
};

/**
 * FileConflictDialog — the duplicate-upload resolver (a port of v4
 * `components/chat/FileConflictDialog.tsx`). Shown when a chat-file upload
 * collides on name/content; offers Replace / Keep Both / Skip. Microcopy is
 * v4-verbatim.
 */
@Component({
  selector: 'qt-file-conflict-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal title="File Already Exists" maxWidth="lg" [closeOnBackdrop]="!resolving()" (close)="cancel.emit()">
      <div class="space-y-4">
        <p class="qt-text-secondary">{{ description() }}</p>

        <div class="p-3 qt-bg-muted rounded-lg">
          <div class="flex items-center gap-2 mb-2">
            <span class="text-lg">📄</span>
            <span class="font-medium">Existing File</span>
          </div>
          <div class="text-sm space-y-1 ml-7">
            <div class="font-medium">{{ conflict().existingFile.filename }}</div>
            <div class="qt-text-xs qt-text-secondary">{{ bytes(conflict().existingFile.size) }}</div>
          </div>
        </div>

        <div class="p-3 qt-bg-muted rounded-lg">
          <div class="flex items-center gap-2 mb-2">
            <span class="text-lg">📎</span>
            <span class="font-medium">New File</span>
          </div>
          <div class="text-sm space-y-1 ml-7">
            <div class="font-medium">{{ conflict().newFile.filename }}</div>
            <div class="qt-text-xs qt-text-secondary">{{ bytes(conflict().newFile.size) }}</div>
          </div>
        </div>

        <div class="qt-text-xs qt-text-secondary space-y-1 border-t pt-3">
          <p><strong>Replace:</strong> Delete the existing file and upload the new one.</p>
          <p><strong>Keep Both:</strong> Upload the new file with a modified name (e.g., "file (1).pdf").</p>
          <p><strong>Skip Upload:</strong> Cancel and keep the existing file.</p>
        </div>
      </div>

      <div qt-modal-footer class="flex flex-col sm:flex-row justify-end gap-2">
        <button type="button" class="qt-button-secondary qt-button-sm" [disabled]="resolving()" (click)="cancel.emit()">
          Cancel
        </button>
        <button type="button" class="qt-button-secondary qt-button-sm" [disabled]="resolving()" (click)="resolve.emit('skip')">
          Skip Upload
        </button>
        <button type="button" class="qt-button-secondary qt-button-sm" [disabled]="resolving()" (click)="resolve.emit('keepBoth')">
          Keep Both
        </button>
        <button type="button" class="qt-button-primary qt-button-sm" [disabled]="resolving()" (click)="resolve.emit('replace')">
          Replace
        </button>
      </div>
    </qt-modal>
  `,
})
export class FileConflictDialog {
  readonly conflict = input.required<FileConflictInfo>();
  readonly resolving = input(false);

  readonly resolve = output<ConflictResolution>();
  readonly cancel = output<void>();

  protected readonly description = computed(() => CONFLICT_DESCRIPTION[this.conflict().conflictType]);

  protected bytes(n: number): string {
    return formatBytes(n);
  }
}
