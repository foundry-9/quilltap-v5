import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { ToastService } from '../ui/toast.service';
import type { WardrobeContainer } from './wardrobe-container';
import { WardrobeImageViewer, pictureFilename } from './wardrobe-image-viewer';

/**
 * P4.D277 — v4 `components/wardrobe/wardrobe-image-viewer.tsx` (NEW in
 * `01a83539d`, 127 lines, read at that pin). The frame and the save dialog
 * both portal to `document.body`, so queries read the body.
 */
@Component({
  imports: [WardrobeImageViewer],
  template: `
    @if (open()) {
      <qt-wardrobe-image-viewer
        itemId="item-1"
        [itemTitle]="title()"
        [container]="container"
        fileId="file-1"
        [note]="note()"
        [onPrev]="onPrev()"
        [onNext]="onNext()"
        (closed)="closes.set(closes() + 1)"
        (removed)="removals.push(closes())"
      />
    }
  `,
})
class Host {
  readonly open = signal(true);
  readonly title = signal('Blue Velvet Gown');
  readonly note = signal<string | null>(null);
  readonly onPrev = signal<(() => void) | undefined>(undefined);
  readonly onNext = signal<(() => void) | undefined>(undefined);
  readonly closes = signal(0);
  /** `closes()` at each `removed` emission — pins removed-before-closed. */
  readonly removals: number[] = [];
  readonly container: WardrobeContainer = { scope: 'character', id: 'char-1' };
}

interface Seen {
  requests: Array<{ type: string; [k: string]: unknown }>;
}

function stubCore(seen: Seen): Partial<CoreClient> {
  return {
    dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
      seen.requests.push(req);
      if (req.type === 'wardrobeItemImageSaveTargets') {
        return {
          albums: [{ mountPointId: 'mp-gen', name: 'Quilltap General', kind: 'general', isDefault: true }],
        };
      }
      if (req.type === 'wardrobeItemImageSaveToStore') {
        return { saved: true, mountPoint: 'Quilltap General', relativePath: 'photos/Blue_Velvet_Gown.webp' };
      }
      return {};
    }) as CoreClient['dispatchData'],
  };
}

async function flush(fixture: ComponentFixture<Host>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(
  setup: (host: Host) => void = () => {},
  seen: Seen = { requests: [] },
): Promise<ComponentFixture<Host>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: stubCore(seen) },
    ],
  });
  const fixture = TestBed.createComponent(Host);
  setup(fixture.componentInstance);
  fixture.detectChanges();
  await flush(fixture);
  return fixture;
}

const body = (): HTMLElement => document.body;
const byTitle = (title: string): HTMLButtonElement | null =>
  body().querySelector(`qt-full-screen-image-viewer button[title="${title}"]`);
const key = (k: string): void => {
  document.dispatchEvent(new KeyboardEvent('keydown', { key: k }));
};
const toasts = (): string[] => TestBed.inject(ToastService).toasts().map((t) => t.message);

/** v4 `:45-49`'s two regexes, evaluated in this engine — the spec's own oracle. */
const v4PictureFilename = (title: string): string => {
  const stem = title.replace(/[^a-zA-Z0-9]+/g, '_').replace(/^_+|_+$/g, '');
  return `${stem || 'wardrobe_item'}.webp`;
};

describe('pictureFilename (v4 wardrobe-image-viewer.tsx:45-49 at 01a83539d)', () => {
  it.each([
    ['Blue Velvet Gown', 'Blue_Velvet_Gown.webp'],
    // Non-ASCII is a separator: `[^a-zA-Z0-9]` matches the é.
    ['  --Café au Lait!! ', 'Caf_au_Lait.webp'],
    ['!!!', 'wardrobe_item.webp'],
    ['', 'wardrobe_item.webp'],
    ['A__B', 'A_B.webp'],
    ['Dress (2)', 'Dress_2.webp'],
  ])('%j → %s', (title, expected) => {
    expect(v4PictureFilename(title)).toBe(expected);
    expect(pictureFilename(title)).toBe(expected);
  });
});

