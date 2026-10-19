import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import { CoreDispatchError, type WardrobeGalleryImageEntry } from '../../../../core/core-contract';
import { ToastService } from '../../../../ui/toast.service';
import { WardrobePicturesSection } from './wardrobe-pictures-section';

/**
 * The Wardrobe Pictures section at v4 `WardrobeImagesSection.tsx` +
 * `hooks/useWardrobeImages.ts` + `GalleryImage.tsx`'s caption arm parity
 * (`1cfc9b504`). v4 ships no client test for this family, so the case names
 * are the planning survey's §4.2 contract, each pinning a transcribed
 * behaviour by `file:line`.
 */

function entry(overrides: Partial<WardrobeGalleryImageEntry> = {}): WardrobeGalleryImageEntry {
  return {
    fileId: 'f1',
    fileName: 'coat-generated.webp',
    url: '/api/v1/files/f1',
    thumbnailUrl: '/api/v1/files/f1?action=thumbnail',
    mimeType: 'image/webp',
    fileSizeBytes: 2048,
    width: 512,
    height: 768,
    createdAt: '2026-10-17T10:00:00.000Z',
    generationPrompt: 'ZZTOPSECRETPROMPT',
    generationModel: 'gpt-image-1',
    itemId: 'i-own',
    itemTitle: 'Velvet Coat',
    scope: 'character',
    locationLabel: 'Own wardrobe',
    itemArchived: false,
    isCurrentItemImage: true,
    ...overrides,
  };
}

interface SeenRequest {
  type: string;
  [k: string]: unknown;
}

function stubClient(options: {
  entries: WardrobeGalleryImageEntry[];
  total?: number;
  /** Throwing here exercises the failed-read arm. */
  fail?: boolean;
}): { client: Partial<CoreClient>; seen: SeenRequest[] } {
  const seen: SeenRequest[] = [];
  const client: Partial<CoreClient> = {
    dispatchData: (async (req: SeenRequest) => {
      seen.push(req);
      if (req.type === 'characterWardrobeImages') {
        if (options.fail) throw new CoreDispatchError({ kind: 'internal', message: 'the vault is unwell' });
        return { entries: options.entries, total: options.total ?? options.entries.length };
      }
      return {};
    }) as CoreClient['dispatchData'],
  };
  return { client, seen };
}

async function render(
  client: Partial<CoreClient>,
): Promise<ComponentFixture<WardrobePicturesSection>> {
  TestBed.configureTestingModule({
    imports: [WardrobePicturesSection],
    providers: [
      // trap 5 — default TanStack retries would hide the failed-read arm.
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: client },
    ],
  });
  const fixture = TestBed.createComponent(WardrobePicturesSection);
  fixture.componentRef.setInput('characterId', 'c1');
  fixture.componentRef.setInput('entityName', 'Aria');
  fixture.componentRef.setInput('thumbnailSize', 120);
  fixture.detectChanges();
  await flush(fixture);
  return fixture;
}

