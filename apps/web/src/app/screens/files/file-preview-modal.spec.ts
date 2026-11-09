import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import type { FileEntry } from '../../core/core-contract';
import { ToastService } from '../../ui/toast.service';
import { FilePreviewModal } from './file-preview-modal';

function file(over: Partial<FileEntry> = {}): FileEntry {
  return {
    originalFilename: 'notes.txt',
    filename: 'notes.txt',
    mimeType: 'text/plain',
    size: 10,
    category: 'general',
    description: null,
    projectId: null,
    folderPath: '/',
    filepath: '/api/v1/files/f1',
    fileStatus: 'ok',
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    ...over,
  } as FileEntry;
}

function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

async function render(f: FileEntry): Promise<ComponentFixture<FilePreviewModal>> {
  TestBed.configureTestingModule({ imports: [FilePreviewModal] });
  const fixture = TestBed.createComponent(FilePreviewModal);
  fixture.componentRef.setInput('file', f);
  fixture.componentRef.setInput('files', [f]);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture;
}

/** v4 `useFileActions.ts:70-83` — the one action this modal performs itself
 *  (delete/dissociate ride the parent FileBrowser, already toasted there). */
describe('FilePreviewModal download toast', () => {
  it('toasts "Download started" on a successful download', async () => {
    const fixture = await render(file());
    const downloadBtn = (
      Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[]
    ).find((b) => b.title === 'Download');
    expect(downloadBtn).toBeTruthy();
    downloadBtn!.click();
    fixture.detectChanges();

    expect(toasts()).toEqual([{ type: 'success', message: 'Download started' }]);
  });
});

/**
 * P4.D387 item 21 — `NATIVE_PDF_VIEWER` (SPA ruling R10): a PDF renders in the
 * platform viewer through a native `<object type="application/pdf">` on the
 * file route, the old download prompt as its fallback content. v4 renders
 * PDFs with `pdfjs-dist` (`FilePreviewPdf.tsx` — `Loading PDF...`, `Failed to
 * load PDF. Try downloading instead.`), which v5 does not add.
 */
describe('FilePreviewModal PDF (NATIVE_PDF_VIEWER)', () => {
  it('embeds the PDF in an <object>, the download prompt inside it', async () => {
    const fixture = await render(
      file({
        id: 'f9',
        mimeType: 'application/pdf',
        originalFilename: 'scan.pdf',
        filename: 'scan.pdf',
      }),
    );
    const object = fixture.nativeElement.querySelector('object') as HTMLObjectElement;
    expect(object.getAttribute('type')).toBe('application/pdf');
    expect(object.getAttribute('data')).toContain('f9');
    expect(object.querySelector('a')!.textContent!.trim()).toBe('Download PDF');
    expect(fixture.nativeElement.querySelector('[role="dialog"]')).not.toBeNull();
  });
});
