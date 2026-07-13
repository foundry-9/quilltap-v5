import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

import { Modal } from '../../ui/modal';
import type { DocumentStore } from './scriptorium.api';

/**
 * The fs→database convert confirm (v4 `ConvertToDatabaseDialog.tsx`): warns
 * about the image-transcoding caveat, reassures the source files stay on disk.
 * Confirm fires the live-guarded `mountConvert` — the server answers the typed
 * refusal (P4.6y), surfaced loudly by the list store's error flash.
 */
@Component({
  selector: 'qt-convert-store-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal
      [title]="'Convert “' + store().name + '” to Database-Backed Storage'"
      maxWidth="lg"
      (close)="close.emit()"
    >
      <div class="space-y-3">
        <p class="qt-text-small">
          Every file currently indexed in this document store will be read from
          <code class="mx-1 rounded qt-bg-muted px-1.5 py-0.5 text-xs">{{ store().basePath }}</code>
          and stored inside the encrypted mount-index database.
        </p>
        <ul class="list-disc pl-5 qt-text-small space-y-1">
          <li>Text files (<code>.md</code>, <code>.txt</code>) are stored as documents you can edit in-app.</li>
          <li>Binary files (<code>.pdf</code>, <code>.docx</code>, images) are stored as blobs.</li>
          <li><strong>Existing chunks and embeddings are preserved</strong> — no re-embedding required.</li>
          <li><strong>Your files on disk are left exactly where they are.</strong> You may delete them afterwards if you wish.</li>
        </ul>
        <div class="qt-alert-info">
          <p class="text-xs">
            <strong>Image caveat:</strong> images are transcoded to WebP on storage. A later Deconvert
            will round-trip them as <code>.webp</code>, not their original format.
          </p>
        </div>
      </div>

      <div qt-modal-footer class="flex gap-2 justify-end">
        <button type="button" class="qt-button-secondary" (click)="close.emit()">Cancel</button>
        <button type="button" class="qt-button-primary" (click)="confirm.emit()">Convert</button>
      </div>
    </qt-modal>
  `,
})
export class ConvertStoreDialog {
  readonly store = input.required<DocumentStore>();
  readonly close = output<void>();
  readonly confirm = output<void>();
}
