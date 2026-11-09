import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { CoreDispatchError } from '../core/core-contract';
import { file, provideDocumentStoreTesting, settle, type Route } from './document-browser.testing';
import { DocumentViewer, fileInfoOf, type ViewableEntry } from './document-viewer';
import { DocumentViewerModal } from './document-viewer-modal';

/**
 * v4 `components/document-browser/DocumentViewer.tsx` (`c34986421`, pin
 * `624d0d25c`; P4.D387 item 9 + Tier 3 items 26–27): one case per kind; the
 * `.docx` 422 `not_available` → the fallback (`DOCX_RENDER_HTML_REFUSED`, SPA
 * arm) and any other failure → v4's error line; the PDF arm's native
 * `<object>` with the download prompt inside (`NATIVE_PDF_VIEWER` — v4 renders
 * with `pdfjs-dist`, `Loading PDF...` / `Failed to load PDF. Try downloading
 * instead.`; v5 asserts its own shape and names v4's here); `fileInfoOf`'s
 * MIME fallback to `mimeTypeForName`.
 */

@Component({
  imports: [DocumentViewer],
  template: `<qt-document-viewer [entry]="entry()" />`,
})
class Host {
  readonly entry = signal<ViewableEntry>(file('notes.md'));
}

async function render(entry: ViewableEntry, route: Route = () => ({})) {
  const { seen } = provideDocumentStoreTesting(route);
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  fixture.componentInstance.entry.set(entry);
  fixture.detectChanges();
  await settle(fixture);
  return { el: fixture.nativeElement as HTMLElement, seen };
}

const RAW = (path: string) => `/api/v1/mount-points/store-a/files/${path}?raw=1`;

describe('DocumentViewer (v4 DocumentViewer.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('image: the raw bytes in an <img>', async () => {
    const { el } = await render(file('a.png', { fileType: 'blob', originalMimeType: 'image/png' }));
    expect(el.querySelector('img')!.getAttribute('src')).toBe(RAW('a.png'));
  });

  it('pdf (NATIVE_PDF_VIEWER): a native <object> on the raw leg, the download prompt as its fallback', async () => {
    const { el } = await render(file('scan.pdf', { fileType: 'pdf' }));
    const object = el.querySelector('object')!;
    expect(object.getAttribute('type')).toBe('application/pdf');
    expect(object.getAttribute('data')).toBe(RAW('scan.pdf'));
    const link = object.querySelector('a')!;
    expect(link.textContent!.trim()).toBe('Download PDF');
    expect(link.getAttribute('href')).toBe(RAW('scan.pdf'));
    // v4's pdf.js texts are not v5's (no PDF dependency).
    expect(el.textContent).not.toContain('Loading PDF...');
  });

  it('markdown / text: the utf-8 text read, shown as the preview <pre>', async () => {
    const { el, seen } = await render(file('notes.md'), (req) =>
      req.type === 'mountFileRead' ? { content: '# Title\nbody', encoding: 'utf-8' } : {},
    );
    expect(seen).toContainEqual({
      type: 'mountFileRead',
      mountPointId: 'store-a',
      path: 'notes.md',
      encoding: 'utf-8',
    });
    expect(el.querySelector('pre code')!.textContent).toBe('# Title\nbody');
  });

  it('text: a failed read shows v4 line through the server text', async () => {
    const { el } = await render(file('a.txt', { fileType: 'txt' }), (req) =>
      req.type === 'mountFileRead'
        ? new CoreDispatchError({ kind: 'not-found', message: 'File not found' })
        : {},
    );
    expect(el.textContent).toContain('File not found');
  });

  it('docx: the render through the sanitizer into qt-markdown', async () => {
    const { el, seen } = await render(file('r.docx', { fileType: 'docx' }), (req) =>
      req.type === 'mountFileRenderHtml' ? { html: '<h1>Title</h1><script>x()</script>' } : {},
    );
    expect(seen).toContainEqual({
      type: 'mountFileRenderHtml',
      mountPointId: 'store-a',
      path: 'r.docx',
    });
    expect(el.querySelector('.qt-markdown')!.innerHTML).toBe('<h1>Title</h1>');
  });

  it('docx (DOCX_RENDER_HTML_REFUSED): the server 422 not_available shows the fallback, not the error', async () => {
    const { el } = await render(file('r.docx', { fileType: 'docx', fileSizeBytes: 2048 }), (req) =>
      req.type === 'mountFileRenderHtml'
        ? new CoreDispatchError({
            kind: 'not_available',
            message: 'not_available: docx render-html',
          })
        : {},
    );
    expect(el.querySelector('[data-testid="document-viewer-fallback"]')).not.toBeNull();
    expect(el.textContent).toContain('Preview not available for this file type');
    expect(el.textContent).toContain('2.0 KB');
    expect(el.querySelector('.qt-text-destructive')).toBeNull();
  });

  it('docx: any other failure shows the error line (the server text, else v4 sentence)', async () => {
    const { el } = await render(file('r.docx', { fileType: 'docx' }), (req) =>
      req.type === 'mountFileRenderHtml'
        ? new CoreDispatchError({ kind: 'internal', message: 'The converter tripped' })
        : {},
    );
    expect(el.querySelector('.qt-text-destructive')!.textContent!.trim()).toBe(
      'The converter tripped',
    );
    expect(el.querySelector('[data-testid="document-viewer-fallback"]')).toBeNull();
  });

  it('unsupported: the fallback with type, size, Added and Download File', async () => {
    const { el } = await render(file('a.zip', { fileType: 'blob' }));
    expect(el.textContent).toContain('Preview not available for this file type');
    expect(el.textContent).toContain('Added');
    const link = [...el.querySelectorAll('a')].find(
      (a) => a.textContent!.trim() === 'Download File',
    )!;
    expect(link.getAttribute('download')).toBe('a.zip');
  });

  it('fileInfoOf falls back to mimeTypeForName when the row carries no MIME type', () => {
    expect(fileInfoOf(file('Drafts/c.MD')).mimeType).toBe('text/markdown');
    expect(fileInfoOf(file('a.bin', { originalMimeType: 'image/png' })).mimeType).toBe('image/png');
  });
});

@Component({
  imports: [DocumentViewerModal],
  template: `<qt-document-viewer-modal [entry]="entry" (closed)="closed = closed + 1" />`,
})
class ModalHost {
  readonly entry = file('Drafts/a.zip', { fileType: 'blob' });
  closed = 0;
}

describe('DocumentViewerModal (v4 DocumentViewer.tsx:127-155)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('titles the 4xl dialog with the name; the footer says read-only, Download, Close', async () => {
    provideDocumentStoreTesting(() => ({}));
    TestBed.configureTestingModule({ imports: [ModalHost] });
    const fixture = TestBed.createComponent(ModalHost);
    fixture.detectChanges();
    await settle(fixture);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelector('.qt-dialog-title')!.textContent).toBe('a.zip');
    expect((el.querySelector('.qt-dialog') as HTMLElement).style.maxWidth).toBe('56rem');
    expect(el.querySelector('[qt-modal-footer]')!.textContent).toContain(
      'Drafts/a.zip · read-only',
    );
    const close = [...el.querySelectorAll('[qt-modal-footer] button')].find(
      (b) => b.textContent!.trim() === 'Close',
    ) as HTMLButtonElement;
    close.click();
    expect(fixture.componentInstance.closed).toBe(1);
  });
});
