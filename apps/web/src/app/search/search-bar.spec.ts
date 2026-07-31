import { provideRouter } from '@angular/router';
import { TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { SearchBar } from './search-bar';
import type { SearchResponse } from './search.types';

function response(partial: Partial<SearchResponse>): Response {
  const body: SearchResponse = {
    results: [],
    totalCount: 0,
    query: '',
    types: [],
    hasMore: false,
    countsByType: {},
    ...partial,
  };
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
}

const AIRSHIP_RESULT = {
  id: 'c1',
  type: 'characters' as const,
  name: 'Airship Zara',
  matchedField: 'name',
  matchedValue: 'Airship Zara',
  snippet: 'Airship Zara',
  url: '/aurora/c1',
  matchPriority: 1 as const,
  createdAt: '2026-01-01T00:00:00.000Z',
  updatedAt: '2026-01-01T00:00:00.000Z',
  title: null,
  isFavorite: false,
};

async function flush(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}

describe('SearchBar (v4 search-bar.tsx)', () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.useFakeTimers();
    fetchMock = vi.fn(async () => response({}));
    vi.stubGlobal('fetch', fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  function render() {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [SearchBar],
      // A wildcard sink so result-click navigations resolve in the test router.
      providers: [provideRouter([{ path: '**', children: [] }])],
    });
    const fixture = TestBed.createComponent(SearchBar);
    fixture.detectChanges();
    return fixture;
  }

  function type(fixture: ReturnType<typeof render>, value: string): HTMLInputElement {
    const input = fixture.nativeElement.querySelector('input') as HTMLInputElement;
    input.value = value;
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    return input;
  }

  it('debounces 300ms and calls the inline endpoint with q + limit=20', async () => {
    fetchMock.mockImplementation(async () =>
      response({ results: [AIRSHIP_RESULT], countsByType: { characters: 1 }, totalCount: 1 }),
    );
    const fixture = render();
    type(fixture, 'airship');
    expect(fetchMock).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(299);
    expect(fetchMock).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    await flush();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(String(fetchMock.mock.calls[0][0])).toBe('/api/v1/ui/search?q=airship&limit=20');
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Airship Zara');
  });

  it('a sub-2-char query never fetches and renders no dropdown (v4 gate: length >= 2 || hasSearched)', async () => {
    const fixture = render();
    type(fixture, 'a');
    await vi.advanceTimersByTimeAsync(500);
    expect(fetchMock).not.toHaveBeenCalled();
    expect(fixture.nativeElement.querySelector('.z-50')).toBeNull();
  });

  it('a result click closes the dropdown and clears the query (v4 handleResultClick)', async () => {
    fetchMock.mockImplementation(async () =>
      response({ results: [AIRSHIP_RESULT], countsByType: { characters: 1 }, totalCount: 1 }),
    );
    const fixture = render();
    type(fixture, 'airship');
    await vi.advanceTimersByTimeAsync(300);
    await flush();
    fixture.detectChanges();
    const link = fixture.nativeElement.querySelector('a[href="/characters/c1"]') as HTMLElement;
    expect(link).not.toBeNull();
    link.click();
    fixture.detectChanges();
    const input = fixture.nativeElement.querySelector('input') as HTMLInputElement;
    expect(input.value).toBe('');
    expect(fixture.nativeElement.querySelector('.z-50')).toBeNull();
  });

  it('"See all results →" opens the dialog seeded with the query', async () => {
    fetchMock.mockImplementation(async () =>
      response({ results: [AIRSHIP_RESULT], countsByType: { characters: 1 }, totalCount: 1 }),
    );
    const fixture = render();
    type(fixture, 'airship');
    await vi.advanceTimersByTimeAsync(300);
    await flush();
    fixture.detectChanges();
    const buttons = Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLElement[];
    const seeAll = buttons.find((b) => b.textContent?.includes('See all results'));
    expect(seeAll).toBeDefined();
    seeAll!.click();
    await vi.advanceTimersByTimeAsync(300);
    await flush();
    fixture.detectChanges();
    // The dialog is open (its placeholder input exists) and ran the seeded search.
    // Queried from `document`, not the fixture: SearchDialog portals its host to
    // document.body so its fixed-position backdrop escapes the toolbar's
    // `backdrop-filter` containing block (dogfood #45).
    expect(
      document.querySelector(
        'input[placeholder="Search chats, characters, messages, tags, memories..."]',
      ),
    ).not.toBeNull();
  });
});
