import { signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { PhotoAlbumOption } from '../../core/core-contract';
import { WORKSPACE_TAB_VISIBLE } from '../../workspace/workspace-contract';
import { ToastService } from '../../ui/toast.service';
import { PhotosPage } from './photos-page';
import {
  GalleryEntry,
  cardCaption,
  cardLinkerLabel,
  detailCaption,
  linkerBadge,
} from './photos.api';

/**
 * Unit specs for the My Photos screen (v4 `app/photos/PhotosView.tsx`), pinning
 * the transcribed logic with v4 file:line references: the caption fallback
 * chains, the linker label and Vault/Album badge, the counter-label arms, the
 * stale-generation guard, and the defensive de-dupe on append.
 */

function linker(over: Partial<GalleryEntry['linkSummary']['linkers'][0]> = {}) {
  return {
    linkId: 'L1',
    mountPointId: 'MP1',
    mountPointName: 'Quilltap Uploads',
    mountStoreType: 'documents' as const,
    relativePath: 'photos/a.webp',
    isPhotoAlbum: true,
    linkedAt: '2026-02-01T00:00:00.000Z',
    linkedBy: 'You',
    linkedById: 'U1',
    caption: null,
    tags: [],
    ...over,
  };
}

function entry(over: Partial<GalleryEntry> = {}): GalleryEntry {
  return {
    linkId: 'L1',
    mountPointId: 'MP1',
    relativePath: 'photos/a.webp',
    fileName: 'a.webp',
    blobUrl: '/api/v1/mount-points/MP1/blobs/photos/a.webp',
    mimeType: 'image/webp',
    sha256: 'abc',
    fileSizeBytes: 10,
    keptAt: '2026-02-01T00:00:00.000Z',
    caption: null,
    tags: [],
    generationPromptExcerpt: '',
    linkSummary: { count: 1, linkers: [linker()] },
    ...over,
  };
}

describe('photos.api projections (v4 PhotosView.tsx)', () => {
  it('cardCaption falls through caption → prompt excerpt → fileName (v4 :338)', () => {
    expect(cardCaption(entry({ caption: 'A caption' }))).toBe('A caption');
    expect(cardCaption(entry({ generationPromptExcerpt: 'A prompt' }))).toBe('A prompt');
    expect(cardCaption(entry())).toBe('a.webp');
  });

  it('cardCaption treats an EMPTY caption as absent — v4 chains with ||, not ??', () => {
    // The distinction is real: generationPromptExcerpt is '' (never null) when
    // there is no prompt, so a `??` chain would render a blank label.
    expect(cardCaption(entry({ caption: '', generationPromptExcerpt: 'A prompt' }))).toBe(
      'A prompt',
    );
    expect(cardCaption(entry({ caption: '', generationPromptExcerpt: '' }))).toBe('a.webp');
  });

  it('detailCaption drops the prompt fallback (v4 :403)', () => {
    expect(detailCaption(entry({ generationPromptExcerpt: 'A prompt' }))).toBe('a.webp');
  });

  it('cardLinkerLabel prefers THIS entry’s own linker row (v4 :339-343)', () => {
    const e = entry({
      linkId: 'L2',
      linkSummary: {
        count: 2,
        linkers: [
          linker({ linkId: 'L1', linkedBy: 'Aria' }),
          linker({ linkId: 'L2', linkedBy: 'Bramwell' }),
        ],
      },
    });
    expect(cardLinkerLabel(e)).toBe('Bramwell');
  });

  it('cardLinkerLabel falls back to the first linker, then the mount name', () => {
    const missing = entry({
      linkId: 'NOPE',
      linkSummary: { count: 1, linkers: [linker({ linkId: 'L9', linkedBy: 'Aria' })] },
    });
    expect(cardLinkerLabel(missing)).toBe('Aria');

    const unnamed = entry({
      linkSummary: { count: 1, linkers: [linker({ linkedBy: null })] },
    });
    expect(cardLinkerLabel(unnamed)).toBe('Quilltap Uploads');

    const empty = entry({ linkSummary: { count: 0, linkers: [] } });
    expect(cardLinkerLabel(empty)).toBe('Unfiled');
  });

  it('linkerBadge reads a character store as Vault, anything else Album (v4 :471)', () => {
    expect(linkerBadge(linker({ mountStoreType: 'character' }))).toBe('Vault');
    expect(linkerBadge(linker({ mountStoreType: 'documents' }))).toBe('Album');
  });
});

// ---------------------------------------------------------------------------

interface Page {
  entries: GalleryEntry[];
  total: number;
  hasMore: boolean;
}

/**
 * A CoreClient stub whose list responses are scripted per call. The album
 * selector's `photoGalleryAlbums` read (P4.D330, v4 `680b71044`) answers
 * `albums` (none by default) and is NOT counted in `calls`, which records the
 * gallery reads and the deletes; `albumCalls` counts it.
 */
function stubClient(
  pages: Page[],
  albums: PhotoAlbumOption[] = [],
): { client: Partial<CoreClient>; calls: unknown[]; albumCalls: unknown[] } {
  const calls: unknown[] = [];
  const albumCalls: unknown[] = [];
  let i = 0;
  const client = {
    dispatchData: (req: unknown) => {
      if ((req as { type?: string }).type === 'photoGalleryAlbums') {
        albumCalls.push(req);
        return Promise.resolve({ albums } as unknown as Record<string, unknown>);
      }
      calls.push(req);
      const page = pages[Math.min(i++, pages.length - 1)];
      return Promise.resolve(page as unknown as Record<string, unknown>);
    },
  } as Partial<CoreClient>;
  return { client, calls, albumCalls };
}

/** The TanStack client the album selector's read needs (retry off). */
function tanStack() {
  return provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } }));
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 8): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(
  pages: Page[],
  albums: PhotoAlbumOption[] = [],
): Promise<{
  fixture: ComponentFixture<PhotosPage>;
  calls: unknown[];
  albumCalls: unknown[];
}> {
  const { client, calls, albumCalls } = stubClient(pages, albums);
  TestBed.configureTestingModule({
    imports: [PhotosPage],
    providers: [{ provide: CoreClient, useValue: client }, provideRouter([]), tanStack()],
  });
  const fixture = TestBed.createComponent(PhotosPage);
  fixture.detectChanges();
  await settle(fixture);
  return { fixture, calls, albumCalls };
}

