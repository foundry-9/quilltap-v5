import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../core/core-client';
import { CoreDispatchError } from '../core/core-contract';
import type { AlbumOption, CoreResponse, MessageAttachment } from '../core/core-contract';
import { SaveImageDialog, type SaveImageTarget } from './save-image-dialog';

const IMG: MessageAttachment = {
  id: 'file-1',
  filename: 'sketch.png',
  filepath: 'uploads/sketch.png',
  mimeType: 'image/png',
};

function albums(): AlbumOption[] {
  return [
    { mountPointId: 'mp-char', name: 'Lorian', kind: 'character', isUserCharacter: false },
    { mountPointId: 'mp-you', name: 'Riya', kind: 'character', isUserCharacter: true },
    { mountPointId: 'mp-gen', name: 'Quilltap General', kind: 'general', isDefault: true },
  ];
}

interface DispatchLog {
  requests: { type: string; [k: string]: unknown }[];
}

function stubClient(
  log: DispatchLog,
  albumList: AlbumOption[],
  saveResponse?: CoreResponse,
): Partial<CoreClient> {
  return {
    dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
      if (req.type === 'chatPhotoAlbums') return { albums: albumList };
      if (req.type === 'wardrobeItemImageSaveTargets') {
        log.requests.push(req);
        return { albums: albumList };
      }
      if (req.type === 'inventoryItemImageSaveTargets') {
        log.requests.push(req);
        return { albums: albumList };
      }
      if (req.type === 'wardrobeItemImageSaveToStore' || req.type === 'inventoryItemImageSaveToStore') {
        log.requests.push(req);
        const resp = saveResponse ?? {
          type: 'ack',
          data: { saved: true, mountPoint: 'Quilltap General', relativePath: 'photos/x.webp' },
        };
        if (resp.type === 'error') throw new CoreDispatchError(resp.data);
        return resp.data as Record<string, unknown>;
      }
      return {};
    }) as CoreClient['dispatchData'],
    dispatch: (async (req: { type: string; [k: string]: unknown }) => {
      log.requests.push(req);
      return (
        saveResponse ?? { type: 'ack', data: { mountPoint: 'mp-gen', relativePath: 'photos/x.webp' } }
      );
    }) as CoreClient['dispatch'],
  };
}

