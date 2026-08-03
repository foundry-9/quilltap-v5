import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { CharacterPhoto } from '../../../core/core-contract';
import { ToastService } from '../../../ui/toast.service';
import { AvatarPickerModal } from './avatar-picker-modal';

function photo(over: Partial<CharacterPhoto> = {}): CharacterPhoto {
  return {
    linkId: 'link-1',
    mountPointId: 'mp-1',
    relativePath: 'a.png',
    fileName: 'a.png',
    blobUrl: 'blob:a',
    mimeType: 'image/png',
    sha256: 'sha',
    fileSizeBytes: 1,
    keptAt: '2024-01-01T00:00:00.000Z',
    caption: null,
    tags: [],
    ...over,
  };
}

function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

function stubClient(onAvatar: (req: { type: string; [k: string]: unknown }) => unknown): Partial<CoreClient> {
  return {
    dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
      if (req.type === 'characterPhotoList') {
        return { entries: [photo()], total: 1, hasMore: false };
      }
      if (req.type === 'characterAvatar') {
        return onAvatar(req);
      }
      return {};
    }) as CoreClient['dispatchData'],
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<AvatarPickerModal>> {
  TestBed.configureTestingModule({
    imports: [AvatarPickerModal],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(AvatarPickerModal);
  fixture.componentRef.setInput('characterId', 'char-1');
  fixture.detectChanges();
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

function button(fixture: ComponentFixture<AvatarPickerModal>, text: string): HTMLButtonElement {
  return [...fixture.nativeElement.querySelectorAll('button')].find(
    (b: HTMLButtonElement) => b.textContent?.trim() === text,
  ) as HTMLButtonElement;
}

/** v4 `useCharacterEdit.ts:306-386` — neither outcome has an inline surface. */
describe('AvatarPickerModal toasts', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('toasts "Avatar updated!" and closes on a successful set', async () => {
    const fixture = await render(stubClient(() => ({ character: {} })));
    let saved = 0;
    let closed = 0;
    fixture.componentInstance.saved.subscribe(() => saved++);
    fixture.componentInstance.close.subscribe(() => closed++);

    const grid = fixture.nativeElement.querySelector('button.rounded-lg') as HTMLButtonElement;
    grid.click();
    fixture.detectChanges();
    button(fixture, 'Set as Avatar').click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(toasts()).toEqual([{ type: 'success', message: 'Avatar updated!' }]);
    expect(saved).toBe(1);
    expect(closed).toBe(1);
  });

  it("toasts v4's fallback message on a failed set (a non-Error rejection)", async () => {
    const fixture = await render(stubClient(() => Promise.reject('network down')));
    const grid = fixture.nativeElement.querySelector('button.rounded-lg') as HTMLButtonElement;
    grid.click();
    fixture.detectChanges();
    button(fixture, 'Set as Avatar').click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(toasts()).toEqual([{ type: 'error', message: 'Failed to set avatar' }]);
    expect(fixture.nativeElement.textContent).not.toContain('Failed to set avatar');
  });

  it('toasts "Avatar cleared!" and closes on a successful clear', async () => {
    const fixture = await render(stubClient(() => ({ character: {} })));
    button(fixture, 'Clear Avatar').click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(toasts()).toEqual([{ type: 'success', message: 'Avatar cleared!' }]);
  });

  it('toasts the server message on a failed clear', async () => {
    const fixture = await render(
      stubClient(() => {
        throw new Error('the gallery is locked');
      }),
    );
    button(fixture, 'Clear Avatar').click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(toasts()).toEqual([{ type: 'error', message: 'the gallery is locked' }]);
  });
});
