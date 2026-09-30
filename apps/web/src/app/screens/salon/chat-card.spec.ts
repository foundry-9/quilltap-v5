import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { provideRouter } from '@angular/router';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { EnrichedChatSummary } from '../../core/core-contract';
import { ToastService } from '../../ui/toast.service';
import { Tooltip } from '../../ui/tooltip';
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
    conciergeState: 'moderated',
    dangerCategories: [],
    chatType: 'salon',
    scriptoriumStatus: 'none',
    ...over,
  } as unknown as EnrichedChatSummary;
}

function mount(c: EnrichedChatSummary, deletable = false) {
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
  if (deletable) {
    fixture.componentRef.setInput('deletable', true);
  }
  fixture.detectChanges();
  return fixture;
}

/** The `content` of every `qt-tooltip` on the card (a spec cannot read source). */
function tooltipContents(fixture: ComponentFixture<ChatCard>): string[] {
  return fixture.debugElement
    .queryAll(By.directive(Tooltip))
    .map((d) => (d.componentInstance as Tooltip).content() ?? '');
}

function render(c: EnrichedChatSummary): HTMLElement {
  return mount(c).nativeElement as HTMLElement;
}

describe('ChatCard — the activity date', () => {
  it('shows when a character last posted', () => {
    const el = render(chat({ lastMessageAt: '2026-05-01T00:00:00.000Z' }));
    expect(el.textContent).toContain(new Date('2026-05-01T00:00:00.000Z').toLocaleDateString());
  });

  it('falls back to createdAt, never updatedAt, when nobody has spoken', () => {
    const el = render(chat({ lastMessageAt: null }));
    expect(el.textContent).toContain(new Date('2024-03-04T00:00:00.000Z').toLocaleDateString());
    expect(el.textContent).not.toContain(new Date('2026-08-30T00:00:00.000Z').toLocaleDateString());
  });
});

/**
 * P4.80 (dogfood finding #117) — v4 `ChatCard.tsx:364-385`. The card does NOT
 * delete: v4's `onDelete` prop carries both the confirmation and the caller's
 * own list update (the Salon refetches, the Conversations tab filters locally),
 * so a card that deleted for itself could not tell either list what happened.
 *
 * The `preventDefault` arm is the one worth pinning by consequence: the whole
 * card is a `routerLink`, so a delete button that let the click through would
 * navigate INTO the chat it just asked to remove.
 */
describe('ChatCard — the delete action', () => {
  it('renders nothing without the deletable flag (v4 gates on the callback)', () => {
    const el = render(chat({}));
    expect(el.querySelector('button[aria-label="Delete chat"]')).toBeNull();
  });

  it('renders v4’s destructive trash button with v4’s label and tooltip', () => {
    const fixture = mount(chat({}), true);
    const el = fixture.nativeElement as HTMLElement;
    const button = el.querySelector<HTMLButtonElement>('button[aria-label="Delete chat"]');
    expect(button).not.toBeNull();
    expect(button!.className).toContain('qt-bg-destructive');
    expect(button!.className).toContain('qt-text-on-destructive');
    expect(button!.querySelector('qt-icon')).not.toBeNull();
    expect(button!.hasAttribute('title')).toBe(false);
    expect(tooltipContents(fixture)).toContain('Delete chat');
  });

  it('emits the chat id and suppresses the card’s own navigation', () => {
    const fixture = mount(chat({ id: 'c-victim' }), true);
    const seen: string[] = [];
    fixture.componentInstance.delete.subscribe((id: string) => seen.push(id));
    const el = fixture.nativeElement as HTMLElement;
    const button = el.querySelector<HTMLButtonElement>('button[aria-label="Delete chat"]')!;
    const event = new MouseEvent('click', { bubbles: true, cancelable: true });
    button.dispatchEvent(event);
    fixture.detectChanges();
    expect(seen).toEqual(['c-victim']);
    expect(event.defaultPrevented).toBe(true);
  });
});

/**
 * v4 `concierge-mark.test.tsx` `ChatCard — the Concierge mark` (at
 * `ce2f1dabf`), transcribed onto v5's card, which binds the list DTO directly
 * (v5 has no `ChatCardData` / `chat-utils` transform layer — the two new keys
 * ride on `EnrichedChatSummary`).
 */
