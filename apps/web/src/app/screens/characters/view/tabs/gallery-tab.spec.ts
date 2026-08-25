import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import type { CharacterPhoto } from '../../../../core/core-contract';
import { CharacterGalleryTab } from './gallery-tab';
import { ToastService } from '../../../../ui/toast.service';

/** A gallery entry in the pinned P4.6i shape (`listCharacterGallery`). */
function entry(overrides: Partial<CharacterPhoto>): CharacterPhoto {
  return {
    linkId: 'link-1',
    mountPointId: 'mp-1',
    relativePath: 'photos/portrait.webp',
    fileName: 'portrait.webp',
    blobUrl: '/api/v1/mount-points/mp-1/blobs/photos%2Fportrait.webp',
    mimeType: 'image/webp',
    sha256: 'abc123',
    fileSizeBytes: 1024,
    keptAt: '2026-04-01T12:00:00.000Z',
    caption: null,
    tags: [],
    ...overrides,
  };
}

interface SeenRequest {
  type: string;
  [k: string]: unknown;
}

function stubClient(options: {
  photos: CharacterPhoto[];
  defaultImageId?: string | null;
  onRequest?: (req: SeenRequest) => void;
}): Partial<CoreClient> {
  return {
    dispatchData: (async (req: SeenRequest) => {
      options.onRequest?.(req);
      if (req.type === 'characterPhotoList') {
        return { entries: options.photos, total: options.photos.length, hasMore: false };
      }
      if (req.type === 'characterGet') {
        return {
          character: { id: 'c1', name: 'Aria', defaultImageId: options.defaultImageId ?? null },
        };
      }
      if (req.type === 'characterList') return { characters: [] };
      if (req.type === 'imageInfoGet') return { data: { characterGalleryLinks: [] } };
      return {};
    }) as CoreClient['dispatchData'],
    dispatch: (async (req: SeenRequest) => {
      options.onRequest?.(req);
      return { type: 'ack', data: {} };
    }) as CoreClient['dispatch'],
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<CharacterGalleryTab>> {
  TestBed.configureTestingModule({
    imports: [CharacterGalleryTab],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(CharacterGalleryTab);
  fixture.componentRef.setInput('characterId', 'c1');
  fixture.detectChanges();
  for (let i = 0; i < 5; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

async function flush(fixture: ComponentFixture<CharacterGalleryTab>): Promise<void> {
  for (let i = 0; i < 5; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

/**
 * The Photo Gallery tab at `EmbeddedPhotoGallery` parity. The v4 embedded
 * gallery family has no test of its own — the cases pin transcribed behaviors
 * by `file:line` (`EmbeddedPhotoGallery.tsx`, `GalleryImage.tsx`,
 * `GalleryControls.tsx`, `GalleryEmpty.tsx`, `useGalleryData.ts`).
 */
/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('CharacterGalleryTab (EmbeddedPhotoGallery parity)', () => {
  afterEach(() => {
    document.body.style.overflow = '';
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    TestBed.resetTestingModule();
  });

  // GalleryEmpty.tsx:11-20 — the empty card with the entity name.
  it('shows the v4 empty state with the character name', async () => {
    const fixture = await render(stubClient({ photos: [] }));
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('No photos in Aria’s album yet');
    expect(text).toContain('keep_image');
  });

  // GalleryControls.tsx:31-33 — the count line.
  it('renders the photo count line', async () => {
    const fixture = await render(
      stubClient({ photos: [entry({ linkId: 'a' }), entry({ linkId: 'b' })] }),
    );
    expect(fixture.nativeElement.textContent).toContain('2 photos');
  });

  it('renders a thumbnail per entry from its blobUrl', async () => {
    const fixture = await render(stubClient({ photos: [entry({ linkId: 'link-1' })] }));
    const imgs = fixture.nativeElement.querySelectorAll('img');
    expect(imgs.length).toBe(1);
    expect((imgs[0] as HTMLImageElement).src).toContain('/api/v1/mount-points/mp-1/blobs/');
  });

  // GalleryImage.tsx:32-34/:53-58 — the avatar tile carries the success ring
  // and badge; GalleryControls.tsx:49-58 — Clear Avatar appears only then.
  it('marks the current avatar tile and offers Clear Avatar', async () => {
    const fixture = await render(
      stubClient({ photos: [entry({ linkId: 'link-1' })], defaultImageId: 'link-1' }),
    );
    expect(fixture.nativeElement.textContent).toContain('Avatar');
    expect(fixture.nativeElement.querySelector('.ring-success')).toBeTruthy();
    const clear = Array.from(fixture.nativeElement.querySelectorAll('button')).find((b) =>
      ((b as HTMLElement).textContent ?? '').includes('Clear Avatar'),
    );
    expect(clear).toBeTruthy();
  });

  // useGalleryData.ts:88-113 — set avatar dispatches `characterAvatar` with
  // the link id (vault photos ARE the avatar ids).
  it('sets the avatar from the tile overlay', async () => {
    const seen: SeenRequest[] = [];
    const fixture = await render(
      stubClient({ photos: [entry({ linkId: 'link-1' })], onRequest: (r) => seen.push(r) }),
    );
    (
      fixture.nativeElement.querySelector('button[title="Set as avatar"]') as HTMLButtonElement
    ).click();
    await flush(fixture);
    expect(seen.find((r) => r.type === 'characterAvatar')).toEqual({
      type: 'characterAvatar',
      characterId: 'c1',
      imageId: 'link-1',
    });
  });

  // useGalleryData.ts:115-140 — clear avatar sends `imageId: null`.
  it('clears the avatar with imageId null', async () => {
    const seen: SeenRequest[] = [];
    const fixture = await render(
      stubClient({
        photos: [entry({ linkId: 'link-1' })],
        defaultImageId: 'link-1',
        onRequest: (r) => seen.push(r),
      }),
    );
    const clear = Array.from(fixture.nativeElement.querySelectorAll('button')).find((b) =>
      ((b as HTMLElement).textContent ?? '').includes('Clear Avatar'),
    ) as HTMLButtonElement;
    clear.click();
    await flush(fixture);
    expect(seen.find((r) => r.type === 'characterAvatar')).toEqual({
      type: 'characterAvatar',
      characterId: 'c1',
      imageId: null,
    });
  });

  // EmbeddedPhotoGallery.tsx:84-98 — delete is a CONFIRM-DOUBLE-CLICK: the
  // first click arms ("Click again to confirm delete"), the second deletes.
  it('deletes a photo on the confirming second click', async () => {
    const seen: SeenRequest[] = [];
    const fixture = await render(
      stubClient({ photos: [entry({ linkId: 'link-1' })], onRequest: (r) => seen.push(r) }),
    );
    const deleteButton = fixture.nativeElement.querySelector(
      'button[title="Delete image"]',
    ) as HTMLButtonElement;
    deleteButton.click();
    fixture.detectChanges();
    expect(seen.find((r) => r.type === 'characterPhotoRemove')).toBeUndefined();
    expect(
      fixture.nativeElement.querySelector('button[title="Click again to confirm delete"]'),
    ).toBeTruthy();

    (
      fixture.nativeElement.querySelector(
        'button[title="Click again to confirm delete"]',
      ) as HTMLButtonElement
    ).click();
    await flush(fixture);
    expect(seen.find((r) => r.type === 'characterPhotoRemove')).toEqual({
      type: 'characterPhotoRemove',
      characterId: 'c1',
      linkId: 'link-1',
    });
  });

  // EmbeddedPhotoGallery.tsx:170-196 — a tile click opens the deep detail
  // modal with `linkId: selectedImage.id` (the LINK-RELINK branch) and this
  // host alone passes onAvatarSet.
  it('opens the deep detail modal with the link-relink identity', async () => {
    const fixture = await render(stubClient({ photos: [entry({ linkId: 'link-7' })] }));
    (
      fixture.nativeElement.querySelector('button.aspect-square') as HTMLButtonElement
    ).click();
    await flush(fixture);
    // The modal PORTALS to document.body (bug 99), so it is NOT in this
    // component's subtree — querying the fixture would silently pass forever.
    const modal = document.querySelector('qt-image-detail-modal');
    expect(modal).toBeTruthy();
    expect(modal!.parentElement).toBe(document.body);
    expect(fixture.nativeElement.querySelector('qt-image-detail-modal')).toBeNull();
  });

  // GalleryImage.tsx:36-39 — a failed thumbnail collapses to the icon tile.
  it('shows the missing-image icon when a thumbnail errors', async () => {
    const fixture = await render(stubClient({ photos: [entry({ linkId: 'link-1' })] }));
    (fixture.nativeElement.querySelector('img') as HTMLImageElement).dispatchEvent(
      new Event('error'),
    );
    await flush(fixture);
    expect(fixture.nativeElement.querySelector('img')).toBeNull();
  });

  // --- Upload Photo rider (multipart web route, B↔C contract) ---

  function pickFile(fixture: ComponentFixture<CharacterGalleryTab>): void {
    const input = fixture.nativeElement.querySelector(
      'input[aria-label="Upload photo"]',
    ) as HTMLInputElement;
    const file = new File([new Uint8Array([1, 2, 3])], 'portrait.png', { type: 'image/png' });
    Object.defineProperty(input, 'files', { value: [file], configurable: true });
    input.dispatchEvent(new Event('change'));
  }

  it('uploads a picked file via the multipart photos web route', async () => {
    const calls: Array<{ url: string; method?: string; body: unknown }> = [];
    vi.stubGlobal(
      'fetch',
      vi.fn(async (url: string, init?: RequestInit) => {
        calls.push({ url, method: init?.method, body: init?.body });
        return {
          ok: true,
          status: 201,
          json: async () => ({
            linkId: 'new-link',
            mountPointId: 'mp-1',
            relativePath: 'photos/new.png',
            keptAt: '2026-04-02T00:00:00.000Z',
            sha256: 'deadbeef',
          }),
        } as Response;
      }),
    );
    const fixture = await render(stubClient({ photos: [] }));
    pickFile(fixture);
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    expect(calls.length).toBe(1);
    expect(calls[0].url).toBe('/api/v1/characters/c1/photos');
    expect(calls[0].method).toBe('POST');
    expect(calls[0].body).toBeInstanceOf(FormData);
  });

  it('surfaces the v4 error message when the upload fails', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          ({
            ok: false,
            status: 400,
            json: async () => ({ error: 'Unsupported MIME type' }),
          }) as Response,
      ),
    );
    const fixture = await render(stubClient({ photos: [] }));
    pickFile(fixture);
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(toasts().at(-1)?.type).toBe('error');
    expect(toasts().at(-1)?.message).toContain('Unsupported MIME type');
  });

  // ==========================================================================
  // The hover Download button (v4 bug 99 — `8018c487`).
  // GalleryImage.tsx:84-95 for the control, useGalleryData.ts:143-160 for the
  // hook, EmbeddedPhotoGallery.tsx:85-88 for the stopPropagation wrapper.
  // ==========================================================================

  /** The `<a download>` the browser arm of `triggerBlobDownload` clicks. */
  function captureAnchorClicks(): { href: string; download: string }[] {
    const seen: { href: string; download: string }[] = [];
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (
      this: HTMLAnchorElement,
    ) {
      seen.push({ href: this.href, download: this.download });
    });
    vi.stubGlobal('URL', {
      ...URL,
      createObjectURL: () => 'blob:stub-object-url',
      revokeObjectURL: () => undefined,
    });
    return seen;
  }

  it('offers Download on every tile that has an image', async () => {
    const fixture = await render(
      stubClient({ photos: [entry({ linkId: 'link-1' }), entry({ linkId: 'link-2' })] }),
    );
    const buttons = fixture.nativeElement.querySelectorAll('button[title="Download image"]');
    expect(buttons.length).toBe(2);
    // v4 gives it both a title and an aria-label (`GalleryImage.tsx:92-93`).
    expect((buttons[0] as HTMLElement).getAttribute('aria-label')).toBe('Download image');
  });

  // GalleryImage.tsx:85 — `!isMissingImage`, so a tile whose bytes 404'd
  // offers no download at all.
  it('hides Download on a tile whose image failed to load', async () => {
    const fixture = await render(stubClient({ photos: [entry({ linkId: 'link-1' })] }));
    fixture.nativeElement.querySelector('img')!.dispatchEvent(new Event('error'));
    await flush(fixture);
    expect(fixture.nativeElement.querySelectorAll('button[title="Download image"]').length).toBe(0);
  });

  it('downloads the blob under the entry filename, and does NOT open the detail view', async () => {
    const anchors = captureAnchorClicks();
    const blob = new Blob(['x'], { type: 'image/webp' });
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => ({ ok: true, status: 200, blob: async () => blob }) as unknown as Response),
    );
    const fixture = await render(
      stubClient({ photos: [entry({ linkId: 'link-1', fileName: 'zeppelin.webp' })] }),
    );
    (
      fixture.nativeElement.querySelector('button[title="Download image"]') as HTMLElement
    ).click();
    await flush(fixture);

    // The src is v4's expression (`useGalleryData.ts:154`): `image.url` first,
    // which for this host is the blobUrl through `apiUrl`.
    expect((globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0][0]).toContain(
      '/api/v1/mount-points/mp-1/blobs/',
    );
    // The filename is the entry's own `fileName` — NOT the blob path's basename.
    expect(anchors).toEqual([{ href: 'blob:stub-object-url', download: 'zeppelin.webp' }]);
    // EmbeddedPhotoGallery.tsx:86 carries `stopPropagation` and so does this
    // port, though in BOTH the action overlay is a sibling of the tile button
    // rather than a child, so nothing bubbles into it either way. The pin that
    // matters is the observable one: downloading opens no detail view.
    // Queried on the DOCUMENT, not the fixture — the modal portals to the body,
    // so a fixture-scoped query could never see it and would never fail.
    expect(document.querySelector('qt-image-detail-modal')).toBeNull();
    expect(toasts()).toEqual([]);
  });

  // useGalleryData.ts:156-159 — the toast sentence is a FIXED string; the
  // thrown message (`Failed to fetch image (404)`) goes only to the console.
  it('reports a failed fetch with v4 fixed sentence, and logs the real one', async () => {
    captureAnchorClicks();
    const logged: unknown[][] = [];
    vi.spyOn(console, 'error').mockImplementation((...args: unknown[]) => {
      logged.push(args);
    });
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => ({ ok: false, status: 404 }) as unknown as Response),
    );
    const fixture = await render(stubClient({ photos: [entry({ linkId: 'link-1' })] }));
    (
      fixture.nativeElement.querySelector('button[title="Download image"]') as HTMLElement
    ).click();
    await flush(fixture);

    expect(toasts().at(-1)).toEqual({ type: 'error', message: 'Failed to download image' });
    expect(logged.at(-1)?.[0]).toBe('Error downloading image:');
    expect(logged.at(-1)?.[1]).toEqual({
      error: 'Failed to fetch image (404)',
      entityId: 'c1',
      imageId: 'link-1',
    });
  });
});
