import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { EnrichedChatSummary } from '../../core/core-contract';
import { SalonList } from './salon-list';

function chat(over: Partial<EnrichedChatSummary>): EnrichedChatSummary {
  return {
    id: 'c1',
    title: 'A Chat',
    contextSummary: null,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-02T00:00:00.000Z',
    lastMessageAt: null,
    participants: [],
    tags: [],
    project: null,
    storyBackground: null,
    conciergeState: 'monitored',
    dangerCategories: [],
    chatType: 'salon',
    scriptoriumStatus: 'none',
    _count: { messages: 3, memories: 0 },
    ...over,
  };
}

function stubClient(chats: EnrichedChatSummary[]): Partial<CoreClient> {
  return {
    dispatchExpect: (async () => ({ type: 'chats', data: chats })) as CoreClient['dispatchExpect'],
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<SalonList>> {
  TestBed.configureTestingModule({
    imports: [SalonList],
    providers: [
      provideRouter([]),
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: client },
    ],
  });
  const fixture = TestBed.createComponent(SalonList);
  fixture.detectChanges();
  // Let the TanStack query's async settle (zoneless whenStable doesn't track it).
  for (let i = 0; i < 5; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

describe('SalonList', () => {
  it('renders the v4-verbatim heading', async () => {
    const fixture = await render(stubClient([chat({})]));
    expect(fixture.nativeElement.querySelector('h1').textContent).toContain('Chats');
  });

  it('shows the empty state when there are no chats', async () => {
    const fixture = await render(stubClient([]));
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('No chats yet');
    expect(text).toContain('Start a new chat');
  });

  it('renders a card per chat with title, message count, and the Concierge mark', async () => {
    const fixture = await render(
      stubClient([
        chat({ id: 'a', title: 'Tea with Bertie', _count: { messages: 5, memories: 2 } }),
        chat({ id: 'b', title: 'A Dangerous Salon', conciergeState: 'flagged' }),
      ]),
    );
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Tea with Bertie');
    expect(text).toContain('5');
    // The Concierge asterisk — one, on the flagged chat, in the danger tone
    // (the base rule, no modifier) and with no native title.
    const marks = Array.from(
      fixture.nativeElement.querySelectorAll('.qt-concierge-mark'),
    ) as HTMLElement[];
    expect(marks).toHaveLength(1);
    expect(marks[0].getAttribute('aria-label')).toBe('Concierge: Flagged');
    // Angular's `[class]` binding applies tokens through `classList`, so the
    // attribute's ORDER is not v4's; the ordered string is pinned at its
    // source by `conciergeMarkClasses` (concierge-mark.spec.ts). What matters
    // in the DOM is the set — base rule, no modifier, plus the caller's two.
    expect([...marks[0].classList].sort()).toEqual([
      'flex-shrink-0',
      'qt-concierge-mark',
      'text-sm',
    ]);
    expect(marks[0].hasAttribute('title')).toBe(false);
  });

  it('falls back to "Untitled Chat" for a blank title', async () => {
    const fixture = await render(stubClient([chat({ title: '' })]));
    expect(fixture.nativeElement.textContent).toContain('Untitled Chat');
  });
});
