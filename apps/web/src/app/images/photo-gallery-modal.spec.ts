import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { ChatFileDto } from '../core/core-contract';
import { PhotoGalleryModal } from './photo-gallery-modal';

function file(over: Partial<ChatFileDto>): ChatFileDto {
  return {
    id: 'f-1',
    filename: 'valid.png',
    filepath: 'chat/valid.png',
    mimeType: 'image/png',
    ...over,
  };
}

/** A character-photo entry in the pinned P4.6i list shape. */
function photoEntry(over: Record<string, unknown>): Record<string, unknown> {
  return {
    linkId: 'img-1',
    mountPointId: 'mp-1',
    relativePath: 'photos/char-img.png',
    fileName: 'char-img.png',
    blobUrl: '/uploads/char-img.png',
    mimeType: 'image/png',
    sha256: 'abc',
    fileSizeBytes: 1024,
    keptAt: '2025-01-01T00:00:00Z',
    caption: null,
    tags: [],
    ...over,
  };
}

interface StubOptions {
  files?: ChatFileDto[];
  entries?: Record<string, unknown>[];
  onDispatch?: (req: { type: string; [k: string]: unknown }) => void;
}

function stubClient(options: StubOptions): Partial<CoreClient> {
  return {
    dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
      options.onDispatch?.(req);
      if (req.type === 'chatFilesList') return { files: options.files ?? [] };
      if (req.type === 'characterPhotoList') {
        return { entries: options.entries ?? [], total: 0, hasMore: false };
      }
      if (req.type === 'characterList') return { characters: [] };
      return {};
    }) as CoreClient['dispatchData'],
    dispatch: (async (req: { type: string; [k: string]: unknown }) => {
      options.onDispatch?.(req);
      return { type: 'ack', data: {} };
    }) as CoreClient['dispatch'],
  };
}

