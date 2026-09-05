/**
 * The Guide tab and its three small components — asserted against v4
 * `HelpGuideTab.tsx`, `HelpCategorySection.tsx`, `HelpGuideSearch.tsx` and
 * `HelpWelcomeCard.tsx` at `d883a5ee1`.
 *
 * The load-bearing behaviours, in the order a reader meets them: the document
 * map is keyed by SLUG with `EXCLUDED_DOCUMENTS` filtered; the welcome card is
 * gated on `chatCount < 3` AND no active search; search runs at two speeds
 * (instant title filter, 200 ms-debounced server text search) with the server
 * hits TAGGED so a stale response cannot leak; and the reader's back stack
 * restores the scroll position of the topic you came from.
 *
 * The welcome card's copy is pinned against `__fixtures__/welcome-card.json`,
 * a capture of v4's real component — the Wodehouse register is not something to
 * paraphrase and not something a reviewer would catch drifting.
 */

import { ComponentFixture, TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { Subject } from 'rxjs';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

beforeAll(() => {
  const proto = globalThis.HTMLElement?.prototype as unknown as {
    scrollIntoView?: () => void;
    scrollTo?: () => void;
  };
  if (proto && !proto.scrollIntoView) proto.scrollIntoView = () => undefined;
  if (proto && !proto.scrollTo) proto.scrollTo = () => undefined;
});

import { HelpCategorySection } from './help-category-section';
import { HelpGuideTab, SEARCH_DEBOUNCE_MS, buildDocumentMap } from './help-guide-tab';
import { HelpNavigate } from './help-navigate';
import { HelpWelcomeCard, WELCOME_LINKS } from './help-welcome-card';
import { HelpApi, type HelpDocIndexRow, type HelpDocSearchMatch } from './help-wire';
import { HelpService } from './help.service';
import welcomeCard from './__fixtures__/welcome-card.json';

const DOCS: HelpDocIndexRow[] = [
  { id: 'd1', slug: 'chats', title: 'Chats', path: 'chats.md', url: '/salon' },
  {
    id: 'd2',
    slug: 'chat-settings',
    title: 'Chat Settings',
    path: 'chat-settings.md',
    url: '/settings?tab=chat',
  },
  { id: 'd3', slug: 'homepage', title: 'Homepage', path: 'homepage.md', url: '/' },
  { id: 'd4', slug: 'characters', title: 'Characters', path: 'characters.md', url: '/aurora' },
  // Excluded by EXCLUDED_DOCUMENTS — must never reach the Guide.
  { id: 'd5', slug: 'help-chat', title: 'Help Chat', path: 'help-chat.md', url: '/help' },
];

/* eslint-disable @typescript-eslint/no-explicit-any */
type AnyMock = ReturnType<typeof vi.fn<(...a: any[]) => any>>;
let docsList: AnyMock;
let docsChatCount: AnyMock;
let docsSearch: AnyMock;
let docGet: AnyMock;
let go: AnyMock;

function stubApi(chatCount: number | null, matches: HelpDocSearchMatch[] = []) {
  docsList = vi.fn(async () => DOCS);
  docsChatCount = vi.fn(async () => chatCount);
  docsSearch = vi.fn(async () => matches);
  docGet = vi.fn(async (id: string) => ({
    id,
    title: 'Doc ' + id,
    path: id + '.md',
    url: '/x',
    content: '# ' + id + '\n\nBody of ' + id + '.',
  }));
  // Delegate rather than hand the functions over directly: a case that swaps
  // `docGet` after `render()` must reach the component, and a captured
  // reference would silently keep the original (a spec bug that reads exactly
  // like a product bug).
  return {
    docsList: (...a: any[]) => docsList(...a),
    docsChatCount: (...a: any[]) => docsChatCount(...a),
    docsSearch: (...a: any[]) => docsSearch(...a),
    docGet: (...a: any[]) => docGet(...a),
  } as unknown as HelpApi;
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 8): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(
  pageUrl = '/salon',
  chatCount: number | null = 10,
  matches: HelpDocSearchMatch[] = [],
): Promise<ComponentFixture<HelpGuideTab>> {
  go = vi.fn();
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [HelpGuideTab],
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: HelpApi, useValue: stubApi(chatCount, matches) },
      { provide: HelpNavigate, useValue: { go } },
      { provide: Router, useValue: { events: new Subject().asObservable(), url: pageUrl } },
      {
        provide: HelpService,
        useValue: { currentPageUrl: () => pageUrl } as Partial<HelpService>,
      },
    ],
  });
  const fixture = TestBed.createComponent(HelpGuideTab);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

