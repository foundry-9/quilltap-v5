import { provideRouter } from '@angular/router';
import { TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { SearchDialog } from './search-dialog';
import type { SearchResponse, SearchType } from './search.types';

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

async function flush(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}

/**
 * The §3 unification-review regression pin for the open-seeding effect: it must
 * register on [isOpen, initialQuery, initialTypes] ONLY (v4
 * search-dialog.tsx:110-140). Reading `selectedTypes()` inside it made every
 * later chip toggle re-run the seeding — snapping the chips back to the initial
 * types, clobbering the typed query, and re-firing the initial search.
 */
describe('SearchDialog (v4 search-dialog.tsx)', () => {
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

  function render(initialQuery: string, initialTypes?: SearchType[]) {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [SearchDialog],
      providers: [provideRouter([{ path: '**', children: [] }])],
    });
    const fixture = TestBed.createComponent(SearchDialog);
    fixture.componentRef.setInput('isOpen', true);
    fixture.componentRef.setInput('initialQuery', initialQuery);
    fixture.componentRef.setInput('initialTypes', initialTypes);
    fixture.detectChanges();
    return fixture;
  }

  function chipButtons(fixture: ReturnType<typeof render>): HTMLButtonElement[] {
    const all = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll<HTMLButtonElement>('button'),
    );
    return all.filter((b) => /^(Chats|Characters|Messages|Tags|Memories)$/.test((b.textContent ?? '').trim()));
  }

  it('a chip toggle after an initial-query open sticks: the seeding effect must not track selectedTypes', async () => {
    const fixture = render('airship');
    await flush();
    expect(fetchMock).toHaveBeenCalledTimes(1); // the initial search

    const chips = chipButtons(fixture);
    expect(chips.length).toBe(5);
    const chats = chips.find((b) => (b.textContent ?? '').trim() === 'Chats')!;
    chats.click();
    fixture.detectChanges();
    await flush();

    // The toggle deselects Chats and re-searches with the remaining four types
    // — it must NOT re-run the open seeding (which would snap all five back on,
    // reset the query to the initial one, and re-fire the initial search).
    const searchCalls = fetchMock.mock.calls.map((c) => String(c[0]));
    expect(searchCalls.length).toBe(2);
    // The sixth type (P4.D122) joins the CSV in ALL_SEARCH_TYPES order.
    expect(searchCalls[1]).toContain('types=characters,messages,documents,tags,memories');
    expect(chats.className).toBe('qt-filter-chip');
  });

  it('a type-count open (one initial type) allows widening to another chip', async () => {
    const fixture = render('airship', ['characters']);
    await flush();
    expect(fetchMock).toHaveBeenCalledTimes(1);

    const chips = chipButtons(fixture);
    const memories = chips.find((b) => (b.textContent ?? '').trim() === 'Memories')!;
    memories.click();
    fixture.detectChanges();
    await flush();

    const searchCalls = fetchMock.mock.calls.map((c) => String(c[0]));
    expect(searchCalls.length).toBe(2);
    expect(searchCalls[1]).toContain('types=characters,memories');
  });
});

/**
 * The P4.D122 chip list + copy strings (v4 `search-dialog.tsx`, `b220999d`).
 * v4 stopped keeping a local `ALL_TYPES` copy and reads the shared constant, so
 * the chips and the route's accepted `types` can't drift; the two copy strings
 * gained `documents` in v4's exact positions.
 */
describe('SearchDialog — the Documents chip (v4 b220999d)', () => {
  it('renders every type chip in ALL_SEARCH_TYPES order, with the new copy', () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [SearchDialog],
      providers: [provideRouter([])],
    });
    const fixture = TestBed.createComponent(SearchDialog);
    fixture.componentRef.setInput('isOpen', true);
    fixture.detectChanges();

    const host = fixture.nativeElement as HTMLElement;
    const chips = Array.from(
      host.querySelectorAll('button.qt-filter-chip, button.qt-filter-chip-active'),
    ).map((b) => (b.textContent ?? '').trim());
    // chats, characters, messages, documents, tags, memories — the order v4's
    // constant declares, which is NOT the server's fan-out order.
    expect(chips).toEqual([
      'Chats',
      'Characters',
      'Messages',
      'Documents',
      'Tags',
      'Memories',
    ]);

    const input = host.querySelector('input[type="text"]') as HTMLInputElement;
    expect(input.getAttribute('placeholder')).toBe(
      'Search chats, characters, messages, documents, tags, memories...',
    );
    expect(host.textContent).toContain(
      'Search across your chats, characters, messages, documents, tags, and memories',
    );
  });
});