describe('PhotosPage (v4 app/photos/PhotosView.tsx)', () => {
  beforeEach(() => {
    // jsdom has no IntersectionObserver; the screen only needs it to exist.
    (globalThis as unknown as { IntersectionObserver: unknown }).IntersectionObserver = class {
      observe() {}
      disconnect() {}
    };
  });
  afterEach(() => TestBed.resetTestingModule());

  it('renders the grid, the link badge, and the caption fallback', async () => {
    const { fixture } = await render([
      {
        entries: [
          entry({ linkId: 'L1', caption: 'Dawn flight', linkSummary: { count: 3, linkers: [linker()] } }),
          entry({ linkId: 'L2', fileName: 'b.webp' }),
        ],
        total: 2,
        hasMore: false,
      },
    ]);
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('My Photos');
    expect(text).toContain('Dawn flight');
    // The badge shows only when the same bytes live in more than one place.
    expect(text).toContain('3');
    expect(text).toContain('b.webp');
    expect(text).toContain('2 photos');
  });

  it('counterLabel covers v4’s five arms (v4 :188-195)', async () => {
    const { fixture } = await render([{ entries: [], total: 0, hasMore: false }]);
    const page = fixture.componentInstance as unknown as {
      total: { set: (n: number) => void };
      appliedQuery: { set: (s: string) => void };
      counterLabel: () => string;
    };

    expect(page.counterLabel()).toBe('No photos yet');

    page.total.set(1);
    expect(page.counterLabel()).toBe('1 photo');
    page.total.set(4);
    expect(page.counterLabel()).toBe('4 photos');

    page.appliedQuery.set('dawn');
    expect(page.counterLabel()).toBe('4 matches for “dawn”');
    page.total.set(1);
    expect(page.counterLabel()).toBe('1 match for “dawn”');
    page.total.set(0);
    expect(page.counterLabel()).toBe('No matches for “dawn”');
  });

  it('a search refetches from offset 0 and sends the TRIMMED query', async () => {
    const { fixture, calls } = await render([
      { entries: [entry()], total: 1, hasMore: false },
      { entries: [entry({ linkId: 'L9' })], total: 1, hasMore: false },
    ]);
    const page = fixture.componentInstance as unknown as {
      queryText: string;
      onSearch: (e: Event) => void;
    };
    page.queryText = '  dawn flight  ';
    page.onSearch(new Event('submit'));
    await settle(fixture);

    expect(calls).toHaveLength(2);
    expect(calls[0]).toMatchObject({ type: 'photoGalleryList', limit: 60, offset: 0 });
    // v4 sets `q` only when the trimmed query is non-empty, and sends it trimmed.
    expect(calls[0]).not.toHaveProperty('q');
    expect(calls[1]).toMatchObject({ q: 'dawn flight', offset: 0 });
  });

  it('the stale-generation guard drops a load-more that outlived its query (v4 :77,:97)', async () => {
    // The scenario the guard exists for: a load-more is in flight when the
    // reader runs a NEW search. The search's own first page lands first, then
    // the OLD query's page-2 resolves — and must be discarded, or the new
    // results grow rows that never matched the new query.
    let releaseLoadMore: (() => void) | undefined;
    const calls: Array<Record<string, unknown>> = [];
    const client = {
      dispatchData: (req: Record<string, unknown>) => {
        if (req['type'] === 'photoGalleryAlbums') {
          return Promise.resolve({ albums: [] } as unknown as Record<string, unknown>);
        }
        calls.push(req);
        if (req['offset'] === 0 && calls.length === 1) {
          // The initial page: one row, more available.
          return Promise.resolve({
            entries: [entry({ linkId: 'OLD1' })],
            total: 60,
            hasMore: true,
          } as unknown as Record<string, unknown>);
        }
        if ((req['offset'] as number) > 0) {
          // The load-more — held open until the new search has landed.
          return new Promise<Record<string, unknown>>((resolve) => {
            releaseLoadMore = () =>
              resolve({
                entries: [entry({ linkId: 'STALE' })],
                total: 60,
                hasMore: true,
              } as unknown as Record<string, unknown>);
          });
        }
        // The new search's first page.
        return Promise.resolve({
          entries: [entry({ linkId: 'NEW1' })],
          total: 1,
          hasMore: false,
        } as unknown as Record<string, unknown>);
      },
    } as Partial<CoreClient>;

    TestBed.configureTestingModule({
      imports: [PhotosPage],
      providers: [{ provide: CoreClient, useValue: client }, provideRouter([]), tanStack()],
    });
    const fixture = TestBed.createComponent(PhotosPage);
    fixture.detectChanges();
    await settle(fixture);

    const page = fixture.componentInstance as unknown as {
      entries: () => GalleryEntry[];
      queryText: string;
      onSearch: (e: Event) => void;
      loadMore: () => Promise<void>;
    };

    const inFlight = page.loadMore.call(page);
    page.queryText = 'dawn';
    page.onSearch(new Event('submit'));
    await settle(fixture);
    // The new query's page is in place before the stale one resolves.
    expect(page.entries().map((e) => e.linkId)).toEqual(['NEW1']);

    releaseLoadMore?.();
    await inFlight;
    await settle(fixture);

    // The stale page was discarded whole — no STALE row, and hasMore/total
    // still describe the NEW query.
    expect(page.entries().map((e) => e.linkId)).toEqual(['NEW1']);
  });

  it('the defensive de-dupe drops a linkId that arrives twice (v4 :125-131)', async () => {
    // Page 2 repeats page 1's row — what a save landing mid-scroll produces.
    const { fixture } = await render([
      { entries: [entry({ linkId: 'L1' })], total: 2, hasMore: true },
      { entries: [entry({ linkId: 'L1' }), entry({ linkId: 'L2' })], total: 2, hasMore: false },
    ]);
    const page = fixture.componentInstance as unknown as {
      entries: () => GalleryEntry[];
      loadMore: () => Promise<void>;
    };
    await page.loadMore.call(page);
    await settle(fixture);

    const ids = page.entries().map((e) => e.linkId);
    expect(ids).toEqual(['L1', 'L2']);
  });

  it('delete is confirm-gated and removes optimistically (v4 :166-186)', async () => {
    const { fixture } = await render([{ entries: [entry({ linkId: 'L1' })], total: 1, hasMore: false }]);
    const page = fixture.componentInstance as unknown as {
      entries: () => GalleryEntry[];
      total: () => number;
      onDelete: (e: GalleryEntry) => Promise<void>;
    };

    // Declined: nothing happens.
    const declined = vi.spyOn(window, 'confirm').mockReturnValue(false);
    await page.onDelete.call(page, entry({ linkId: 'L1' }));
    expect(page.entries()).toHaveLength(1);
    declined.mockRestore();

    // Accepted: the row leaves and the counter drops.
    const accepted = vi.spyOn(window, 'confirm').mockReturnValue(true);
    await page.onDelete.call(page, entry({ linkId: 'L1' }));
    await settle(fixture);
    expect(page.entries()).toHaveLength(0);
    expect(page.total()).toBe(0);
    accepted.mockRestore();
  });
});

