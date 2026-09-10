import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { coreStreamStub } from '../core/core-client.testing';
import type { ChatGalleryEntry, ChatGallerySource, ChatGalleryResult } from '../core/core-contract';
import { ToastService } from '../ui/toast.service';
import { PhotoGalleryModal } from './photo-gallery-modal';

function entry(over: Partial<ChatGalleryEntry> = {}): ChatGalleryEntry {
  return {
    id: 'file-1',
    idKind: 'file',
    url: '/api/v1/files/file-1',
    filename: 'valid.png',
    mimeType: 'image/png',
    size: 1024,
    createdAt: '2026-03-05T12:00:00Z',
    source: 'attachment',
    isCurrent: false,
    deletable: true,
    ...over,
  };
}

const EMPTY_COUNTS: Record<ChatGallerySource, number> = {
  'story-background': 0,
  avatar: 0,
  portrait: 0,
  generated: 0,
  attachment: 0,
  kept: 0,
  inline: 0,
};

function counts(over: Partial<Record<ChatGallerySource, number>>): Record<ChatGallerySource, number> {
  return { ...EMPTY_COUNTS, ...over };
}

function galleryResult(entries: ChatGalleryEntry[]): ChatGalleryResult {
  const bySource = counts(
    entries.reduce<Partial<Record<ChatGallerySource, number>>>((acc, e) => {
      acc[e.source] = (acc[e.source] ?? 0) + 1;
      return acc;
    }, {}),
  );
  return { entries, counts: bySource, total: entries.length };
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
  entries?: ChatGalleryEntry[];
  characterEntries?: Record<string, unknown>[];
  onDispatch?: (req: { type: string; [k: string]: unknown }) => void;
}

function stubClient(options: StubOptions): CoreClient {
  return {
    ...coreStreamStub(),
    dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
      options.onDispatch?.(req);
      if (req.type === 'chatGallery') return galleryResult(options.entries ?? []);
      if (req.type === 'characterPhotoList') {
        return { entries: options.characterEntries ?? [], total: 0, hasMore: false };
      }
      if (req.type === 'chatPhotoAlbums') return { albums: [] };
      return {};
    }) as CoreClient['dispatchData'],
    dispatch: (async (req: { type: string; [k: string]: unknown }) => {
      options.onDispatch?.(req);
      return { type: 'ack', data: {} };
    }) as CoreClient['dispatch'],
  } as unknown as CoreClient;
}

async function flush(fixture: ComponentFixture<PhotoGalleryModal>): Promise<void> {
  for (let i = 0; i < 5; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(
  client: CoreClient,
  inputs: Record<string, unknown> = { chatId: 'chat-1' },
): Promise<ComponentFixture<PhotoGalleryModal>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [PhotoGalleryModal],
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: client },
    ],
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
  return Array.from(document.querySelectorAll('.qt-dialog img'));
}

/**
 * The gallery grid — a full rewrite of v4 `components/images/
 * PhotoGalleryModal.tsx` at `78b381a96` (P4.D176) over the `chatGallery` query
 * (§C.3). Chat-mode cases replace the old `chatFilesList`-era suite; the
 * character/user-character cases (v4's `photo-gallery-modal-deleted-handling`
 * jest suite, Tier 2) and the routing/z-layering/arrow invariants (no v4 test
 * exists) carry over unchanged — that half of the component did not move.
 *
 * The component portals to `document.body` (bug 99) — every DOM query below
 * reads from `document`, not the fixture, to match.
 */