function headers(fixture: ComponentFixture<unknown>): string[] {
  return Array.from(
    (fixture.nativeElement as HTMLElement).querySelectorAll('.qt-help-guide-category-label'),
  ).map((el) => el.textContent?.trim() ?? '');
}

function topics(fixture: ComponentFixture<unknown>): string[] {
  return Array.from(
    (fixture.nativeElement as HTMLElement).querySelectorAll('.qt-help-guide-topic'),
  ).map((el) => el.textContent?.trim() ?? '');
}

async function type(fixture: ComponentFixture<HelpGuideTab>, value: string): Promise<void> {
  const input = (fixture.nativeElement as HTMLElement).querySelector(
    '.qt-help-guide-search-input',
  ) as HTMLInputElement;
  input.value = value;
  input.dispatchEvent(new Event('input'));
  fixture.detectChanges();
  // Outlast the real SEARCH_DEBOUNCE_MS — a `setTimeout(0)` settle returns long
  // before the server search fires, which makes every text-search case
  // vacuously "pass" by measuring the title filter alone.
  await new Promise((r) => setTimeout(r, SEARCH_DEBOUNCE_MS + 50));
  await settle(fixture);
}

afterEach(() => TestBed.resetTestingModule());

describe('HelpGuideTab — the document index', () => {
  it('keys documents by SLUG and drops EXCLUDED_DOCUMENTS', async () => {
    const fixture = await render();
    // `chats` and `chat-settings` live in the Chats category; `help-chat` is
    // excluded and must not appear anywhere.
    expect(text(fixture)).not.toContain('Help Chat');
    expect(topics(fixture)).toContain('Chats');
  });

  it('renders every category with no search, empty ones included', async () => {
    const fixture = await render();
    // v4's `filteredCategories` returns the categories UNFILTERED when the box
    // is empty, so a category whose slugs resolved to nothing still renders —
    // with a `(0)` badge. Only a search drops empty categories.
    expect(headers(fixture)).toHaveLength(11);
    const badges = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('.qt-help-guide-category-badge'),
    ).map((el) => el.textContent);
    expect(badges).toContain('(0)');
    expect(badges).toContain('(2)'); // Chats: chats + chat-settings
  });

  it('drops empty categories once a search is active', async () => {
    const fixture = await render();
    await type(fixture, 'chat');
    expect(headers(fixture).length).toBeLessThan(11);
  });

  it('auto-expands the category matching the current page', async () => {
    const fixture = await render('/aurora');
    // Characters is expanded; its topic is rendered. The others are collapsed.
    expect(topics(fixture)).toEqual(['Characters']);
  });

  it('expands nothing when the page matches no category', async () => {
    const fixture = await render('/unknown-page');
    expect(topics(fixture)).toEqual([]);
  });
});

