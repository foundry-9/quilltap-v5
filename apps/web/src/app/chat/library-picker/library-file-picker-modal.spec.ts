import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { LibraryFilePickerModal, type LinkedLibraryFile } from './library-file-picker-modal';
import { ToastService } from '../../ui/toast.service';

interface Dispatched {
  type: string;
  [k: string]: unknown;
}

interface FetchCall {
  url: string;
  body: Record<string, unknown>;
}

const STORE = {
  id: 'store-1',
  name: 'Research',
  mountType: 'database' as const,
  storeType: 'documents' as const,
  enabled: true,
};

const CHARACTER_VAULT = { ...STORE, id: 'vault-1', name: 'Lorian', storeType: 'character' as const };

const GALLERY_ENTRY = {
  linkId: 'link-1',
  mountPointId: 'mp-persona',
  relativePath: 'photos/dawn.webp',
  fileName: 'dawn.webp',
  blobUrl: '/api/v1/mount-points/mp-persona/blobs/photos/dawn.webp',
  mimeType: 'image/webp',
  caption: 'Dawn over the rooftops',
  keptAt: '2026-01-01T00:00:00.000Z',
};

const LEGACY_FILE = {
  id: 'file-1',
  originalFilename: 'note.txt',
  filename: 'note.txt',
  mimeType: 'text/plain',
  size: 12,
  category: 'general',
  description: null,
  projectId: null,
  folderPath: '/',
  filepath: '/api/v1/files/file-1',
  fileStatus: 'ok',
  createdAt: '2026-01-01T00:00:00.000Z',
  updatedAt: '2026-01-01T00:00:00.000Z',
};

const STORE_ROW = {
  id: 'row-1',
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
};

function stubClient(
  log: Dispatched[],
  answers: Record<string, Record<string, unknown>>,
): Partial<CoreClient> {
  return {
    dispatchData: (async (req: Dispatched) => {
      log.push(req);
      return answers[req.type] ?? {};
    }) as CoreClient['dispatchData'],
  };
}

const DEFAULT_ANSWERS: Record<string, Record<string, unknown>> = {
  projectList: { projects: [{ id: 'p1', name: 'The Novel', icon: null }] },
  mountPointList: { mountPoints: [STORE, CHARACTER_VAULT] },
  chatGroupStores: { stores: [] },
  chatPhotoAlbums: { albums: [] },
  filesList: { files: [LEGACY_FILE] },
  filesFoldersList: { folders: [] },
  mountFilesList: { files: [STORE_ROW] },
  characterPhotoList: { entries: [GALLERY_ENTRY] },
  photoGalleryList: { entries: [GALLERY_ENTRY] },
};

let fetchCalls: FetchCall[] = [];
let fetchResponse: () => Response;

function okJson(body: unknown): Response {
  return {
    ok: true,
    status: 200,
    statusText: 'OK',
    json: async () => body,
  } as unknown as Response;
}

