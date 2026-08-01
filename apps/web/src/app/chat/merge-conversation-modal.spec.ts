import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { beforeEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../core/core-client';
import { ToastService } from '../ui/toast.service';
import type { EnrichedChatSummary } from '../core/core-contract';
import { MergeConversationModal, presentCharacters } from './merge-conversation-modal';

/**
 * Merge a Conversation In (v4 `MergeConversationModal.tsx`). The load-bearing
 * behaviours are the candidate filters, the "everyone in by default" gate, and
 * the outfit choices being narrowed to the included set before they travel.
 */

interface Req {
  type: string;
  [k: string]: unknown;
}

const sent: Req[] = [];
let mergeAnswer: Record<string, unknown> | Error = {
  merge: { mergedCharacterIds: ['c2', 'c3'] },
};

function chat(
  id: string,
  title: string,
  characters: { id: string; name: string; status?: string }[],
  over: Partial<EnrichedChatSummary> = {},
): EnrichedChatSummary {
  return {
    id,
    title,
    contextSummary: null,
    createdAt: '2026-07-01T00:00:00.000Z',
    updatedAt: '2026-07-01T00:00:00.000Z',
    lastMessageAt: null,
    participants: characters.map((c, i) => ({
      id: `p-${c.id}`,
      type: 'CHARACTER',
      displayOrder: i,
      isActive: true,
      status: c.status ?? 'active',
      character: { id: c.id, name: c.name },
    })),
    tags: [],
    project: null,
    storyBackground: null,
    isDangerousChat: false,
    conciergeOverride: null,
    chatType: 'salon',
    scriptoriumStatus: 'none',
    _count: { messages: 0, memories: 0 },
    ...over,
  } as unknown as EnrichedChatSummary;
}

let chats: EnrichedChatSummary[] = [];

function stubClient(): Partial<CoreClient> {
  return {
    dispatchExpect: (async () => ({ type: 'chats', data: chats })) as unknown as CoreClient['dispatchExpect'],
    dispatchData: (async (req: Req) => {
      sent.push(req);
      if (req['type'] === 'chatOutfitSummary') return { summary: {} };
      if (mergeAnswer instanceof Error) throw mergeAnswer;
      return mergeAnswer;
    }) as unknown as CoreClient['dispatchData'],
  };
}

@Component({
  imports: [MergeConversationModal],
  template: `
    <qt-merge-conversation-modal
      [targetChatId]="'chat-here'"
      [existingCharacterIds]="existing()"
      (merged)="emitted = emitted + 1"
      (close)="closed = closed + 1"
    />
  `,
})
class Host {
  readonly existing = signal<string[]>(['c1']);
  emitted = 0;
  closed = 0;
}

async function render(): Promise<ComponentFixture<Host>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [
      { provide: CoreClient, useValue: stubClient() },
      provideTanStackQuery(new QueryClient()),
    ],
  });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

