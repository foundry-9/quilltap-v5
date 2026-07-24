import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute } from '@angular/router';

import { CollapsibleCard } from '../../../ui/collapsible-card';
import { AutoLockSettingsCard } from './auto-lock-settings-card';
import { BackupRestoreCard } from './backup-restore-card';
import { ChangePassphraseCard } from './change-passphrase-card';
import { DeleteDataCard } from './delete-data-card';
import { ImportExportCard } from './import-export-card';
import { LlmLoggingSettingsCard } from './llm-logging-settings-card';
import { LlmLogsCard } from './llm-logs-card';
import { TasksQueueCard } from './tasks-queue-card';

/**
 * The Data & System tab (v4 `components/settings/tabs/DataSystemTabContent.tsx`,
 * subsystem Prospero): the collapsible-card stack in v4's exact order —
 * Encryption Passphrase, Auto-Lock, (Plugins — WON'T-PORT), Backup & Restore,
 * Import / Export, LLM Logging, Tasks Queue, LLM Logs, Delete All Data. Cards
 * default CLOSED (v4's CollapsibleCard default; this tab passes no `defaultOpen`)
 * and honour the `?section=` deep link.
 *
 * The Plugins slot is a LOCKED WON'T-PORT decision (`phase-4.md:273-276`) — it
 * renders NOTHING (the Memory-tab convention: no dead cards).
 */
@Component({
  selector: 'qt-settings-system',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    CollapsibleCard,
    ChangePassphraseCard,
    AutoLockSettingsCard,
    BackupRestoreCard,
    ImportExportCard,
    LlmLoggingSettingsCard,
    TasksQueueCard,
    LlmLogsCard,
    DeleteDataCard,
  ],
  template: `
    <div>
      <p class="qt-text-small qt-text-muted italic mb-6">
        Task queue, capabilities report, and LLM logs
      </p>

      <div class="space-y-4">
        <qt-collapsible-card
          title="Encryption Passphrase"
          description="Change or remove the passphrase protecting your encryption key"
          sectionId="encryption-passphrase"
          [forceOpen]="section() === 'encryption-passphrase'"
        >
          <qt-change-passphrase-card />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Auto-Lock"
          description="Automatically lock Quilltap after a period of inactivity"
          sectionId="auto-lock"
          [forceOpen]="section() === 'auto-lock'"
        >
          <qt-auto-lock-settings-card />
        </qt-collapsible-card>

        <!-- Plugins (plugins) — WON'T-PORT (locked decision, phase-4.md:273-276);
             renders nothing (the Memory-tab convention: no dead cards). -->

        <qt-collapsible-card
          title="Backup & Restore"
          description="Create and restore backups of your data"
          sectionId="backup-restore"
          [forceOpen]="section() === 'backup-restore'"
        >
          <qt-backup-restore-card />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Import / Export"
          description="Import and export characters, chats, and settings"
          sectionId="import-export"
          [forceOpen]="section() === 'import-export'"
        >
          <qt-import-export-card />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="LLM Logging"
          description="Configure LLM request logging"
          sectionId="llm-logging"
          [forceOpen]="section() === 'llm-logging'"
        >
          <qt-llm-logging-settings-card />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Tasks Queue"
          description="View and manage background tasks"
          sectionId="tasks-queue"
          [forceOpen]="section() === 'tasks-queue'"
        >
          <qt-tasks-queue-card />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="LLM Logs"
          description="View detailed logs of LLM requests and responses"
          sectionId="llm-logs"
          [forceOpen]="section() === 'llm-logs'"
        >
          <qt-llm-logs-card />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Delete All Data"
          description="Permanently delete all application data"
          sectionId="delete-all-data"
          [forceOpen]="section() === 'delete-all-data'"
        >
          <qt-delete-data-card />
        </qt-collapsible-card>
      </div>
    </div>
  `,
})
export class SystemTab {
  // Optional so the tab renders when hosted as a workspace tab (no ActivatedRoute).
  private readonly route = inject(ActivatedRoute, { optional: true });
  private readonly queryParams = this.route
    ? toSignal(this.route.queryParamMap, { requireSync: true })
    : undefined;

  protected readonly section = computed(() => this.queryParams?.().get('section') ?? null);
}