describe('WardrobeImageViewer (v4 wardrobe-image-viewer.tsx at 01a83539d)', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    TestBed.resetTestingModule();
    document.body.style.overflow = '';
  });

  it('renders the frame at z-[90], named "Picture of <title>", body-portalled (:90-98)', async () => {
    await render();
    const frame = body().querySelector('qt-full-screen-image-viewer')!;
    expect(frame.parentElement).toBe(document.body);
    const dialog = frame.querySelector('[role="dialog"]')!;
    expect(dialog.getAttribute('aria-label')).toBe('Picture of Blue Velvet Gown');
    expect(dialog.classList.contains('z-[90]')).toBe(true);
    expect(frame.querySelector('img')!.getAttribute('src')).toBe('/api/v1/files/file-1');
  });

  it('carries the three action titles in order, then Close (:99-103)', async () => {
    await render();
    const titles = [
      ...body().querySelectorAll<HTMLButtonElement>('qt-full-screen-image-viewer .top-4 button'),
    ].map((b) => b.title);
    expect(titles).toEqual(['Save to a document store', 'Download', 'Copy to clipboard', 'Close (Escape)']);
  });

  it('captions the title, and the note only when given (:104-109)', async () => {
    const fixture = await render();
    const caption = () => body().querySelector('qt-full-screen-image-viewer .qt-bg-overlay-caption')!;
    expect(caption().textContent!.trim()).toBe('Blue Velvet Gown');
    expect(caption().querySelector('.text-xs.opacity-80')).toBeNull();
    fixture.componentInstance.note.set('Current picture · 1 of 2');
    await flush(fixture);
    expect(caption().querySelector('.text-xs.opacity-80')!.textContent).toBe('Current picture · 1 of 2');
  });

  it('Save opens the dialog on the wardrobe target at z-[100]; while it is up the keys leave the viewer alone (:12-15, :113-122)', async () => {
    const seen: Seen = { requests: [] };
    const nav: string[] = [];
    const fixture = await render((h) => {
      h.onPrev.set(() => nav.push('prev'));
      h.onNext.set(() => nav.push('next'));
    }, seen);
    byTitle('Save to a document store')!.click();
    await flush(fixture);
    const dialog = body().querySelector('qt-save-image-dialog')!;
    expect(dialog.parentElement).toBe(document.body);
    expect(dialog.querySelector('.qt-dialog-overlay')!.classList.contains('z-[100]')).toBe(true);
    expect(seen.requests).toEqual([
      { type: 'wardrobeItemImageSaveTargets', scope: 'character', containerId: 'char-1', itemId: 'item-1' },
    ]);
    key('ArrowLeft');
    key('ArrowRight');
    expect(nav).toEqual([]);
    // One Escape closes ONLY the dialog.
    key('Escape');
    await flush(fixture);
    expect(body().querySelector('qt-save-image-dialog')).toBeNull();
    expect(fixture.componentInstance.closes()).toBe(0);
    // The dialog's close restores the keys: the next Escape closes the viewer.
    key('ArrowRight');
    expect(nav).toEqual(['next']);
    key('Escape');
    expect(fixture.componentInstance.closes()).toBe(1);
  });

  it('toasts "Saved to Quilltap General" when the dialog saves (:122)', async () => {
    const seen: Seen = { requests: [] };
    const fixture = await render(() => {}, seen);
    byTitle('Save to a document store')!.click();
    await flush(fixture);
    (body().querySelector('qt-save-image-dialog .qt-dialog-footer button:last-child') as HTMLButtonElement).click();
    await flush(fixture);
    expect(seen.requests[1]).toEqual({
      type: 'wardrobeItemImageSaveToStore',
      scope: 'character',
      containerId: 'char-1',
      itemId: 'item-1',
      fileId: 'file-1',
      mountPointId: 'mp-gen',
    });
    expect(toasts()).toContain('Saved to Quilltap General');
    expect(body().querySelector('qt-save-image-dialog')).toBeNull();
  });

  it('Download anchors the full-size URL under pictureFilename(title) (:65-74)', async () => {
    await render();
    const clicked: Array<{ href: string; download: string }> = [];
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) {
      clicked.push({ href: this.href, download: this.download });
    });
    byTitle('Download')!.click();
    expect(clicked).toHaveLength(1);
    expect(clicked[0].href).toContain('/api/v1/files/file-1?download=1');
    expect(clicked[0].download).toBe('Blue_Velvet_Gown.webp');
  });

  it('a Download that throws toasts "Failed to download image" and logs v4\'s line (:68-73)', async () => {
    await render();
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {
      throw new Error('blocked');
    });
    const errors = vi.spyOn(console, 'error').mockImplementation(() => {});
    byTitle('Download')!.click();
    expect(toasts()).toContain('Failed to download image');
    expect(errors).toHaveBeenCalledWith('Failed to download wardrobe picture:', { error: 'blocked' });
  });

  it('Copy toasts v4\'s success sentence (:76-80)', async () => {
    const fixture = await render();
    vi.stubGlobal('fetch', vi.fn(async () => ({ blob: async () => new Blob(['x'], { type: 'image/png' }) })));
    vi.stubGlobal('ClipboardItem', class {
      constructor(readonly items: unknown) {}
    });
    // `writable` too: a later spec in the same worker reassigns the clipboard
    // with `Object.assign` (`chat/courier-bubble.spec.ts`), which throws on a
    // read-only property (measured by P4.D286's gate).
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      writable: true,
      value: { write: vi.fn(async () => undefined) },
    });
    byTitle('Copy to clipboard')!.click();
    await flush(fixture);
    expect(toasts()).toContain('Image copied to clipboard');
  });

  it('a failed Copy toasts v4\'s failure sentence (:81-85)', async () => {
    const fixture = await render();
    vi.stubGlobal('fetch', vi.fn(async () => ({ blob: async () => new Blob(['x'], { type: 'image/png' }) })));
    vi.stubGlobal('ClipboardItem', class {
      constructor(readonly items: unknown) {}
    });
    // `writable` too: a later spec in the same worker reassigns the clipboard
    // with `Object.assign` (`chat/courier-bubble.spec.ts`), which throws on a
    // read-only property (measured by P4.D286's gate).
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      writable: true,
      value: { write: vi.fn(async () => Promise.reject(new Error('denied'))) },
    });
    const errors = vi.spyOn(console, 'error').mockImplementation(() => {});
    byTitle('Copy to clipboard')!.click();
    await flush(fixture);
    expect(toasts()).toContain('Failed to copy image to clipboard');
    expect(errors).toHaveBeenCalledWith('Failed to copy wardrobe picture:', {
      error: 'No clipboard write method available',
    });
  });

  it('Escape closes the viewer when no dialog is up', async () => {
    const fixture = await render();
    key('Escape');
    expect(fixture.componentInstance.closes()).toBe(1);
  });
});

