import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import type { CharacterPhoto } from '../../../../core/core-contract';
import { CharacterGalleryTab } from './gallery-tab';

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

function stubClient(
  photos: CharacterPhoto[],
  onDispatch?: (req: { type: string; [k: string]: unknown }) => void,
): Partial<CoreClient> {
  return {
    dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
      onDispatch?.(req);
      if (req.type === 'characterPhotoList') {
        return { entries: photos, total: photos.length, hasMore: false };
      }
      return {};
    }) as CoreClient['dispatchData'],
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
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

describe('CharacterGalleryTab', () => {
  it('shows the empty state when there are no photos', async () => {
    const fixture = await render(stubClient([]));
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('No photos yet');
  });

  it('renders a thumbnail per entry from its blobUrl', async () => {
    const fixture = await render(stubClient([entry({ linkId: 'link-1' })]));
    const imgs = fixture.nativeElement.querySelectorAll('img');
    expect(imgs.length).toBe(1);
    expect((imgs[0] as HTMLImageElement).src).toContain('/api/v1/mount-points/mp-1/blobs/');
  });

  it('renders the caption as alt text and an overlay', async () => {
    const fixture = await render(
      stubClient([entry({ linkId: 'link-9', caption: 'By the sea', tags: ['beach'] })]),
    );
    const img = fixture.nativeElement.querySelector('img') as HTMLImageElement;
    expect(img.alt).toBe('By the sea');
    expect(fixture.nativeElement.textContent).toContain('By the sea');
  });

  it('dispatches characterPhotoRemove with the linkId when a photo is deleted', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient([entry({ linkId: 'link-1' })], (r) => seen.push(r)));
    const deleteButton = fixture.nativeElement.querySelector(
      'button[title="Delete this photo"]',
    ) as HTMLButtonElement;
    expect(deleteButton).toBeTruthy();
    deleteButton.click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    expect(seen.find((r) => r.type === 'characterPhotoRemove')).toEqual({
      type: 'characterPhotoRemove',
      characterId: 'c1',
      linkId: 'link-1',
    });
  });

  // --- Upload Photo rider (multipart web route, B↔C contract) ---

  afterEach(() => {
    vi.unstubAllGlobals();
  });

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
    const fixture = await render(stubClient([]));
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
    const fixture = await render(stubClient([]));
    pickFile(fixture);
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(fixture.nativeElement.querySelector('.qt-alert-error')).toBeTruthy();
    expect(fixture.nativeElement.textContent).toContain('Unsupported MIME type');
  });
});
