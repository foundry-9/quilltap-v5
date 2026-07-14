import { ChangeDetectionStrategy, Component, inject, input, output, signal } from '@angular/core';

import { CoreClient } from '../../core/core-client';
import { Modal } from '../../ui/modal';

/**
 * Create a new folder in the current directory (v4
 * `FolderManagement/CreateFolderModal.tsx`). The new path is derived from the
 * current folder + trimmed name; dispatch is idempotent (v4 `alreadyExists`).
 * On success the parent refetches and navigates into the created folder.
 */
@Component({
  selector: 'qt-create-folder-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal title="Create Folder" maxWidth="md" (close)="close.emit()">
      <div class="mb-4">
        <label for="folder-name" class="qt-label mb-1">Folder Name</label>
        <input
          id="folder-name"
          type="text"
          class="qt-input"
          placeholder="Enter folder name..."
          maxlength="100"
          [value]="folderName()"
          [disabled]="saving()"
          (input)="folderName.set($any($event.target).value)"
          (keydown.enter)="onCreate()"
        />
      </div>
      <p class="qt-text-xs qt-text-secondary">
        Will be created in: <span class="font-mono">{{ currentFolder() }}</span>
      </p>
      @if (error(); as e) {
        <p class="qt-alert-error text-sm mt-2" role="alert">{{ e }}</p>
      }

      <div qt-modal-footer class="flex justify-end gap-2">
        <button
          type="button"
          class="qt-button qt-button-secondary"
          [disabled]="saving()"
          (click)="close.emit()"
        >
          Cancel
        </button>
        <button
          type="button"
          class="qt-button qt-button-primary"
          [disabled]="saving() || !folderName().trim()"
          (click)="onCreate()"
        >
          {{ saving() ? 'Creating...' : 'Create Folder' }}
        </button>
      </div>
    </qt-modal>
  `,
})
export class CreateFolderDialog {
  private readonly core = inject(CoreClient);

  readonly currentFolder = input.required<string>();
  readonly projectId = input<string | null>(null);

  readonly close = output<void>();
  /** The created (or existing) folder path — the parent navigates into it. */
  readonly success = output<string>();

  protected readonly folderName = signal('');
  protected readonly saving = signal(false);
  protected readonly error = signal<string | null>(null);

  protected async onCreate(): Promise<void> {
    const trimmedName = this.folderName().trim();
    if (!trimmedName) {
      this.error.set('Folder name cannot be empty');
      return;
    }
    const current = this.currentFolder();
    const newFolderPath = current === '/' ? `/${trimmedName}/` : `${current}${trimmedName}/`;
    this.saving.set(true);
    this.error.set(null);
    try {
      const data = await this.core.dispatchData({
        type: 'filesFolderCreate',
        path: newFolderPath,
        projectId: this.projectId() ?? undefined,
      });
      const folder = data['folder'] as { path?: string } | undefined;
      this.success.emit(folder?.path || newFolderPath);
      this.close.emit();
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'Failed to create folder');
    } finally {
      this.saving.set(false);
    }
  }
}