/**
 * P4.D286 — bug 194: v4 `9ecebf986`
 * `__tests__/unit/components/wardrobe/wardrobe-image-viewer.missing-cleanup.test.tsx`
 * `:50` at the pin `4a38222e6`, transcribed. A wardrobe picture whose bytes are
 * gone is removed through the images route's `delete-image` action (v5's
 * `wardrobeItemImageDelete` verb), never the generic image DELETE; v4's
 * `invalidateQueries({ queryKey: queryKeys.wardrobe.all })` is the `removed`
 * output each host answers with its refresh; then the viewer closes.
 */
describe('WardrobeImageViewer — missing-picture cleanup (bug 194)', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    TestBed.resetTestingModule();
    document.body.style.overflow = '';
  });

  it('removes the picture through the wardrobe images route and refreshes the wardrobe', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const fetchMock = vi.fn(async (_url: string) => ({ ok: true, json: async () => ({}) }));
    vi.stubGlobal('fetch', fetchMock);
    const seen: Seen = { requests: [] };
    const fixture = await render(() => {}, seen);
    body().querySelector('qt-full-screen-image-viewer img')!.dispatchEvent(new Event('error'));
    await flush(fixture);
    const remove = [...body().querySelectorAll('qt-deleted-image-placeholder button')].find(
      (b) => (b.textContent ?? '').trim() === 'Remove',
    ) as HTMLButtonElement;
    remove.click();
    await flush(fixture);

    expect(seen.requests).toEqual([
      {
        type: 'wardrobeItemImageDelete',
        scope: 'character',
        containerId: 'char-1',
        itemId: 'item-1',
        fileId: 'file-1',
      },
    ]);
    expect(fetchMock.mock.calls.some(([u]) => String(u).includes('/api/v1/images/'))).toBe(false);
    // Refreshed before it closed.
    expect(fixture.componentInstance.removals).toEqual([0]);
    expect(fixture.componentInstance.closes()).toBe(1);
  });

  it('a refused removal toasts the server’s sentence and keeps the viewer open', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [Host],
      providers: [
        provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
        {
          provide: CoreClient,
          useValue: {
            dispatchData: async () => {
              throw new Error('That picture is not one of this item’s');
            },
          },
        },
      ],
    });
    const fixture = TestBed.createComponent(Host);
    fixture.detectChanges();
    await flush(fixture);
    body().querySelector('qt-full-screen-image-viewer img')!.dispatchEvent(new Event('error'));
    await flush(fixture);
    (
      [...body().querySelectorAll('qt-deleted-image-placeholder button')].find(
        (b) => (b.textContent ?? '').trim() === 'Remove',
      ) as HTMLButtonElement
    ).click();
    await flush(fixture);
    expect(toasts()).toContain('That picture is not one of this item’s');
    expect(fixture.componentInstance.removals).toEqual([]);
    expect(fixture.componentInstance.closes()).toBe(0);
  });
});
