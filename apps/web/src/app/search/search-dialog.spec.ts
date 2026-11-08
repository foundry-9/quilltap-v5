import { provideRouter } from '@angular/router';
import { TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ConversationSearchSource } from '../chat/conversation-search';
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

  it('the panel announces as a modal dialog named "Search", and the field is labelled (P4.D358)', async () => {
    const fixture = render('');
    await flush();
    const host = fixture.nativeElement as HTMLElement;
    const dlg = host.querySelector('[role="dialog"]') as HTMLElement;
    // v4 `4f5047241`: the panel moved off `bg-background` onto the popover surface.
    expect(dlg.classList.contains('qt-bg-popover')).toBe(true);
    expect(dlg.classList.contains('bg-background')).toBe(false);
    expect(dlg.getAttribute('aria-modal')).toBe('true');
    expect(dlg.getAttribute('tabindex')).toBe('-1');
    expect(dlg.getAttribute('aria-label')).toBe('Search');
    const input = dlg.querySelector('input[type="text"]') as HTMLInputElement;
    expect(input.getAttribute('aria-label')).toBe('Search');
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

/**
 * P4.D384 — v4 `4f5047241` (+ `94f041764` / `29ba4d3f8`): the "This
 * conversation" filter. With a Salon's source passed in, a chip BEFORE the type
 * chips (on by default, off when opened on one type) lists up to 50 in-chat
 * matches above an `Everywhere` divider; picking one hands the trimmed query
 * back, reveals the message, and closes.
 */
describe('SearchDialog — This conversation (v4 4f5047241)', () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.useFakeTimers();
    fetchMock = vi.fn(async () =>
      response({
        results: [
          {
            id: 'c1',
            type: 'characters',
            name: 'Fine Fellow',
            matchedField: 'name',
            matchedValue: 'Fine Fellow',
            snippet: 'Fine Fellow',
            url: '/aurora/c1',
            matchPriority: 1,
            createdAt: '2026-01-01T00:00:00.000Z',
            updatedAt: '2026-01-01T00:00:00.000Z',
            title: null,
            isFavorite: false,
          },
        ],
        totalCount: 1,
        countsByType: { characters: 1 },
      }),
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
            messageId: 'msg-9',
            speaker: 'Abigail',
            isAnnouncement: false,
            snippet: 'a fine morning',
            occurrences: 1,
            createdAt: '2026-10-29T14:05:00.000Z',
          },
        ],
        total: 1,
      })),
      reveal: vi.fn<(messageId: string) => void>(),
      setHighlight: vi.fn<(query: string | null) => void>(),
    } satisfies ConversationSearchSource;
  }

  function render(
    conversation: ConversationSearchSource | null,
    initialQuery = '',
    initialTypes?: SearchType[],
  ) {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [SearchDialog],
      providers: [provideRouter([{ path: '**', children: [] }])],
    });
    const fixture = TestBed.createComponent(SearchDialog);
    fixture.componentRef.setInput('isOpen', true);
    fixture.componentRef.setInput('initialQuery', initialQuery);
    fixture.componentRef.setInput('initialTypes', initialTypes);
    fixture.componentRef.setInput('conversation', conversation);
    const events: string[] = [];
    fixture.componentInstance.close.subscribe(() => events.push('close'));
    fixture.componentInstance.conversationReveal.subscribe((q) => events.push(`reveal-query:${q}`));
    fixture.detectChanges();
    return { fixture, host: fixture.nativeElement as HTMLElement, events };
  }

  const chips = (host: HTMLElement) =>
    Array.from(
      host.querySelectorAll<HTMLButtonElement>('button.qt-filter-chip, button.qt-filter-chip-active'),
    );

  it('without a conversation there is no chip, and every type chip carries aria-pressed', async () => {
    const { host } = render(null);
    await flush();
    const all = chips(host);
    expect(all.map((b) => (b.textContent ?? '').trim())).not.toContain('This conversation');
    expect(all).toHaveLength(6);
    for (const b of all) {
      expect(b.getAttribute('type')).toBe('button');
      expect(b.getAttribute('aria-pressed')).toBe('true');
    }
    expect(host.querySelector('[aria-label="Matches in this conversation"]')).toBeNull();
  });

  it('the chip comes FIRST, pressed by default; a deselected type chip reads aria-pressed=false', async () => {
    const { fixture, host } = render(source(), 'fine');
    await flush();
    const all = chips(host);
    expect((all[0].textContent ?? '').trim()).toBe('This conversation');
    expect(all[0].getAttribute('type')).toBe('button');
    expect(all[0].getAttribute('aria-pressed')).toBe('true');
    expect(all[0].className).toBe('qt-filter-chip-active');
    all[1].click(); // Chats off
    fixture.detectChanges();
    expect(all[1].getAttribute('aria-pressed')).toBe('false');
  });

  it('lists up to 50 in-chat matches above an Everywhere divider and the global results', async () => {
    const src = source();
    const { fixture, host } = render(src, '  fine ');
    // Let the seeded global search's fetch → json() → signal writes settle.
    for (let i = 0; i < 10; i++) await flush();
    fixture.detectChanges();
    expect(src.search).toHaveBeenCalledWith('fine', 50);
    const scroll = host.querySelector('.max-h-\\[60vh\\]') as HTMLElement;
    const group = scroll.querySelector('[aria-label="Matches in this conversation"]') as HTMLElement;
    expect(group).not.toBeNull();
    // v5 wraps the group in its component host; the divider is the host's sibling.
    const divider = group.closest('qt-conversation-matches')!.nextElementSibling as HTMLElement;
    expect(divider.className).toBe(
      'px-3 pt-2 pb-1 border-t qt-border-default qt-text-xs font-semibold uppercase tracking-wider',
    );
    expect((divider.textContent ?? '').trim()).toBe('Everywhere');
    // The global results render AFTER the divider.
    const fellow = Array.from(scroll.querySelectorAll('a')).find((a) =>
      (a.textContent ?? '').includes('Fine Fellow'),
    )!;
    expect(fellow).toBeDefined();
    expect(divider.compareDocumentPosition(fellow) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('a one-character query asks the source nothing', async () => {
    const src = source();
    render(src, 'f');
    await flush();
    expect(src.search).not.toHaveBeenCalled();
  });

  it('toggling the chip off hides the group (and stops asking)', async () => {
    const src = source();
    const { fixture, host } = render(src, 'fine');
    await flush();
    fixture.detectChanges();
    chips(host)[0].click();
    fixture.detectChanges();
    expect(chips(host)[0].getAttribute('aria-pressed')).toBe('false');
    expect(chips(host)[0].className).toBe('qt-filter-chip');
    expect(host.querySelector('[aria-label="Matches in this conversation"]')).toBeNull();
    expect((host.textContent ?? '')).not.toContain('Everywhere');
  });

  it('opened on one type, the chip starts OFF', async () => {
    const src = source();
    const { host } = render(src, 'fine', ['characters']);
    await flush();
    expect(chips(host)[0].getAttribute('aria-pressed')).toBe('false');
    expect(host.querySelector('[aria-label="Matches in this conversation"]')).toBeNull();
    expect(src.search).not.toHaveBeenCalled();
  });

  it('picking a match hands back the TRIMMED query, reveals it, then closes', async () => {
    const src = source();
    const order: string[] = [];
    src.reveal.mockImplementation((id: string) => order.push(`reveal:${id}`));
    const { fixture, host, events } = render(src, ' fine  ');
    await flush();
    fixture.detectChanges();
    fixture.componentInstance.conversationReveal.subscribe(() => order.push('handback'));
    fixture.componentInstance.close.subscribe(() => order.push('close'));
    (host.querySelector('[role="button"]') as HTMLElement).click();
    expect(events).toEqual(['reveal-query:fine', 'close']);
    expect(src.reveal).toHaveBeenCalledWith('msg-9');
    expect(order).toEqual(['handback', 'reveal:msg-9', 'close']);
  });
});