describe('PhotosPage detail-modal Download + Copy (v4 af1bc479)', () => {
  /**
   * These drive the REAL `core/download-utils` + `core/clipboard-utils` (ESM
   * exports cannot be `vi.spyOn`-ed, and mocking them would prove only that the
   * page called something). The anchor click and the `ClipboardItem` write are
   * intercepted instead, so the filename and the copied MIME are measured at
   * the browser boundary.
   */
  let clicked: { download: string; href: string } | null;
  let clipboardWrites: unknown[][];
  let clipboardFails: boolean;

  beforeEach(() => {
    (globalThis as unknown as { IntersectionObserver: unknown }).IntersectionObserver = class {
      observe(): void {}
      disconnect(): void {}
    };
    clicked = null;
    clipboardWrites = [];
    clipboardFails = false;
    (URL as unknown as { createObjectURL: unknown }).createObjectURL = () => 'blob:photo';
    (URL as unknown as { revokeObjectURL: unknown }).revokeObjectURL = () => {};
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (
      this: HTMLAnchorElement,
    ) {
      clicked = { download: this.download, href: this.href };
    });
    (globalThis as unknown as { ClipboardItem: unknown }).ClipboardItem = class {
      constructor(public readonly items: Record<string, Blob>) {}
    };
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      // `writable` matters: specs share a worker environment, and a
      // defineProperty default of writable:false leaves the NEXT spec to touch
      // `navigator.clipboard` throwing "Cannot assign to read only property" —
      // an ordering-dependent break that surfaces only when vitest happens to
      // shard the two files together (the rule `chat/tool-message.spec.ts`
      // already records; P4.D125's new spec files reshuffled the shards and
      // made this one fire against `chat/courier-bubble.spec.ts`).
      writable: true,
      value: {
        write: (items: unknown[]) => {
          if (clipboardFails) return Promise.reject(new Error('denied'));
          clipboardWrites.push(items);
          return Promise.resolve();
        },
      },
    });
  });
  afterEach(() => {
    vi.restoreAllMocks();
    TestBed.resetTestingModule();
  });

  /** Open the modal on the first card so the footer renders. */
  async function openModal(): Promise<ComponentFixture<PhotosPage>> {
    const { fixture } = await render([
      { entries: [entry({ linkId: 'L1', fileName: 'a.webp' })], total: 1, hasMore: false },
    ]);
    (
      fixture.componentInstance as unknown as { selected: { set: (e: GalleryEntry) => void } }
    ).selected.set(entry({ linkId: 'L1', fileName: 'a.webp' }));
    fixture.detectChanges();
    return fixture;
  }

  it('renders v4’s three-button footer row with its icons and title', async () => {
    const fixture = await openModal();
    const buttons = [...(fixture.nativeElement as HTMLElement).querySelectorAll('footer button')];
    expect(buttons.map((b) => (b.textContent ?? '').trim())).toEqual([
      'Remove from this album',
      'Download',
      'Copy',
      'Close',
    ]);
    expect(
      [...(fixture.nativeElement as HTMLElement).querySelectorAll('footer qt-icon')].map((i) =>
        i.getAttribute('name'),
      ),
    ).toEqual(['download', 'copy']);
    expect(buttons[2].getAttribute('title')).toBe('Copy image to clipboard');
  });

  it('Download fetches the displayed bytes and saves them under entry.fileName', async () => {
    const fixture = await openModal();
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue({ ok: true, blob: async () => new Blob(['bytes']) } as unknown as Response);

    const page = fixture.componentInstance as unknown as {
      onDownload: (e: GalleryEntry) => Promise<void>;
      downloading: () => boolean;
    };
    await page.onDownload.call(page, entry({ linkId: 'L1', fileName: 'a.webp' }));

    expect(fetchSpy).toHaveBeenCalledWith('/api/v1/mount-points/MP1/blobs/photos/a.webp');
    expect(clicked).toEqual({ download: 'a.webp', href: 'blob:photo' });
    // The busy latch is released again (v4's `finally`).
    expect(page.downloading()).toBe(false);
  });

  it('a non-ok response toasts v4’s sentence and downloads nothing', async () => {
    const fixture = await openModal();
    vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      ok: false,
      status: 404,
    } as unknown as Response);
    const err = vi.spyOn(TestBed.inject(ToastService), 'showError');

    const page = fixture.componentInstance as unknown as {
      onDownload: (e: GalleryEntry) => Promise<void>;
      downloading: () => boolean;
    };
    await page.onDownload.call(page, entry({ linkId: 'L1', fileName: 'a.webp' }));

    expect(clicked).toBeNull();
    expect(err).toHaveBeenCalledWith('Failed to download photo');
    expect(page.downloading()).toBe(false);
  });

  it('Copy writes an image/png ClipboardItem and toasts the success sentence', async () => {
    const fixture = await openModal();
    vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      ok: true,
      blob: async () => new Blob(['bytes'], { type: 'image/png' }),
    } as unknown as Response);
    const ok = vi.spyOn(TestBed.inject(ToastService), 'showSuccess');

    const page = fixture.componentInstance as unknown as {
      onCopy: (e: GalleryEntry) => Promise<void>;
    };
    await page.onCopy.call(page, entry({ linkId: 'L1' }));

    expect(clipboardWrites).toHaveLength(1);
    // v4's clipboard util only ever writes `image/png`.
    expect(Object.keys((clipboardWrites[0][0] as { items: Record<string, Blob> }).items)).toEqual([
      'image/png',
    ]);
    expect(ok).toHaveBeenCalledWith('Image copied to clipboard');
  });

  it('a refused clipboard write toasts v4’s failure sentence', async () => {
    const fixture = await openModal();
    clipboardFails = true;
    vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      ok: true,
      blob: async () => new Blob(['bytes'], { type: 'image/png' }),
    } as unknown as Response);
    const err = vi.spyOn(TestBed.inject(ToastService), 'showError');

    const page = fixture.componentInstance as unknown as {
      onCopy: (e: GalleryEntry) => Promise<void>;
    };
    await page.onCopy.call(page, entry({ linkId: 'L1' }));

    expect(err).toHaveBeenCalledWith('Failed to copy image to clipboard');
  });
});

