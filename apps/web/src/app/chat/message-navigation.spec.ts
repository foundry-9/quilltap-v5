import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  MESSAGE_REVEAL_EVENT,
  PENDING_REVEAL_KEY,
  requestMessageReveal,
  takePendingMessageReveal,
} from './message-navigation';

/**
 * The storage / event pair of v4 `lib/chat/message-navigation.ts`, against v4's
 * own cases (`__tests__/unit/lib/chat/message-navigation.test.ts:45-77`, pinned
 * at `624d0d25c`) plus the storage-failure arms v4's comments promise. The pure
 * `revealRequestFromSalonHref` is proven by the oracle corpus instead.
 */
describe('requestMessageReveal / takePendingMessageReveal (v4 message-navigation.test.ts:45-77)', () => {
  beforeEach(() => sessionStorage.clear());
  afterEach(() => {
    vi.restoreAllMocks();
    sessionStorage.clear();
  });

  it('parks the request under v4’s key', () => {
    expect(PENDING_REVEAL_KEY).toBe('pendingMessageReveal');
    expect(MESSAGE_REVEAL_EVENT).toBe('quilltap:reveal-message');
    requestMessageReveal('chat-a', 'msg-1');
    expect(sessionStorage.getItem('pendingMessageReveal')).toBe(
      JSON.stringify({ chatId: 'chat-a', messageId: 'msg-1' }),
    );
  });

  it('is taken once, by the chat it names', () => {
    requestMessageReveal('chat-a', 'msg-1');

    expect(takePendingMessageReveal('chat-b')).toBeNull();
    expect(takePendingMessageReveal('chat-a')).toBe('msg-1');
    expect(takePendingMessageReveal('chat-a')).toBeNull();
  });

  it('leaves another chat’s request in storage', () => {
    requestMessageReveal('chat-a', 'msg-1');
    expect(takePendingMessageReveal('chat-b')).toBeNull();
    expect(sessionStorage.getItem('pendingMessageReveal')).not.toBeNull();
  });

  it('announces the request to an open Salon', () => {
    const listener = vi.fn();
    window.addEventListener(MESSAGE_REVEAL_EVENT, listener);
    requestMessageReveal('chat-a', 'msg-1');
    window.removeEventListener(MESSAGE_REVEAL_EVENT, listener);

    expect(listener).toHaveBeenCalledTimes(1);
    expect((listener.mock.calls[0][0] as CustomEvent).detail).toEqual({
      chatId: 'chat-a',
      messageId: 'msg-1',
    });
  });

  it('replaces an earlier request', () => {
    requestMessageReveal('chat-a', 'msg-1');
    requestMessageReveal('chat-a', 'msg-2');

    expect(takePendingMessageReveal('chat-a')).toBe('msg-2');
  });

  it('discards a malformed stored request', () => {
    sessionStorage.setItem('pendingMessageReveal', '{not json');

    expect(takePendingMessageReveal('chat-a')).toBeNull();
    expect(sessionStorage.getItem('pendingMessageReveal')).toBeNull();
  });

  it('discards a stored JSON null (the property read throws inside the try)', () => {
    sessionStorage.setItem('pendingMessageReveal', 'null');

    expect(takePendingMessageReveal('chat-a')).toBeNull();
    expect(sessionStorage.getItem('pendingMessageReveal')).toBeNull();
  });

  it('leaves a well-formed request whose messageId is not a string', () => {
    sessionStorage.setItem('pendingMessageReveal', JSON.stringify({ chatId: 'chat-a', messageId: 7 }));

    expect(takePendingMessageReveal('chat-a')).toBeNull();
    // v4 returns before removeItem on this arm: it stays.
    expect(sessionStorage.getItem('pendingMessageReveal')).not.toBeNull();
  });

  it('still announces when storage refuses the write', () => {
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('QuotaExceededError');
    });
    const listener = vi.fn();
    window.addEventListener(MESSAGE_REVEAL_EVENT, listener);
    expect(() => requestMessageReveal('chat-a', 'msg-1')).not.toThrow();
    window.removeEventListener(MESSAGE_REVEAL_EVENT, listener);

    expect(listener).toHaveBeenCalledTimes(1);
  });

  it('takes nothing when storage refuses the read', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('SecurityError');
    });
    expect(takePendingMessageReveal('chat-a')).toBeNull();
  });
});
