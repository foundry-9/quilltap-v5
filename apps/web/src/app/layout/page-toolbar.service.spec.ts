import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import type { ConversationSearchSource } from '../chat/conversation-search';
import { PageToolbarService } from './page-toolbar.service';

function source(chatId: string): ConversationSearchSource {
  return {
    chatId,
    search: () => ({ matches: [], total: 0 }),
    reveal: () => undefined,
    setHighlight: () => undefined,
  };
}

describe('PageToolbarService — the conversation-search slot (P4.D384)', () => {
  function svc(): PageToolbarService {
    TestBed.resetTestingModule();
    return TestBed.inject(PageToolbarService);
  }

  it('starts empty and holds the source it is given', () => {
    const s = svc();
    expect(s.conversationSearch()).toBeNull();
    const a = source('chat-a');
    s.setConversationSearch(a);
    expect(s.conversationSearch()).toBe(a);
  });

  it('the owner clears its own source', () => {
    const s = svc();
    const a = source('chat-a');
    s.setConversationSearch(a);
    expect(s.clearConversationSearch(a)).toBe(true);
    expect(s.conversationSearch()).toBeNull();
  });

  it("a backgrounded Salon cannot wipe the focused one's source", () => {
    const s = svc();
    const a = source('chat-a');
    const b = source('chat-b');
    s.setConversationSearch(a);
    // Tab switch: b takes focus first, then a hears it was hidden.
    s.setConversationSearch(b);
    expect(s.clearConversationSearch(a)).toBe(false);
    expect(s.conversationSearch()).toBe(b);
  });

  it('a different object for the SAME chat is not the owner', () => {
    // Two Salon tabs can show one chat; identity, not just the chat id, decides.
    const s = svc();
    const first = source('chat-a');
    const second = source('chat-a');
    s.setConversationSearch(second);
    expect(s.clearConversationSearch(first)).toBe(false);
    expect(s.conversationSearch()).toBe(second);
  });

  it('clearing an empty slot is a no-op', () => {
    const s = svc();
    expect(s.clearConversationSearch(source('chat-a'))).toBe(false);
    expect(s.conversationSearch()).toBeNull();
  });

  it('setConversationSearch(null) withdraws unconditionally', () => {
    const s = svc();
    s.setConversationSearch(source('chat-a'));
    s.setConversationSearch(null);
    expect(s.conversationSearch()).toBeNull();
  });
});
