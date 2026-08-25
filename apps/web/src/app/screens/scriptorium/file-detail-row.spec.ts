import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ToastService } from '../../ui/toast.service';
import { FileDetailRow } from './file-detail-row';
import type { DocumentStoreBlob, DocumentStoreFile } from './scriptorium.api';

function file(over: Partial<DocumentStoreFile> = {}): DocumentStoreFile {
  return {
    id: 'f1',
    mountPointId: 'm1',
    relativePath: 'Images/holiday/photo.webp',
    fileName: 'photo.webp',
    fileType: 'blob',
    sha256: 'abc',
    fileSizeBytes: 10,
    lastModified: '2026-02-01T00:00:00.000Z',
    conversionStatus: 'converted',
    conversionError: null,
    plainTextLength: null,
    chunkCount: 0,
    createdAt: '2026-02-01T00:00:00.000Z',
    updatedAt: '2026-02-01T00:00:00.000Z',
    ...over,
  };
}

function blob(over: Partial<DocumentStoreBlob> = {}): DocumentStoreBlob {
  return {
    id: 'b1',
    mountPointId: 'm1',
    relativePath: 'Images/holiday/photo.webp',
    // Deliberately disagrees with the stored name — the whole point of v4's
    // why-comment: uploads are transcoded to WebP.
    originalFileName: 'holiday photo.HEIC',
    originalMimeType: 'image/heic',
    storedMimeType: 'image/webp',
    sizeBytes: 10,
    sha256: 'abc',
    description: '',
    descriptionUpdatedAt: null,
    extractedText: null,
    extractedTextSha256: null,
    extractionStatus: 'none',
    extractionError: null,
    createdAt: '2026-02-01T00:00:00.000Z',
    updatedAt: '2026-02-01T00:00:00.000Z',
    ...over,
  };
}

/**
 * The detail row's Download button (v4 `af1bc479`, `FileTable.tsx`
 * `FileDetailRow.handleDownload`). Drives the REAL `core/download-utils` and
 * intercepts the anchor click, so the saved filename is measured at the
 * browser boundary rather than at a mock.
 */
describe('FileDetailRow — Download (v4 af1bc479)', () => {
  let clicked: { download: string; href: string } | null;

  beforeEach(() => {
    clicked = null;
    (URL as unknown as { createObjectURL: unknown }).createObjectURL = () => 'blob:file';
    (URL as unknown as { revokeObjectURL: unknown }).revokeObjectURL = () => {};
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (
      this: HTMLAnchorElement,
    ) {
      clicked = { download: this.download, href: this.href };
    });
  });
  afterEach(() => {
    vi.restoreAllMocks();
    TestBed.resetTestingModule();
  });

  function render(inputs: Record<string, unknown>): ComponentFixture<FileDetailRow> {
    TestBed.configureTestingModule({ imports: [FileDetailRow] });
    const fixture = TestBed.createComponent(FileDetailRow);
    for (const [k, v] of Object.entries(inputs)) {
      fixture.componentRef.setInput(k, v);
    }
    fixture.detectChanges();
    return fixture;
  }

  function buttons(fixture: ComponentFixture<FileDetailRow>): string[] {
    return [...fixture.nativeElement.querySelectorAll('button')].map((b: HTMLButtonElement) =>
      (b.textContent ?? '').trim(),
    );
  }

  it('renders Download between Open bytes and Copy link when there are bytes to fetch', () => {
    const fixture = render({ file: file(), blob: blob(), blobUrl: '/api/v1/blobs/photo.webp' });
    expect(buttons(fixture)).toEqual(['Download', 'Copy link']);
  });

  it('is absent without a blobUrl — v4 gates it on the same value as Open bytes', () => {
    const fixture = render({ file: file(), blob: blob(), blobUrl: null });
    expect(buttons(fixture)).toEqual(['Copy link']);
  });

  it('saves under file.fileName, NOT the blob’s originalFileName', async () => {
    const fixture = render({ file: file(), blob: blob(), blobUrl: '/api/v1/blobs/photo.webp' });
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue({ ok: true, blob: async () => new Blob(['b']) } as unknown as Response);

    (
      [...fixture.nativeElement.querySelectorAll('button')].find(
        (b: HTMLButtonElement) => (b.textContent ?? '').trim() === 'Download',
      ) as HTMLButtonElement
    ).click();
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }

    expect(fetchSpy).toHaveBeenCalledWith('/api/v1/blobs/photo.webp');
    expect(clicked).toEqual({ download: 'photo.webp', href: 'blob:file' });
  });

  it('a non-ok response toasts v4’s sentence and saves nothing', async () => {
    const fixture = render({ file: file(), blob: blob(), blobUrl: '/api/v1/blobs/photo.webp' });
    vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      ok: false,
      status: 500,
    } as unknown as Response);
    const err = vi.spyOn(TestBed.inject(ToastService), 'showError');

    (
      [...fixture.nativeElement.querySelectorAll('button')].find(
        (b: HTMLButtonElement) => (b.textContent ?? '').trim() === 'Download',
      ) as HTMLButtonElement
    ).click();
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }

    expect(clicked).toBeNull();
    expect(err).toHaveBeenCalledWith('Failed to download file');
  });
});
