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
 * response has no `filename`, so the client fallback always runs (`:40`). Both
 * of v4's toasts are here (P4.28): the error one alongside the inline message
 * (v4 does both), and the success one right after the download is triggered.
 * P4.D47 added v4 `7189a968`'s opt-in `compact` checkbox (default OFF).
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

      <label class="mt-4 flex items-start gap-3 cursor-pointer">
        <input
          type="checkbox"
          class="w-4 h-4 mt-0.5"
          [checked]="compact()"
          [disabled]="loading()"
          (change)="compact.set($any($event.target).checked)"
        />
        <span>
          <span class="text-sm qt-text-primary">Compact backup</span>
          <span class="block qt-text-small qt-text-secondary">
            A considerably slimmer archive — the search indexes are left behind and rebuilt
            after restoring. Everything you have written travels either way.
          </span>
        </span>
      </label>

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
  /**
   * v4 `backup-dialog.tsx:18-21`: full fidelity is the default, because a backup
   * restores the same instance — its search indexes are still valid on arrival,
   * and rebuilding them costs real time and money exactly when the user is
   * recovering from something.
   */
  protected readonly compact = signal(false);

  protected async createBackup(): Promise<void> {
    this.error.set(null);
    this.loading.set(true);
    try {
      // v4 `:33` sends `{ compact }` whether or not it is set; the server
      // defaults it to false and only the literal `true` engages compact.
      const data = await this.core.dispatchData({
        type: 'systemBackupCreate',
        compact: this.compact(),
      });
      // ⚠ v5-only key (dogfood #59): the names of files whose bytes could not be
      // read, absent when there were none. v4 warns to the server log and tells
      // the operator nothing, so they discover the loss at RESTORE time — the
      // 2026-08-03 walk hit 19 of them. Warn rather than error: the backup IS
      // valid and downloading, it is simply short of those files.
      const skipped = (data['skippedFiles'] as string[] | undefined) ?? [];
      if (skipped.length > 0) {
        this.toasts.showWarning(
          `${skipped.length} file${skipped.length === 1 ? '' : 's'} could not be read and ` +
            `${skipped.length === 1 ? 'is' : 'are'} missing from this backup: ${skipped.join(', ')}`,
        );
      }
      const backupId = data['backupId'] as string | undefined;
      if (backupId) {
        const filename =
          (data['filename'] as string | undefined) ||
          `quilltap-backup-${new Date().toISOString().replace(/[:.]/g, '-')}.zip`;
        triggerUrlDownload(`/api/v1/system/backup/${backupId}`, filename);
        // v4 `backup-dialog.tsx:43` toasts right after its own
        // `triggerUrlDownload` returns. The earlier note here — that the toast
        // was OWED because an anchor click gives no completion signal — read v4
        // as having one; it does not (its helper resolves on the same click), so
        // the honest port is v4's own sequence.
        this.toasts.showSuccess('Backup downloaded successfully');
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
