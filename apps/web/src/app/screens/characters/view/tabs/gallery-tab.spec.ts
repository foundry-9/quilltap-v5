import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import type { CharacterPhoto } from '../../../../core/core-contract';
import { CharacterGalleryTab } from './gallery-tab';

function stubClient(
  photos: CharacterPhoto[],
  onDispatch?: (req: { type: string; [k: string]: unknown }) => void,
): Partial<CoreClient> {
  return {
    dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
      onDispatch?.(req);
      if (req.type === 'characterPhotoList') return { photos };
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

  it('renders a thumbnail per photo', async () => {
    const fixture = await render(
      stubClient([{ id: 'p1', linkId: 'link-1', filepath: '/uploads/p1.png', url: null }]),
    );
    const imgs = fixture.nativeElement.querySelectorAll('img');
    expect(imgs.length).toBe(1);
    expect((imgs[0] as HTMLImageElement).src).toContain('/uploads/p1.png');
  });

  it('dispatches characterPhotoRemove with the linkId when a photo is deleted', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(
      stubClient([{ id: 'p1', linkId: 'link-1', filepath: '/uploads/p1.png', url: null }], (r) => seen.push(r)),
    );
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
