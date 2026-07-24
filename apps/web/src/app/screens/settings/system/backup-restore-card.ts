import { ChangeDetectionStrategy, Component, signal } from '@angular/core';

import { Icon } from '../../../ui/icon';
import { BackupDialog } from './backup-dialog';
import { RestoreDialog } from './restore-dialog';

/**
 * The Backup & Restore card (v4 `components/tools/backup-restore-card.tsx`): two
 * buttons opening the {@link BackupDialog} and {@link RestoreDialog}, plus the
 * standing info blurb. On a completed restore the app reloads (v4 `:66`) — a
 * replace-mode restore replaced everything, so a full reboot is the honest move.
 */
@Component({
  selector: 'qt-backup-restore-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, BackupDialog, RestoreDialog],
  template: `
    <div>
      <div class="flex items-start gap-4 mb-6">
        <div class="flex-1">
          <p class="qt-text-small">
            Download a complete backup or restore from a previous backup file
          </p>
        </div>
        <div class="flex-shrink-0 text-primary"><qt-icon name="upload" class="w-8 h-8" /></div>
      </div>

      <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
        <button
          type="button"
          class="qt-button qt-button-primary flex items-center justify-center gap-2"
          (click)="showBackup.set(true)"
        >
          <qt-icon name="upload" class="w-5 h-5" />
          Create Backup
        </button>
        <button
          type="button"
          class="qt-button qt-button-secondary flex items-center justify-center gap-2"
          (click)="showRestore.set(true)"
        >
          <qt-icon name="download" class="w-5 h-5" />
          Restore from Backup
        </button>
      </div>

      <div class="mt-6 p-4 qt-bg-muted/30 rounded-lg">
        <p class="qt-text-small">
          Backups include all your characters, chats, files, settings, and plugins. Store your backup
          files safely - they contain everything needed to restore your Quilltap environment.
        </p>
      </div>

      @if (showBackup()) {
        <qt-backup-dialog (close)="showBackup.set(false)" />
      }
      @if (showRestore()) {
        <qt-restore-dialog
          (close)="showRestore.set(false)"
          (restoreComplete)="onRestoreComplete()"
        />
      }
    </div>
  `,
})
export class BackupRestoreCard {
  protected readonly showBackup = signal(false);
  protected readonly showRestore = signal(false);

  protected onRestoreComplete(): void {
    this.showRestore.set(false);
    // v4 `:66` — a replace-mode restore replaced everything; reboot the app.
    if (typeof window !== 'undefined') {
      window.location.reload();
    }
  }
}