async function flush(fixture: ComponentFixture<WardrobePicturesSection>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

function header(fixture: ComponentFixture<WardrobePicturesSection>): HTMLButtonElement | null {
  return fixture.nativeElement.querySelector('button[aria-expanded]');
}

async function expand(fixture: ComponentFixture<WardrobePicturesSection>): Promise<void> {
  if (header(fixture)!.getAttribute('aria-expanded') !== 'true') {
    header(fixture)!.click();
    await flush(fixture);
  }
}

function tiles(fixture: ComponentFixture<WardrobePicturesSection>): HTMLElement[] {
  return Array.from(fixture.nativeElement.querySelectorAll('.grid > .relative.group'));
}

describe('WardrobePicturesSection', () => {
  afterEach(() => {
    document.body.style.overflow = '';
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    TestBed.resetTestingModule();
  });

  // v4 `WardrobeImagesSection.tsx:56` — `if (!loading && total === 0) return null`.
  it('renders nothing at all when the character can reach no wardrobe pictures', async () => {
    const stub = stubClient({ entries: [] });
    const fixture = await render(stub.client);
    expect(fixture.nativeElement.querySelector('section')).toBeNull();
    expect((fixture.nativeElement.textContent as string).trim()).toBe('');
    // The one read, through the verb — no query parameters (v4 reads none).
    expect(stub.seen).toEqual([{ type: 'characterWardrobeImages', characterId: 'c1' }]);
  });

  // v4 `:59-80` — the toggle, the count, the description; `:35` `useState(false)`.
  it('shows the Wardrobe Pictures heading, the count (… while loading, 1 picture / N pictures) and the description, collapsed by default', async () => {
    // Pending: the shelf is up with the ellipsis.
    TestBed.configureTestingModule({
      imports: [WardrobePicturesSection],
      providers: [
        provideTanStackQuery(new QueryClient()),
        {
          provide: CoreClient,
          useValue: { dispatchData: () => new Promise(() => {}) } as Partial<CoreClient>,
        },
      ],
    });
    const pending = TestBed.createComponent(WardrobePicturesSection);
    pending.componentRef.setInput('characterId', 'c1');
    pending.componentRef.setInput('entityName', 'Aria');
    pending.componentRef.setInput('thumbnailSize', 120);
    pending.detectChanges();
    const badge = pending.nativeElement.querySelectorAll('span.qt-text-label-xs')[0] as HTMLElement;
    expect(badge.textContent!.trim()).toBe('…');
    TestBed.resetTestingModule();

    const one = await render(stubClient({ entries: [entry()] }).client);
    const labels = one.nativeElement.querySelectorAll('button[aria-expanded] > span');
    expect((labels[0] as HTMLElement).className).toBe('qt-text-label');
    expect((labels[0] as HTMLElement).textContent!.trim()).toBe('Wardrobe Pictures');
    expect((labels[1] as HTMLElement).textContent!.trim()).toBe('1 picture');
    expect(header(one)!.getAttribute('aria-expanded')).toBe('false');
    expect(one.nativeElement.querySelector('img')).toBeNull();
    const paragraph = (one.nativeElement.querySelector('p.qt-text-label-xs.mt-1.ml-6')!
      .textContent as string)
      .replace(/\s+/g, ' ')
      .trim();
    expect(paragraph).toBe(
      'Every likeness the house has sketched of the garments within Aria’s reach — ' +
        'from Aria’s own wardrobe, the wardrobes of any company Aria keeps, and the ' +
        'Quilltap General collection. Each plate bears a little card naming the garment and ' +
        'the wardrobe it hangs in.',
    );
    TestBed.resetTestingModule();

    // `total`, not the page length, drives the plural.
    const many = await render(stubClient({ entries: [entry()], total: 3 }).client);
    const count = many.nativeElement.querySelectorAll('button[aria-expanded] > span')[1] as HTMLElement;
    expect(count.textContent!.trim()).toBe('3 pictures');
  });

  // GalleryImage `:59-68` + WardrobeImagesSection `:43-49` + `:79` (top-1).
  it('expands to a captioned grid: the item title (with " (archived)") over the tier label, the hover actions at the top corner', async () => {
    const fixture = await render(
      stubClient({
        entries: [
          entry(),
          entry({
            fileId: 'f2',
            itemTitle: 'Guild Sash',
            scope: 'group',
            locationLabel: 'Group · The Guild',
            itemArchived: true,
          }),
        ],
      }).client,
    );
    await expand(fixture);
    const grid = fixture.nativeElement.querySelector('.grid') as HTMLElement;
    expect(grid.style.gridTemplateColumns).toContain('120px');
    const all = tiles(fixture);
    expect(all).toHaveLength(2);
    const caption = all[1].querySelector('.qt-gallery-caption') as HTMLElement;
    expect(caption.getAttribute('class')).toBe(
      'qt-gallery-caption absolute inset-x-0 bottom-0 px-1.5 pt-4 pb-1 text-left pointer-events-none',
    );
    // Inside the tile button, after the <img>.
    expect(caption.parentElement!.tagName).toBe('BUTTON');
    expect(caption.previousElementSibling!.tagName).toBe('IMG');
    expect(caption.querySelector('.qt-gallery-caption-title')!.textContent!.trim()).toBe(
      'Guild Sash (archived)',
    );
    expect(caption.querySelector('.qt-gallery-caption-detail')!.textContent!.trim()).toBe(
      'Group · The Guild',
    );
    expect(
      all[0].querySelector('.qt-gallery-caption-title')!.textContent!.trim(),
    ).toBe('Velvet Coat');
    expect(
      all[0].querySelector('.qt-gallery-caption-detail')!.getAttribute('class'),
    ).toBe('qt-gallery-caption-detail truncate');
    const overlay = all[0].querySelector(':scope > div') as HTMLElement;
    expect(overlay.getAttribute('class')).toBe(
      'absolute top-1 right-1 flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity',
    );
    // No tile is ever the portrait here — no Avatar badge.
    expect(fixture.nativeElement.textContent).not.toContain('Avatar');
  });

  // GalleryImage `:81`, `:89`, `:135` — every optional action absent.
  it('offers ONLY Download — no set-as-avatar, keep or delete button', async () => {
    const fixture = await render(stubClient({ entries: [entry()] }).client);
    await expand(fixture);
    const buttons = Array.from(
      tiles(fixture)[0].querySelectorAll(':scope > div button'),
    ) as HTMLButtonElement[];
    expect(buttons.map((b) => b.getAttribute('title'))).toEqual(['Download image']);
    expect(buttons[0].getAttribute('aria-label')).toBe('Download image');
    for (const title of [
      'Set as avatar',
      'Keep in the photo album',
      'Already in the photo album',
      'Delete image',
    ]) {
      expect(fixture.nativeElement.querySelector(`button[title="${title}"]`)).toBeNull();
    }
  });

  // GalleryImage `:61` — `title={`${caption.title} — ${caption.detail}`}`.
  it('the caption\'s tooltip reads "<title> — <tier>"', async () => {
    const fixture = await render(
      stubClient({
        entries: [
          entry({
            itemTitle: 'Canvas Apron',
            scope: 'general',
            locationLabel: 'Shared · Quilltap General',
            itemArchived: true,
          }),
        ],
      }).client,
    );
    await expand(fixture);
    const caption = fixture.nativeElement.querySelector('.qt-gallery-caption') as HTMLElement;
    expect(caption.getAttribute('title')).toBe(
      'Canvas Apron (archived) — Shared · Quilltap General',
    );
  });

  // useWardrobeImages `:35-58` (url = thumbnail, fullUrl = full) + `:85-99`.
  it('the tile shows the thumbnail; the detail modal and the download use the full-size URL', async () => {
    const fetched: string[] = [];
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: unknown) => {
        fetched.push(String(input));
        return { ok: true, blob: async () => new Blob(['x']) } as unknown as Response;
      }),
    );
    const fixture = await render(stubClient({ entries: [entry()] }).client);
    await expand(fixture);
    const img = fixture.nativeElement.querySelector('img') as HTMLImageElement;
    expect(img.getAttribute('src')).toContain('/api/v1/files/f1?action=thumbnail');
    expect(img.getAttribute('alt')).toBe('coat-generated.webp');
    (fixture.nativeElement.querySelector('button[title="Download image"]') as HTMLButtonElement).click();
    await flush(fixture);
    expect(fetched).toHaveLength(1);
    expect(fetched[0]).toMatch(/\/api\/v1\/files\/f1$/);
    // The download click never opens the modal (stopPropagation).
    expect(document.querySelector('qt-image-detail-modal')).toBeNull();
    (fixture.nativeElement.querySelector('.grid button') as HTMLButtonElement).click();
    await flush(fixture);
    const modal = fixture.debugElement.query((n) => n.name === 'qt-image-detail-modal');
    const image = modal.componentInstance.image() as Record<string, unknown>;
    expect(image['filepath']).toBe('/api/v1/files/f1');
    expect(String(image['url'])).toMatch(/\/api\/v1\/files\/f1$/);
  });

  // WardrobeImagesSection `:106-134`.
  it('a click opens the shared detail modal with no linkId; Prev/Next walk the list', async () => {
    const fixture = await render(
      stubClient({
        entries: [
          entry(),
          entry({ fileId: 'f2', fileName: 'sash.webp', mimeType: null, width: null, height: null }),
        ],
      }).client,
    );
    await expand(fixture);
    (fixture.nativeElement.querySelectorAll('.grid button')[0] as HTMLButtonElement).click();
    await flush(fixture);
    // The modal PORTALS to document.body (the gallery tab's spec records the trap).
    const host = document.querySelector('qt-image-detail-modal');
    expect(host).toBeTruthy();
    expect(host!.parentElement).toBe(document.body);
    // R-7: no generation prompt reaches any rendered surface.
    expect(document.body.textContent).not.toContain('ZZTOPSECRETPROMPT');
    const modal = () => fixture.debugElement.query((n) => n.name === 'qt-image-detail-modal');
    const first = modal().componentInstance.image() as Record<string, unknown>;
    expect(first).toEqual({
      id: 'f1',
      filename: 'coat-generated.webp',
      filepath: '/api/v1/files/f1',
      url: expect.stringMatching(/\/api\/v1\/files\/f1$/),
      mimeType: 'image/webp',
      size: 2048,
      width: 512,
      height: 768,
      createdAt: '2026-10-17T10:00:00.000Z',
      tags: [],
    });
    expect(first['linkId']).toBeUndefined();
    expect(modal().componentInstance.onPrev()).toBeUndefined();
    (modal().componentInstance.onNext() as () => void)();
    await flush(fixture);
    const second = modal().componentInstance.image() as Record<string, unknown>;
    expect(second['id']).toBe('f2');
    // `mimeType ?? 'image/webp'`; nullable dimensions collapse to undefined.
    expect(second['mimeType']).toBe('image/webp');
    expect(second['width']).toBeUndefined();
    expect(modal().componentInstance.onNext()).toBeUndefined();
    (modal().componentInstance.onPrev() as () => void)();
    await flush(fixture);
    expect((modal().componentInstance.image() as Record<string, unknown>)['id']).toBe('f1');
  });

  // useWardrobeImages `:85-99` — the FIXED sentence + the console line.
  it('a failed download toasts "Failed to download image"', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => ({ ok: false, status: 503 }) as unknown as Response));
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    const fixture = await render(stubClient({ entries: [entry()] }).client);
    await expand(fixture);
    (fixture.nativeElement.querySelector('button[title="Download image"]') as HTMLButtonElement).click();
    await flush(fixture);
    expect(toasts().at(-1)).toEqual({ type: 'error', message: 'Failed to download image' });
    expect(error).toHaveBeenCalledWith('Error downloading wardrobe picture', {
      characterId: 'c1',
      imageId: 'f1',
      error: 'Failed to fetch image (503)',
    });
  });

  // useWardrobeImages `:80-83` + GalleryImage `:42-45`, `:121`.
  it('a tile whose image fails to load shows the placeholder and drops Download', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const fixture = await render(stubClient({ entries: [entry()] }).client);
    await expand(fixture);
    (fixture.nativeElement.querySelector('img') as HTMLImageElement).dispatchEvent(new Event('error'));
    await flush(fixture);
    expect(fixture.nativeElement.querySelector('img')).toBeNull();
    expect(fixture.nativeElement.querySelector('.grid button qt-icon')).not.toBeNull();
    expect(warn).toHaveBeenCalledWith('Wardrobe picture failed to load', { imageId: 'f1' });
    expect(fixture.nativeElement.querySelector('button[title="Download image"]')).toBeNull();
    // The caption survives the missing bytes (v4 renders it either way).
    expect(fixture.nativeElement.querySelector('.qt-gallery-caption')).not.toBeNull();
  });

  // trap 9 — on a TanStack error `isPending` is false and `total` 0, so the
  // shelf VANISHES, exactly as v4's `!isLoading && total === 0` does.
  it('vanishes on a failed read, with no error text', async () => {
    const fixture = await render(stubClient({ entries: [], fail: true }).client);
    expect(fixture.nativeElement.querySelector('section')).toBeNull();
    expect((fixture.nativeElement.textContent as string).trim()).toBe('');
    expect(toasts()).toEqual([]);
  });
});
