import { ChangeDetectionStrategy, Component, inject, output, signal } from '@angular/core';

import { CoreClient } from '../../../core/core-client';
import { CoreDispatchError } from '../../../core/core-contract';
import { Modal } from '../../../ui/modal';
import { ToastService } from '../../../ui/toast.service';
import { triggerUrlDownload } from '../../../core/download-utils';

/**
 * The Create Backup dialog (v4 `components/tools/backup-dialog.tsx`): a single
 * step. On confirm it creates the backup (`systemBackupCreate` → `{backupId,
 * filename?}`) then streams the single-use zip via `GET /system/backup/{id}`
 * (a web-edge leg, reached with an anchor-click through `apiUrl()`). v4's create
 * response has no `filename`, so the client fallback always runs (`:40`). v4's
 * success toast ("Backup downloaded successfully") is still OWED — the download
 * leg is an anchor click with no completion signal to hang it on; recorded as
 * an OPEN census row (P4.25).
 */
@Component({
  selector: 'qt-backup-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal title="Create Backup" maxWidth="md" (close)="handleClose()">
      <p class="qt-dialog-description qt-text-small mb-4">
        Create a complete backup of your Quilltap data
      </p>

      <div class="p-4 qt-bg-muted/50 rounded-lg">
        <p class="text-sm qt-text-primary mb-2">Your backup will include:</p>
        <ul class="qt-text-small space-y-1 list-disc list-inside">
          <li>All characters, chats, and messages</li>
          <li>Memories and relationships</li>
          <li>All uploaded files and images</li>
          <li>Connection, image, and embedding profiles</li>
          <li>Templates, projects, and character groups</li>
          <li>Plugin configurations and npm plugins</li>
        </ul>
      </div>

      @if (error(); as msg) {
        <div class="mt-4 p-3 qt-bg-destructive/10 border qt-border-destructive rounded-lg">
          <p class="text-sm qt-text-destructive">{{ msg }}</p>
        </div>
      }

      <div qt-modal-footer class="flex gap-3 justify-end w-full">
        <button
          type="button"
          class="qt-button qt-button-secondary"
          [disabled]="loading()"
          (click)="handleClose()"
        >
          Cancel
        </button>
        <button
          type="button"
          class="qt-button qt-button-primary"
          [disabled]="loading()"
          (click)="createBackup()"
        >
          @if (loading()) {
            <span class="inline-block animate-spin rounded-full h-4 w-4 border-b-2 border-current"></span>
          }
          {{ loading() ? 'Creating Backup...' : 'Download Backup' }}
        </button>
      </div>
    </qt-modal>
  `,
})
export class BackupDialog {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly close = output<void>();

  protected readonly loading = signal(false);
  protected readonly error = signal<string | null>(null);

  protected async createBackup(): Promise<void> {
    this.error.set(null);
    this.loading.set(true);
    try {
      const data = await this.core.dispatchData({ type: 'systemBackupCreate' });
      const backupId = data['backupId'] as string | undefined;
      if (backupId) {
        const filename =
          (data['filename'] as string | undefined) ||
          `quilltap-backup-${new Date().toISOString().replace(/[:.]/g, '-')}.zip`;
        triggerUrlDownload(`/api/v1/system/backup/${backupId}`, filename);
      }
      this.close.emit();
    } catch (err) {
      const message =
        err instanceof CoreDispatchError || err instanceof Error
          ? err.message || 'Failed to create backup'
          : 'Failed to create backup';
      this.error.set(message);
      // v4 `backup-dialog.tsx:53-58` does BOTH: the inline error AND the toast.
      this.toasts.showError(message);
    } finally {
      this.loading.set(false);
    }
  }

  protected handleClose(): void {
    this.error.set(null);
    this.close.emit();
  }
}