describe('buildDocumentMap — the EXCLUDED_DOCUMENTS filter', () => {
  // Measured: no excluded slug appears in any HELP_CATEGORIES list, so an
  // excluded document has no category to render under either way, and a
  // mutation deleting the filter survives every rendering test. These two
  // cases are what give the filter a discriminator.
  it('drops an excluded slug', () => {
    const map = buildDocumentMap([
      { id: 'd1', slug: 'help-chat', title: 'Help Chat', path: 'help-chat.md', url: '/help' },
      { id: 'd2', slug: 'chats', title: 'Chats', path: 'chats.md', url: '/salon' },
    ]);
    expect(map.has('help-chat')).toBe(false);
    expect(map.has('chats')).toBe(true);
  });

  it('keys by SLUG, never by the database id', () => {
    const map = buildDocumentMap([
      { id: 'db-id-1', slug: 'chats', title: 'Chats', path: 'chats.md', url: '/salon' },
    ]);
    expect(map.has('db-id-1')).toBe(false);
    // ...and the entry's own `id` is the slug too — categories and in-document
    // links are keyed by slug throughout.
    expect(map.get('chats')).toEqual({ id: 'chats', title: 'Chats', url: '/salon' });
  });
});

describe('HelpGuideTab — the welcome card', () => {
  it('shows below three chats', async () => {
    const fixture = await render('/salon', 2);
    expect(text(fixture)).toContain('Welcome to Quilltap');
  });

  it('hides at three chats', async () => {
    const fixture = await render('/salon', 3);
    expect(text(fixture)).not.toContain('Welcome to Quilltap');
  });

  it('hides when the count is unknown', async () => {
    const fixture = await render('/salon', null);
    expect(text(fixture)).not.toContain('Welcome to Quilltap');
  });

  it('hides while a search is active, even below three chats', async () => {
    const fixture = await render('/salon', 1);
    await type(fixture, 'chat');
    expect(text(fixture)).not.toContain('Welcome to Quilltap');
  });

  it('opens a welcome link as a topic', async () => {
    const fixture = await render('/salon', 1);
    const link = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('.qt-help-welcome-link'),
    ).find((el) => el.textContent?.includes('Chats Overview')) as HTMLButtonElement;
    link.click();
    await settle(fixture);
    expect(docGet).toHaveBeenCalledWith('chats');
    expect(text(fixture)).toContain('Chats (The Salon)');
  });
});

describe('HelpWelcomeCard — byte parity with v4', () => {
  it('carries v4 four links, in order', () => {
    expect(WELCOME_LINKS.map((l) => l.docId)).toEqual([
      'homepage',
      'setup-wizard',
      'character-creation',
      'chats',
    ]);
  });

  it('renders v4 exact copy and labels', async () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [HelpWelcomeCard] });
    const fixture = TestBed.createComponent(HelpWelcomeCard);
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    // Compare the visible WORDS against v4's captured markup. The markup itself
    // legitimately differs (Angular's template emits whitespace between
    // elements where React's JSX does not), so tags collapse to a single space
    // on BOTH sides and runs of whitespace collapse after that — what survives
    // is the word sequence, which is the actual claim: v4's copy, verbatim.
    const strip = (s: string) =>
      s
        .replace(/<[^>]+>/g, ' ')
        .replace(/&#x27;/g, "'")
        .replace(/\s+/g, ' ')
        .trim();
    expect(strip(el.innerHTML)).toBe(strip(welcomeCard.html));
  });
});

describe('HelpGuideTab — search', () => {
  it('filters on TITLE instantly, without waiting on the server', async () => {
    const fixture = await render('/salon', 10);
    await type(fixture, 'Homepage');
    expect(topics(fixture)).toEqual(['Homepage']);
  });

  it('force-expands every surviving category while searching', async () => {
    const fixture = await render('/unknown-page', 10);
    expect(topics(fixture)).toEqual([]);
    await type(fixture, 'chat');
    expect(topics(fixture).length).toBeGreaterThan(0);
  });

  it('shows v4 empty state when nothing matches', async () => {
    const fixture = await render('/salon', 10);
    await type(fixture, 'zzzz-nothing');
    expect(text(fixture)).toContain('No topics match');
    expect(text(fixture)).toContain('zzzz-nothing');
  });

  it('does not hit the server below two characters', async () => {
    const fixture = await render('/salon', 10);
    await type(fixture, 'c');
    expect(docsSearch).not.toHaveBeenCalled();
  });

  it('adds documents the server matched on their PROSE, with the snippet', async () => {
    const fixture = await render('/salon', 10, [
      { slug: 'characters', titleHit: false, snippet: '…a turn of phrase…' },
    ]);
    // "phrase" matches no TITLE, so only the server hit can put Characters here.
    await type(fixture, 'phrase');
    expect(topics(fixture).join(' ')).toContain('Characters');
    expect(text(fixture)).toContain('…a turn of phrase…');
  });

  it('ignores hits tagged with a STALE query', async () => {
    // The tag is the whole point: a slow response for an earlier query must not
    // leak into the current filter.
    const fixture = await render('/salon', 10, [
      { slug: 'characters', titleHit: false, snippet: 'stale' },
    ]);
    await type(fixture, 'phrase');
    expect(topics(fixture).join(' ')).toContain('Characters');
    docsSearch.mockImplementation(async () => new Promise(() => undefined));
    await type(fixture, 'phrasey');
    // The in-flight query for "phrasey" has not answered; the "phrase" hits are
    // stale and must not survive into this filter.
    expect(topics(fixture).join(' ')).not.toContain('Characters');
  });
});

