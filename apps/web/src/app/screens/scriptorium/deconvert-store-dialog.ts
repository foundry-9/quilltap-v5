import { ChangeDetectionStrategy, Component, computed, input, output, signal } from '@angular/core';

import { Modal } from '../../ui/modal';
import { DirectoryPicker } from './directory-picker';
import type { DocumentStore } from './scriptorium.api';

/**
 * The database→fs deconvert dialog (v4 `DeconvertToFilesystemDialog.tsx`):
 * prompts for a target directory (absolute, must be empty/nonexistent) and fires
 * the live-guarded `mountDeconvert` — the server answers the typed refusal
 * (P4.6y), surfaced loudly by the list store's error flash.
 */
@Component({
  selector: 'qt-deconvert-store-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, DirectoryPicker],
  template: `
    <qt-modal
      [title]="'Deconvert “' + store().name + '” to Filesystem Storage'"
      maxWidth="lg"
      (close)="close.emit()"
    >
      <form id="qt-deconvert-form" (ngSubmit)="submit()">
        <div class="space-y-3 mb-5">
          <p class="qt-text-small">
            Every document and blob in this store will be written out to disk under the target
            directory. The store will then switch to filesystem-backed, watching that directory for
            live changes.
          </p>
          <ul class="list-disc pl-5 qt-text-small space-y-1">
            <li>The target directory must be <strong>empty</strong> or not yet exist (it will be created).</li>
            <li>Existing chunks and embeddings are preserved — no re-embedding required.</li>
            <li>Files leave the encrypted mount-index database and land on disk as plain bytes.</li>
          </ul>
        </div>
        <div>
          <label class="qt-label mb-2 block">Target directory</label>
          <qt-directory-picker [(value)]="targetPath" [required]="true" placeholder="/absolute/path/to/export" />
          <p class="mt-1 text-xs qt-text-secondary">Absolute filesystem path. Must be empty or nonexistent.</p>
        </div>
      </form>

      <div qt-modal-footer class="flex gap-2 justify-end">
        <button type="button" class="qt-button-secondary" (click)="close.emit()">Cancel</button>
        <button
          type="submit"
          form="qt-deconvert-form"
          class="qt-button-primary"
          [disabled]="!canSubmit()"
        >
          Deconvert
        </button>
      </div>
    </qt-modal>
  `,
})
export class DeconvertStoreDialog {
  readonly store = input.required<DocumentStore>();
  readonly close = output<void>();
  readonly confirm = output<string>();

  protected readonly targetPath = signal('');
  protected readonly canSubmit = computed(() => {
    const t = this.targetPath().trim();
    return t.length > 0 && t.startsWith('/');
  });

  protected submit(): void {
    if (!this.canSubmit()) {
      return;
    }
    this.confirm.emit(this.targetPath().trim());
  }
}
