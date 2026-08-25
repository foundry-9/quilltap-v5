import { ChangeDetectionStrategy, Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../core/core-client';
import { ImageDetailModal } from './image-detail-modal';
import type { ImageData } from './images.api';
import { ToastService } from '../ui/toast.service';

function imageData(over: Partial<ImageData>): ImageData {
  return {
    id: 'img-1',
    filename: 'shot.png',
    filepath: '/api/v1/files/img-1',
    mimeType: 'image/png',
    size: 1024,
    createdAt: '2026-01-01T00:00:00.000Z',
    ...over,
  };
}

interface SeenRequest {
  type: string;
  [k: string]: unknown;
}

function stubClient(options: {
  characters?: Array<{ id: string; name: string; defaultImageId?: string | null }>;
  links?: Array<{ characterId: string; characterName: string; linkId: string }>;
  onRequest?: (req: SeenRequest) => void;
  failDispatch?: string;
}): Partial<CoreClient> {
  const answer = (req: SeenRequest): Record<string, unknown> => {
    if (req.type === 'characterList') return { characters: options.characters ?? [] };
    if (req.type === 'imageInfoGet') {
      return { data: { characterGalleryLinks: options.links ?? [] } };
    }
    if (req.type === 'characterPhotoSaveById') return { linkId: 'minted-link' };
    return {};
  };
  return {
    dispatchData: (async (req: SeenRequest) => {
      options.onRequest?.(req);
      return answer(req);
    }) as CoreClient['dispatchData'],
    dispatch: (async (req: SeenRequest) => {
      options.onRequest?.(req);
      if (options.failDispatch === req.type) {
        return { type: 'error', data: { kind: 'internal', message: 'nope' } };
      }
      return { type: 'ack', data: answer(req) };
    }) as CoreClient['dispatch'],
  };
}

async function render(
  client: Partial<CoreClient>,
  image: ImageData,
  inputs: Record<string, unknown> = {},
): Promise<ComponentFixture<ImageDetailModal>> {
  TestBed.configureTestingModule({
    imports: [ImageDetailModal],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(ImageDetailModal);
  fixture.componentRef.setInput('image', image);
  for (const [key, value] of Object.entries(inputs)) {
    fixture.componentRef.setInput(key, value);
  }
  fixture.detectChanges();
  for (let i = 0; i < 5; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

/** A pane that mounts the modal under an `@if`, as every real host does. */
@Component({
  selector: 'qt-pane-host',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ImageDetailModal],
  template: `
    <div class="pane">
      @if (open()) {
        <qt-image-detail-modal [image]="image" />
      }
    </div>
  `,
})
class PaneHost {
  readonly open = signal(true);
  readonly image = imageData({});
}

async function flush(fixture: ComponentFixture<ImageDetailModal>): Promise<void> {
  for (let i = 0; i < 5; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

/**
 * The deep detail modal. v4 has NO test for `ImageDetailModal.tsx` /
 * `useImageActions.ts` — every case pins a transcribed behavior by
 * `file:line` (the only fidelity record these components have).
 */
/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('ImageDetailModal', () => {
  afterEach(() => {
    document.body.style.overflow = '';
    TestBed.resetTestingModule();
  });

  // ImageDetailModal.tsx:43-46 — the PARALLEL characterList + imageInfoGet
  // load on mount.
  it('loads the roster and the image info in parallel on mount', async () => {
    const seen: string[] = [];
    await render(
      stubClient({ onRequest: (r) => seen.push(r.type) }),
      imageData({}),
    );
    expect(seen).toContain('characterList');
    expect(seen).toContain('imageInfoGet');
  });

  // ImageDetailModal.tsx:55 — the album panel state comes from
  // `data.characterGalleryLinks`; ImageMetadata renders the row.
  it('derives the In-Photo-Albums panel from the imageInfoGet read-back', async () => {
    const fixture = await render(
      stubClient({
        characters: [{ id: 'c-1', name: 'Aria', defaultImageId: null }],
        links: [{ characterId: 'c-1', characterName: 'Aria', linkId: 'l-1' }],
      }),
      imageData({}),
    );
    expect(fixture.nativeElement.textContent).toContain('In Photo Albums');
    expect(fixture.nativeElement.textContent).toContain('Aria');
  });

  /**
   * v4 bug 99 (`8018c487`): the overlay renders through
   * `createPortal(…, document.body)`. Inside `.qt-workspace` — whose
   * `isolation: isolate` makes it a stacking context — the `z-[60]` below is
   * resolved within the workspace and the sticky `.qt-page-toolbar` (z-30, an
   * ancestor context) paints over the whole control cluster. jsdom runs no
   * cascade, so THIS spec can only pin the mechanism (where the host ends up);
   * the consequence is pinned in `e2e/workspace-gallery-modal-flow.spec.ts`
   * with a real hit test.
   */
  it('portals its host out of the pane it is mounted in', async () => {
    // Mounted inside a pane, under an `@if`, which is how every real host
    // mounts it. A bare `TestBed.createComponent` would attach the host to the
    // body ANYWAY and the assertion could never fail.
    TestBed.configureTestingModule({
      imports: [PaneHost],
      providers: [{ provide: CoreClient, useValue: stubClient({}) }],
    });
    const fixture = TestBed.createComponent(PaneHost);
    fixture.detectChanges();
    for (let i = 0; i < 5; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    const pane = (fixture.nativeElement as HTMLElement).querySelector('.pane')!;
    const modal = document.querySelector('qt-image-detail-modal');
    expect(modal, 'the modal rendered').toBeTruthy();
    expect(modal!.parentElement).toBe(document.body);
    expect(pane.querySelector('qt-image-detail-modal')).toBeNull();
    expect(modal!.querySelector('.fixed.inset-0')).toBeTruthy();

    // And it leaves nothing behind on the body when the host closes it.
    fixture.componentInstance.open.set(false);
    fixture.detectChanges();
    await new Promise((r) => setTimeout(r, 0));
    expect(document.querySelector('qt-image-detail-modal')).toBeNull();
  });

  // ImageDetailModal.tsx:87 — the overlay is z-[60] (one above the z-50 gallery).
  it('renders the overlay at z-[60]', async () => {
    const fixture = await render(stubClient({}), imageData({}));
    expect(fixture.nativeElement.querySelector('.fixed.inset-0.z-\\[60\\]')).toBeTruthy();
  });

  // useImageActions.ts:46-48 — the ImageData split: an images-v2 file saves
  // `{fileId}`.
  it('saves to an album by fileId when the image has no linkId', async () => {
    const seen: SeenRequest[] = [];
    const fixture = await render(
      stubClient({
        characters: [{ id: 'c-1', name: 'Aria', defaultImageId: null }],
        links: [],
        onRequest: (r) => seen.push(r),
      }),
      imageData({ id: 'file-9' }),
    );
    const select = fixture.nativeElement.querySelector('select') as HTMLSelectElement;
    select.value = 'c-1';
    select.dispatchEvent(new Event('change'));
    await flush(fixture);

    const save = seen.find((r) => r.type === 'characterPhotoSaveById');
    expect(save).toEqual({
      type: 'characterPhotoSaveById',
      characterId: 'c-1',
      fileId: 'file-9',
    });
  });

  // useImageActions.ts:46-48 — a vault photo (linkId set) saves `{linkId}`
  // so the server re-links from the existing blob.
  it('saves to an album by linkId when the image is a vault photo', async () => {
    const seen: SeenRequest[] = [];
    const fixture = await render(
      stubClient({
        characters: [{ id: 'c-1', name: 'Aria', defaultImageId: null }],
        links: [],
        onRequest: (r) => seen.push(r),
      }),
      imageData({ id: 'link-7', linkId: 'link-7' }),
    );
    const select = fixture.nativeElement.querySelector('select') as HTMLSelectElement;
    select.value = 'c-1';
    select.dispatchEvent(new Event('change'));
    await flush(fixture);

    const save = seen.find((r) => r.type === 'characterPhotoSaveById');
    expect(save).toEqual({
      type: 'characterPhotoSaveById',
      characterId: 'c-1',
      linkId: 'link-7',
    });
  });

  // useImageActions.ts:87-120 — the per-character remove sends the LINK id
  // from the panel state.
  it('removes from an album with the link id', async () => {
    const seen: SeenRequest[] = [];
    const fixture = await render(
      stubClient({
        characters: [{ id: 'c-1', name: 'Aria', defaultImageId: null }],
        links: [{ characterId: 'c-1', characterName: 'Aria', linkId: 'l-42' }],
        onRequest: (r) => seen.push(r),
      }),
      imageData({}),
    );
    (
      fixture.nativeElement.querySelector(
        'button[title="Remove from photo album"]',
      ) as HTMLButtonElement
    ).click();
    await flush(fixture);

    expect(seen.find((r) => r.type === 'characterPhotoRemove')).toEqual({
      type: 'characterPhotoRemove',
      characterId: 'c-1',
      linkId: 'l-42',
    });
  });

  // useImageActions.ts:122-165 — Set Avatar dispatches `characterAvatar`
  // with the image id and emits avatarSet (v4 `onAvatarSet?.()`).
  it('sets the avatar and emits avatarSet', async () => {
    const seen: SeenRequest[] = [];
    const fixture = await render(
      stubClient({
        characters: [{ id: 'c-1', name: 'Aria', defaultImageId: null }],
        links: [{ characterId: 'c-1', characterName: 'Aria', linkId: 'l-1' }],
        onRequest: (r) => seen.push(r),
      }),
      imageData({ id: 'img-5' }),
    );
    const avatarSets: number[] = [];
    fixture.componentInstance.avatarSet.subscribe(() => avatarSets.push(1));

    (
      fixture.nativeElement.querySelector('button[title="Set as avatar"]') as HTMLButtonElement
    ).click();
    await flush(fixture);

    expect(seen.find((r) => r.type === 'characterAvatar')).toEqual({
      type: 'characterAvatar',
      characterId: 'c-1',
      imageId: 'img-5',
    });
    expect(avatarSets.length).toBe(1);
  });

  // useImageActions.ts:180-200 — Save to my gallery rides `photoGallerySave`.
  it('saves to the user gallery from the toolbar', async () => {
    const seen: SeenRequest[] = [];
    const fixture = await render(
      stubClient({ onRequest: (r) => seen.push(r) }),
      imageData({ id: 'file-3' }),
    );
    (
      fixture.nativeElement.querySelector('button[title="Save to my gallery"]') as HTMLButtonElement
    ).click();
    await flush(fixture);

    expect(seen.find((r) => r.type === 'photoGallerySave')).toEqual({
      type: 'photoGallerySave',
      fileId: 'file-3',
    });
    expect(toasts()).toEqual([{ type: 'success', message: 'Saved to your gallery' }]);
  });

  // ImageDetailModal.tsx:72-78 — Escape closes; ArrowRight fires the
  // conditionally-present onNext.
  it('wires Escape and ArrowRight through the navigation helper', async () => {
    let nexts = 0;
    const fixture = await render(stubClient({}), imageData({}), {
      onNext: () => {
        nexts += 1;
      },
    });
    const closes: number[] = [];
    fixture.componentInstance.closeModal.subscribe(() => closes.push(1));

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight' }));
    expect(nexts).toBe(1);
    // ArrowLeft has no handler (onPrev undefined) — nothing happens.
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft' }));
    expect(nexts).toBe(1);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(closes.length).toBe(1);
  });

  // ImageActions.tsx:28/:41 — the arrows render only when the callback exists.
  it('renders arrows only for the provided callbacks', async () => {
    const fixture = await render(stubClient({}), imageData({}), {
      onNext: () => undefined,
    });
    expect(
      fixture.nativeElement.querySelector('button[title="Next image (Right Arrow)"]'),
    ).toBeTruthy();
    expect(
      fixture.nativeElement.querySelector('button[title="Previous image (Left Arrow)"]'),
    ).toBeNull();
  });

  // ImageDetailModal.tsx:105-121 — a failing image swaps to the placeholder
  // (600×400) and hides the album panel.
  it('swaps to the deleted-image placeholder when the image errors', async () => {
    const fixture = await render(stubClient({}), imageData({}));
    (fixture.nativeElement.querySelector('img') as HTMLImageElement).dispatchEvent(
      new Event('error'),
    );
    await flush(fixture);
    expect(fixture.nativeElement.querySelector('qt-deleted-image-placeholder')).toBeTruthy();
    expect(fixture.nativeElement.querySelector('qt-image-metadata')).toBeNull();
  });
});