describe('HelpGuideTab — the reader and its back stack', () => {
  it('opens a topic and comes back to the list', async () => {
    const fixture = await render('/aurora');
    (
      (fixture.nativeElement as HTMLElement).querySelector('.qt-help-guide-topic') as HTMLElement
    ).click();
    await settle(fixture);
    expect(docGet).toHaveBeenCalledWith('characters');
    expect(text(fixture)).toContain('Characters (Aurora)');

    (
      (fixture.nativeElement as HTMLElement).querySelector('.qt-help-guide-back') as HTMLElement
    ).click();
    await settle(fixture);
    expect(topics(fixture)).toEqual(['Characters']);
  });

  it('a sibling .md link pushes the back stack; Back returns to it', async () => {
    const fixture = await render('/aurora');
    docGet.mockImplementation(async (id: string) => ({
      id,
      title: id,
      path: id + '.md',
      url: '/x',
      content: id === 'characters' ? 'See [more](chat-settings.md).' : '# Chat Settings',
    }));
    (
      (fixture.nativeElement as HTMLElement).querySelector('.qt-help-guide-topic') as HTMLElement
    ).click();
    await settle(fixture);

    (
      (fixture.nativeElement as HTMLElement).querySelector(
        '.qt-help-guide-doc-link',
      ) as HTMLElement
    ).click();
    await settle(fixture);
    expect(docGet).toHaveBeenCalledWith('chat-settings');
    // The category label follows the NEW document's category (v4).
    expect(text(fixture)).toContain('Chats (The Salon)');

    (
      (fixture.nativeElement as HTMLElement).querySelector('.qt-help-guide-back') as HTMLElement
    ).click();
    await settle(fixture);
    // Back goes to the document we came FROM, not out to the list. Assert the
    // READER is still up: the category list renders a "Characters (Aurora)"
    // label of its own, so a text match alone cannot tell the two apart — and
    // measurably does not (a mutation that never pushes the stack survived it).
    expect(
      (fixture.nativeElement as HTMLElement).querySelector('.qt-help-guide-reader'),
    ).not.toBeNull();
    expect(
      (fixture.nativeElement as HTMLElement).querySelector('.qt-help-guide-search-input'),
    ).toBeNull();
    expect(
      (fixture.nativeElement as HTMLElement).querySelector('.qt-help-guide-back')?.textContent,
    ).toContain('Characters (Aurora)');
    expect(docGet).toHaveBeenLastCalledWith('characters');
  });

  it('an in-document page link navigates instead of opening a topic', async () => {
    const fixture = await render('/aurora');
    // AFTER render — `render()` rebuilds the stubs, so a pre-assignment is
    // simply overwritten (and the case then measures the default document).
    docGet.mockImplementation(async (id: string) => ({
      id,
      title: id,
      path: id + '.md',
      url: '/x',
      content: '> **[Open this page in Quilltap](/settings?tab=chat)**',
    }));
    (
      (fixture.nativeElement as HTMLElement).querySelector('.qt-help-guide-topic') as HTMLElement
    ).click();
    await settle(fixture);
    (
      (fixture.nativeElement as HTMLElement).querySelector(
        '.qt-help-guide-page-link',
      ) as HTMLElement
    ).click();
    expect(go).toHaveBeenCalledWith('/settings?tab=chat');
  });

  it('surfaces a load failure in the reader', async () => {
    const fixture = await render('/aurora');
    docGet.mockImplementation(async () => null);
    (
      (fixture.nativeElement as HTMLElement).querySelector('.qt-help-guide-topic') as HTMLElement
    ).click();
    await settle(fixture);
    expect(text(fixture)).toContain('Document not found');
  });
});

