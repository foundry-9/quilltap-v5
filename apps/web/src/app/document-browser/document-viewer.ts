import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import { DomSanitizer, type SafeHtml, type SafeResourceUrl } from '@angular/platform-browser';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient, coreErrorMessage } from '../core/core-client';
import { CoreDispatchError, type DocMountBrowserEntry } from '../core/core-contract';
import { triggerBlobDownload } from '../core/download-utils';
import { formatFileDate, getFileIcon, getFileTypeLabel } from '../screens/files/file-model';
import { formatBytes } from '../ui/format-bytes';
import { ToastService } from '../ui/toast.service';
import { documentPreviewKind } from './document-actions';
import {
  buildMountFileRawUrl,
  fetchDocumentRenderedHtml,
  fetchDocumentText,
} from './document-store.api';
import { mountPointKeys } from './document-store.keys';
import { mimeTypeForName } from './preview-kind';
import { sanitizeDocxHtml } from './sanitize-docx';

/** What a viewer needs of a store file (v4 `ViewableEntry`). */
export type ViewableEntry = Pick<
  DocMountBrowserEntry,
  | 'id'
  | 'mountPointId'
  | 'relativePath'
  | 'fileName'
  | 'fileType'
  | 'fileSizeBytes'
  | 'originalMimeType'
  | 'description'
  | 'createdAt'
  | 'updatedAt'
>;

/** The shared preview fields of a store row (v4 `fileInfoOf`, the `FileInfo` the preview components read). */
export function fileInfoOf(entry: ViewableEntry) {
  return {
    id: entry.id,
    originalFilename: entry.fileName,
    filename: entry.fileName,
    mimeType: entry.originalMimeType || mimeTypeForName(entry.fileName),
    size: entry.fileSizeBytes,
    category: 'DOCUMENT' as const,
    description: entry.description || null,
    createdAt: entry.createdAt,
    updatedAt: entry.updatedAt,
    mountPointId: entry.mountPointId,
    relativePath: entry.relativePath,
  };
}

/**
 * v4 `downloadDocument` (`DocumentViewer.tsx:119-125`) over v4's
 * `downloadFetchedFile`: fetch the raw bytes, save them under the file's name;
 * a failure toasts v4's line.
 */
export async function downloadDocument(
  toasts: ToastService,
  entry: Pick<ViewableEntry, 'mountPointId' | 'relativePath' | 'fileName'>,
): Promise<void> {
  try {
    const res = await fetch(buildMountFileRawUrl(entry.mountPointId, entry.relativePath));
    // v4 `downloadFetchedFile`'s own sentence, image or not.
    if (!res.ok) throw new Error(`Failed to fetch image (${res.status})`);
    triggerBlobDownload(await res.blob(), entry.fileName);
  } catch (error) {
    toasts.showError(coreErrorMessage(error, 'The file would not download'));
  }
}

/** The 422 `not_available` refusal (`DOCX_RENDER_HTML_REFUSED`, R-E) — the viewer shows the fallback. */
function isRenderRefused(error: unknown): boolean {
  return (
    error instanceof CoreDispatchError &&
    (error.kind === 'not_available' || error.kind === 'not-available')
  );
}

