import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
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

/** A CoreClient stub whose list responses are scripted per call. */
function stubClient(pages: Page[]): { client: Partial<CoreClient>; calls: unknown[] } {
  const calls: unknown[] = [];
  let i = 0;
  const client = {
    dispatchData: (req: unknown) => {
      calls.push(req);
      const page = pages[Math.min(i++, pages.length - 1)];
      return Promise.resolve(page as unknown as Record<string, unknown>);
    },
  } as Partial<CoreClient>;
  return { client, calls };
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 8): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(pages: Page[]): Promise<{
  fixture: ComponentFixture<PhotosPage>;
  calls: unknown[];
}> {
  const { client, calls } = stubClient(pages);
  TestBed.configureTestingModule({
    imports: [PhotosPage],
    providers: [{ provide: CoreClient, useValue: client }, provideRouter([])],
  });
  const fixture = TestBed.createComponent(PhotosPage);
  fixture.detectChanges();
  await settle(fixture);
  return { fixture, calls };
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
      providers: [{ provide: CoreClient, useValue: client }, provideRouter([])],
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
