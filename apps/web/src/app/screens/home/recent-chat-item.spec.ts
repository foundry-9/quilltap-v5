import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { RecentChat } from './home.api';
import { RecentChatItem } from './recent-chat-item';

/**
 * v4 `homepage-components.test.tsx` `RecentChatItem — the Concierge mark` (at
 * `4d370a90f`), transcribed by name onto v5's row. The asterisk marks any state
 * other than the default, in the same two tones as the Salon header's pill;
 * who opened the door is in the bubble, never the colour.
 */
function recentChat(over: Partial<RecentChat> = {}): RecentChat {
  return {
    id: 'c1',
    title: 'A Chat',
    createdAt: '2025-12-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    lastMessageAt: '2026-01-02T00:00:00Z',
    participants: [],
    _count: { messages: 3 },
    ...over,
  };
}

function render(chat: RecentChat): ComponentFixture<RecentChatItem> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [RecentChatItem], providers: [provideRouter([])] });
  const fixture = TestBed.createComponent(RecentChatItem);
  fixture.componentRef.setInput('chat', chat);
  fixture.detectChanges();
  return fixture;
}

const el = (f: ComponentFixture<unknown>) => f.nativeElement as HTMLElement;
const byLabel = (f: ComponentFixture<unknown>, label: string) =>
  el(f).querySelector(`[aria-label="${label}"]`) as HTMLElement;
const bubble = () => document.body.querySelector('.qt-tooltip') as HTMLElement | null;

async function hover(f: ComponentFixture<unknown>): Promise<void> {
  el(f).querySelector('qt-concierge-mark qt-tooltip')!.dispatchEvent(new Event('pointerenter'));
  await vi.advanceTimersByTimeAsync(250);
  f.detectChanges();
  await vi.advanceTimersByTimeAsync(0);
  f.detectChanges();
}

describe('RecentChatItem — the Concierge mark', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('draws no mark for a Moderated chat', () => {
    expect(el(render(recentChat({ conciergeState: 'moderated' }))).querySelector('.qt-concierge-mark')).toBeNull();
  });

  it('draws no mark when the payload carries no state at all', () => {
    expect(el(render(recentChat({ conciergeState: undefined }))).querySelector('.qt-concierge-mark')).toBeNull();
  });

  for (const [conciergeState, label, absentModifiers] of [
    ['unmoderated', 'Concierge: Unmoderated', ['qt-concierge-mark-muted']],
    ['locked', 'Concierge: Locked', [] as string[]],
  ] as const) {
    it(`marks a ${conciergeState} chat and labels it "${label}"`, () => {
      const f = render(recentChat({ conciergeState }));

      const mark = byLabel(f, label);
      expect(mark.textContent).toContain('*');
      expect(mark.classList.contains('qt-concierge-mark')).toBe(true);
      for (const absent of absentModifiers) {
        expect(el(f).querySelector(`.${absent}`)).toBeNull();
      }
    });
  }

  it('gives each state its own tone class', () => {
    // Red is the base rule (no modifier); grey is a modifier.
    expect(
      el(render(recentChat({ conciergeState: 'unmoderated' }))).querySelector('.qt-concierge-mark')!
        .className,
    ).toBe('qt-concierge-mark');
    expect(
      el(render(recentChat({ conciergeState: 'locked' })))
        .querySelector('.qt-concierge-mark')!
        .classList.contains('qt-concierge-mark-muted'),
    ).toBe(true);
  });

  describe('the tooltip', () => {
    beforeEach(() => {
      vi.useFakeTimers({
        toFake: ['setTimeout', 'clearTimeout', 'requestAnimationFrame', 'cancelAnimationFrame'],
      });
    });
    afterEach(() => {
      vi.useRealTimers();
      TestBed.resetTestingModule();
    });

    it('explains the state after the pointer has dwelt on the mark', async () => {
      const f = render(recentChat({ conciergeState: 'locked' }));
      expect(bubble()).toBeNull();

      await hover(f);

      const text = bubble()!.textContent ?? '';
      expect(text).toContain('Locked');
      expect(text).toContain('Only the usual providers, ever.');
      expect(text).toContain("Change it from the Salon sidebar's Chat section.");
    });

    it('says who opened the door — the Concierge or the operator', async () => {
      let f = render(
        recentChat({ conciergeState: 'unmoderated', conciergeSetBy: 'concierge', conciergeReason: 'refusals' }),
      );
      await hover(f);
      expect(bubble()!.textContent).toMatch(/The Concierge moved this chat to the uncensored desk/);

      f = render(
        recentChat({ conciergeState: 'unmoderated', conciergeSetBy: 'operator', conciergeReason: 'manual' }),
      );
      await hover(f);
      const text = bubble()!.textContent ?? '';
      expect(text).toMatch(/opened the uncensored door yourself/);
      expect(text).not.toMatch(/The Concierge moved this chat/);
    });

    it("names the classifier's categories when the classifier moved the chat", async () => {
      const f = render(
        recentChat({
          conciergeState: 'unmoderated',
          conciergeSetBy: 'concierge',
          conciergeReason: 'classifier',
          dangerCategories: ['NSFW', 'Violence'],
        }),
      );

      await hover(f);

      const text = bubble()!.textContent ?? '';
      expect(text).toContain('Categories');
      expect(text).toContain('NSFW, Violence');
    });

    it('never surfaces a preserved category list on an operator state', async () => {
      const f = render(
        recentChat({ conciergeState: 'unmoderated', conciergeSetBy: 'operator', dangerCategories: ['NSFW'] }),
      );

      await hover(f);

      expect(bubble()!.textContent).not.toContain('Categories');
    });
  });
});
