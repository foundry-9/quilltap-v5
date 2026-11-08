import { provideRouter } from '@angular/router';
import { TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ConversationSearchSource } from '../chat/conversation-search';
import { PageToolbarService } from '../layout/page-toolbar.service';
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
        'input[placeholder="Search chats, characters, messages, documents, tags, memories..."]',
      ),
    ).not.toBeNull();
  });
});

/**
 * P4.D384 — v4 `4f5047241`: in a Salon chat the bar searches the focused
 * conversation first (the toolbar service's source), paints the query on it,
 * and gains the clear button and the two-stage Escape.
 */
describe('SearchBar — This conversation (v4 4f5047241)', () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.useFakeTimers();
    fetchMock = vi.fn(async () =>
      response({ results: [AIRSHIP_RESULT], countsByType: { characters: 1 }, totalCount: 1 }),
    );
    vi.stubGlobal('fetch', fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  function source() {
    return {
      chatId: 'chat-1',
      search: vi.fn<ConversationSearchSource['search']>(() => ({
        matches: [
          {
            messageId: 'msg-1',
            speaker: 'Abigail',
            isAnnouncement: false,
            snippet: 'the airship at dawn',
            occurrences: 1,
            createdAt: '2026-10-29T14:05:00.000Z',
          },
        ],
        total: 3,
      })),
      reveal: vi.fn<(messageId: string) => void>(),
      setHighlight: vi.fn<(query: string | null) => void>(),
    } satisfies ConversationSearchSource;
  }

  function render(conversation: ConversationSearchSource | null) {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [SearchBar],
      providers: [provideRouter([{ path: '**', children: [] }])],
    });
    TestBed.inject(PageToolbarService).setConversationSearch(conversation);
    const fixture = TestBed.createComponent(SearchBar);
    fixture.detectChanges();
    return fixture;
  }

  const inputOf = (fixture: ReturnType<typeof render>) =>
    fixture.nativeElement.querySelector('input[aria-label="Search"]') as HTMLInputElement;

  function type(fixture: ReturnType<typeof render>, value: string): HTMLInputElement {
    const input = inputOf(fixture);
    input.value = value;
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    return input;
  }

  function key(fixture: ReturnType<typeof render>, k: string): void {
    inputOf(fixture).dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true }));
    fixture.detectChanges();
  }

  const dropdown = (fixture: ReturnType<typeof render>) =>
    fixture.nativeElement.querySelector('.z-50') as HTMLElement | null;

  it('the placeholder names the chat only while a conversation is offered', () => {
    expect(inputOf(render(null)).getAttribute('placeholder')).toBe('Search... (⌘K)');
    const fixture = render(source());
    expect(inputOf(fixture).getAttribute('placeholder')).toBe('Search this chat & all… (⌘K)');
    // The slot empties → the plain placeholder again.
    TestBed.inject(PageToolbarService).setConversationSearch(null);
    fixture.detectChanges();
    expect(inputOf(fixture).getAttribute('placeholder')).toBe('Search... (⌘K)');
  });

  it('the input is pr-8 and a Clear search button appears with a query', () => {
    const fixture = render(null);
    const input = inputOf(fixture);
    expect(input.className).toContain('pr-8');
    expect(input.className).not.toContain('pr-3');
    expect(fixture.nativeElement.querySelector('button[aria-label="Clear search"]')).toBeNull();
    type(fixture, 'air');
    const clear = fixture.nativeElement.querySelector('button[aria-label="Clear search"]') as HTMLButtonElement;
    expect(clear.getAttribute('type')).toBe('button');
    expect(clear.className).toBe(
      'absolute right-2 top-1/2 -translate-y-1/2 p-0.5 qt-text-secondary hover:text-foreground',
    );
    expect(clear.querySelector('[data-icon]')!.getAttribute('data-icon')).toBe('close');
    expect([...clear.querySelector('[data-icon]')!.classList].sort()).toEqual(['h-4', 'qt-icon', 'w-4']);
  });

  it('Clear search empties the box, cancels the pending search, and refocuses', async () => {
    const fixture = render(null);
    type(fixture, 'airship');
    const clear = fixture.nativeElement.querySelector('button[aria-label="Clear search"]') as HTMLButtonElement;
    clear.click();
    fixture.detectChanges();
    expect(inputOf(fixture).value).toBe('');
    expect(document.activeElement).toBe(inputOf(fixture));
    await vi.advanceTimersByTimeAsync(500);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('asks the conversation for 5 matches on the TRIMMED query and paints it', () => {
    const src = source();
    const fixture = render(src);
    type(fixture, '  airship ');
    expect(src.search).toHaveBeenLastCalledWith('airship', 5);
    expect(src.setHighlight).toHaveBeenLastCalledWith('airship');
    type(fixture, 'a');
    expect(src.setHighlight).toHaveBeenLastCalledWith(null);
    fixture.destroy();
    expect(src.setHighlight).toHaveBeenLastCalledWith(null);
  });

  it('withdrawing the source clears its highlight', () => {
    const src = source();
    const fixture = render(src);
    type(fixture, 'airship');
    src.setHighlight.mockClear();
    TestBed.inject(PageToolbarService).setConversationSearch(null);
    fixture.detectChanges();
    TestBed.tick();
    expect(src.setHighlight).toHaveBeenCalledWith(null);
  });

  it('the dropdown is qt-bg-popover, with the matches group and an Everywhere divider ABOVE the global results', async () => {
    const fixture = render(source());
    type(fixture, 'airship');
    await vi.advanceTimersByTimeAsync(300);
    await flush();
    fixture.detectChanges();
    const dd = dropdown(fixture)!;
    expect(dd.className).toContain('qt-bg-popover');
    expect(dd.className).not.toContain('bg-background');
    const group = dd.querySelector('[aria-label="Matches in this conversation"]') as HTMLElement;
    expect(group.textContent).toContain('This conversation (1/3)');
    // v5 wraps the group in its component host; the divider is the host's sibling.
    const divider = group.closest('qt-conversation-matches')!.nextElementSibling as HTMLElement;
    expect(divider.className).toBe(
      'px-3 pt-2 pb-1 border-t qt-border-default qt-text-xs font-semibold uppercase tracking-wider',
    );
    expect((divider.textContent ?? '').trim()).toBe('Everywhere');
    const link = dd.querySelector('a[href="/characters/c1"]') as HTMLElement;
    expect(divider.compareDocumentPosition(link) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('without a conversation there is no group and no divider', async () => {
    const fixture = render(null);
    type(fixture, 'airship');
    await vi.advanceTimersByTimeAsync(300);
    await flush();
    fixture.detectChanges();
    expect(dropdown(fixture)!.querySelector('[aria-label="Matches in this conversation"]')).toBeNull();
    expect(dropdown(fixture)!.textContent).not.toContain('Everywhere');
  });

  it('a conversation pick reveals and closes the dropdown but KEEPS the query', () => {
    const src = source();
    const fixture = render(src);
    type(fixture, 'airship');
    (dropdown(fixture)!.querySelector('[role="button"]') as HTMLElement).click();
    fixture.detectChanges();
    expect(src.reveal).toHaveBeenCalledWith('msg-1');
    expect(dropdown(fixture)).toBeNull();
    expect(inputOf(fixture).value).toBe('airship');
    expect(src.setHighlight).toHaveBeenLastCalledWith('airship');
  });

  it('Escape is two-stage: close the dropdown, then clear the query, then blur', () => {
    const src = source();
    const fixture = render(src);
    const input = type(fixture, 'airship');
    input.focus();
    expect(dropdown(fixture)).not.toBeNull();
    key(fixture, 'Escape');
    expect(dropdown(fixture)).toBeNull();
    expect(input.value).toBe('airship');
    expect(document.activeElement).toBe(input);
    key(fixture, 'Escape');
    expect(input.value).toBe('');
    expect(src.setHighlight).toHaveBeenLastCalledWith(null);
    expect(document.activeElement).toBe(input);
    key(fixture, 'Escape');
    expect(document.activeElement).not.toBe(input);
  });

  it('the mobile button clears the query first, then opens the dialog with the conversation', () => {
    const src = source();
    const fixture = render(src);
    type(fixture, 'airship');
    const mobile = fixture.nativeElement.querySelector('button.md\\:hidden') as HTMLButtonElement;
    mobile.click();
    fixture.detectChanges();
    expect(inputOf(fixture).value).toBe('');
    expect(src.setHighlight).toHaveBeenLastCalledWith(null);
    // The dialog (portaled to body) offers the chip.
    const chip = Array.from(document.querySelectorAll('button.qt-filter-chip-active')).find(
      (b) => (b.textContent ?? '').trim() === 'This conversation',
    );
    expect(chip).toBeDefined();
  });

  it("a dialog pick carries the dialog's query back to the bar and closes the dropdown", () => {
    const src = source();
    const fixture = render(src);
    (fixture.nativeElement.querySelector('button.md\\:hidden') as HTMLButtonElement).click();
    fixture.detectChanges();
    const dialogInput = document.querySelector(
      'input[placeholder="Search chats, characters, messages, documents, tags, memories..."]',
    ) as HTMLInputElement;
    dialogInput.value = ' airship ';
    dialogInput.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    const row = document.querySelector('[role="dialog"] [role="button"]') as HTMLElement;
    row.click();
    fixture.detectChanges();
    expect(src.reveal).toHaveBeenCalledWith('msg-1');
    expect(inputOf(fixture).value).toBe('airship');
    expect(dropdown(fixture)).toBeNull();
    expect(document.querySelector('[role="dialog"]')).toBeNull();
    expect(src.setHighlight).toHaveBeenLastCalledWith('airship');
  });
});