async function flush(fixture: ComponentFixture<PhotoGalleryModal>): Promise<void> {
  for (let i = 0; i < 5; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(
  client: Partial<CoreClient>,
  inputs: Record<string, unknown> = { chatId: 'chat-1' },
): Promise<ComponentFixture<PhotoGalleryModal>> {
  TestBed.configureTestingModule({
    imports: [PhotoGalleryModal],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(PhotoGalleryModal);
  for (const [key, value] of Object.entries(inputs)) {
    fixture.componentRef.setInput(key, value);
  }
  fixture.detectChanges();
  await flush(fixture);
  return fixture;
}

function gridImgs(fixture: ComponentFixture<PhotoGalleryModal>): HTMLImageElement[] {
  return Array.from(fixture.nativeElement.querySelectorAll('.qt-dialog img'));
}

/**
 * The host + deleted-handling suite. The v4 cases come from
 * `__tests__/unit/photo-gallery-modal-deleted-handling.test.tsx` (380 lines),
 * ported case-for-case; the nested-Escape / arrow-idiom / z-layering specs
 * pin the uncovered `PhotoGalleryModal.tsx` invariants by `file:line` (those
 * behaviors have NO v4 test — this suite is their only fidelity record).
 * v4's "not rendered when isOpen false" case (:328) has no v5 equivalent:
 * the host mounts this component conditionally (`@if (showGallery())`), which
 * IS the isOpen gate.
 */
describe('PhotoGalleryModal', () => {
  afterEach(() => {
    document.body.style.overflow = '';
    TestBed.resetTestingModule();
  });

  describe('chat mode — deleted image handling', () => {
    // v4 :78 — render chat photos and detect missing images.
    it('renders chat photos and swaps a failing image for the placeholder', async () => {
      const fixture = await render(
        stubClient({ files: [file({ id: 'file-1' }), file({ id: 'file-2', filename: 'missing.png' })] }),
      );
      expect(fixture.nativeElement.textContent).toContain('Chat Photos');
      const imgs = gridImgs(fixture);
      expect(imgs.length).toBe(2);

      imgs[1].dispatchEvent(new Event('error'));
      await flush(fixture);

      const placeholder = fixture.nativeElement.querySelector('qt-deleted-image-placeholder');
      expect(placeholder).toBeTruthy();
      expect(placeholder.textContent).toContain('Image Deleted');
    });

    // v4 :107 — a div container for missing images (the placeholder holds a
    // button), :136 — a button container for valid images.
    it('uses a div container for missing tiles and a button for valid ones', async () => {
      const fixture = await render(
        stubClient({ files: [file({ id: 'file-1' }), file({ id: 'file-2' })] }),
      );
      gridImgs(fixture)[1].dispatchEvent(new Event('error'));
      await flush(fixture);

      const placeholder = fixture.nativeElement.querySelector('qt-deleted-image-placeholder');
      expect(placeholder.closest('div.relative.rounded')?.tagName).toBe('DIV');
      const validTile = gridImgs(fixture)[0].closest('.relative.rounded');
      expect(validTile?.tagName).toBe('BUTTON');
    });

    // v4 :157 — reload gallery after cleanup.
    it('reloads the gallery after a placeholder cleanup', async () => {
      const seen: string[] = [];
      vi.spyOn(window, 'confirm').mockReturnValue(true);
      vi.stubGlobal(
        'fetch',
        vi.fn(async () => ({ ok: true, json: async () => ({ success: true }) })),
      );
      const fixture = await render(
        stubClient({
          files: [file({ id: 'file-1' }), file({ id: 'file-2' })],
          onDispatch: (req) => seen.push(req.type),
        }),
      );
      gridImgs(fixture)[1].dispatchEvent(new Event('error'));
      await flush(fixture);
      const listCallsBefore = seen.filter((t) => t === 'chatFilesList').length;

      const removeButton = Array.from(
        fixture.nativeElement.querySelectorAll('qt-deleted-image-placeholder button'),
      ).find((b) => /remove/i.test((b as HTMLElement).textContent ?? '')) as HTMLButtonElement;
      removeButton.click();
      await flush(fixture);

      expect(seen.filter((t) => t === 'chatFilesList').length).toBeGreaterThan(listCallsBefore);
      vi.unstubAllGlobals();
      vi.restoreAllMocks();
    });
  });

  describe('character modes — deleted image handling', () => {
    // v4 :221 — character mode.
    it('handles deleted images in character mode', async () => {
      const fixture = await render(
        stubClient({ entries: [photoEntry({ linkId: 'img-1' })] }),
        { mode: 'character', characterId: 'char-1', characterName: 'Test Character' },
      );
      expect(fixture.nativeElement.textContent).toContain("Test Character's Photos");
      const imgs = gridImgs(fixture);
      expect(imgs.length).toBe(1);

      imgs[0].dispatchEvent(new Event('error'));
      await flush(fixture);
      expect(fixture.nativeElement.querySelector('qt-deleted-image-placeholder')).toBeTruthy();
    });

    // v4 :278 — user-character mode.
    it('handles deleted images in user-character mode', async () => {
      const fixture = await render(
        stubClient({ entries: [photoEntry({ linkId: 'img-2', fileName: 'persona-img.png' })] }),
        {
          mode: 'user-character',
          userCharacterId: 'user-char-1',
          userCharacterName: 'Test User Character',
        },
      );
      expect(fixture.nativeElement.textContent).toContain("Test User Character's Photos");
      gridImgs(fixture)[0].dispatchEvent(new Event('error'));
      await flush(fixture);
      expect(fixture.nativeElement.querySelector('qt-deleted-image-placeholder')).toBeTruthy();
    });
  });

  describe('modal behavior', () => {
    // v4 :307 — close button.
    it('emits close when the close button is clicked', async () => {
      const fixture = await render(stubClient({ files: [file({})] }));
      const closes: number[] = [];
      fixture.componentInstance.close.subscribe(() => closes.push(1));
      (fixture.nativeElement.querySelector('button[title="Close"]') as HTMLButtonElement).click();
      expect(closes.length).toBe(1);
    });

    // v4 :341 — body-overflow lock while open.
    it('locks body overflow while mounted', async () => {
      await render(stubClient({ files: [] }));
      expect(document.body.style.overflow).toBe('hidden');
    });

    // v4 :358 — zoom in/out controls present and enabled at the default index.
    it('supports zoom in/out', async () => {
      const fixture = await render(stubClient({ files: [file({})] }));
      const zoomIn = fixture.nativeElement.querySelector(
        'button[title="Larger thumbnails"]',
      ) as HTMLButtonElement;
      const zoomOut = fixture.nativeElement.querySelector(
        'button[title="Smaller thumbnails"]',
      ) as HTMLButtonElement;
      expect(zoomIn.disabled).toBe(false);
      expect(zoomOut.disabled).toBe(false);
    });
  });

  describe('the routing split and the two pinned invariants (no v4 test exists)', () => {
    // PhotoGalleryModal.tsx:338-351 — a chat item routes to the chat viewer.
    it('routes a chat item to qt-chat-gallery-image-view-modal', async () => {
      const fixture = await render(stubClient({ files: [file({ id: 'file-1' })] }));
      (gridImgs(fixture)[0].closest('button') as HTMLButtonElement).click();
      await flush(fixture);
      expect(fixture.nativeElement.querySelector('qt-chat-gallery-image-view-modal')).toBeTruthy();
      // The detail modal PORTALS to document.body (bug 99), so it is looked for
      // there — a fixture-scoped query could not fail either way.
      expect(document.querySelector('qt-image-detail-modal')).toBeNull();
    });

    // PhotoGalleryModal.tsx:353-361 — an image item routes to the deep detail modal.
    it('routes a character-mode item to qt-image-detail-modal', async () => {
      const fixture = await render(
        stubClient({ entries: [photoEntry({ linkId: 'img-1' })] }),
        { mode: 'character', characterId: 'char-1', characterName: 'C' },
      );
      (gridImgs(fixture)[0].closest('button') as HTMLButtonElement).click();
      await flush(fixture);
      // Portaled to document.body (bug 99) rather than rendered in the pane.
      const detail = document.querySelector('qt-image-detail-modal');
      expect(detail).toBeTruthy();
      expect(detail!.parentElement).toBe(document.body);
      expect(fixture.nativeElement.querySelector('qt-chat-gallery-image-view-modal')).toBeNull();
    });

    // PhotoGalleryModal.tsx:170-174 — the gallery's Escape handler is
    // SUPPRESSED while a detail modal is open: the first press closes the
    // DETAIL layer only, the second closes the gallery.
    it('suppresses its own Escape handler while a detail modal is open', async () => {
      const fixture = await render(
        stubClient({ files: [file({ id: 'file-1' }), file({ id: 'file-2' })] }),
      );
      const closes: number[] = [];
      fixture.componentInstance.close.subscribe(() => closes.push(1));

      (gridImgs(fixture)[0].closest('button') as HTMLButtonElement).click();
      await flush(fixture);
      expect(fixture.nativeElement.querySelector('qt-chat-gallery-image-view-modal')).toBeTruthy();

      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      await flush(fixture);
      // The DETAIL modal closed; the gallery did NOT.
      expect(fixture.nativeElement.querySelector('qt-chat-gallery-image-view-modal')).toBeNull();
      expect(closes.length).toBe(0);

      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      await flush(fixture);
      expect(closes.length).toBe(1);
    });

    // PhotoGalleryModal.tsx:299 vs ImageDetailModal.tsx:87 /
    // ChatGalleryImageViewModal.tsx:208 — the z-layering: gallery z-50,
    // detail z-[60].
    it('layers the gallery at z-50 and the detail modal at z-[60]', async () => {
      const fixture = await render(stubClient({ files: [file({ id: 'file-1' })] }));
      expect(fixture.nativeElement.querySelector('.fixed.inset-0.z-50')).toBeTruthy();
      (gridImgs(fixture)[0].closest('button') as HTMLButtonElement).click();
      await flush(fixture);
      const detail = fixture.nativeElement.querySelector('qt-chat-gallery-image-view-modal');
      expect(detail.querySelector('.fixed.inset-0.z-\\[60\\]')).toBeTruthy();
    });

    // PhotoGalleryModal.tsx:343-344 / :358-359 — the prev/next callbacks are
    // conditionally `undefined` at the ends of the list; the arrows disappear
    // (no disabled state), and the index arithmetic clamps (:188-194).
    it('hands the arrows down as undefined at the ends of the list', async () => {
      const fixture = await render(
        stubClient({ files: [file({ id: 'file-1' }), file({ id: 'file-2' }), file({ id: 'file-3' })] }),
      );
      // First item: no prev arrow, a next arrow.
      (gridImgs(fixture)[0].closest('button') as HTMLButtonElement).click();
      await flush(fixture);
      const detail = () => fixture.nativeElement.querySelector('qt-chat-gallery-image-view-modal');
      expect(detail().querySelector('button[title="Previous image (Left Arrow)"]')).toBeNull();
      expect(detail().querySelector('button[title="Next image (Right Arrow)"]')).toBeTruthy();

      // Arrow to the last item: prev appears, next disappears.
      (detail().querySelector('button[title="Next image (Right Arrow)"]') as HTMLButtonElement).click();
      await flush(fixture);
      (detail().querySelector('button[title="Next image (Right Arrow)"]') as HTMLButtonElement).click();
      await flush(fixture);
      expect(detail().querySelector('button[title="Previous image (Left Arrow)"]')).toBeTruthy();
      expect(detail().querySelector('button[title="Next image (Right Arrow)"]')).toBeNull();
    });
  });

  it('serves a mountFile tile from the server URL, not the id-keyed files route', async () => {
    // Dogfood #48: the announcement walk contributes `mountFile` entries whose
    // `id` is a doc_mount_file_links id. Building /api/v1/files/{id} for those
    // 404s and the grid renders "Image Deleted". v4 uses `url || filepath` for
    // EVERY item (PhotoGalleryModal.tsx:250).
    const fixture = await render(
      stubClient({
        files: [
        file({
          id: 'link-1',
          filename: 'yacht.png',
          type: 'mountFile',
          url: '/api/v1/mount-points/mp-1/blobs/photos%2Fyacht.png',
          filepath: '/api/v1/mount-points/mp-1/blobs/photos%2Fyacht.png',
        }),
        file({ id: 'f-2', filename: 'upload.png', type: 'chatFile' }),
        ],
      }),
    );
    await flush(fixture);

    const srcs = gridImgs(fixture).map((i) => (i as HTMLImageElement).getAttribute('src'));
    expect(srcs[0]).toBe('/api/v1/mount-points/mp-1/blobs/photos%2Fyacht.png');
    expect(srcs[0]).not.toContain('/api/v1/files/');
    // A genuine chat file still takes the v5 thumbnail route.
    expect(srcs[1]).toContain('/api/v1/files/f-2');
    expect(srcs[1]).toContain('action=thumbnail');
  });
});
