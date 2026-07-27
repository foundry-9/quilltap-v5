import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { LibraryBrowsePanel, type BrowseScope } from './library-browse-panel';
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

async function render(
  client: Partial<CoreClient>,
  scope: BrowseScope,
): Promise<ComponentFixture<LibraryBrowsePanel>> {
  TestBed.configureTestingModule({
    imports: [LibraryBrowsePanel],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(LibraryBrowsePanel);
  fixture.componentRef.setInput('scope', scope);
  fixture.detectChanges();
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

describe('qt-library-browse-panel — mode resolution (v4 FileBrowser :226-262)', () => {
  it('General browses the legacy general files, never a store', async () => {
    const log: Dispatched[] = [];
    const fixture = await render(
      stubClient(log, {
        filesList: { files: [legacyFile({ id: 'a' })] },
        filesFoldersList: { folders: [] },
      }),
      { projectId: null },
    );
    expect(log.map((r) => r.type)).toEqual(['filesList', 'filesFoldersList']);
    expect(log[0]['filter']).toBe('general');
    expect(log[0]['projectId']).toBeUndefined();
    expect(fixture.nativeElement.textContent).toContain('a.txt');
  });

  it('an explicit database store browses THAT store, with no auto-resolve', async () => {
    const log: Dispatched[] = [];
    const fixture = await render(
      stubClient(log, {
        mountFilesList: {
          files: [
            {
              id: 'r1',
              mountPointId: 'store-1',
              relativePath: 'notes/plan.md',
              fileName: 'plan.md',
              fileType: 'markdown',
              sha256: 'x',
              fileSizeBytes: 5,
              lastModified: '2026-01-02T00:00:00.000Z',
              conversionStatus: 'converted',
              conversionError: null,
              plainTextLength: 1,
              chunkCount: 1,
              createdAt: '2026-01-01T00:00:00.000Z',
              updatedAt: '2026-01-01T00:00:00.000Z',
            },
          ],
        },
      }),
      {
        projectId: null,
        mountPoint: {
          id: 'store-1',
          name: 'Research',
          mountType: 'database',
          storeType: 'documents',
          enabled: true,
        },
      },
    );
    expect(log.map((r) => r.type)).toEqual(['mountFilesList']);
    // The store badge names the store the panel is reading.
    expect(fixture.nativeElement.textContent).toContain('Research');
    // The row lives at /notes/, so root shows the folder, not the file.
    expect(fixture.nativeElement.textContent).toContain('notes');
    expect(fixture.nativeElement.textContent).not.toContain('plan.md');
  });

  it('a project with no explicit store auto-resolves its own "Project Files:" store', async () => {
    const log: Dispatched[] = [];
    await render(
      stubClient(log, {
        projectMountPointList: {
          mountPoints: [
            {
              id: 'linked',
              name: 'Reference shelf',
              mountType: 'database',
              storeType: 'documents',
              enabled: true,
            },
            {
              id: 'own',
              name: 'Project Files: Novel',
              mountType: 'database',
              storeType: 'documents',
              enabled: true,
            },
          ],
        },
        mountFilesList: { files: [] },
      }),
      { projectId: 'p1' },
    );
    expect(log.map((r) => r.type)).toEqual(['projectMountPointList', 'mountFilesList']);
    expect(log[1]['mountPointId']).toBe('own');
  });

  it('a failing auto-resolve falls back to the project’s legacy files (v4: silent)', async () => {
    const log: Dispatched[] = [];
    const fixture = await render(
      stubClient(log, {
        projectMountPointList: () => {
          throw new Error('no link');
        },
        filesList: { files: [legacyFile({ id: 'p-file' })] },
        filesFoldersList: { folders: [] },
      }),
      { projectId: 'p1' },
    );
    expect(log.map((r) => r.type)).toEqual([
      'projectMountPointList',
      'filesList',
      'filesFoldersList',
    ]);
    expect(log[1]['projectId']).toBe('p1');
    expect(log[1]['filter']).toBeUndefined();
    expect(fixture.nativeElement.textContent).toContain('p-file.txt');
  });

  it('a filesystem mount stays in legacy mode (its files live on disk)', async () => {
    const log: Dispatched[] = [];
    await render(
      stubClient(log, { filesList: { files: [] }, filesFoldersList: { folders: [] } }),
      {
        projectId: null,
        mountPoint: {
          id: 'fs-1',
          name: 'On disk',
          mountType: 'filesystem',
          storeType: 'documents',
          enabled: true,
        },
      },
    );
    expect(log.map((r) => r.type)).toEqual(['filesList', 'filesFoldersList']);
  });
});

describe('qt-library-browse-panel — picking and sealing', () => {
  it('emits the clicked row, mount fields intact', async () => {
    const log: Dispatched[] = [];
    const fixture = await render(
      stubClient(log, {
        mountFilesList: {
          files: [
            {
              id: 'r1',
              mountPointId: 'store-1',
              relativePath: 'plan.md',
              fileName: 'plan.md',
              fileType: 'markdown',
              sha256: 'x',
              fileSizeBytes: 5,
              lastModified: '2026-01-02T00:00:00.000Z',
              conversionStatus: 'converted',
              conversionError: null,
              plainTextLength: 1,
              chunkCount: 1,
              createdAt: '2026-01-01T00:00:00.000Z',
              updatedAt: '2026-01-01T00:00:00.000Z',
            },
          ],
        },
      }),
      {
        projectId: null,
        mountPoint: {
          id: 'store-1',
          name: 'Research',
          mountType: 'database',
          storeType: 'documents',
          enabled: true,
        },
      },
    );
    const picked: PickerFile[] = [];
    fixture.componentInstance.pick.subscribe((f) => picked.push(f));
    const rows = fixture.nativeElement.querySelectorAll('li button') as NodeListOf<HTMLElement>;
    rows[rows.length - 1].click();
    expect(picked).toHaveLength(1);
    expect(picked[0].mountPointId).toBe('store-1');
    expect(picked[0].relativePath).toBe('plan.md');
  });

  it('seals the panel while linking (the overlay shows; rows disable)', async () => {
    const log: Dispatched[] = [];
    const fixture = await render(
      stubClient(log, {
        filesList: { files: [legacyFile({ id: 'a' })] },
        filesFoldersList: { folders: [] },
      }),
      { projectId: null },
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
      { projectId: null },
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
