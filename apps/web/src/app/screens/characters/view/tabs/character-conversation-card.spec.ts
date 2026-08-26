import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import type { CharacterChatSummary } from '../../../../core/core-contract';
import { __resetNowTickersForTests } from '../../../../shared/now.service';
import { ToastService } from '../../../../ui/toast.service';
import { CharacterConversationCard } from './character-conversation-card';

/**
 * The card's date readout after `f3892158d` (P4.D125): it reads the SHARED
 * day-granularity clock, so a list left open overnight rolls "today" over to
 * "Yesterday" at local midnight instead of whenever the card next happens to
 * re-render. v4's `ChatCard` takes exactly this tick for exactly this reason.
 */

function chat(lastMessageAt: string): CharacterChatSummary {
  return {
    id: 'chat-1',
    title: 'A conversation',
    lastMessageAt,
    updatedAt: lastMessageAt,
    messages: [],
    tags: [],
    _count: { messages: 0, memories: 0 },
  } as unknown as CharacterChatSummary;
}

function render(lastMessageAt: string) {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [CharacterConversationCard],
    providers: [
      provideRouter([]),
      { provide: CoreClient, useValue: { renderConversation: vi.fn() } },
      { provide: ToastService, useValue: { showSuccess: vi.fn(), showError: vi.fn() } },
    ],
  });
  const fixture = TestBed.createComponent(CharacterConversationCard);
  fixture.componentRef.setInput('chat', chat(lastMessageAt));
  fixture.detectChanges();
  return fixture;
}

describe('CharacterConversationCard — the day-boundary rollover', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    __resetNowTickersForTests();
  });
  afterEach(() => {
    __resetNowTickersForTests();
    vi.useRealTimers();
  });

  it('rolls over to "Yesterday" on a local-midnight tick, without a re-render of its own', () => {
    // Noon on day 1; the message is an hour old, so it reads as a time.
    //
    // v4's ladder floors ELAPSED MILLISECONDS, not calendar days
    // (`Math.floor(diffMs / 86_400_000)`) — a quirk this port carries and the
    // formatter's own spec pins — so "Yesterday" arrives at the midnight tick
    // AFTER 24 h have elapsed, not at the first one. Either way the point
    // stands: the readout changes because a shared timer fired, and nothing
    // else re-rendered the card.
    const noon = new Date(2026, 7, 26, 12, 0, 0, 0);
    vi.setSystemTime(noon);
    const fixture = render(new Date(2026, 7, 26, 11, 0, 0, 0).toISOString());
    const text = () => (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text()).not.toContain('Yesterday');

    // First tick: local midnight of day 2. 13 h elapsed — still day 0.
    vi.advanceTimersByTime(new Date(2026, 7, 27, 0, 0, 0, 0).getTime() - noon.getTime() + 1);
    TestBed.tick();
    fixture.detectChanges();
    expect(text()).not.toContain('Yesterday');

    // Second tick: local midnight of day 3. 37 h elapsed — day 1.
    vi.advanceTimersByTime(86_400_000);
    TestBed.tick();
    fixture.detectChanges();
    expect(text()).toContain('Yesterday');
  });

  it('does NOT re-read the clock more often than once a day', () => {
    vi.setSystemTime(new Date(2026, 7, 26, 12, 0, 0, 0));
    render(new Date(2026, 7, 26, 11, 0, 0, 0).toISOString());
    // One armed timer for the whole card — not a minute or second ticker.
    expect(vi.getTimerCount()).toBe(1);
    vi.advanceTimersByTime(60_000 * 60 * 11);
    TestBed.tick();
    // Still before midnight: the timer has not fired, so it is still the one.
    expect(vi.getTimerCount()).toBe(1);
  });
});
