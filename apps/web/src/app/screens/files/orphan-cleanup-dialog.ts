import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import { formatBytes } from '../../ui/format-bytes';
import { Modal } from '../../ui/modal';

/** The dry-run report shape (v4 `cleanup-orphans` dry response). */
export interface OrphanCleanupStats {
  orphanedCount: number;
  rescuedCount: number;
  duplicateCount: number;
  uniqueCount: number;
  totalSize: number;
  uniqueSize: number;
}

/**
 * The orphan-cleanup dialog (v4 `components/files/OrphanCleanupModal.tsx`) —
 * shown AFTER the dry run so the report precedes the choice. Offers "Relocate to
 * /orphans/" (move unique, drop duplicates) or "Delete All". Pure presentational
 * (the parent runs the wet dispatch); the whimsical copy is v4-verbatim.
 */
@Component({
  selector: 'qt-orphan-cleanup-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal title="Untracked Files Detected" maxWidth="lg" (close)="cancel.emit()">
      <div class="space-y-4">
        <p class="text-base font-semibold">
          We have discovered <strong>{{ stats().orphanedCount }} files</strong> loitering about the
          premises without proper documentation — rather like uninvited guests at a garden party who
          nonetheless appear to have brought their own sandwiches.
        </p>

        @if (stats().rescuedCount > 0) {
          <div class="p-3 rounded qt-bg-success/10 qt-border-success/30 border">
            <p class="qt-text-small">
              <strong>{{ stats().rescuedCount }}</strong>
              {{ stats().rescuedCount === 1 ? 'file has' : 'files have' }} been identified as still
              serving in active duty — attached to character galleries or avatars — and shall be
              restored to good standing forthwith.
            </p>
          </div>
        }

        @if (stats().duplicateCount > 0) {
          <div>
            <p class="qt-text-small qt-text-secondary">
              Of these, <strong>{{ stats().duplicateCount }}</strong> are mere duplicates of files
              already in good standing and shall be disposed of regardless of your choice below.
            </p>
          </div>
        }

        <div>
          <p class="qt-text-small qt-text-secondary">
            The remaining <strong>{{ stats().uniqueCount }} unique files</strong> ({{
              uniqueSizeLabel()
            }}) await your instruction.
          </p>
        </div>

        <div class="space-y-3 pt-2">
          <div>
            <p class="qt-text-small font-medium mb-1">Relocate to /orphans/</p>
            <p class="qt-text-small qt-text-secondary">
              Move unique files to a dedicated folder for your later review. All duplicates will be
              removed.
            </p>
          </div>
          <div>
            <p class="qt-text-small font-medium mb-1">Delete All</p>
            <p class="qt-text-small qt-text-secondary">
              Permanently remove all untracked files, both unique and duplicates. This action cannot
              be undone.
            </p>
          </div>
        </div>
      </div>

      <div qt-modal-footer class="flex justify-end gap-2">
        <button
          type="button"
          class="qt-button qt-button-secondary"
          [disabled]="isProcessing()"
          (click)="cancel.emit()"
        >
          Cancel
        </button>
        <button
          type="button"
          class="qt-button qt-button-secondary"
          [disabled]="isProcessing() || stats().uniqueCount === 0"
          [title]="moveTooltip()"
          (click)="move.emit()"
        >
          {{ isProcessing() ? 'Processing...' : 'Relocate to /orphans/' }}
        </button>
        <button
          type="button"
          class="qt-button bg-destructive qt-text-on-destructive disabled:opacity-50"
          [disabled]="isProcessing()"
          (click)="delete.emit()"
        >
          {{ isProcessing() ? 'Processing...' : 'Delete All' }}
        </button>
      </div>
    </qt-modal>
  `,
})
export class OrphanCleanupDialog {
  readonly stats = input.required<OrphanCleanupStats>();
  readonly isProcessing = input(false);

  readonly move = output<void>();
  readonly delete = output<void>();
  readonly cancel = output<void>();

  protected readonly uniqueSizeLabel = computed(() => formatBytes(this.stats().uniqueSize));

  protected readonly moveTooltip = computed(() =>
    this.stats().uniqueCount === 0
      ? 'No unique files to relocate'
      : 'Move unique files to an /orphans/ folder for later review. Duplicates will be removed.',
  );
}
