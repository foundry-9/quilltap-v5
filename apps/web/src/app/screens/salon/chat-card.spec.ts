import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { EnrichedChatSummary } from '../../core/core-contract';
import { ToastService } from '../../ui/toast.service';
import { ChatCard } from './chat-card';

/**
 * P4.D140 (v4 `735d9408c`, bug 112): the Salon card dates a chat by when a
 * CHARACTER last posted, falling back to when it was created. It must never
 * read `updatedAt` — that moves for a story background landing, a folded
 * summary or a cost tally, none of which is the conversation moving forward.
 * (Before this port the card read `updatedAt` ONLY, under a comment claiming
 * the Salon transform omitted `lastMessageAt` — it does not.)
 */
function chat(over: Partial<EnrichedChatSummary>): EnrichedChatSummary {
  return {
    id: 'c1',
    title: 'A conversation',
    contextSummary: null,
    createdAt: '2024-03-04T00:00:00.000Z',
    updatedAt: '2026-08-30T00:00:00.000Z',
    lastMessageAt: null,
    participants: [],
    tags: [],
    project: null,
    storyBackground: null,
    conciergeState: 'monitored',
    dangerCategories: [],
    chatType: 'salon',
    scriptoriumStatus: 'none',
    ...over,
  } as unknown as EnrichedChatSummary;
}

function render(c: EnrichedChatSummary): HTMLElement {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [ChatCard],
    providers: [
      provideRouter([]),
      { provide: CoreClient, useValue: { renderConversation: vi.fn() } },
      { provide: ToastService, useValue: { showSuccess: vi.fn(), showError: vi.fn() } },
    ],
  });
  const fixture = TestBed.createComponent(ChatCard);
  fixture.componentRef.setInput('chat', c);
  fixture.detectChanges();
  return fixture.nativeElement as HTMLElement;
}

describe('ChatCard — the activity date', () => {
  it('shows when a character last posted', () => {
    const el = render(chat({ lastMessageAt: '2026-05-01T00:00:00.000Z' }));
    expect(el.textContent).toContain(new Date('2026-05-01T00:00:00.000Z').toLocaleDateString());
  });

  it('falls back to createdAt, never updatedAt, when nobody has spoken', () => {
    const el = render(chat({ lastMessageAt: null }));
    expect(el.textContent).toContain(new Date('2024-03-04T00:00:00.000Z').toLocaleDateString());
    expect(el.textContent).not.toContain(
      new Date('2026-08-30T00:00:00.000Z').toLocaleDateString(),
    );
  });
});
