import { ChangeDetectionStrategy, Component, signal } from '@angular/core';

import { Icon } from '../../../ui/icon';
import { ExportDialog } from './export-dialog';
import { ImportDialog } from './import-dialog';

/**
 * The Import / Export card (v4 `components/tools/import-export-card.tsx`): two
 * buttons opening the {@link ExportDialog} and {@link ImportDialog}. v4's inner
 * `qt-card`/h2 are dropped — the CollapsibleCard already supplies the card + the
 * "Import / Export" title (matching the other tools cards in this tab).
 */
@Component({
  selector: 'qt-import-export-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, ExportDialog, ImportDialog],
  template: `
    <div>
      <div class="flex items-start gap-4 mb-6">
        <div class="p-3 rounded-lg qt-bg-info/10"><qt-icon name="cloud-upload" class="h-8 w-8 qt-text-info" /></div>
        <p class="qt-text-small">
          Export individual entity types or import from Quilltap export files (.qtap)
        </p>
      </div>

      <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
        <button
          type="button"
          class="qt-button qt-button-primary flex items-center justify-center gap-2"
          (click)="showExport.set(true)"
        >
          <qt-icon name="download" class="h-5 w-5" />
          Export Data
        </button>
        <button
          type="button"
          class="qt-button qt-button-secondary flex items-center justify-center gap-2"
          (click)="showImport.set(true)"
        >
          <qt-icon name="upload" class="h-5 w-5" />
          Import Data
        </button>
      </div>

      @if (showExport()) {
        <qt-export-dialog (close)="showExport.set(false)" />
      }
      @if (showImport()) {
        <qt-import-dialog (close)="showImport.set(false)" />
      }
    </div>
  `,
})
export class ImportExportCard {
  protected readonly showExport = signal(false);
  protected readonly showImport = signal(false);
}
