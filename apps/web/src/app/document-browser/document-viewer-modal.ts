import { ChangeDetectionStrategy, Component, inject, input, output } from '@angular/core';

import { Modal } from '../ui/modal';
import { ToastService } from '../ui/toast.service';
import { DocumentViewer, downloadDocument, type ViewableEntry } from './document-viewer';

/**
 * v4 `DocumentViewerModal` (`DocumentViewer.tsx:127-155`, `c34986421`): a
 * store file in a `4xl` dialog titled with its name, the footer naming its
 * path `· read-only` beside Download and Close (P4.D387 item 9). The host
 * renders it only while a file is being viewed (v4's `isOpen={entry !== null}`).
 */
@Component({
  selector: 'qt-document-viewer-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, DocumentViewer],
  template: `
    <qt-modal [title]="entry().fileName" maxWidth="4xl" (close)="closed.emit()">
      @for (e of [entry()]; track e.id) {
        <qt-document-viewer [entry]="e" />
      }
      <div qt-modal-footer class="flex items-center justify-between gap-2">
        <span class="qt-text-xs qt-text-secondary truncate" [attr.title]="entry().relativePath"
          >{{ entry().relativePath }} · read-only</span
        >
        <div class="flex gap-2">
          <button type="button" class="qt-button-secondary qt-button-sm" (click)="download()">
            Download
          </button>
          <button type="button" class="qt-button-primary qt-button-sm" (click)="closed.emit()">
            Close
          </button>
        </div>
      </div>
    </qt-modal>
  `,
})
export class DocumentViewerModal {
  private readonly toasts = inject(ToastService);
  readonly entry = input.required<ViewableEntry>();
  readonly closed = output<void>();

  protected download(): void {
    void downloadDocument(this.toasts, this.entry());
  }
}
