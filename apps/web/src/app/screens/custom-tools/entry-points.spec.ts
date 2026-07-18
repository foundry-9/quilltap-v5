import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { afterEach, describe, expect, it } from 'vitest';

import { FileDetailRow } from '../scriptorium/file-detail-row';
import type { DocumentStoreFile } from '../scriptorium/scriptorium.api';

/**
 * The Workbench entry points that lanes P4.6z / P4.6an / P4.6ba left as named
 * omissions, now landed (P4.6bb unit 8).
 *
 * The left-rail item and the Settings-card button are asserted in `shell` /
 * `custom-tools-settings.spec.ts`; the composer-popup trio in
 * `custom-tools-popup.spec.ts`. This file covers the Scriptorium row action and
 * — importantly — the path predicate that decides which rows get it at all.
 */

function file(over: Partial<DocumentStoreFile> = {}): DocumentStoreFile {
  return {
    id: 'f1',
    mountPointId: 'm1',
    relativePath: 'Tools/unlock.tool.json',
    fileName: 'unlock.tool.json',
    fileType: 'text',
    sizeBytes: 100,
    sha256: 'abc',
    mtime: 1,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    ...over,
  } as DocumentStoreFile;
}

async function render(f: DocumentStoreFile): Promise<ComponentFixture<FileDetailRow>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [FileDetailRow],
    providers: [provideRouter([])],
  });
  const fixture = TestBed.createComponent(FileDetailRow);
  fixture.componentRef.setInput('file', f);
  fixture.detectChanges();
  return fixture;
}

function workbenchButton(fixture: ComponentFixture<unknown>): HTMLButtonElement | undefined {
  return [...(fixture.nativeElement as HTMLElement).querySelectorAll('button')].find((b) =>
    b.textContent?.includes('Pascal'),
  );
}

describe('Scriptorium row action (v4 FileTable isCustomToolDefinition)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('offers the Workbench on a ROOT-level Tools/*.tool.json', async () => {
    const fixture = await render(file());
    expect(workbenchButton(fixture)?.textContent).toContain("Open in Pascal's Workbench");
  });

  it('is case-insensitive on the folder and suffix, as v4 is', async () => {
    const fixture = await render(
      file({ relativePath: 'TOOLS/Unlock.TOOL.JSON', fileName: 'Unlock.TOOL.JSON' }),
    );
    expect(workbenchButton(fixture)).toBeDefined();
  });

  it('does NOT offer it on a nested Tools path — the folder must be at the root', async () => {
    const fixture = await render(
      file({ relativePath: 'Notes/Tools/unlock.tool.json' }),
    );
    expect(workbenchButton(fixture)).toBeUndefined();
  });

  it('does NOT offer it on a deeper file inside Tools/', async () => {
    const fixture = await render(file({ relativePath: 'Tools/sub/unlock.tool.json' }));
    expect(workbenchButton(fixture)).toBeUndefined();
  });

  it('does NOT offer it on an ordinary file', async () => {
    const fixture = await render(
      file({ relativePath: 'Tools/readme.md', fileName: 'readme.md' }),
    );
    expect(workbenchButton(fixture)).toBeUndefined();
  });

  it('emits the relative path so the table can build the deep link', async () => {
    const fixture = await render(file());
    let emitted: string | undefined;
    fixture.componentInstance.openWorkbench.subscribe((p) => (emitted = p));
    workbenchButton(fixture)?.dispatchEvent(new Event('click'));
    expect(emitted).toBe('Tools/unlock.tool.json');
  });
});