/**
 * A store file shown read-only — a port of v4
 * `components/document-browser/DocumentViewer.tsx` (`c34986421`, pin
 * `624d0d25c`; P4.D387 item 9). The kind (`documentPreviewKind`) picks the
 * viewer:
 *
 * - `image` — the raw bytes (`?raw=1`) in an `<img>`;
 * - `pdf` — **`NATIVE_PDF_VIEWER`** (SPA ruling R10, a NAMED divergence): v4
 *   renders PDFs with `pdfjs-dist` (`FilePreviewPdf.tsx` — `Loading PDF...`,
 *   `Failed to load PDF. Try downloading instead.`); v5 adds no PDF
 *   dependency and hands the raw bytes to the platform's viewer through
 *   `<object type="application/pdf">`, whose fallback content is the
 *   download prompt. The raw leg is served `inline` with
 *   `X-Frame-Options: SAMEORIGIN` / `frame-ancestors 'self'`, so a
 *   same-origin `<object>` may embed it;
 * - `markdown` / `text` — the text read (`mountFileRead`, `encoding: 'utf-8'`:
 *   a text file stored as a blob reads as base64 by default) as the file
 *   preview's `<pre>` + Copy — the recorded rich-text deferral
 *   (`file-preview-modal.ts`: v4's ReactMarkdown, syntax highlighting,
 *   wikilinks and KaTeX are not ported);
 * - `docx` — the server's `render-html` through {@link sanitizeDocxHtml}; on
 *   the server's 422 `not_available` (**`DOCX_RENDER_HTML_REFUSED`**, SPA arm:
 *   v5 has no mammoth twin, R-E) the FALLBACK, not the error line; any other
 *   failure is v4's `The document would not open`;
 * - `unsupported` — v4 `FilePreviewFallback`: the type, size, `Added <date>`,
 *   and a download.
 */
