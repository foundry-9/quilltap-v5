import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import type { CharacterChatSummary } from '../../../../core/core-contract';
import { CharacterConversationsTab } from './conversations-tab';

function chat(over: Partial<CharacterChatSummary>): CharacterChatSummary {
  return {
    id: 'chat-1',
    title: 'Tea at Midnight',
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-02T00:00:00.000Z',
    lastMessageAt: '2024-01-02T00:00:00.000Z',
    character: { id: 'c1', name: 'Jeeves' },
    project: null,
    storyBackground: null,
    messages: [
      {
        id: 'm1',
        role: 'ASSISTANT',
        content: 'Very good, sir.',
        createdAt: '2024-01-02T00:00:00.000Z',
      },
    ],
    tags: [],
    isDangerousChat: false,
    _count: { messages: 4, memories: 0 },
    scriptoriumStatus: 'none',
    ...over,
  };
}

interface DispatchReq {
  type: string;
  characterId?: string;
  search?: string;
  limit?: number;
  offset?: number;
  [k: string]: unknown;
}

/** A CoreClient stub that paginates + title-filters a fixed chat list. */
function stubClient(
  all: CharacterChatSummary[],
  onDispatch?: (req: DispatchReq) => void,
): Partial<CoreClient> {
  return {
    dispatchData: (async (req: DispatchReq) => {
      onDispatch?.(req);
      if (req.type === 'characterChats') {
        const filtered = req.search
          ? all.filter((c) => (c.title ?? '').toLowerCase().includes(req.search!.toLowerCase()))
          : all;
        const offset = req.offset ?? 0;
        const limit = req.limit ?? 10;
        return { chats: filtered.slice(offset, offset + limit), total: filtered.length };
      }
      return {};
    }) as CoreClient['dispatchData'],
  };
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 6): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(
  client: Partial<CoreClient>,
): Promise<ComponentFixture<CharacterConversationsTab>> {
  TestBed.configureTestingModule({
    imports: [CharacterConversationsTab],
    providers: [
      provideRouter([]),
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: client },
    ],
  });
  const fixture = TestBed.createComponent(CharacterConversationsTab);
  fixture.componentRef.setInput('characterId', 'c1');
  fixture.componentRef.setInput('characterName', 'Jeeves');
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

describe('CharacterConversationsTab', () => {
  it('shows the empty state naming the character when there are no chats', async () => {
    const fixture = await render(stubClient([]));
    expect(fixture.nativeElement.textContent).toContain('No conversations with Jeeves yet');
  });

  it('renders a card per chat with title, message count, and scriptorium badge', async () => {
    const fixture = await render(
      stubClient([
        chat({ id: 'chat-1', title: 'Tea at Midnight', _count: { messages: 7, memories: 2 } }),
      ]),
    );
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Tea at Midnight');
    expect(text).toContain('7');
    // memory badge
    expect(text).toContain('2');
    const scriptorium = fixture.nativeElement.querySelector('[title^="Scriptorium"]');
    expect(scriptorium).toBeTruthy();
  });

  it('links each card into the Salon conversation', async () => {
    const fixture = await render(stubClient([chat({ id: 'chat-42' })]));
    const link = fixture.nativeElement.querySelector('a.chat-card') as HTMLAnchorElement;
    expect(link.getAttribute('href')).toBe('/salon/chat-42');
  });

  it('marks a dangerous chat', async () => {
    const fixture = await render(stubClient([chat({ isDangerousChat: true })]));
    expect(fixture.nativeElement.querySelector('[title="Flagged as dangerous"]')).toBeTruthy();
  });

  it('paginates: a full page shows "Load more" and appends the next page', async () => {
    const many = Array.from({ length: 15 }, (_, i) =>
      chat({ id: `chat-${i}`, title: `Chat ${i}` }),
    );
    const fixture = await render(stubClient(many));
    // First page = 10 → load-more button present.
    let cards = fixture.nativeElement.querySelectorAll('qt-character-conversation-card');
    expect(cards.length).toBe(10);
    const loadMore = fixture.nativeElement.querySelector('.py-4 button') as HTMLButtonElement;
    expect(loadMore.textContent).toContain('Load more conversations');
    loadMore.click();
    await settle(fixture);
    cards = fixture.nativeElement.querySelectorAll('qt-character-conversation-card');
    expect(cards.length).toBe(15);
    // Tail page had 5 (< 10) → "No more conversations".
    expect(fixture.nativeElement.textContent).toContain('No more conversations');
  });

  it('debounces the search box and re-queries with the term', async () => {
    const seen: DispatchReq[] = [];
    const fixture = await render(
      stubClient(
        [chat({ id: 'a', title: 'Tea Party' }), chat({ id: 'b', title: 'War Council' })],
        (r) => seen.push(r),
      ),
    );
    expect(fixture.nativeElement.querySelectorAll('qt-character-conversation-card').length).toBe(2);

    const input = fixture.nativeElement.querySelector('input[type="text"]') as HTMLInputElement;
    input.value = 'war';
    input.dispatchEvent(new Event('input'));
    // Wait past the 300ms debounce, then let the query settle.
    await new Promise((r) => setTimeout(r, 350));
    await settle(fixture);

    expect(seen.some((r) => r.type === 'characterChats' && r.search === 'war')).toBe(true);
    const cards = fixture.nativeElement.querySelectorAll('qt-character-conversation-card');
    expect(cards.length).toBe(1);
    expect(fixture.nativeElement.textContent).toContain('War Council');
  });
});