async function render(
  client: Partial<CoreClient>,
  target: SaveImageTarget = { kind: 'message', chatId: 'chat-1', messageId: 'm-1', fileId: 'file-1' },
  attachments: MessageAttachment[] = [IMG],
  overlayClass?: string,
  queryClient: QueryClient = new QueryClient(),
): Promise<ComponentFixture<SaveImageDialog>> {
  TestBed.configureTestingModule({
    imports: [SaveImageDialog],
    providers: [provideTanStackQuery(queryClient), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(SaveImageDialog);
  fixture.componentRef.setInput('target', target);
  fixture.componentRef.setInput('attachments', attachments);
  if (overlayClass !== undefined) fixture.componentRef.setInput('overlayClass', overlayClass);
  fixture.detectChanges();
  for (let i = 0; i < 5; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

describe('SaveImageDialog', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('groups albums into kind optgroups and labels the user character', async () => {
    const fixture = await render(stubClient({ requests: [] }, albums()));
    const groups = fixture.nativeElement.querySelectorAll('optgroup');
    expect(groups.length).toBe(2); // character + general (no project/document-store)
    expect(groups[0].getAttribute('label')).toBe('Character');
    expect(groups[1].getAttribute('label')).toBe('Quilltap General');
    expect(fixture.nativeElement.textContent).toContain('Riya (you)');
  });

  // v4 `SaveImageDialog.tsx:298-304` at `01a83539d` — the empty sentence by door.
  it('shows the chat empty state when no albums are available', async () => {
    const fixture = await render(stubClient({ requests: [] }, []));
    expect(fixture.nativeElement.textContent).toContain('No photo albums are available for this chat.');
    expect(fixture.nativeElement.textContent).not.toContain('No document stores are available.');
  });

  it('saves via messageSaveImage with the default album and trimmed caption (the message door)', async () => {
    const log: DispatchLog = { requests: [] };
    const fixture = await render(stubClient(log, albums()));
    const caption = fixture.nativeElement.querySelector('#save-image-caption') as HTMLInputElement;
    caption.value = '  by the sea  ';
    caption.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    (fixture.nativeElement.querySelector('.qt-dialog-footer button:last-child') as HTMLButtonElement).click();
    await new Promise((r) => setTimeout(r, 0));
    expect(log.requests[0]).toEqual({
      type: 'messageSaveImage',
      chatId: 'chat-1',
      messageId: 'm-1',
      fileId: 'file-1',
      mountPointId: 'mp-gen',
      caption: 'by the sea',
    });
  });

  // P4.D176 — the chat gallery's door: NO messageId, a `chatSaveGalleryImage`
  // dispatch instead of `messageSaveImage`.
  it('saves via chatSaveGalleryImage from the gallery door — no messageId anywhere', async () => {
    const log: DispatchLog = { requests: [] };
    const galleryAttachment: MessageAttachment = {
      id: 'gallery-file-9',
      filename: 'backdrop.webp',
      filepath: '/api/v1/files/gallery-file-9',
      mimeType: 'image/webp',
    };
    const fixture = await render(
      stubClient(log, albums()),
      { kind: 'chat', chatId: 'chat-1', fileId: 'gallery-file-9' },
      [galleryAttachment],
    );
    (fixture.nativeElement.querySelector('.qt-dialog-footer button:last-child') as HTMLButtonElement).click();
    await new Promise((r) => setTimeout(r, 0));
    expect(log.requests[0]).toEqual({
      type: 'chatSaveGalleryImage',
      chatId: 'chat-1',
      fileId: 'gallery-file-9',
      mountPointId: 'mp-gen',
      caption: undefined,
    });
  });

  // v4 `:157-168` — `res.status === 409 || body.code === 'ALREADY_SAVED'`: BOTH
  // doors, and `body.keptAt` formatted into the sentence when present. The §3
  // unification review of the `78b381a96` round: the arm was gated on the chat
  // leg alone and the riders were unreachable, so neither sentence could show.
  it('says "already in this album", dated, when the chat leg answers 409 with keptAt', async () => {
    const fixture = await render(
      stubClient({ requests: [] }, albums(), {
        type: 'error',
        data: {
          kind: 'conflict',
          message: 'Image already saved to this album',
          code: 'ALREADY_SAVED',
          alreadySaved: { relativePath: 'photos/x.webp', keptAt: '2026-09-01T12:00:00.000Z' },
        },
      }),
      { kind: 'chat', chatId: 'chat-1', fileId: 'file-1' },
    );
    (fixture.nativeElement.querySelector('.qt-dialog-footer button:last-child') as HTMLButtonElement).click();
    for (let i = 0; i < 5; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    const when = new Date('2026-09-01T12:00:00.000Z').toLocaleDateString();
    expect(fixture.nativeElement.textContent).toContain(
      `That picture is already in this album — it was filed there on ${when}.`,
    );
  });

  it('says "already in this album" (undated) when the MESSAGE leg answers 400 with the same code', async () => {
    const fixture = await render(
      stubClient({ requests: [] }, albums(), {
        type: 'error',
        data: { kind: 'badRequest', message: 'Image already saved to this album', code: 'ALREADY_SAVED' },
      }),
    );
    (fixture.nativeElement.querySelector('.qt-dialog-footer button:last-child') as HTMLButtonElement).click();
    for (let i = 0; i < 5; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(fixture.nativeElement.textContent).toContain('That picture is already in this album.');
    expect(fixture.nativeElement.textContent).not.toContain('filed there on');
  });

  // v4 `:82-84` is `target.fileId || imageAttachments[0]?.id || ''` — JS `||`,
  // so an EMPTY fileId falls through to the first image.
  it('falls through to the first attachment when target.fileId is an empty string', async () => {
    // Two attachments so the picker renders (a single one hides it); the FIRST
    // must be the selected one, not "nothing" — a `??` would have kept ''.
    const two: MessageAttachment = { ...IMG, id: 'file-2', filename: 'two.png' };
    const fixture = await render(
      stubClient({ requests: [] }, albums()),
      { kind: 'chat', chatId: 'chat-1', fileId: '' },
      [IMG, two],
    );
    const selected = fixture.nativeElement.querySelector('.qt-chat-attachment-button.ring-2');
    expect(selected?.getAttribute('title')).toBe(IMG.filename);
  });

  it('pre-selects the attachment from target.fileId (v4 :82-84)', async () => {
    const two: MessageAttachment = { ...IMG, id: 'file-2', filename: 'two.png' };
    const fixture = await render(
      stubClient({ requests: [] }, albums()),
      { kind: 'message', chatId: 'chat-1', messageId: 'm-1', fileId: 'file-2' },
      [IMG, two],
    );
    const selected = fixture.nativeElement.querySelector('.qt-chat-attachment-button.ring-2');
    expect(selected.getAttribute('title')).toBe('two.png');
  });

  it('omits the multi-image picker when there is a single attachment', async () => {
    const fixture = await render(stubClient({ requests: [] }, albums()));
    // The picker buttons only render with >1 image (the single preview img has no button).
    expect(fixture.nativeElement.querySelectorAll('.qt-chat-attachment-button').length).toBe(0);
  });

  it('shows a picker button per image when the message carries several', async () => {
    const second: MessageAttachment = { ...IMG, id: 'file-2', filename: 'two.png' };
    const fixture = await render(stubClient({ requests: [] }, albums()), undefined, [IMG, second]);
    expect(fixture.nativeElement.querySelectorAll('.qt-chat-attachment-button').length).toBe(2);
  });

  // v4 :159-166 — the chat leg's 409 reads as "already in this album", not a
  // generic failure.
  it("reads the chat leg's 409 (kind: 'conflict') as an already-saved answer, not a failure", async () => {
    const log: DispatchLog = { requests: [] };
    const fixture = await render(
      stubClient(log, albums(), {
        type: 'error',
        data: { kind: 'conflict', message: 'That picture is already in this album.' },
      }),
      { kind: 'chat', chatId: 'chat-1', fileId: 'file-1' },
    );
    (fixture.nativeElement.querySelector('.qt-dialog-footer button:last-child') as HTMLButtonElement).click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('[role="alert"]').textContent).toContain(
      'already in this album',
    );
  });

  it('falls back to v4’s undated sentence on a conflict with no server message', async () => {
    const fixture = await render(
      stubClient({ requests: [] }, albums(), {
        type: 'error',
        data: { kind: 'conflict', message: '' },
      }),
      { kind: 'chat', chatId: 'chat-1', fileId: 'file-1' },
    );
    (fixture.nativeElement.querySelector('.qt-dialog-footer button:last-child') as HTMLButtonElement).click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('[role="alert"]').textContent).toBe(
      'That picture is already in this album.',
    );
  });

  // v4 :140 — the message leg still answers 400 with the SAME ALREADY_SAVED
  // code; v5's dispatch envelope has no typed way to tell it apart from any
  // other 400, so it falls to the server's own sentence (measured deferral).
  it('the message leg surfaces a 400 as the server’s own sentence', async () => {
    const fixture = await render(
      stubClient({ requests: [] }, albums(), {
        type: 'error',
        data: { kind: 'bad-request', message: 'Image is not in this chat' },
      }),
    );
    (fixture.nativeElement.querySelector('.qt-dialog-footer button:last-child') as HTMLButtonElement).click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('[role="alert"]').textContent).toBe(
      'Image is not in this chat',
    );
  });

  // ---------------------------------------------------------------------------
  // P4.D277 — `01a83539d`: the wardrobe door, `chatId` on the chat targets,
  // `overlayClassName`, and (R-C) v4 BaseModal's portal + closeOnEscape.
  // ---------------------------------------------------------------------------

  const WARDROBE: SaveImageTarget = {
    kind: 'wardrobe',
    itemId: 'item-1',
    container: { scope: 'character', id: 'char-1' },
    fileId: 'file-1',
  };

  async function settle(fixture: ComponentFixture<SaveImageDialog>): Promise<void> {
    for (let i = 0; i < 5; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
  }

  function clickSave(fixture: ComponentFixture<SaveImageDialog>): void {
    (fixture.nativeElement.querySelector('.qt-dialog-footer button:last-child') as HTMLButtonElement).click();
  }

  it('the wardrobe door reads wardrobeItemImageSaveTargets and saves through wardrobeItemImageSaveToStore (v4 :56-72)', async () => {
    const log: DispatchLog = { requests: [] };
    const fixture = await render(stubClient(log, albums()), WARDROBE);
    const caption = fixture.nativeElement.querySelector('#save-image-caption') as HTMLInputElement;
    caption.value = '  for the ball  ';
    caption.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    clickSave(fixture);
    await settle(fixture);
    expect(log.requests).toEqual([
      { type: 'wardrobeItemImageSaveTargets', scope: 'character', containerId: 'char-1', itemId: 'item-1' },
      {
        type: 'wardrobeItemImageSaveToStore',
        scope: 'character',
        containerId: 'char-1',
        itemId: 'item-1',
        fileId: 'file-1',
        mountPointId: 'mp-gen',
        caption: 'for the ball',
      },
    ]);
  });

  // The `01a83539d` unification (§S.9's flipped P4.D277 beat): a SECOND open of
  // the dialog in one session gets the album list from TanStack's cache without
  // the query function running, so a seed placed INSIDE `queryFn` never ran and
  // the browser showed the first option. v4 seeds from the fetched albums on
  // every mount (`SaveImageDialog.tsx`'s effect over the list), cached or not.
  it('a second open seeds the default album from the CACHED list (no refetch, no query function)', async () => {
    const log: DispatchLog = { requests: [] };
    const queryClient = new QueryClient({ defaultOptions: { queries: { staleTime: Infinity } } });
    queryClient.setQueryData(['wardrobeItemImageSaveTargets', 'item-1', 'character', 'char-1'], albums());
    const fixture = await render(stubClient(log, albums()), WARDROBE, [IMG], undefined, queryClient);
    const select = fixture.nativeElement.querySelector('#save-image-album') as HTMLSelectElement;
    expect(select.value).toBe('mp-gen');
    expect(log.requests.filter((r) => r.type === 'wardrobeItemImageSaveTargets')).toEqual([]);
    clickSave(fixture);
    await settle(fixture);
    expect(log.requests.map((r) => r.type)).toEqual(['wardrobeItemImageSaveToStore']);
  });

  it('the wardrobe door omits a blank caption (never "") and General carries no containerId', async () => {
    const log: DispatchLog = { requests: [] };
    const saved: string[] = [];
    const fixture = await render(stubClient(log, albums()), {
      kind: 'wardrobe',
      itemId: 'item-2',
      container: { scope: 'general', id: null },
      fileId: 'file-1',
    });
    fixture.componentInstance.saved.subscribe((info) => saved.push(info.mountPoint));
    const caption = fixture.nativeElement.querySelector('#save-image-caption') as HTMLInputElement;
    caption.value = '   ';
    caption.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    clickSave(fixture);
    await settle(fixture);
    expect(log.requests[1]).toEqual({
      type: 'wardrobeItemImageSaveToStore',
      scope: 'general',
      itemId: 'item-2',
      fileId: 'file-1',
      mountPointId: 'mp-gen',
    });
    expect('caption' in log.requests[1]).toBe(false);
    expect(saved).toEqual(['Quilltap General']);
  });

  it("the wardrobe door's empty sentence is v4's document-store one (v4 :298-304)", async () => {
    const fixture = await render(stubClient({ requests: [] }, []), WARDROBE);
    expect(fixture.nativeElement.textContent).toContain('No document stores are available.');
    expect(fixture.nativeElement.textContent).not.toContain('No photo albums are available for this chat.');
  });

  it('lands overlayClass on the modal overlay (v4 :84-88 → BaseModal :107)', async () => {
    const fixture = await render(stubClient({ requests: [] }, albums()), WARDROBE, [IMG], 'z-[100]');
    const overlay = fixture.nativeElement.querySelector('.qt-dialog-overlay') as HTMLElement;
    expect(overlay.classList.contains('z-[100]')).toBe(true);
  });

  it('portals its host to document.body (v4 BaseModal :106 createPortal — R-C)', async () => {
    const fixture = await render(stubClient({ requests: [] }, albums()));
    expect((fixture.nativeElement as HTMLElement).parentElement).toBe(document.body);
  });

  it('closes on Escape, once (v4 BaseModal closeOnEscape :97-100 — R-C)', async () => {
    const fixture = await render(stubClient({ requests: [] }, albums()), WARDROBE);
    let closes = 0;
    fixture.componentInstance.close.subscribe(() => (closes += 1));
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(closes).toBe(1);
  });

  // P4.D275 moves the message door's ALREADY_SAVED 400 → 409 (C2) — transparent.
  it("reads the message door's NEW 409 (conflict + ALREADY_SAVED + keptAt) dated", async () => {
    const fixture = await render(
      stubClient({ requests: [] }, albums(), {
        type: 'error',
        data: {
          kind: 'conflict',
          message: 'Image already saved to this album',
          code: 'ALREADY_SAVED',
          alreadySaved: { relativePath: 'photos/x.webp', keptAt: '2026-09-01T12:00:00.000Z' },
        },
      }),
    );
    clickSave(fixture);
    await settle(fixture);
    const when = new Date('2026-09-01T12:00:00.000Z').toLocaleDateString();
    expect(fixture.nativeElement.querySelector('[role="alert"]').textContent).toBe(
      `That picture is already in this album — it was filed there on ${when}.`,
    );
  });

  it("the wardrobe door's ALREADY_SAVED 409 reads dated", async () => {
    const fixture = await render(
      stubClient({ requests: [] }, albums(), {
        type: 'error',
        data: {
          kind: 'conflict',
          message: 'That picture is already in this album',
          code: 'ALREADY_SAVED',
          alreadySaved: { relativePath: 'photos/x.webp', keptAt: '2026-09-01T12:00:00.000Z' },
        },
      }),
      WARDROBE,
    );
    clickSave(fixture);
    await settle(fixture);
    const when = new Date('2026-09-01T12:00:00.000Z').toLocaleDateString();
    expect(fixture.nativeElement.querySelector('[role="alert"]').textContent).toBe(
      `That picture is already in this album — it was filed there on ${when}.`,
    );
  });

  it('the archived-vault 409 (kind conflict, no code) reads as already-saved — v4-faithful (R-A)', async () => {
    const fixture = await render(
      stubClient({ requests: [] }, albums(), {
        type: 'error',
        data: {
          kind: 'conflict',
          message: 'That store belongs to an archived character and cannot be written to',
        },
      }),
      WARDROBE,
    );
    clickSave(fixture);
    await settle(fixture);
    expect(fixture.nativeElement.querySelector('[role="alert"]').textContent).toBe(
      'That picture is already in this album.',
    );
  });

  it("a non-conflict wardrobe refusal shows the server's sentence", async () => {
    const fixture = await render(
      stubClient({ requests: [] }, albums(), {
        type: 'error',
        data: { kind: 'bad-request', message: 'That picture does not belong to this wardrobe item' },
      }),
      WARDROBE,
    );
    clickSave(fixture);
    await settle(fixture);
    expect(fixture.nativeElement.querySelector('[role="alert"]').textContent).toBe(
      'That picture does not belong to this wardrobe item',
    );
  });

  // === P4.D330: the Inventory door (v4 `4fbdad186` SaveImageDialog.tsx:30-74, :304) ===
  const INVENTORY: SaveImageTarget = {
    kind: 'inventory',
    itemId: 'sword',
    container: { scope: 'character', id: 'c-viv' },
    fileId: 'file-1',
  };

  it('the inventory door reads inventoryItemImageSaveTargets and saves through inventoryItemImageSaveToStore', async () => {
    const log: DispatchLog = { requests: [] };
    const fixture = await render(stubClient(log, albums()), INVENTORY);
    const caption = fixture.nativeElement.querySelector('#save-image-caption') as HTMLInputElement;
    caption.value = ' the blade ';
    caption.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    clickSave(fixture);
    await settle(fixture);
    expect(log.requests).toEqual([
      { type: 'inventoryItemImageSaveTargets', container: { scope: 'character', id: 'c-viv' }, itemId: 'sword' },
      {
        type: 'inventoryItemImageSaveToStore',
        container: { scope: 'character', id: 'c-viv' },
        itemId: 'sword',
        fileId: 'file-1',
        mountPointId: 'mp-gen',
        caption: 'the blade',
      },
    ]);
  });

  it("the inventory door's albums are cached under their own key", async () => {
    const log: DispatchLog = { requests: [] };
    const queryClient = new QueryClient({ defaultOptions: { queries: { staleTime: Infinity } } });
    queryClient.setQueryData(['inventoryItemImageSaveTargets', 'sword', 'character', 'c-viv'], albums());
    const fixture = await render(stubClient(log, albums()), INVENTORY, [IMG], undefined, queryClient);
    expect((fixture.nativeElement.querySelector('#save-image-album') as HTMLSelectElement).value).toBe('mp-gen');
    expect(log.requests).toEqual([]);
  });

  it("the inventory door's empty sentence is the document-store one", async () => {
    const fixture = await render(stubClient({ requests: [] }, []), INVENTORY);
    expect(fixture.nativeElement.textContent).toContain('No document stores are available.');
  });
  // === end P4.D330 ===
});
