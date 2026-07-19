import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { ChatGalleryImageViewModal, type ChatGalleryFile } from './chat-gallery-image-view-modal';

const FILE: ChatGalleryFile = {
  id: 'file-1',
  filename: 'shot.png',
  url: '/api/v1/files/file-1',
};

interface SeenRequest {
  type: string;
  [k: string]: unknown;
}

function stubClient(options: {
  links?: Array<{ characterId: string; characterName: string; linkId: string }>;
  onRequest?: (req: SeenRequest) => void;
}): Partial<CoreClient> {
  return {
    dispatchData: (async (req: SeenRequest) => {
      options.onRequest?.(req);
      if (req.type === 'imageInfoGet') {
        return { data: { characterGalleryLinks: options.links ?? [] } };
      }
      return {};
    }) as CoreClient['dispatchData'],
    dispatch: (async (req: SeenRequest) => {
      options.onRequest?.(req);
      if (req.type === 'characterPhotoSaveById') {
        return { type: 'ack', data: { linkId: 'minted-link' } };
      }
      return { type: 'ack', data: {} };
    }) as CoreClient['dispatch'],
  };
}

async function render(
  client: Partial<CoreClient>,
  inputs: Record<string, unknown> = {},
): Promise<ComponentFixture<ChatGalleryImageViewModal>> {
  TestBed.configureTestingModule({
    imports: [ChatGalleryImageViewModal],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(ChatGalleryImageViewModal);
  fixture.componentRef.setInput('file', FILE);
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

async function flush(fixture: ComponentFixture<ChatGalleryImageViewModal>): Promise<void> {
  for (let i = 0; i < 5; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

/**
 * The chat-item detail modal. v4 has NO test for
 * `ChatGalleryImageViewModal.tsx` — every case pins a transcribed behavior by
 * `file:line` (this suite is its only fidelity record).
 */
describe('ChatGalleryImageViewModal', () => {
  afterEach(() => {
    document.body.style.overflow = '';
    vi.restoreAllMocks();
    TestBed.resetTestingModule();
  });

  // ChatGalleryImageViewModal.tsx:49-90 — the already-saved state derives
  // from `imageInfoGet`'s characterGalleryLinks: a matching characterId
  // flips the toggle to its "Remove from …" form. This is the read that made
  // v5's save buttons more than write-only.
  it('derives the already-saved state from the imageInfoGet read-back', async () => {
    const fixture = await render(
      stubClient({
        links: [{ characterId: 'c-1', characterName: 'Aria', linkId: 'l-1' }],
      }),
      { characterId: 'c-1', characterName: 'Aria', userCharacterId: 'u-1', userCharacterName: 'You' },
    );
    expect(
      fixture.nativeElement.querySelector('button[title="Remove from Aria\'s photo album"]'),
    ).toBeTruthy();
    // The user-character has no link → its toggle offers Save.
    expect(
      fixture.nativeElement.querySelector('button[title="Save to You\'s photo album"]'),
    ).toBeTruthy();
  });

  // :136-149 — the save leg posts `{fileId}` for the chat file.
  it('saves to the character album by fileId when not yet saved', async () => {
    const seen: SeenRequest[] = [];
    const fixture = await render(
      stubClient({ links: [], onRequest: (r) => seen.push(r) }),
      { characterId: 'c-1', characterName: 'Aria' },
    );
    (
      fixture.nativeElement.querySelector(
        'button[title="Save to Aria\'s photo album"]',
      ) as HTMLButtonElement
    ).click();
    await flush(fixture);

    expect(seen.find((r) => r.type === 'characterPhotoSaveById')).toEqual({
      type: 'characterPhotoSaveById',
      characterId: 'c-1',
      fileId: 'file-1',
    });
    // The toggle flipped with the minted link id (:146-148).
    expect(
      fixture.nativeElement.querySelector('button[title="Remove from Aria\'s photo album"]'),
    ).toBeTruthy();
  });

  // :125-135 — the remove leg deletes by the STORED link id.
  it('removes from the character album by the stored link id', async () => {
    const seen: SeenRequest[] = [];
    const fixture = await render(
      stubClient({
        links: [{ characterId: 'c-1', characterName: 'Aria', linkId: 'l-9' }],
        onRequest: (r) => seen.push(r),
      }),
      { characterId: 'c-1', characterName: 'Aria' },
    );
    (
      fixture.nativeElement.querySelector(
        'button[title="Remove from Aria\'s photo album"]',
      ) as HTMLButtonElement
    ).click();
    await flush(fixture);

    expect(seen.find((r) => r.type === 'characterPhotoRemove')).toEqual({
      type: 'characterPhotoRemove',
      characterId: 'c-1',
      linkId: 'l-9',
    });
    expect(
      fixture.nativeElement.querySelector('button[title="Save to Aria\'s photo album"]'),
    ).toBeTruthy();
  });

  // :159-196 — the user-character toggle runs the same pair against the
  // user-character id (v4 carries the flow TWICE OVER).
  it('toggles the user-character album independently', async () => {
    const seen: SeenRequest[] = [];
    const fixture = await render(
      stubClient({ links: [], onRequest: (r) => seen.push(r) }),
      { userCharacterId: 'u-1', userCharacterName: 'You' },
    );
    (
      fixture.nativeElement.querySelector(
        'button[title="Save to You\'s photo album"]',
      ) as HTMLButtonElement
    ).click();
    await flush(fixture);

    expect(seen.find((r) => r.type === 'characterPhotoSaveById')).toEqual({
      type: 'characterPhotoSaveById',
      characterId: 'u-1',
      fileId: 'file-1',
    });
  });

  // :198-202 — the delete asks v4's exact confirmation, then the HOST deletes.
  it('emits deleteFile after the confirmation', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
    const fixture = await render(stubClient({}));
    const deletes: number[] = [];
    fixture.componentInstance.deleteFile.subscribe(() => deletes.push(1));

    (
      fixture.nativeElement.querySelector(
        'button[title="Delete image permanently"]',
      ) as HTMLButtonElement
    ).click();
    expect(confirm).toHaveBeenCalledWith('Permanently delete this photo? This cannot be undone.');
    expect(deletes.length).toBe(1);
  });

  it('does not emit deleteFile when the confirmation is declined', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(false);
    const fixture = await render(stubClient({}));
    const deletes: number[] = [];
    fixture.componentInstance.deleteFile.subscribe(() => deletes.push(1));
    (
      fixture.nativeElement.querySelector(
        'button[title="Delete image permanently"]',
      ) as HTMLButtonElement
    ).click();
    expect(deletes.length).toBe(0);
  });

  // :208 — the overlay is z-[60]; :214-238 — the conditional arrows.
  it('layers at z-[60] and renders only the provided arrows', async () => {
    let prevs = 0;
    const fixture = await render(stubClient({}), {
      onPrev: () => {
        prevs += 1;
      },
    });
    expect(fixture.nativeElement.querySelector('.fixed.inset-0.z-\\[60\\]')).toBeTruthy();
    const prev = fixture.nativeElement.querySelector(
      'button[title="Previous image (Left Arrow)"]',
    ) as HTMLButtonElement;
    expect(prev).toBeTruthy();
    expect(fixture.nativeElement.querySelector('button[title="Next image (Right Arrow)"]')).toBeNull();
    prev.click();
    expect(prevs).toBe(1);
  });

  // :93-98 — Escape closes through the navigation helper.
  it('closes on Escape', async () => {
    const fixture = await render(stubClient({}));
    const closes: number[] = [];
    fixture.componentInstance.closeModal.subscribe(() => closes.push(1));
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(closes.length).toBe(1);
  });

  // :241 / :316 — the toolbar and delete button hide while the image is missing.
  it('hides the toolbar and delete button for a missing image', async () => {
    const fixture = await render(stubClient({}));
    (fixture.nativeElement.querySelector('img') as HTMLImageElement).dispatchEvent(
      new Event('error'),
    );
    await flush(fixture);
    expect(fixture.nativeElement.querySelector('qt-deleted-image-placeholder')).toBeTruthy();
    expect(fixture.nativeElement.querySelector('button[title="Download"]')).toBeNull();
    expect(
      fixture.nativeElement.querySelector('button[title="Delete image permanently"]'),
    ).toBeNull();
  });
});
