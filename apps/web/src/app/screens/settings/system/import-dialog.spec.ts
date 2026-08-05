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

/** The preview payload contract C3 describes, in v4's own key order. */
function previewBody(): string {
  return JSON.stringify({
    manifest: { exportType: 'plugin-configs', appVersion: '4.8.0' },
    entities: {
      documentStores: [{ id: 'ds1', name: 'Aria’s Vault', exists: true }],
      files: [
        { id: 'f1', name: 'who-is-friday.md', exists: false },
        {
          id: 'f2',
          name: 'catastrophe.md',
          exists: false,
          detail: 'contents missing — will be skipped',
        },
      ],
      promptTemplates: [{ id: 'pt1', name: 'The Lantern', exists: true, matchedExistingId: 'pt9' }],
      providerModels: [{ id: 'pm1', name: 'anthropic / claude-opus-4', exists: false }],
      pluginConfigs: [
        {
          id: 'pc1',
          name: 'qtap-plugin-anthropic',
          exists: false,
          detail: 'secrets withheld: apiKey, orgId',
        },
        {
          id: 'pc2',
          name: 'qtap-plugin-mystery',
          exists: false,
          detail: 'all settings withheld — re-enter them here',
        },
      ],
      instanceSettings: [{ id: 'siteTitle', name: 'siteTitle', exists: false }],
      characters: [{ id: 'c1', name: 'Aria', exists: false }],
      chats: [{ id: 'ch1', title: 'A Solo Voyage', exists: false }],
      tags: [{ id: 't1', name: 'canon', exists: true }],
    },
  });
}

/**
 * P4.D47 unit 3 — the six new preview sections and the per-entity `detail`
 * line (contract C3; v4 `ImportPreviewStep.tsx:14` + `:123-127` at `7189a968`).
 */
describe('ImportDialog — the widened preview (contract C3)', () => {
  afterEach(() => vi.unstubAllGlobals());

  it("lists the six new sections first, in the server's key order", async () => {
    const fixture = await mountAtPreviewWith(
      new Response(previewBody(), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );

    const headings = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('h4'),
    ).map((h) => h.textContent?.trim());
    // v4 serializes the configuration-shaped types BEFORE characters/chats/tags,
    // and both apps walk `Object.keys` — so this order is the server's.
    expect(headings).toEqual([
      'Document Stores',
      'Files & Folders',
      'Prompt Templates',
      'Provider Models',
      'Plugin Settings',
      'Instance Settings',
      'Characters',
      'Chats',
      'Tags',
    ]);
  });

  it("renders each detail as a nested block span under the entity's name", async () => {
    const fixture = await mountAtPreviewWith(
      new Response(previewBody(), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );

    const details = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll(
        'span.block.qt-text-small.qt-text-secondary',
      ),
    );
    // v4's three detail strings, verbatim — and only for the entities carrying
    // one (four of the ten rows here have no detail and must render none).
    expect(details.map((d) => d.textContent?.trim())).toEqual([
      'contents missing — will be skipped',
      'secrets withheld: apiKey, orgId',
      'all settings withheld — re-enter them here',
    ]);

    // v4 nests the note INSIDE the name span (`:123-127`), so it wraps under
    // the name rather than beside the Exists chip.
    for (const d of details) {
      expect(d.parentElement?.className).toContain('flex-1');
      expect(d.parentElement?.textContent).toMatch(/\S/);
    }
  });

  it('leaves a detail-free entity with just its name', async () => {
    const fixture = await mountAtPreviewWith(
      new Response(previewBody(), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );
    const rows = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('label'),
    ).filter((l) => l.textContent?.includes('who-is-friday.md'));
    expect(rows).toHaveLength(1);
    expect(rows[0].querySelector('span.block.qt-text-small.qt-text-secondary')).toBeNull();
  });
});

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