describe('PhotosPage tab re-activation (P4.D90, v4 PhotosView.tsx:113-116)', () => {
  beforeEach(() => {
    (globalThis as unknown as { IntersectionObserver: unknown }).IntersectionObserver = class {
      observe(): void {}
      disconnect(): void {}
    };
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('reloads the first page silently — the grid never flips back to its loading state', async () => {
    const { client, calls } = stubClient([
      { entries: [entry({ linkId: 'L1', fileName: 'a.webp' })], total: 1, hasMore: false },
      { entries: [entry({ linkId: 'L2', fileName: 'b.webp' })], total: 1, hasMore: false },
    ]);
    const visible = signal(true);
    TestBed.configureTestingModule({
      imports: [PhotosPage],
      providers: [
        { provide: CoreClient, useValue: client },
        { provide: WORKSPACE_TAB_VISIBLE, useValue: visible },
        provideRouter([]),
        tanStack(),
      ],
    });
    const fixture = TestBed.createComponent(PhotosPage);
    fixture.detectChanges();
    await settle(fixture);
    expect(calls.length).toBe(1);

    const loading = () =>
      (fixture.componentInstance as unknown as { loading: () => boolean }).loading();

    // Leave the tab and come back.
    visible.set(false);
    fixture.detectChanges();
    expect(calls.length).toBe(1); // hiding refreshes nothing
    visible.set(true);
    fixture.detectChanges();
    // The refresh is in flight and the grid is still up: v4's `{ silent: true }`.
    expect(loading()).toBe(false);
    expect(fixture.nativeElement.textContent).toContain('a.webp');

    await settle(fixture);
    expect(calls.length).toBe(2);
    expect(fixture.nativeElement.textContent).toContain('b.webp');
  });
});

// === P4.D330: the album selector (v4 `680b71044`, `PhotosView.tsx` +122 / −24) ===
describe('PhotosPage — the album selector (v4 680b71044)', () => {
  beforeEach(() => {
    (globalThis as unknown as { IntersectionObserver: unknown }).IntersectionObserver = class {
      observe(): void {}
      disconnect(): void {}
    };
  });
  afterEach(() => {
    vi.restoreAllMocks();
    TestBed.resetTestingModule();
  });

  /** The server's order: characters, projects, document stores, General. */
  const ALBUMS: PhotoAlbumOption[] = [
    { mountPointId: 'mp-aria', name: 'Aria', kind: 'character', characterId: 'char-aria' },
    { mountPointId: 'mp-bertie', name: 'Bertie', kind: 'character', characterId: 'char-bertie' },
    { mountPointId: 'mp-quest', name: 'The Quest', kind: 'project' },
    { mountPointId: 'mp-notes', name: 'Field Notes', kind: 'document-store' },
    { mountPointId: 'mp-general', name: 'Quilltap General', kind: 'general' },
  ];
  const EMPTY: Page = { entries: [], total: 0, hasMore: false };

  function albumSelect(fixture: ComponentFixture<PhotosPage>): HTMLSelectElement {
    return (fixture.nativeElement as HTMLElement).querySelector(
      'select[aria-label="Choose which album to show"]',
    ) as HTMLSelectElement;
  }
  function choose(fixture: ComponentFixture<PhotosPage>, value: string): void {
    const select = albumSelect(fixture);
    select.value = value;
    select.dispatchEvent(new Event('change'));
  }
  const title = (fixture: ComponentFixture<PhotosPage>) =>
    (fixture.nativeElement as HTMLElement).querySelector('h1')!.textContent!.trim();
  const blurb = (fixture: ComponentFixture<PhotosPage>) =>
    (fixture.nativeElement as HTMLElement).querySelector('header p.qt-text-small')!.textContent!.trim();

  it('offers My Photos first, then one optgroup per kind in the server’s order', async () => {
    const { fixture, albumCalls } = await render([EMPTY], ALBUMS);
    expect(albumCalls).toEqual([{ type: 'photoGalleryAlbums' }]);
    const select = albumSelect(fixture);
    expect(select.className).toBe('qt-select w-auto min-w-[14rem]');
    const label = select.closest('label')!;
    expect(label.className).toBe('flex flex-wrap items-center gap-2 pt-1 text-sm');
    expect(label.querySelector('span')!.className).toBe('qt-text-muted');
    expect(label.querySelector('span')!.textContent).toBe('Album');
    expect(select.options[0].value).toBe('');
    expect(select.options[0].textContent!.trim()).toBe('My Photos');
    const groups = Array.from(select.querySelectorAll('optgroup')).map((g) => [
      g.label,
      Array.from(g.querySelectorAll('option')).map((o) => [o.value, o.textContent!.trim()]),
    ]);
    expect(groups).toEqual([
      ['Characters', [['mp-aria', 'Aria'], ['mp-bertie', 'Bertie']]],
      ['Projects', [['mp-quest', 'The Quest']]],
      ['Document stores', [['mp-notes', 'Field Notes']]],
      ['Quilltap General', [['mp-general', 'Quilltap General']]],
    ]);
  });

  it('My Photos with an empty albums answer: only the one option, the roll-up title and blurb', async () => {
    const { fixture } = await render([EMPTY]);
    expect(albumSelect(fixture).options).toHaveLength(1);
    expect(albumSelect(fixture).querySelectorAll('optgroup')).toHaveLength(0);
    expect(title(fixture)).toBe('My Photos');
    expect(blurb(fixture)).toBe(
      'Every image you’ve kept, gathered from each project album, Quilltap General, and your private uploads — listed once and linked to wherever it lives. A character’s own album is a separate cabinet; choose it above to look inside.',
    );
  });

  it('a character album titles itself “<name>’s Photos” with the vault blurb; a store album its own name', async () => {
    const { fixture } = await render([EMPTY], ALBUMS);
    choose(fixture, 'mp-aria');
    await settle(fixture);
    expect(title(fixture)).toBe('Aria’s Photos');
    expect(blurb(fixture)).toBe(
      'The pictures Aria keeps in their own vault — kept photographs and promoted portraits alike.',
    );
    choose(fixture, 'mp-quest');
    await settle(fixture);
    expect(title(fixture)).toBe('The Quest');
    expect(blurb(fixture)).toBe('The pictures filed in this store’s album.');
  });

  it('a list request carries `album` first, then `q`, `limit`, `offset` — and none for My Photos', async () => {
    const { fixture, calls } = await render([EMPTY], ALBUMS);
    expect(calls[0]).toEqual({ type: 'photoGalleryList', limit: 60, offset: 0 });
    const page = fixture.componentInstance as unknown as { queryText: string; onSearch: (e: Event) => void };
    page.queryText = ' dawn ';
    page.onSearch(new Event('submit'));
    await settle(fixture);
    choose(fixture, 'mp-aria');
    await settle(fixture);
    expect(calls).toHaveLength(3);
    expect(Object.keys(calls[2] as object)).toEqual(['type', 'album', 'q', 'limit', 'offset']);
    expect(calls[2]).toEqual({ type: 'photoGalleryList', album: 'mp-aria', q: 'dawn', limit: 60, offset: 0 });
  });

  it('changing the album clears the open photo and keeps the applied search (v4 trap 16)', async () => {
    const { fixture } = await render(
      [{ entries: [entry({ linkId: 'L1' })], total: 1, hasMore: false }, EMPTY],
      ALBUMS,
    );
    const page = fixture.componentInstance as unknown as {
      selected: { set: (e: GalleryEntry | null) => void; (): GalleryEntry | null };
      appliedQuery: { set: (s: string) => void; (): string };
    };
    page.appliedQuery.set('dawn');
    page.selected.set(entry({ linkId: 'L1' }));
    fixture.detectChanges();
    choose(fixture, 'mp-notes');
    await settle(fixture);
    expect(page.selected()).toBeNull();
    expect(page.appliedQuery()).toBe('dawn');
  });

  it('a load-more carries the album', async () => {
    const { fixture, calls } = await render(
      [EMPTY, { entries: [entry({ linkId: 'L1' })], total: 2, hasMore: true }, EMPTY],
      ALBUMS,
    );
    choose(fixture, 'mp-quest');
    await settle(fixture);
    const page = fixture.componentInstance as unknown as { loadMore: () => Promise<void> };
    await page.loadMore.call(page);
    expect(calls[2]).toEqual({ type: 'photoGalleryList', album: 'mp-quest', limit: 60, offset: 1 });
  });

  it('delete goes through the character door when the SELECTED album is a character’s (v4 trap 17)', async () => {
    const { fixture, calls } = await render(
      [EMPTY, { entries: [entry({ linkId: 'L1' })], total: 1, hasMore: false }],
      ALBUMS,
    );
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    choose(fixture, 'mp-aria');
    await settle(fixture);
    const page = fixture.componentInstance as unknown as { onDelete: (e: GalleryEntry) => Promise<void> };
    await page.onDelete.call(page, entry({ linkId: 'L1' }));
    expect(calls.at(-1)).toEqual({ type: 'characterPhotoRemove', characterId: 'char-aria', linkId: 'L1' });
  });

  it('delete takes the generic door in My Photos and in a store album', async () => {
    const { fixture, calls } = await render(
      [{ entries: [entry({ linkId: 'L1' })], total: 1, hasMore: false }, EMPTY],
      ALBUMS,
    );
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const page = fixture.componentInstance as unknown as { onDelete: (e: GalleryEntry) => Promise<void> };
    await page.onDelete.call(page, entry({ linkId: 'L1' }));
    expect(calls.at(-1)).toEqual({ type: 'photoGalleryEntryRemove', id: 'L1' });
    choose(fixture, 'mp-quest');
    await settle(fixture);
    await page.onDelete.call(page, entry({ linkId: 'L2' }));
    expect(calls.at(-1)).toEqual({ type: 'photoGalleryEntryRemove', id: 'L2' });
  });
});
// === end P4.D330 ===