beforeEach(() => {
  fetchCalls = [];
  fetchResponse = () => okJson({});
  vi.stubGlobal('fetch', async (url: string, init: RequestInit) => {
    fetchCalls.push({ url, body: JSON.parse(String(init.body)) as Record<string, unknown> });
    return fetchResponse();
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(
  answers: Record<string, Record<string, unknown>> = {},
  log: Dispatched[] = [],
): Promise<ComponentFixture<LibraryFilePickerModal>> {
  // Two beats render twice (with and without a persona / group stores), so the
  // module must be torn down between renders, not just between tests.
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [LibraryFilePickerModal],
    providers: [
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: stubClient(log, { ...DEFAULT_ANSWERS, ...answers }) },
    ],
  });
  const fixture = TestBed.createComponent(LibraryFilePickerModal);
  fixture.componentRef.setInput('chatId', 'chat-1');
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

function buttons(fixture: ComponentFixture<unknown>): HTMLButtonElement[] {
  return Array.from(fixture.nativeElement.querySelectorAll('button'));
}

function clickByText(fixture: ComponentFixture<unknown>, text: string): void {
  const button = buttons(fixture).find((b) => b.textContent?.includes(text));
  if (!button) throw new Error(`no button containing "${text}"`);
  button.click();
  fixture.detectChanges();
}

/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('qt-library-file-picker-modal — the scope step', () => {
  it('always offers General and the gallery, and titles the step v4’s way', async () => {
    const fixture = await render();
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Choose File Source');
    expect(text).toContain('General');
    expect(text).toContain('Files not assigned to any project');
    expect(text).toContain('My Gallery');
    expect(text).toContain("Photos you've saved from chats");
  });

  it('excludes character vaults from Document Stores (the composer may not reach them)', async () => {
    const fixture = await render();
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Document Stores');
    expect(text).toContain('Research');
    // The character vault is in the same mountPointList response and must not show.
    expect(text).not.toContain('Lorian');
  });

  it('names the gallery after the DEFAULT user persona when there is one', async () => {
    const fixture = await render({
      chatPhotoAlbums: {
        albums: [
          { mountPointId: 'a', name: 'Alt', kind: 'character', isUserCharacter: true },
          {
            mountPointId: 'b',
            name: 'Riya',
            kind: 'character',
            isUserCharacter: true,
            isDefault: true,
            characterId: 'char-riya',
          },
        ],
      },
    });
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain("Riya's Photos");
    expect(text).toContain("Photos saved to Riya's vault");
  });

  it('renders Group Files only when the chat resolves group stores', async () => {
    const withoutGroups = await render();
    expect(withoutGroups.nativeElement.textContent).not.toContain('Group Files');

    const withGroups = await render({
      chatGroupStores: { stores: [{ ...STORE, id: 'g1', name: 'The Cartographers' }] },
    });
    const text = withGroups.nativeElement.textContent as string;
    expect(text).toContain('Group Files');
    expect(text).toContain('The Cartographers');
    expect(text).toContain('Shared with your group');
  });
});

describe('qt-library-file-picker-modal — the pick legs', () => {
  it('LINKS a legacy file and hands it to the composer tray, with v4’s sentence', async () => {
    fetchResponse = () =>
      okJson({
        file: {
          id: 'file-1',
          filename: 'note.txt',
          filepath: 'uploads/note.txt',
          mimeType: 'text/plain',
          url: '/api/v1/files/file-1',
        },
      });
    const fixture = await render();
    const linked: LinkedLibraryFile[] = [];
    let attached = 0;
    let closed = 0;
    fixture.componentInstance.fileLinked.subscribe((f) => linked.push(f));
    fixture.componentInstance.mountFileAttached.subscribe(() => attached++);
    fixture.componentInstance.close.subscribe(() => closed++);

    clickByText(fixture, 'General');
    await settle(fixture);
    clickByText(fixture, 'note.txt');
    await settle(fixture);

    expect(fetchCalls).toHaveLength(1);
    expect(fetchCalls[0].url).toBe('/api/v1/chats/chat-1/files?action=link');
    expect(fetchCalls[0].body).toEqual({ fileId: 'file-1' });
    expect(linked).toHaveLength(1);
    expect(linked[0].file.filename).toBe('note.txt');
    expect(toasts()).toEqual([{ type: 'success', message: 'Linked "note.txt" to chat' }]);
    // The tray hand-off is the legacy leg's ALONE.
    expect(attached).toBe(0);
    expect(closed).toBe(1);
  });

  it('ATTACHES a document-store file — no tray hand-off, the announcement stands', async () => {
    const fixture = await render();
    const linked: LinkedLibraryFile[] = [];
    let attached = 0;
    fixture.componentInstance.fileLinked.subscribe((f) => linked.push(f));
    fixture.componentInstance.mountFileAttached.subscribe(() => attached++);

    clickByText(fixture, 'Research');
    await settle(fixture);
    clickByText(fixture, 'plan.md');
    await settle(fixture);

    expect(fetchCalls).toHaveLength(1);
    expect(fetchCalls[0].url).toBe('/api/v1/chats/chat-1/files?action=attach-mount-file');
    expect(fetchCalls[0].body).toEqual({ mountPointId: 'store-1', relativePath: 'plan.md' });
    expect(attached).toBe(1);
    expect(toasts()).toEqual([
      { type: 'success', message: 'Attached "plan.md" — the Librarian has noted it' },
    ]);
    expect(linked).toEqual([]);
  });

  it('a gallery pick ALWAYS attaches, named by its caption', async () => {
    const fixture = await render({
      chatPhotoAlbums: {
        albums: [
          {
            mountPointId: 'mp-persona',
            name: 'Riya',
            kind: 'character',
            isUserCharacter: true,
            isDefault: true,
            characterId: 'char-riya',
          },
        ],
      },
    });
    let attached = 0;
    fixture.componentInstance.mountFileAttached.subscribe(() => attached++);

    clickByText(fixture, "Riya's Photos");
    await settle(fixture);
    (fixture.nativeElement.querySelector('li button') as HTMLButtonElement).click();
    await settle(fixture);

    expect(fetchCalls).toHaveLength(1);
    expect(fetchCalls[0].url).toBe('/api/v1/chats/chat-1/files?action=attach-mount-file');
    expect(fetchCalls[0].body).toEqual({
      mountPointId: 'mp-persona',
      relativePath: 'photos/dawn.webp',
    });
    expect(attached).toBe(1);
    expect(toasts()).toEqual([
      { type: 'success', message: 'Attached "Dawn over the rooftops" — the Librarian has noted it' },
    ]);
  });

  it('reads the persona’s vault photos, falling back to the global gallery', async () => {
    const withPersona: Dispatched[] = [];
    const fixture = await render(
      {
        chatPhotoAlbums: {
          albums: [
            {
              mountPointId: 'mp-persona',
              name: 'Riya',
              kind: 'character',
              isUserCharacter: true,
              isDefault: true,
              characterId: 'char-riya',
            },
          ],
        },
      },
      withPersona,
    );
    clickByText(fixture, "Riya's Photos");
    await settle(fixture);
    const personaRead = withPersona.find((r) => r.type === 'characterPhotoList');
    expect(personaRead).toBeDefined();
    expect(personaRead!['characterId']).toBe('char-riya');
    expect(personaRead!['limit']).toBe(200);
    expect(withPersona.some((r) => r.type === 'photoGalleryList')).toBe(false);

    const noPersona: Dispatched[] = [];
    const plain = await render({}, noPersona);
    clickByText(plain, 'My Gallery');
    await settle(plain);
    expect(noPersona.some((r) => r.type === 'photoGalleryList')).toBe(true);
    expect(noPersona.some((r) => r.type === 'characterPhotoList')).toBe(false);
  });

  it('surfaces the server’s message on a failed write and does not close', async () => {
    fetchResponse = () =>
      ({
        ok: false,
        status: 404,
        statusText: 'Not Found',
        json: async () => ({ error: 'Mount-point file not found' }),
      }) as unknown as Response;
    const fixture = await render();
    let closed = 0;
    fixture.componentInstance.close.subscribe(() => closed++);

    clickByText(fixture, 'Research');
    await settle(fixture);
    clickByText(fixture, 'plan.md');
    await settle(fixture);

    expect(toasts().at(-1)?.message).toContain('Mount-point file not found');
    expect(closed).toBe(0);
  });
});

describe('qt-library-file-picker-modal — navigation and sealing', () => {
  it('titles the browse step after the scope, and Back returns to the scope list', async () => {
    const fixture = await render();
    clickByText(fixture, 'Research');
    await settle(fixture);
    expect(fixture.nativeElement.textContent).toContain('Browse Files — Research');

    clickByText(fixture, 'Back');
    await settle(fixture);
    expect(fixture.nativeElement.textContent).toContain('Choose File Source');
  });

  it('offers Back only past the scope step', async () => {
    const fixture = await render();
    expect(buttons(fixture).some((b) => b.textContent?.trim() === 'Back')).toBe(false);
    clickByText(fixture, 'General');
    await settle(fixture);
    expect(buttons(fixture).some((b) => b.textContent?.trim() === 'Back')).toBe(true);
  });

  it('refuses to close while a pick is in flight (v4 seals the dialog)', async () => {
    let release: (() => void) | undefined;
    fetchResponse = () => okJson({ file: LEGACY_FILE });
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    vi.stubGlobal('fetch', async (url: string, init: RequestInit) => {
      fetchCalls.push({ url, body: JSON.parse(String(init.body)) as Record<string, unknown> });
      await gate;
      return okJson({ file: LEGACY_FILE });
    });

    const fixture = await render();
    let closed = 0;
    fixture.componentInstance.close.subscribe(() => closed++);

    clickByText(fixture, 'General');
    await settle(fixture);
    clickByText(fixture, 'note.txt');
    fixture.detectChanges();

    // The overlay is up and Cancel is inert.
    expect(
      fixture.nativeElement.querySelector('[data-testid="library-picker-linking"]'),
    ).not.toBeNull();
    clickByText(fixture, 'Cancel');
    expect(closed).toBe(0);

    release!();
    await settle(fixture);
  });
});
