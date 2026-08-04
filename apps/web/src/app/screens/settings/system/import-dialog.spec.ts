import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { ImportDialog } from './import-dialog';

/**
 * Dogfood #61/#63 — the wizard's preview step must SAY when the preview failed.
 *
 * Both apps recorded the failure and drew nothing: `loadPreview` sets `error`
 * but leaves the wizard on `preview`, whose template had a loading branch and a
 * `preview()` branch and no third one. A human importing a 791 MB `.qtap` saw a
 * blank "Step 2 of 5" with live Back/Next buttons and no way to learn that the
 * transport had refused the body. v5 diverges here deliberately (the
 * 2026-08-03 backup/restore/import/export ruling); v4's fix is queued post-5.0.
 */
async function mountAtPreviewWith(response: Response): Promise<ComponentFixture<ImportDialog>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [ImportDialog],
    providers: [{ provide: CoreClient, useValue: { dispatchData: async () => ({}) } }],
  });
  const fixture = TestBed.createComponent(ImportDialog);
  fixture.detectChanges();

  // Seed what step 1 would have produced (a chosen file, already sniffed), then
  // take the wizard forward with the gesture the operator uses: Next.
  const cmp = fixture.componentInstance as unknown as {
    selectedFile: { set: (f: File) => void };
    exportData: { set: (v: unknown) => void };
    handleNext: () => Promise<void>;
    step: () => string;
  };
  cmp.selectedFile.set(new File(['{}'], 'friday.qtap'));
  cmp.exportData.set({ manifest: {}, data: {} });

  vi.stubGlobal('fetch', vi.fn(async () => response));
  await cmp.handleNext();
  fixture.detectChanges();
  return fixture;
}

describe('ImportDialog — a failed preview', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('renders the server error on step 2 instead of an empty step', async () => {
    // The exact shape finding #63 produced: a 413 whose body is text/plain, so
    // the dialog's `res.json()` rescue yields the generic message.
    const fixture = await mountAtPreviewWith(
      new Response('Failed to buffer the request body: length limit exceeded', { status: 413 }),
    );

    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Step 2 of 5');
    expect(text).toContain('Failed to load preview');
  });

  it('surfaces the server-supplied message when the body carries one', async () => {
    const fixture = await mountAtPreviewWith(
      new Response(JSON.stringify({ error: 'Invalid export file' }), {
        status: 400,
        headers: { 'content-type': 'application/json' },
      }),
    );

    expect((fixture.nativeElement as HTMLElement).textContent).toContain('Invalid export file');
  });

  it('still draws the preview when the request succeeds', async () => {
    // The guard: the new branch must not shadow the happy path.
    const fixture = await mountAtPreviewWith(
      new Response(
        JSON.stringify({
          manifest: { exportType: 'characters', appVersion: '4.8.0' },
          entities: { characters: [{ id: 'c1', name: 'Aria', exists: false }] },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      ),
    );

    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Aria');
    expect(text).not.toContain('Failed to load preview');
  });
});
