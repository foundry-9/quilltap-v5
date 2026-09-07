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
    expect(el.querySelector('button[title="Delete chat"]')).toBeNull();
  });

  it('renders v4’s destructive trash button with v4’s title', () => {
    const el = mount(chat({}), true).nativeElement as HTMLElement;
    const button = el.querySelector<HTMLButtonElement>('button[title="Delete chat"]');
    expect(button).not.toBeNull();
    expect(button!.className).toContain('qt-bg-destructive');
    expect(button!.className).toContain('qt-text-on-destructive');
    expect(button!.querySelector('qt-icon')).not.toBeNull();
  });

  it('emits the chat id and suppresses the card’s own navigation', () => {
    const fixture = mount(chat({ id: 'c-victim' }), true);
    const seen: string[] = [];
    fixture.componentInstance.delete.subscribe((id: string) => seen.push(id));
    const el = fixture.nativeElement as HTMLElement;
    const button = el.querySelector<HTMLButtonElement>('button[title="Delete chat"]')!;
    const event = new MouseEvent('click', { bubbles: true, cancelable: true });
    button.dispatchEvent(event);
    fixture.detectChanges();
    expect(seen).toEqual(['c-victim']);
    expect(event.defaultPrevented).toBe(true);
  });
});
