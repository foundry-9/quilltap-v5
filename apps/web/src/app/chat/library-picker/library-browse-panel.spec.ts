import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { LibraryBrowsePanel } from './library-browse-panel';
import type { PickerFile } from './library-picker.model';

interface Dispatched {
  type: string;
  [k: string]: unknown;
}

function legacyFile(over: Partial<PickerFile> & { id: string }): PickerFile {
  return {
    originalFilename: `${over.id}.txt`,
    filename: `${over.id}.txt`,
    mimeType: 'text/plain',
    size: 10,
    category: 'general',
    description: null,
    projectId: null,
    folderPath: '/',
    filepath: `/api/v1/files/${over.id}`,
    fileStatus: 'ok',
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    ...over,
  };
}

function stubClient(
  log: Dispatched[],
  answers: Record<string, Record<string, unknown> | (() => never)>,
): Partial<CoreClient> {
  return {
    dispatchData: (async (req: Dispatched) => {
      log.push(req);
      const answer = answers[req.type];
      if (typeof answer === 'function') answer();
      return answer ?? {};
    }) as CoreClient['dispatchData'],
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<LibraryBrowsePanel>> {
  TestBed.configureTestingModule({
    imports: [LibraryBrowsePanel],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(LibraryBrowsePanel);
  fixture.detectChanges();
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

/**
 * RE-AIMED by P4.D387 (item 18; v4 `c17033f04`): the panel is General-only.
 * REMOVED with v4's own source — no counterpart after `c17033f04`, which
 * deleted `FileBrowser.tsx:226-262`'s mount mode and project auto-resolve:
 * "an explicit database store browses THAT store", "a project … auto-resolves
 * its own store", "a failing auto-resolve falls back", "a filesystem mount
 * stays in legacy mode", and "emits the clicked row, mount fields intact" (a
 * mount row). A project / store now browses through the store browser
 * (`library-file-picker-modal.spec.ts`).
 */
describe('qt-library-browse-panel — General (v4 c17033f04 FileBrowser, general files only)', () => {
  it('General browses the legacy general files, never a store', async () => {
    const log: Dispatched[] = [];
    const fixture = await render(
      stubClient(log, {
        filesList: { files: [legacyFile({ id: 'a' })] },
        filesFoldersList: { folders: [] },
      }),
    );
    expect(log.map((r) => r.type)).toEqual(['filesList', 'filesFoldersList']);
    expect(log[0]['filter']).toBe('general');
    expect(log[0]['projectId']).toBeUndefined();
    expect(fixture.nativeElement.textContent).toContain('a.txt');
  });
});

describe('qt-library-browse-panel — picking and sealing', () => {
  it('emits the clicked legacy row', async () => {
    const log: Dispatched[] = [];
    const fixture = await render(
      stubClient(log, {
        filesList: { files: [legacyFile({ id: 'a' })] },
        filesFoldersList: { folders: [] },
      }),
    );
    const picked: PickerFile[] = [];
    fixture.componentInstance.pick.subscribe((f) => picked.push(f));
    (fixture.nativeElement.querySelector('li button') as HTMLElement).click();
    expect(picked.map((f) => f.id)).toEqual(['a']);
  });

  it('seals the panel while linking (the overlay shows; rows disable)', async () => {
    const log: Dispatched[] = [];
    const fixture = await render(
      stubClient(log, {
        filesList: { files: [legacyFile({ id: 'a' })] },
        filesFoldersList: { folders: [] },
      }),
    );
    expect(
      fixture.nativeElement.querySelector('[data-testid="library-picker-linking"]'),
    ).toBeNull();

    fixture.componentRef.setInput('linking', true);
    fixture.detectChanges();

    const overlay = fixture.nativeElement.querySelector('[data-testid="library-picker-linking"]');
    expect(overlay).not.toBeNull();
    expect(overlay.textContent).toContain('Attaching');
    const fileRow = fixture.nativeElement.querySelector('li button') as HTMLButtonElement;
    expect(fileRow.disabled).toBe(true);
  });
});

describe('qt-library-browse-panel — folder navigation (v4’s pure model, reused)', () => {
  it('walks into a subfolder, shows its files, and climbs back out', async () => {
    const log: Dispatched[] = [];
    const fixture = await render(
      stubClient(log, {
        filesList: {
          files: [
            legacyFile({ id: 'root-note' }),
            legacyFile({ id: 'deep-note', folderPath: '/drafts/' }),
          ],
        },
        filesFoldersList: { folders: [] },
      }),
    );
    const text = () => fixture.nativeElement.textContent as string;
    expect(text()).toContain('root-note.txt');
    expect(text()).toContain('drafts');
    expect(text()).not.toContain('deep-note.txt');

    const folderButton = Array.from(
      fixture.nativeElement.querySelectorAll('li button') as NodeListOf<HTMLElement>,
    ).find((b) => b.textContent?.includes('drafts'))!;
    folderButton.click();
    fixture.detectChanges();

    expect(text()).toContain('deep-note.txt');
    expect(text()).not.toContain('root-note.txt');

    const upButton = Array.from(
      fixture.nativeElement.querySelectorAll('li button') as NodeListOf<HTMLElement>,
    ).find((b) => b.textContent?.trim().endsWith('..'))!;
    upButton.click();
    fixture.detectChanges();
    expect(text()).toContain('root-note.txt');
  });
});
