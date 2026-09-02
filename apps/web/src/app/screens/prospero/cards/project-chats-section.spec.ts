import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { EnrichedChatSummary } from '../../../core/core-contract';
import { ToastService } from '../../../ui/toast.service';
import { ProjectChatsSection } from './project-chats-section';

function chat(over: Partial<EnrichedChatSummary> = {}): EnrichedChatSummary {
  return {
    id: 'c1',
    title: 'An Evening at Sea',
    contextSummary: null,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    lastMessageAt: null,
    participants: [],
    tags: [],
    project: null,
    storyBackground: null,
    conciergeState: 'monitored',
    dangerCategories: [],
    chatType: 'salon',
    scriptoriumStatus: 'none',
    _count: { messages: 0, memories: 0 },
    ...over,
  };
}

function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

function stubClient(fails?: string): Partial<CoreClient> {
  return {
    dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
      if (req.type === fails) {
        throw new Error('the ledger will not let go');
      }
      if (req.type === 'projectChatList') {
        return { chats: [chat()], total: 1 };
      }
      return {};
    }) as CoreClient['dispatchData'],
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<ProjectChatsSection>> {
  TestBed.configureTestingModule({
    imports: [ProjectChatsSection],
    providers: [provideRouter([]), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(ProjectChatsSection);
  fixture.componentRef.setInput('projectId', 'p1');
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

/** v4 `useProjectChats.ts:88-105` — toast only, no inline surface. */
describe('ProjectChatsSection toasts', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('toasts "Chat removed from project" on a successful remove', async () => {
    const fixture = await render(stubClient());
    const removeBtn = fixture.nativeElement.querySelector(
      'button[aria-label="Remove from project"]',
    ) as HTMLButtonElement;
    expect(removeBtn).toBeTruthy();
    removeBtn.click();
    await settle(fixture);

    expect(toasts()).toEqual([{ type: 'success', message: 'Chat removed from project' }]);
    expect(fixture.nativeElement.textContent).not.toContain('the ledger');
  });

  it('toasts the server message on a failed remove, with no inline banner change', async () => {
    const fixture = await render(stubClient('projectChatRemove'));
    const removeBtn = fixture.nativeElement.querySelector(
      'button[aria-label="Remove from project"]',
    ) as HTMLButtonElement;
    removeBtn.click();
    await settle(fixture);

    expect(toasts()).toEqual([{ type: 'error', message: 'the ledger will not let go' }]);
    // The chat stays in the list (it was never removed).
    expect(fixture.nativeElement.textContent).toContain('An Evening at Sea');
  });
});
