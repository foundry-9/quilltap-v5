import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { ExportDialog } from './export-dialog';
import { ImportDialog } from './import-dialog';
import { ImportExportCard } from './import-export-card';

function coreStub(dispatchData?: CoreClient['dispatchData']): Partial<CoreClient> {
  return { dispatchData: dispatchData ?? ((async () => ({})) as CoreClient['dispatchData']) };
}

async function mount<T>(cmp: new (...a: never[]) => T, client: Partial<CoreClient>): Promise<ComponentFixture<T>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [cmp as never],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(cmp as never) as ComponentFixture<T>;
  fixture.detectChanges();
  return fixture;
}

function radioByLabel(fixture: ComponentFixture<unknown>, label: string): HTMLInputElement {
  const el = fixture.nativeElement as HTMLElement;
  const lbl = Array.from(el.querySelectorAll('label')).find((l) => l.textContent?.includes(label));
  return lbl!.querySelector('input') as HTMLInputElement;
}

function buttonWith(fixture: ComponentFixture<unknown>, label: string): HTMLButtonElement {
  return Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).find((b) =>
    b.textContent?.trim().startsWith(label),
  ) as HTMLButtonElement;
}

/**
 * jsdom does not implement `Blob.text()`/`slice().text()`, which the client-side
 * format sniff relies on — the real browser does (and the ACTIVATE-AT-UNIFY e2e
 * beat exercises it end-to-end). A fake File supplies those two reads.
 */
function fakeFile(name: string, content: string): File {
  return {
    name,
    size: content.length,
    text: async () => content,
    slice: () => ({ text: async () => content }),
  } as unknown as File;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

describe('ImportExportCard', () => {
  it('opens the export dialog', async () => {
    const fixture = await mount(ImportExportCard, coreStub());
    buttonWith(fixture, 'Export Data').click();
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Select the type of data you want to export');
  });
});

describe('ExportDialog', () => {
  it('reproduces v4 total-step quirk: characters is "of 5", tags "of 4"', async () => {
    const fixture = await mount(ExportDialog, coreStub());
    radioByLabel(fixture, 'Characters').dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Step 1 of 5');

    radioByLabel(fixture, 'Tags').dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Step 1 of 4');
  });

  it('loads entities on Next and shows Export (not Next) for a non-memory type', async () => {
    const dispatchData = vi.fn(async () => ({ entities: [{ id: 'a', name: 'Alpha' }] })) as unknown as CoreClient['dispatchData'];
    const fixture = await mount(ExportDialog, coreStub(dispatchData));
    radioByLabel(fixture, 'Tags').dispatchEvent(new Event('change'));
    fixture.detectChanges();
    buttonWith(fixture, 'Next').click();
    await Promise.resolve();
    await Promise.resolve();
    fixture.detectChanges();
    expect(dispatchData).toHaveBeenCalledWith({ type: 'systemExportEntities', entityType: 'tags' });
    // On the select step a non-memory type shows Export.
    expect(buttonWith(fixture, 'Export')).toBeTruthy();
  });
});

describe('ImportDialog', () => {
  it('parses a legacy export file and enables Next', async () => {
    const fixture = await mount(ImportDialog, coreStub());
    const next = buttonWith(fixture, 'Next');
    expect(next.disabled).toBe(true);

    const input = fixture.nativeElement.querySelector('input[type="file"]') as HTMLInputElement;
    const file = fakeFile('x.qtap', JSON.stringify({ manifest: { format: 'quilltap-export' } }));
    Object.defineProperty(input, 'files', { value: [file] });
    input.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(fixture.nativeElement.textContent).toContain('x.qtap');
    expect(buttonWith(fixture, 'Next').disabled).toBe(false);
  });

  it('rejects a non-Quilltap file with a message', async () => {
    const fixture = await mount(ImportDialog, coreStub());
    const input = fixture.nativeElement.querySelector('input[type="file"]') as HTMLInputElement;
    const file = fakeFile('bad.qtap', JSON.stringify({ nope: true }));
    Object.defineProperty(input, 'files', { value: [file] });
    input.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(fixture.nativeElement.textContent).toContain('Invalid export file format');
  });
});
