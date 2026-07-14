import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { CoreResponse, FileEntry } from '../../core/core-contract';
import { FilesBrowser } from './files-browser';

function file(partial: Partial<FileEntry> & { id: string }): FileEntry {
  return {
    originalFilename: partial.id,
    filename: partial.id,
    mimeType: 'image/png',
    size: 10,
    category: 'general',
    description: null,
    projectId: null,
    folderPath: '/',
    filepath: `/api/v1/files/${partial.id}`,
    fileStatus: 'ok',
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    ...partial,
  };
}

interface DispatchLog {
  requests: { type: string; [k: string]: unknown }[];
}

/** A stub CoreClient: canned files/folders + a scripted `dispatch` result. */
function stubClient(
  log: DispatchLog,
  files: FileEntry[],
  dispatchResult: CoreResponse,
): Partial<CoreClient> {
  return {
    filesList: (async () => files) as CoreClient['filesList'],
    filesFoldersList: (async () => []) as CoreClient['filesFoldersList'],
    filesGenerateThumbnails: (async () => undefined) as CoreClient['filesGenerateThumbnails'],
    dispatch: (async (req: { type: string }) => {
      log.requests.push(req as { type: string });
      return dispatchResult;
    }) as CoreClient['dispatch'],
    dispatchData: (async (req: { type: string }) => {
      log.requests.push(req as { type: string });
      return {};
    }) as CoreClient['dispatchData'],
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<FilesBrowser>> {
  TestBed.configureTestingModule({
    imports: [FilesBrowser],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(FilesBrowser);
  fixture.detectChanges();
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

describe('FilesBrowser', () => {
  beforeEach(() => {
    vi.stubGlobal(
      'confirm',
      vi.fn(() => true),
    );
    vi.stubGlobal('alert', vi.fn());
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    TestBed.resetTestingModule();
  });

  it('renders the general-files heading with no upload affordance', async () => {
    const fixture = await render(stubClient({ requests: [] }, [], { type: 'ack', data: {} }));
    expect(fixture.nativeElement.textContent).toContain('General Files');
    // v4 parity: the general Files page has NO upload button / file input (the
    // empty-state copy "Upload files to get started" is the only mention).
    expect(fixture.nativeElement.querySelector('input[type=file]')).toBeNull();
    expect(fixture.nativeElement.querySelector('button[title="Upload Files"]')).toBeNull();
  });

  it('shows the file count footer', async () => {
    const fixture = await render(
      stubClient({ requests: [] }, [file({ id: 'a' }), file({ id: 'b' })], {
        type: 'ack',
        data: {},
      }),
    );
    expect(fixture.nativeElement.textContent).toContain('2 files total');
  });

  it('surfaces the FILE_HAS_ASSOCIATIONS envelope as the dissociate confirmation', async () => {
    const log: DispatchLog = { requests: [] };
    const associations = {
      characters: [{ id: 'c1', name: 'Lorian', usage: 'avatar' }],
      messages: [{ chatId: 'ch1', chatName: 'A Chat', messageId: 'm1' }],
    };
    const errorEnvelope: CoreResponse = {
      type: 'error',
      data: {
        kind: 'bad-request',
        message: 'File is linked to other items',
        // Lane A's exact placement reconciles at unification; the extractor
        // tolerates top-level or nested-under-`details`.
        associations,
      } as never,
    };
    const fixture = await render(stubClient(log, [file({ id: 'linked' })], errorEnvelope));

    // Trigger delete via the grid's delete button (hover action, but present in DOM).
    const deleteButton = fixture.nativeElement.querySelector(
      'button[title="Delete file"]',
    ) as HTMLButtonElement;
    deleteButton.click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    const dialog = fixture.nativeElement.querySelector('[role=dialog]');
    expect(dialog).not.toBeNull();
    expect(dialog.textContent).toContain('This file is in use');
    expect(dialog.textContent).toContain('Lorian');
    expect(dialog.textContent).toContain('A Chat');
    expect(dialog.textContent).toContain('Delete Anyway');
    // The first delete attempt sent a bare fileDelete (no dissociate).
    expect(log.requests[0]).toEqual({ type: 'fileDelete', fileId: 'linked' });
  });

  it('dissociate-confirm sends fileDelete with dissociate:true', async () => {
    const log: DispatchLog = { requests: [] };
    const errorEnvelope: CoreResponse = {
      type: 'error',
      data: {
        kind: 'bad-request',
        message: 'File is linked to other items',
        associations: { characters: [], messages: [] },
      } as never,
    };
    const fixture = await render(stubClient(log, [file({ id: 'linked' })], errorEnvelope));
    (fixture.nativeElement.querySelector('button[title="Delete file"]') as HTMLButtonElement).click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    // Click "Delete Anyway".
    const confirmBtn = Array.from(
      fixture.nativeElement.querySelectorAll('[role=dialog] button'),
    ).find((b) => (b as HTMLElement).textContent?.includes('Delete Anyway')) as HTMLButtonElement;
    confirmBtn.click();
    await new Promise((r) => setTimeout(r, 0));

    expect(log.requests.at(-1)).toEqual({
      type: 'fileDelete',
      fileId: 'linked',
      dissociate: true,
    });
  });
});