describe('HelpCategorySection — ordering and the active topic', () => {
  async function renderSection(currentPageUrl: string, snippets?: Map<string, string | null>) {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [HelpCategorySection] });
    const fixture = TestBed.createComponent(HelpCategorySection);
    fixture.componentRef.setInput('label', 'Chats');
    fixture.componentRef.setInput('documents', [
      { id: 'a', title: 'Alpha', url: '/aurora' },
      { id: 'b', title: 'Beta', url: '/salon' },
      { id: 'c', title: 'Gamma', url: '/files' },
    ]);
    fixture.componentRef.setInput('currentPageUrl', currentPageUrl);
    fixture.componentRef.setInput('defaultExpanded', true);
    if (snippets) fixture.componentRef.setInput('snippets', snippets);
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  it('sorts the current page first, keeping the curated order otherwise', async () => {
    const fixture = await renderSection('/salon');
    expect(topics(fixture)).toEqual(['Beta', 'Alpha', 'Gamma']);
  });

  it('leaves the curated order alone when nothing matches', async () => {
    const fixture = await renderSection('/nowhere');
    expect(topics(fixture)).toEqual(['Alpha', 'Beta', 'Gamma']);
  });

  it('marks the active topic on an exact match or a child path', async () => {
    const fixture = await renderSection('/salon/abc');
    const active = (fixture.nativeElement as HTMLElement).querySelector(
      '.qt-help-guide-topic-active',
    );
    expect(active?.textContent?.trim()).toBe('Beta');
  });

  it('does NOT mark a topic whose url is a bare string prefix of the page', async () => {
    // v4's active test appends a trailing slash, so `/salon` lights only on
    // `/salon` itself or a `/salon/…` child — never on `/salonade`. Measured:
    // `/salon/abc` alone cannot tell the two rules apart (both match), so a
    // mutation dropping the slash survived until this case existed. The SORT
    // deliberately keeps the slash-less prefix, exactly as v4 does — only the
    // highlight is segment-aware.
    const fixture = await renderSection('/salonade');
    expect(
      (fixture.nativeElement as HTMLElement).querySelector('.qt-help-guide-topic-active'),
    ).toBeNull();
  });

  it('renders the snippet line and its wrapping class', async () => {
    const fixture = await renderSection('/nowhere', new Map([['a', 'matched prose here']]));
    const el = fixture.nativeElement as HTMLElement;
    expect(el.textContent).toContain('matched prose here');
    expect(el.querySelector('.qt-help-guide-topic.whitespace-normal')).not.toBeNull();
  });

  it('shows the document count in the header badge', async () => {
    const fixture = await renderSection('/nowhere');
    expect(
      (fixture.nativeElement as HTMLElement).querySelector('.qt-help-guide-category-badge')
        ?.textContent,
    ).toBe('(3)');
  });

  it('collapses and expands on click', async () => {
    const fixture = await renderSection('/nowhere');
    const header = (fixture.nativeElement as HTMLElement).querySelector(
      '.qt-help-guide-category-header',
    ) as HTMLElement;
    header.click();
    fixture.detectChanges();
    expect(topics(fixture)).toEqual([]);
    header.click();
    fixture.detectChanges();
    expect(topics(fixture)).toHaveLength(3);
  });
});
