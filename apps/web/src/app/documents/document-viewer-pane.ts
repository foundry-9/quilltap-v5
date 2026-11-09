import { ChangeDetectionStrategy, Component, computed, inject, input, output } from '@angular/core';

import type { DocumentViewTarget } from '../core/core-contract';
import {
  DocumentViewer,
  downloadDocument,
  type ViewableEntry,
} from '../document-browser/document-viewer';
import { Icon } from '../ui/icon';
import { ToastService } from '../ui/toast.service';

/**
 * A store file Document Mode shows read-only — a port of v4
 * `app/salon/[id]/components/DocumentViewerPane.tsx` (`5efe53ea7`, pin
 * `624d0d25c`; P4.D387 item 17): the document header (badge, title, Download,
 * Close), the `Read-only` ribbon, and the store viewer over the server's
 * `viewer` target (C14). No autosave, no frontmatter, no toolbar.
 *
 * v4 binds it in `DocumentPaneBinding.tsx`; here `document-pane.ts` renders it
 * in place of the editor (ruling L-3), so neither the Salon host
 * (`salon-conversation.ts`) nor the standalone view changes.
 */
@Component({
  selector: 'qt-document-viewer-pane',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [DocumentViewer, Icon],
  template: `
    <div class="flex flex-col h-full" data-testid="document-viewer-pane">
      <div class="qt-doc-header">
        <span class="qt-doc-badge">Document</span>
        <span class="qt-doc-title" [attr.title]="viewer().relativePath">{{ displayTitle() }}</span>
        <div class="flex items-center gap-1">
          <button
            type="button"
            class="qt-doc-header-button"
            (click)="download()"
            title="Download"
            aria-label="Download document"
          >
            <qt-icon name="download" class="w-4 h-4" />
          </button>
          <button
            type="button"
            class="qt-doc-header-button"
            (click)="closeDocument.emit()"
            title="Close document"
            aria-label="Close document"
          >
            <qt-icon name="close" class="w-4 h-4" />
          </button>
        </div>
      </div>
      <div class="qt-doc-uri-row" role="note">
        <span class="qt-doc-uri-label">Read-only</span>
        <span class="qt-doc-uri-value">View only — this kind of file cannot be edited here.</span>
      </div>
      <div class="flex-1 min-h-0 overflow-auto relative">
        @for (e of [entry()]; track e.id) {
          <qt-document-viewer [entry]="e" />
        }
      </div>
    </div>
  `,
})
export class DocumentViewerPane {
  private readonly toasts = inject(ToastService);

  readonly displayTitle = input.required<string>();
  readonly viewer = input.required<DocumentViewTarget>();
  readonly closeDocument = output<void>();

  /** v4's entry built from the target: id `<mountPointId>:<relativePath>`, created = updated. */
  protected readonly entry = computed<ViewableEntry>(() => {
    const v = this.viewer();
    return {
      id: `${v.mountPointId}:${v.relativePath}`,
      mountPointId: v.mountPointId,
      relativePath: v.relativePath,
      fileName: v.fileName,
      fileType: v.fileType as ViewableEntry['fileType'],
      fileSizeBytes: v.fileSizeBytes,
      originalMimeType: v.originalMimeType,
      description: '',
      createdAt: v.updatedAt,
      updatedAt: v.updatedAt,
    };
  });

  protected download(): void {
    void downloadDocument(this.toasts, this.entry());
  }
}
