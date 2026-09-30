import { TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { provideRouter } from '@angular/router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import type { CharacterChatSummary } from '../../../../core/core-contract';
import { __resetNowTickersForTests } from '../../../../shared/now.service';
import { ToastService } from '../../../../ui/toast.service';
import { Tooltip } from '../../../../ui/tooltip';
import { CharacterConversationCard } from './character-conversation-card';

/**
 * The card's date readout after `f3892158d` (P4.D125): it reads the SHARED
 * day-granularity clock, so a list left open overnight rolls "today" over to
 * "Yesterday" at local midnight instead of whenever the card next happens to
 * re-render. v4's `ChatCard` takes exactly this tick for exactly this reason.
 */

function chat(
  lastMessageAt: string | null,
  over: Record<string, unknown> = {},
): CharacterChatSummary {
  return {
    id: 'chat-1',
    title: 'A conversation',
    lastMessageAt,
    createdAt: lastMessageAt ?? '2026-01-01T00:00:00.000Z',
    updatedAt: lastMessageAt ?? '2026-01-01T00:00:00.000Z',
    messages: [],
    tags: [],
    _count: { messages: 0, memories: 0 },
    ...over,
  } as unknown as CharacterChatSummary;
}

function render(lastMessageAt: string | null, over: Record<string, unknown> = {}) {
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
  fixture.componentRef.setInput('chat', chat(lastMessageAt, over));
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

/**
 * P4.D140 (v4 `735d9408c`, bug 112): the card dates a chat by when a CHARACTER
 * last posted, falling back to when it was created — never by `updatedAt`,
 * which moves for a background render or a cost tally.
 */
describe('CharacterConversationCard — the activity date', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    __resetNowTickersForTests();
  });
  afterEach(() => {
    __resetNowTickersForTests();
    vi.useRealTimers();
  });

  it('falls back to createdAt, not updatedAt, when nobody has spoken', () => {
    vi.setSystemTime(new Date(2026, 7, 30, 12, 0, 0, 0));
    const fixture = render(null, {
      createdAt: new Date(2024, 0, 15, 9, 0, 0, 0).toISOString(),
      // A Staff announcement moved the row yesterday. It is not activity.
      updatedAt: new Date(2026, 7, 29, 9, 0, 0, 0).toISOString(),
    });
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('2024');
    expect(text).not.toContain('Yesterday');
  });
});

describe('CharacterConversationCard — the in-app tooltips (P4.D236)', () => {
  it('carries the card tooltips as qt-tooltip contents and leaves no native title', () => {
    const fixture = render('2026-01-01T12:00:00.000Z', {
      _count: { messages: 3, memories: 2 },
    });
    const contents = fixture.debugElement
      .queryAll(By.directive(Tooltip))
      .map((d) => (d.componentInstance as Tooltip).content());
    expect(contents).toContain('Messages');
    expect(contents).toContain('Memories — click to delete and re-extract');
    expect((fixture.nativeElement as HTMLElement).querySelector('[title]')).toBeNull();
  });
});

/**
 * The memory badge on the character Conversations tab card — the same v4
 * `ChatCard` button (`ChatCard.tsx:267-283` at `97b25fc53`), P4.125.
 */
describe('CharacterConversationCard — the memory badge (P4.125)', () => {
  const badge = (fixture: ReturnType<typeof render>) =>
    (fixture.nativeElement as HTMLElement).querySelector(
      'button[aria-label$="memories — delete and re-extract"]',
    ) as HTMLButtonElement | null;

  it("is a button with v4's tooltip and aria-label, and renders at zero", () => {
    const two = render('2026-01-01T12:00:00.000Z', { _count: { messages: 3, memories: 2 } });
    expect(badge(two)!.getAttribute('aria-label')).toBe('2 memories — delete and re-extract');
    expect(
      two.debugElement
        .queryAll(By.directive(Tooltip))
        .map((d) => (d.componentInstance as Tooltip).content()),
    ).toContain('Memories — click to delete and re-extract');
    const zero = render('2026-01-01T12:00:00.000Z', { _count: { messages: 3, memories: 0 } });
    expect(badge(zero)!.getAttribute('aria-label')).toBe('0 memories — delete and re-extract');
  });

  it('emits the chat id and stops the card link from navigating', () => {
    const fixture = render('2026-01-01T12:00:00.000Z', {
      id: 'chat-7',
      _count: { messages: 3, memories: 2 },
    });
    const emitted: string[] = [];
    fixture.componentInstance.reextractMemories.subscribe((id) => emitted.push(id));
    const event = new MouseEvent('click', { bubbles: true, cancelable: true });
    badge(fixture)!.dispatchEvent(event);
    expect(emitted).toEqual(['chat-7']);
    expect(event.defaultPrevented).toBe(true);
  });
});