async function settle(fixture: ComponentFixture<Host>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function cards(fixture: ComponentFixture<Host>): HTMLButtonElement[] {
  return Array.from(
    fixture.nativeElement.querySelectorAll('.qt-dialog-body button'),
  ) as HTMLButtonElement[];
}

function mergeButton(fixture: ComponentFixture<Host>): HTMLButtonElement | undefined {
  return (Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[]).find(
    (b) => b.textContent!.trim().startsWith('Merge In'),
  );
}

/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('presentCharacters', () => {
  it('drops removed participants and de-duplicates by character', () => {
    const row = chat('c', 'A Chat', [
      { id: 'x', name: 'Xan' },
      { id: 'x', name: 'Xan' },
      { id: 'y', name: 'Yara', status: 'removed' },
    ]);
    expect(presentCharacters(row)).toEqual([{ id: 'x', name: 'Xan' }]);
  });
});

describe('MergeConversationModal', () => {
  beforeEach(() => {
    sent.length = 0;
    mergeAnswer = { merge: { mergedCharacterIds: ['c2', 'c3'] } };
    chats = [
      chat('chat-here', 'This One', [{ id: 'c1', name: 'Aria' }]),
      chat('chat-a', 'The Long Voyage', [
        { id: 'c1', name: 'Aria' },
        { id: 'c2', name: 'Lorian' },
        { id: 'c3', name: 'Riya' },
      ]),
      chat('chat-b', 'Overnight Watch', [{ id: 'c4', name: 'Suparna' }], {
        chatType: 'autonomous',
      }),
    ];
  });

  it('excludes this chat and every autonomous room from the candidates', async () => {
    const fixture = await render();
    const titles = cards(fixture).map((b) => b.textContent!);
    expect(titles.some((t) => t.includes('The Long Voyage'))).toBe(true);
    expect(titles.some((t) => t.includes('This One'))).toBe(false);
    expect(titles.some((t) => t.includes('Overnight Watch'))).toBe(false);
  });

  it('searches over the title AND the company', async () => {
    const fixture = await render();
    const box = fixture.nativeElement.querySelector('input[type="text"]') as HTMLInputElement;

    box.value = 'lorian';
    box.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(cards(fixture).some((b) => b.textContent!.includes('The Long Voyage'))).toBe(true);

    box.value = 'nobody here';
    box.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('No matching conversations.');
  });

  it('offers only the characters not already here, all checked', async () => {
    const fixture = await render();
    cards(fixture)[0].click();
    await settle(fixture);

    const boxes = Array.from(
      fixture.nativeElement.querySelectorAll('input[type="checkbox"]'),
    ) as HTMLInputElement[];
    expect(boxes.map((b) => b.getAttribute('aria-label'))).toEqual(['Lorian', 'Riya']);
    expect(boxes.every((b) => b.checked)).toBe(true);
    expect(mergeButton(fixture)!.textContent).toContain('Merge In (2)');
  });

  it('refuses with v4’s line when everyone is unchecked', async () => {
    const fixture = await render();
    cards(fixture)[0].click();
    await settle(fixture);
    for (const box of Array.from(
      fixture.nativeElement.querySelectorAll('input[type="checkbox"]'),
    ) as HTMLInputElement[]) {
      box.checked = false;
      box.dispatchEvent(new Event('change'));
    }
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Select at least one character');
    expect(mergeButton(fixture)!.disabled).toBe(true);
  });

  it('sends only the included characters AND only their outfit choices', async () => {
    const fixture = await render();
    cards(fixture)[0].click();
    await settle(fixture);

    // Uncheck Riya; her staged outfit choice must not travel either.
    const riya = (
      Array.from(fixture.nativeElement.querySelectorAll('input[type="checkbox"]')) as HTMLInputElement[]
    ).find((b) => b.getAttribute('aria-label') === 'Riya')!;
    riya.checked = false;
    riya.dispatchEvent(new Event('change'));
    await settle(fixture);

    mergeButton(fixture)!.click();
    await settle(fixture);

    const merge = sent.find((r) => r.type === 'chatMergeConversation')!;
    expect(merge['sourceChatId']).toBe('chat-a');
    expect(merge['characterIds']).toEqual(['c2']);
    expect(merge['outfitSelections']).toEqual([{ characterId: 'c2', mode: 'previous_chat' }]);
  });

  it('seeds every incoming character to "Same as last conversation"', async () => {
    const fixture = await render();
    cards(fixture)[0].click();
    await settle(fixture);
    expect(fixture.nativeElement.textContent).toContain('Same as last conversation');
    const chosen = Array.from(
      fixture.nativeElement.querySelectorAll('input[type="radio"]:checked'),
    ) as HTMLInputElement[];
    expect(chosen.map((r) => r.value)).toEqual(['previous_chat', 'previous_chat']);
  });

  it('reports the server’s count, and falls back to its own', async () => {
    const fixture = await render();
    cards(fixture)[0].click();
    await settle(fixture);
    mergeButton(fixture)!.click();
    await settle(fixture);
    expect(fixture.componentInstance.emitted).toBe(1);
    expect(toasts()).toEqual([
      { type: 'success', message: 'Merged 2 characters from “The Long Voyage”' },
    ]);

    mergeAnswer = {};
    const second = await render();
    second.componentInstance.existing.set(['c1', 'c3']);
    second.detectChanges();
    cards(second)[0].click();
    await settle(second);
    mergeButton(second)!.click();
    await settle(second);
    expect(toasts()).toEqual([
      { type: 'success', message: 'Merged 1 character from “The Long Voyage”' },
    ]);
  });

  it('reports a failed merge and stays open', async () => {
    mergeAnswer = new Error('the two ledgers will not meet');
    const fixture = await render();
    cards(fixture)[0].click();
    await settle(fixture);
    mergeButton(fixture)!.click();
    await settle(fixture);
    expect(toasts()).toEqual([{ type: 'error', message: 'the two ledgers will not meet' }]);
    expect(fixture.componentInstance.closed).toBe(0);
  });
});