describe('ChatCard — the Concierge mark', () => {
  afterEach(() => {
    vi.useRealTimers();
    TestBed.resetTestingModule();
  });

  it('draws no mark for a Moderated chat', () => {
    const el = render(chat({ conciergeState: 'moderated' }));
    expect(el.querySelector('.qt-concierge-mark')).toBeNull();
  });

  it('draws no mark when the payload carries no state at all', () => {
    const el = render(chat({ conciergeState: undefined }));
    expect(el.querySelector('.qt-concierge-mark')).toBeNull();
  });

  for (const [conciergeState, label, modifier] of [
    ['unmoderated', 'Concierge: Unmoderated', ''],
    ['locked', 'Concierge: Locked', 'qt-concierge-mark-muted'],
  ] as const) {
    it(`marks a ${conciergeState} chat`, () => {
      const el = render(chat({ conciergeState }));

      const mark = el.querySelector(`[aria-label="${label}"]`) as HTMLElement;
      expect(mark.textContent).toContain('*');
      expect(mark.classList.contains('qt-concierge-mark')).toBe(true);
      if (modifier) {
        expect(mark.classList.contains(modifier)).toBe(true);
      } else {
        expect(el.querySelector('.qt-concierge-mark-muted')).toBeNull();
      }
    });
  }

  it("passes provenance through to the mark's tooltip", async () => {
    vi.useFakeTimers({
      toFake: ['setTimeout', 'clearTimeout', 'requestAnimationFrame', 'cancelAnimationFrame'],
    });
    const fixture = mount(
      chat({
        conciergeState: 'unmoderated',
        conciergeSetBy: 'concierge',
        conciergeReason: 'classifier',
        dangerCategories: ['NSFW'],
      }),
    );
    const el = fixture.nativeElement as HTMLElement;
    el.querySelector('qt-concierge-mark qt-tooltip')!.dispatchEvent(new Event('pointerenter'));
    await vi.advanceTimersByTimeAsync(250);
    fixture.detectChanges();
    await vi.advanceTimersByTimeAsync(0);
    fixture.detectChanges();

    const bubble = document.body.querySelector('.qt-tooltip')!;
    expect(bubble.textContent).toMatch(/The Concierge moved this chat/);
    expect(bubble.textContent).toContain('NSFW');
  });
});

/** `f7f3d7bf0` (P4.D236): every card `title=` moved into the in-app tooltip. */
describe('ChatCard — the in-app tooltips', () => {
  it('places the remove control by its tooltip HOST, not the button, so the card gains no blank line box', () => {
    const fixture = mount(chat({}), false);
    fixture.componentRef.setInput('removable', true);
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    const button = el.querySelector<HTMLButtonElement>('button[aria-label="Remove from project"]');
    expect(button).not.toBeNull();
    // An inline-flex tooltip host wrapping an `absolute` button still opens a
    // full line box above the content row (82px against 58px, measured); the
    // host carries the corner placement instead (v4's `anchorClassName`).
    expect(button!.className).not.toContain('absolute');
    const host = button!.closest('qt-tooltip') as HTMLElement;
    expect(host.className).toContain('absolute');
    expect(host.className).toContain('top-2');
    expect(host.className).toContain('right-2');
  });

  it('carries the card tooltips as qt-tooltip contents and leaves no native title', () => {
    const fixture = mount(
      chat({ _count: { messages: 3, memories: 2 } } as Partial<EnrichedChatSummary>),
    );
    const el = fixture.nativeElement as HTMLElement;
    const contents = tooltipContents(fixture);
    expect(contents).toContain('Messages');
    expect(contents).toContain('Memories — click to delete and re-extract');
    expect(contents).toContain('Copy link to this chat');
    expect(el.querySelector('[title]')).toBeNull();
  });
});

/**
 * The memory badge is v4's click-to-delete-and-re-extract button
 * (`components/chat/ChatCard.tsx:267-283` at `97b25fc53`): tooltip
 * `Memories — click to delete and re-extract`, aria-label
 * `${n} memories — delete and re-extract`, click = preventDefault +
 * stopPropagation + `onReextractMemories(chat.id)`. v4 gates it on
 * `memoryCount !== undefined` and its transforms always answer a number
 * (`lib/chat-utils.ts:71,129`), so it renders at ZERO too.
 */
describe('ChatCard — the memory badge (P4.125)', () => {
  const badge = (fixture: ComponentFixture<ChatCard>) =>
    (fixture.nativeElement as HTMLElement).querySelector(
      'button[aria-label$="memories — delete and re-extract"]',
    ) as HTMLButtonElement | null;

  it("is a button with v4's tooltip and aria-label", () => {
    const fixture = mount(chat({ _count: { messages: 3, memories: 2 } } as Partial<EnrichedChatSummary>));
    const b = badge(fixture)!;
    expect(b.getAttribute('aria-label')).toBe('2 memories — delete and re-extract');
    expect(b.type).toBe('button');
    expect(tooltipContents(fixture)).toContain('Memories — click to delete and re-extract');
    expect(b.textContent).toContain('2');
  });

  it('renders at zero (v4 gates on !== undefined, and its transform answers ?? 0)', () => {
    const fixture = mount(chat({ _count: { messages: 3, memories: 0 } } as Partial<EnrichedChatSummary>));
    // M4: keeping the old `> 0` gate reddens this.
    expect(badge(fixture)!.getAttribute('aria-label')).toBe('0 memories — delete and re-extract');
  });

  it('emits the chat id and stops the card link from navigating', () => {
    const fixture = mount(chat({ id: 'c9', _count: { messages: 3, memories: 2 } } as Partial<EnrichedChatSummary>));
    const emitted: string[] = [];
    fixture.componentInstance.reextractMemories.subscribe((id) => emitted.push(id));
    const event = new MouseEvent('click', { bubbles: true, cancelable: true });
    const stop = vi.spyOn(event, 'stopPropagation');
    badge(fixture)!.dispatchEvent(event);
    expect(emitted).toEqual(['c9']);
    expect(event.defaultPrevented).toBe(true);
    expect(stop).toHaveBeenCalled();
  });
});