@Component({
  selector: 'qt-document-viewer',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @switch (shown()) {
      @case ('image') {
        <div class="flex items-center justify-center w-full h-full min-h-[300px]">
          <img
            [src]="rawUrl()"
            [alt]="info().originalFilename || 'Image'"
            class="max-w-full max-h-[70vh] object-contain"
          />
        </div>
      }
      @case ('pdf') {
        <object
          type="application/pdf"
          [data]="pdfUrl()"
          class="w-full h-[70vh]"
          data-testid="document-viewer-pdf"
          [attr.aria-label]="info().originalFilename"
        >
          <div
            class="flex flex-col items-center justify-center w-full h-full min-h-[300px] text-center qt-text-secondary"
          >
            <div class="text-4xl mb-2">{{ pageIcon }}</div>
            <p>PDF preview isn't available yet — download to read it.</p>
            <a
              [href]="rawUrl()"
              [download]="info().originalFilename"
              class="qt-button qt-button-primary mt-4 inline-block"
              >Download PDF</a
            >
          </div>
        </object>
      }
      @case ('docx') {
        @if (rendered.isLoading()) {
          <div class="animate-pulse qt-text-secondary p-6">Unfolding the document…</div>
        } @else if (rendered.error()) {
          <div class="qt-text-destructive p-6">{{ renderError() }}</div>
        } @else {
          <div class="w-full overflow-y-auto max-h-[70vh] p-4">
            <div class="qt-markdown" [innerHTML]="docxHtml()"></div>
          </div>
        }
      }
      @case ('text') {
        <div class="qt-file-preview-text-container">
          <button
            type="button"
            class="qt-copy-button"
            [class.qt-copy-button-success]="copied()"
            [title]="copied() ? 'Copied!' : 'Copy to clipboard'"
            (click)="copyText()"
          >
            <span class="qt-copy-button-icon">{{ copied() ? '✓' : '📋' }}</span>
            <span class="qt-copy-button-text">{{ copied() ? 'Copied!' : 'Copy' }}</span>
          </button>
          <div class="qt-file-preview-scroll">
            @if (text.isLoading()) {
              <div class="qt-file-preview-loading">
                <div class="qt-file-preview-loading-text">Loading file...</div>
              </div>
            } @else if (text.error()) {
              <div class="qt-file-preview-empty">
                <div class="qt-file-preview-empty-icon">{{ pageCurlIcon }}</div>
                <p>{{ textError() }}</p>
              </div>
            } @else if (!text.data()) {
              <div class="qt-file-preview-empty">
                <div class="qt-file-preview-empty-icon">{{ pageCurlIcon }}</div>
                <p>No content available</p>
              </div>
            } @else {
              <pre class="qt-file-preview-code"><code>{{ text.data() }}</code></pre>
            }
          </div>
        </div>
      }
      @default {
        <div
          class="flex flex-col items-center justify-center w-full h-full min-h-[300px] text-center p-8"
          data-testid="document-viewer-fallback"
        >
          <div class="text-7xl mb-4">{{ fallbackIcon() }}</div>
          <h3 class="text-xl font-semibold mb-2 break-all max-w-md">
            {{ info().originalFilename }}
          </h3>
          <div class="qt-text-secondary space-y-1 mb-6">
            <p>{{ typeLabel() }}</p>
            <p>{{ sizeLabel() }}</p>
            <p>Added {{ addedLabel() }}</p>
          </div>
          <p class="qt-text-secondary mb-4">Preview not available for this file type</p>
          <a
            [href]="rawUrl()"
            [download]="info().originalFilename"
            class="qt-button qt-button-primary"
            >Download File</a
          >
        </div>
      }
    }
  `,
})
export class DocumentViewer {
  private readonly core = inject(CoreClient);
  private readonly sanitizer = inject(DomSanitizer);

  readonly entry = input.required<ViewableEntry>();

  protected readonly pageIcon = '\u{1F4C4}';
  protected readonly pageCurlIcon = '\u{1F4C3}';
  protected readonly copied = signal(false);

  protected readonly kind = computed(() => documentPreviewKind(this.entry()));
  protected readonly info = computed(() => fileInfoOf(this.entry()));
  protected readonly rawUrl = computed(() =>
    buildMountFileRawUrl(this.entry().mountPointId, this.entry().relativePath),
  );
  /** The `<object data>` is a resource URL; this is the instance's own same-origin raw leg. */
  protected readonly pdfUrl = computed<SafeResourceUrl>(() =>
    this.sanitizer.bypassSecurityTrustResourceUrl(this.rawUrl()),
  );

  protected readonly text = injectQuery(() => ({
    queryKey: mountPointKeys.content(this.entry().mountPointId, this.entry().relativePath),
    queryFn: () =>
      fetchDocumentText(this.core, this.entry().mountPointId, this.entry().relativePath),
    enabled: this.kind() === 'markdown' || this.kind() === 'text',
  }));

  protected readonly rendered = injectQuery(() => ({
    queryKey: mountPointKeys.rendered(this.entry().mountPointId, this.entry().relativePath),
    queryFn: () =>
      fetchDocumentRenderedHtml(this.core, this.entry().mountPointId, this.entry().relativePath),
    enabled: this.kind() === 'docx',
    retry: false,
  }));

  /** The arm rendered: markdown reads as text, and a refused `.docx` as the fallback. */
  protected readonly shown = computed(() => {
    const kind = this.kind();
    if (kind === 'markdown') return 'text';
    if (kind === 'docx' && this.rendered.error() && isRenderRefused(this.rendered.error()))
      return 'unsupported';
    return kind;
  });

  /** DOMPurify already sanitised it (v4's `dangerouslySetInnerHTML`); Angular's sanitizer would strip it twice. */
  protected readonly docxHtml = computed<SafeHtml>(() =>
    this.sanitizer.bypassSecurityTrustHtml(sanitizeDocxHtml(this.rendered.data() ?? '')),
  );
  protected readonly renderError = computed(() =>
    coreErrorMessage(this.rendered.error(), 'The document would not open'),
  );
  protected readonly textError = computed(() =>
    coreErrorMessage(this.text.error(), 'The file would not open'),
  );
  protected readonly fallbackIcon = computed(() => getFileIcon(this.info().mimeType));
  protected readonly typeLabel = computed(() => getFileTypeLabel(this.info().mimeType));
  protected readonly sizeLabel = computed(() => formatBytes(this.info().size));
  protected readonly addedLabel = computed(() => formatFileDate(this.info().createdAt));

  protected async copyText(): Promise<void> {
    const content = this.text.data();
    if (!content) return;
    try {
      await navigator.clipboard.writeText(content);
      this.copied.set(true);
      setTimeout(() => this.copied.set(false), 2000);
    } catch {
      /* clipboard unavailable — no-op (v4 logs and moves on) */
    }
  }
}
