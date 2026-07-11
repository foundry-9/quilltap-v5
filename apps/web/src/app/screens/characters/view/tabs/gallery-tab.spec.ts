import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

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
    const fixture = await render(stubClient([entry({ linkId: 'link-9', caption: 'By the sea', tags: ['beach'] })]));
    const img = fixture.nativeElement.querySelector('img') as HTMLImageElement;
    expect(img.alt).toBe('By the sea');
    expect(fixture.nativeElement.textContent).toContain('By the sea');
  });

  it('dispatches characterPhotoRemove with the linkId when a photo is deleted', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient([entry({ linkId: 'link-1' })], (r) => seen.push(r)));
    const deleteButton = fixture.nativeElement.querySelector('button[title="Delete this photo"]') as HTMLButtonElement;
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
});
