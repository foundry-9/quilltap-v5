import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { CoreResponse, FileEntry } from '../../core/core-contract';
import { ToastService } from '../../ui/toast.service';
import { FilesBrowser } from './files-browser';

function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

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

  it('a clean delete (no associations) skips the dialog entirely', async () => {
    const log: DispatchLog = { requests: [] };
    // A bare success — no FILE_HAS_ASSOCIATIONS envelope.
    const fixture = await render(
      stubClient(log, [file({ id: 'lonely' })], { type: 'ack', data: {} }),
    );
    (
      fixture.nativeElement.querySelector('button[title="Delete file"]') as HTMLButtonElement
    ).click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    // No dialog appears, and only the single bare fileDelete was issued.
    expect(fixture.nativeElement.querySelector('[role=dialog]')).toBeNull();
    expect(log.requests).toEqual([{ type: 'fileDelete', fileId: 'lonely' }]);
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
    (
      fixture.nativeElement.querySelector('button[title="Delete file"]') as HTMLButtonElement
    ).click();
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

/**
 * v4 `FileBrowser.tsx` toasts (P4.29): the load failure, both delete arms,
 * sync, and both cleanup actions. v5 had used `window.alert` for every one of
 * these failures and raised no success feedback at all.
 */
describe('FilesBrowser toasts', () => {
  beforeEach(() => {
    vi.stubGlobal(
      'confirm',
      vi.fn(() => true),
    );
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    TestBed.resetTestingModule();
  });

  /** A stub with independently scriptable dispatch()/dispatchData() outcomes. */
  function flexClient(opts: {
    files?: FileEntry[];
    loadFails?: boolean;
    dispatchResult?: CoreResponse;
    dispatchDataHandler?: (req: { type: string; [k: string]: unknown }) => unknown;
  }): Partial<CoreClient> {
    return {
      filesList: (async () => {
        if (opts.loadFails) throw new Error('network down');
        return opts.files ?? [];
      }) as CoreClient['filesList'],
      filesFoldersList: (async () => []) as CoreClient['filesFoldersList'],
      filesGenerateThumbnails: (async () => undefined) as CoreClient['filesGenerateThumbnails'],
      dispatch: (async () =>
        opts.dispatchResult ?? { type: 'ack', data: {} }) as CoreClient['dispatch'],
      dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
        const out = opts.dispatchDataHandler?.(req);
        if (out instanceof Error) throw out;
        return (out ?? {}) as Record<string, unknown>;
      }) as CoreClient['dispatchData'],
    };
  }

  it('toasts "Failed to load files" on a fetch failure', async () => {
    const fixture = await render(flexClient({ loadFails: true }));
    expect(toasts()).toEqual([{ type: 'error', message: 'Failed to load files' }]);
  });

  it('toasts "File deleted" on a clean delete', async () => {
    const fixture = await render(flexClient({ files: [file({ id: 'a' })] }));
    (
      fixture.nativeElement.querySelector('button[title="Delete file"]') as HTMLButtonElement
    ).click();
    await new Promise((r) => setTimeout(r, 0));
    expect(toasts()).toEqual([{ type: 'success', message: 'File deleted' }]);
  });

  it('toasts the server message on a failed (non-associations) delete', async () => {
    const fixture = await render(
      flexClient({
        files: [file({ id: 'a' })],
        dispatchResult: { type: 'error', data: { message: 'disk is full' } as never },
      }),
    );
    (
      fixture.nativeElement.querySelector('button[title="Delete file"]') as HTMLButtonElement
    ).click();
    await new Promise((r) => setTimeout(r, 0));
    expect(toasts()).toEqual([{ type: 'error', message: 'disk is full' }]);
  });

  it('toasts "File deleted" on a successful dissociate-confirm', async () => {
    const fixture = await render(
      flexClient({
        files: [file({ id: 'linked' })],
        dispatchResult: {
          type: 'error',
          data: {
            message: 'linked',
            associations: { characters: [], messages: [] },
          } as never,
        },
      }),
    );
    (
      fixture.nativeElement.querySelector('button[title="Delete file"]') as HTMLButtonElement
    ).click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    const confirmBtn = Array.from(
      fixture.nativeElement.querySelectorAll('[role=dialog] button'),
    ).find((b) => (b as HTMLElement).textContent?.includes('Delete Anyway')) as HTMLButtonElement;
    confirmBtn.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(toasts()).toEqual([{ type: 'success', message: 'File deleted' }]);
  });

  it('toasts "Filesystem sync complete" on success, and a failure', async () => {
    const fixture = await render(flexClient({}));
    (
      fixture.nativeElement.querySelector(
        'button[title="Sync filesystem — scan disk for new or removed files"]',
      ) as HTMLButtonElement
    ).click();
    await new Promise((r) => setTimeout(r, 0));
    expect(toasts()).toEqual([{ type: 'success', message: 'Filesystem sync complete' }]);

    TestBed.resetTestingModule();
    const failing = await render(
      flexClient({ dispatchResult: { type: 'error', data: { message: 'sync busy' } as never } }),
    );
    (
      failing.nativeElement.querySelector(
        'button[title="Sync filesystem — scan disk for new or removed files"]',
      ) as HTMLButtonElement
    ).click();
    await new Promise((r) => setTimeout(r, 0));
    // v4 `FileBrowser.tsx:562` deliberately DISCARDS the error and shows one
    // fixed sentence — the server's own message ("sync busy" above) must NOT
    // leak through (the unification review's fidelity catch).
    expect(toasts()).toEqual([{ type: 'error', message: 'Failed to sync filesystem' }]);
  });

  it('toasts the dynamic cleanup-move sentence, and a failure', async () => {
    const fixture = await render(
      flexClient({
        files: [file({ id: 'orphan', fileStatus: 'orphaned' })],
        dispatchDataHandler: (r) =>
          r.type === 'filesCleanupOrphans' && r['dryRun'] === true
            ? { orphanedCount: 1, rescuedCount: 0, duplicateCount: 0, uniqueCount: 1 }
            : { moved: 2, deleted: 1, rescuedCount: 1 },
      }),
    );
    (
      fixture.nativeElement.querySelector('button[title*="click to clean up"]') as HTMLButtonElement
    ).click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    const moveBtn = (
      Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[]
    ).find((b) => b.textContent?.includes('Relocate to /orphans/'));
    expect(moveBtn).toBeTruthy();
    moveBtn!.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(toasts()).toEqual([
      {
        type: 'success',
        message: 'Moved 2 files to /orphans/, removed 1 duplicate, rescued 1 referenced file',
      },
    ]);
  });

  it('toasts the dynamic cleanup-delete sentence', async () => {
    const fixture = await render(
      flexClient({
        files: [file({ id: 'orphan', fileStatus: 'orphaned' })],
        dispatchDataHandler: (r) =>
          r.type === 'filesCleanupOrphans' && r['dryRun'] === true
            ? { orphanedCount: 1, rescuedCount: 0, duplicateCount: 0, uniqueCount: 1 }
            : { deleted: 3, rescuedCount: 0 },
      }),
    );
    (
      fixture.nativeElement.querySelector('button[title*="click to clean up"]') as HTMLButtonElement
    ).click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    (fixture.nativeElement.querySelector('button.bg-destructive') as HTMLButtonElement).click();
    await new Promise((r) => setTimeout(r, 0));
    expect(toasts()).toEqual([{ type: 'success', message: 'Removed 3 orphaned files' }]);
  });

  it('toasts "Failed to analyze orphaned files" on a non-Error rejection', async () => {
    const client = flexClient({ files: [file({ id: 'orphan', fileStatus: 'orphaned' })] });
    client.dispatchData = (async () => Promise.reject('offline')) as CoreClient['dispatchData'];
    const fixture = await render(client);
    (
      fixture.nativeElement.querySelector('button[title*="click to clean up"]') as HTMLButtonElement
    ).click();
    await new Promise((r) => setTimeout(r, 0));
    expect(toasts()).toEqual([{ type: 'error', message: 'Failed to analyze orphaned files' }]);
  });

  it('toasts the server message on a failed dissociate-confirm', async () => {
    const fixture = await render(
      flexClient({
        files: [file({ id: 'linked' })],
        dispatchResult: {
          type: 'error',
          data: { message: 'linked', associations: { characters: [], messages: [] } } as never,
        },
        dispatchDataHandler: (r) =>
          r.type === 'fileDelete' ? new Error('vault is sealed') : undefined,
      }),
    );
    (
      fixture.nativeElement.querySelector('button[title="Delete file"]') as HTMLButtonElement
    ).click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    const confirmBtn = Array.from(
      fixture.nativeElement.querySelectorAll('[role=dialog] button'),
    ).find((b) => (b as HTMLElement).textContent?.includes('Delete Anyway')) as HTMLButtonElement;
    confirmBtn.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(toasts()).toEqual([{ type: 'error', message: 'vault is sealed' }]);
  });
});