describe('PhotoGalleryModal', () => {
  afterEach(() => {
    document.body.style.overflow = '';
    TestBed.resetTestingModule();
  });

  describe('chat mode — the chatGallery-backed grid', () => {
    it('renders chat photos from chatGallery and swaps a failing image for the placeholder', async () => {
      const fixture = await render(
        stubClient({ entries: [entry({ id: 'f-1' }), entry({ id: 'f-2', filename: 'missing.png' })] }),
      );
      expect(document.body.textContent).toContain('Chat Photos');
      const imgs = gridImgs(fixture);
      expect(imgs.length).toBe(2);

      imgs[1].dispatchEvent(new Event('error'));
      await flush(fixture);

      const placeholder = document.querySelector('qt-deleted-image-placeholder');
      expect(placeholder).toBeTruthy();
      expect(placeholder!.textContent).toContain('Image Deleted');
    });

    it('a div container for missing tiles, a button for valid ones', async () => {
      const fixture = await render(
        stubClient({ entries: [entry({ id: 'f-1' }), entry({ id: 'f-2' })] }),
      );
      gridImgs(fixture)[1].dispatchEvent(new Event('error'));
      await flush(fixture);

      const placeholder = document.querySelector('qt-deleted-image-placeholder');
      expect(placeholder!.closest('div.relative.rounded')?.tagName).toBe('DIV');
      const validTile = gridImgs(fixture)[0].closest('button');
      expect(validTile).toBeTruthy();
    });

    it('shows the `current` badge only on the entry flagged isCurrent', async () => {
      const fixture = await render(
        stubClient({
          entries: [entry({ id: 'f-1', isCurrent: true }), entry({ id: 'f-2', isCurrent: false })],
        }),
      );
      const badges = document.querySelectorAll('.qt-bg-success');
      expect(badges.length).toBe(1);
      expect(badges[0].textContent?.trim()).toBe('current');
    });

    it('the empty state reads v4’s sentence for the whole gallery', async () => {
      const fixture = await render(stubClient({ entries: [] }));
      expect(document.body.textContent).toContain('No photos in this chat');
    });

    describe('the filter chips (v4 :291-324 — "one kind of picture is not a filter")', () => {
      it('renders NO chips with a single non-zero source', async () => {
        const fixture = await render(
          stubClient({ entries: [entry({ source: 'attachment' }), entry({ source: 'attachment' })] }),
        );
        expect(document.querySelector('[role="group"][aria-label="Filter by where the picture came from"]')).toBeNull();
      });

      it('renders chips (in v4’s source order) once >= 2 sources are non-zero, and filters on click', async () => {
        const fixture = await render(
          stubClient({
            entries: [
              entry({ id: 'a', source: 'story-background' }),
              entry({ id: 'b', source: 'attachment' }),
              entry({ id: 'c', source: 'attachment' }),
            ],
          }),
        );
        const group = document.querySelector(
          '[role="group"][aria-label="Filter by where the picture came from"]',
        )!;
        const chipLabels = Array.from(group.querySelectorAll('button')).map((b) => b.textContent!.trim());
        // Chip order = SOURCE_ORDER: story-background before attachment.
        expect(chipLabels).toEqual(['All (3)', 'Backgrounds (1)', 'Attached (2)']);

        (Array.from(group.querySelectorAll('button')).find((b) =>
          b.textContent!.includes('Attached'),
        ) as HTMLButtonElement).click();
        await flush(fixture);
        expect(gridImgs(fixture).length).toBe(2);

        // The unfiltered empty state names the active filter's label.
        (group.querySelectorAll('button')[0] as HTMLButtonElement).click();
        await flush(fixture);
        expect(gridImgs(fixture).length).toBe(3);
      });

      it('the filtered empty state names the active chip’s label, lowercased', async () => {
        // Two sources so the chips render at all; filter to one, then delete
        // its only entry (the mocked dispatch resolves and the gallery
        // invalidates+refetches empty) to reach that filter's OWN empty text.
        const seen: { type: string }[] = [];
        let entries = [
          entry({ id: 'a', source: 'story-background' }),
          entry({ id: 'b', source: 'attachment' }),
        ];
        const client: CoreClient = {
          ...coreStreamStub(),
          dispatchData: (async (req: { type: string }) => {
            if (req.type === 'chatGallery') return galleryResult(entries);
            return {};
          }) as CoreClient['dispatchData'],
          dispatch: (async (req: { type: string; fileId?: string }) => {
            seen.push(req);
            if (req.type === 'chatFileDelete') {
              entries = entries.filter((e) => e.id !== req.fileId);
              return { type: 'ack', data: {} };
            }
            return { type: 'ack', data: {} };
          }) as CoreClient['dispatch'],
        } as unknown as CoreClient;

        vi.spyOn(window, 'confirm').mockReturnValue(true);
        const fixture = await render(client);
        const group = document.querySelector('[role="group"]')!;
        (Array.from(group.querySelectorAll('button')).find((b) =>
          b.textContent!.includes('Backgrounds'),
        ) as HTMLButtonElement).click();
        await flush(fixture);
        expect(gridImgs(fixture).length).toBe(1);

        (document.querySelector('[title="Delete image"]') as HTMLButtonElement).click();
        await flush(fixture);

        expect(document.body.textContent).toContain('No backgrounds in this chat');
        vi.restoreAllMocks();
      });
    });

    describe('the hover actions and the Delete double-guard', () => {
      it('shows Save and Download always; Delete only when entry.deletable', async () => {
        const fixture = await render(
          stubClient({ entries: [entry({ id: 'f-1', deletable: true }), entry({ id: 'f-2', deletable: false })] }),
        );
        const tiles = document.querySelectorAll('.qt-dialog .group');
        expect(tiles[0].querySelector('[title="Save to a photo album"]')).toBeTruthy();
        expect(tiles[0].querySelector('[title="Download image"]')).toBeTruthy();
        expect(tiles[0].querySelector('[title="Delete image"]')).toBeTruthy();
        expect(tiles[1].querySelector('[title="Delete image"]')).toBeNull();
      });

      it('opens the shared SaveImageDialog with a chat-kind target on Save', async () => {
        const fixture = await render(stubClient({ entries: [entry({ id: 'f-1' })] }));
        (document.querySelector('[title="Save to a photo album"]') as HTMLButtonElement).click();
        await flush(fixture);
        expect(document.querySelector('qt-save-image-dialog')).toBeTruthy();
      });

      // v4 `PhotoGalleryModal.tsx:566-568` — `onSaved` flashes `Saved to ${mountPoint}`
      // and invalidates (the message door already had it; the gallery door's was
      // silent — the §3 unification review of the `78b381a96` round).
      it("flashes `Saved to <mount>` when the gallery door's dialog reports a save", async () => {
        const fixture = await render(stubClient({ entries: [entry({ id: 'f-1' })] }));
        (document.querySelector('[title="Save to a photo album"]') as HTMLButtonElement).click();
        await flush(fixture);
        const comp = fixture.componentInstance as unknown as {
          handleSaved: (info: { mountPoint: string; relativePath: string }) => void;
        };
        comp.handleSaved({ mountPoint: 'Quilltap General', relativePath: 'photos/x.webp' });
        await flush(fixture);
        const toasts = TestBed.inject(ToastService).toasts().map((t) => t.message);
        expect(toasts).toContain('Saved to Quilltap General');
        expect(document.querySelector('qt-save-image-dialog')).toBeNull();
      });

      it("delete double-guard: a non-deletable OR link-kind entry sends NO request even if forced", async () => {
        // Confirm answers YES so the guard, not the dialog, is what stops the
        // request (jsdom's unstubbed `confirm` is falsy and would make the
        // `idKind` half of the mutation vacuous — the §3 unification review).
        vi.spyOn(window, 'confirm').mockReturnValue(true);
        const seen: { type: string }[] = [];
        const fixture = await render(
          stubClient({
            entries: [entry({ id: 'f-1', deletable: true, idKind: 'link' })],
            onDispatch: (r) => seen.push(r),
          }),
        );
        // Delete is not even rendered for a link-kind entry that reads
        // `deletable: true` from a malformed server — the guard is BOTH halves.
        // Exercise the guard directly via the component method.
        const comp = fixture.componentInstance as unknown as {
          handleDeleteEntry: (e: ChatGalleryEntry) => Promise<void>;
        };
        await comp.handleDeleteEntry(entry({ id: 'f-1', deletable: true, idKind: 'link' }));
        expect(seen.filter((r) => r.type === 'chatFileDelete')).toEqual([]);
      });

      it('deletes a deletable file-kind entry after confirmation, then invalidates the gallery', async () => {
        vi.spyOn(window, 'confirm').mockReturnValue(true);
        const seen: { type: string; fileId?: string }[] = [];
        const fixture = await render(
          stubClient({ entries: [entry({ id: 'f-1', deletable: true, idKind: 'file' })], onDispatch: (r) => seen.push(r as never) }),
        );
        (document.querySelector('[title="Delete image"]') as HTMLButtonElement).click();
        await flush(fixture);
        expect(seen.some((r) => r.type === 'chatFileDelete' && r.fileId === 'f-1')).toBe(true);
        vi.restoreAllMocks();
      });

      it('downloads via an anchor carrying ?download=1', async () => {
        const fixture = await render(stubClient({ entries: [entry({ id: 'f-1', url: '/api/v1/files/f-1' })] }));
        const clicked: Array<{ href: string; download: string }> = [];
        const spy = vi
          .spyOn(HTMLAnchorElement.prototype, 'click')
          .mockImplementation(function (this: HTMLAnchorElement) {
            clicked.push({ href: this.href, download: this.download });
          });
        (document.querySelector('[title="Download image"]') as HTMLButtonElement).click();
        expect(spy).toHaveBeenCalledOnce();
        expect(clicked[0].href).toContain('/api/v1/files/f-1?download=1');
        vi.restoreAllMocks();
      });
    });
  });

  describe('character modes — deleted image handling (v4’s photo-gallery-modal-deleted-handling suite)', () => {
    it('handles deleted images in character mode', async () => {
      const fixture = await render(
        stubClient({ characterEntries: [photoEntry({ linkId: 'img-1' })] }),
        { mode: 'character', characterId: 'char-1', characterName: 'Test Character' },
      );
      expect(document.body.textContent).toContain("Test Character's Photos");
      const imgs = gridImgs(fixture);
      expect(imgs.length).toBe(1);

      imgs[0].dispatchEvent(new Event('error'));
      await flush(fixture);
      expect(document.querySelector('qt-deleted-image-placeholder')).toBeTruthy();
    });

    it('handles deleted images in user-character mode', async () => {
      const fixture = await render(
        stubClient({ characterEntries: [photoEntry({ linkId: 'img-2', fileName: 'persona-img.png' })] }),
        {
          mode: 'user-character',
          userCharacterId: 'user-char-1',
          userCharacterName: 'Test User Character',
        },
      );
      expect(document.body.textContent).toContain("Test User Character's Photos");
      gridImgs(fixture)[0].dispatchEvent(new Event('error'));
      await flush(fixture);
      expect(document.querySelector('qt-deleted-image-placeholder')).toBeTruthy();
    });
  });

  describe('modal behavior', () => {
    it('emits close when the close button is clicked', async () => {
      const fixture = await render(stubClient({ entries: [entry({})] }));
      const closes: number[] = [];
      fixture.componentInstance.close.subscribe(() => closes.push(1));
      (document.querySelector('button[title="Close"]') as HTMLButtonElement).click();
      expect(closes.length).toBe(1);
    });

    it('locks body overflow while mounted', async () => {
      await render(stubClient({ entries: [] }));
      expect(document.body.style.overflow).toBe('hidden');
    });

    it('supports zoom in/out', async () => {
      const fixture = await render(stubClient({ entries: [entry({})] }));
      const zoomIn = document.querySelector('button[title="Larger thumbnails"]') as HTMLButtonElement;
      const zoomOut = document.querySelector('button[title="Smaller thumbnails"]') as HTMLButtonElement;
      expect(zoomIn.disabled).toBe(false);
      expect(zoomOut.disabled).toBe(false);
    });

    it('portals to document.body (bug 99)', async () => {
      const fixture = await render(stubClient({ entries: [entry({})] }));
      const dialog = document.querySelector('.qt-dialog');
      expect(dialog).toBeTruthy();
      // Walk up to the reparented host, which must be a direct child of body.
      let node: Element | null = dialog;
      while (node && node.parentElement !== document.body) node = node.parentElement;
      expect(node?.parentElement).toBe(document.body);
    });
  });

  describe('the routing split and the two pinned invariants (no v4 test exists)', () => {
    it('routes a chat item to qt-chat-gallery-image-view-modal', async () => {
      const fixture = await render(stubClient({ entries: [entry({ id: 'file-1' })] }));
      (gridImgs(fixture)[0].closest('button') as HTMLButtonElement).click();
      await flush(fixture);
      expect(document.querySelector('qt-chat-gallery-image-view-modal')).toBeTruthy();
      expect(document.querySelector('qt-image-detail-modal')).toBeNull();
    });

    it('routes a character-mode item to qt-image-detail-modal', async () => {
      const fixture = await render(
        stubClient({ characterEntries: [photoEntry({ linkId: 'img-1' })] }),
        { mode: 'character', characterId: 'char-1', characterName: 'C' },
      );
      (gridImgs(fixture)[0].closest('button') as HTMLButtonElement).click();
      await flush(fixture);
      const detail = document.querySelector('qt-image-detail-modal');
      expect(detail).toBeTruthy();
      expect(document.querySelector('qt-chat-gallery-image-view-modal')).toBeNull();
    });

    it('suppresses its own Escape handler while a detail modal is open', async () => {
      const fixture = await render(
        stubClient({ entries: [entry({ id: 'file-1' }), entry({ id: 'file-2' })] }),
      );
      const closes: number[] = [];
      fixture.componentInstance.close.subscribe(() => closes.push(1));

      (gridImgs(fixture)[0].closest('button') as HTMLButtonElement).click();
      await flush(fixture);
      expect(document.querySelector('qt-chat-gallery-image-view-modal')).toBeTruthy();

      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      await flush(fixture);
      expect(document.querySelector('qt-chat-gallery-image-view-modal')).toBeNull();
      expect(closes.length).toBe(0);

      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      await flush(fixture);
      expect(closes.length).toBe(1);
    });

    it('layers the gallery at z-50 and the detail modal at z-[60]', async () => {
      const fixture = await render(stubClient({ entries: [entry({ id: 'file-1' })] }));
      expect(document.querySelector('.fixed.inset-0.z-50')).toBeTruthy();
      (gridImgs(fixture)[0].closest('button') as HTMLButtonElement).click();
      await flush(fixture);
      const detail = document.querySelector('qt-chat-gallery-image-view-modal');
      expect(detail!.querySelector('.fixed.inset-0.z-\\[60\\]')).toBeTruthy();
    });

    it('hands the arrows down as undefined at the ends of the list', async () => {
      const fixture = await render(
        stubClient({
          entries: [entry({ id: 'file-1' }), entry({ id: 'file-2' }), entry({ id: 'file-3' })],
        }),
      );
      (gridImgs(fixture)[0].closest('button') as HTMLButtonElement).click();
      await flush(fixture);
      const detail = () => document.querySelector('qt-chat-gallery-image-view-modal')!;
      expect(detail().querySelector('button[title="Previous image (Left Arrow)"]')).toBeNull();
      expect(detail().querySelector('button[title="Next image (Right Arrow)"]')).toBeTruthy();

      (detail().querySelector('button[title="Next image (Right Arrow)"]') as HTMLButtonElement).click();
      await flush(fixture);
      (detail().querySelector('button[title="Next image (Right Arrow)"]') as HTMLButtonElement).click();
      await flush(fixture);
      expect(detail().querySelector('button[title="Previous image (Left Arrow)"]')).toBeTruthy();
      expect(detail().querySelector('button[title="Next image (Right Arrow)"]')).toBeNull();
    });
  });

  describe('Jump to message (v4 ChatModals.tsx:168-172 — the three-hop choreography)', () => {
    it('closes BOTH modals and emits jumpToMessage up to the host', async () => {
      const fixture = await render(
        stubClient({ entries: [entry({ id: 'file-1', messageId: 'm-9' })] }),
      );
      const closes: number[] = [];
      const jumps: string[] = [];
      fixture.componentInstance.close.subscribe(() => closes.push(1));
      fixture.componentInstance.jumpToMessage.subscribe((id) => jumps.push(id));

      (gridImgs(fixture)[0].closest('button') as HTMLButtonElement).click();
      await flush(fixture);
      (document.querySelector('button.qt-link.underline') as HTMLButtonElement).click();
      await flush(fixture);

      expect(document.querySelector('qt-chat-gallery-image-view-modal')).toBeNull();
      expect(closes.length).toBe(1);
      expect(jumps).toEqual(['m-9']);
    });
  });
});
