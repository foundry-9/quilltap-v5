/** v4 `components/brahma-console/hooks/useBrahmaConsoleSession.ts` at `236c04e35` (`99463fc56`). */
import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  STORAGE_KEY_FLOATING_LAST_CHAT,
  STORAGE_KEY_TAB_LAST_CHAT,
  createBrahmaConsoleSession,
} from './brahma-console-session';
import type { BrahmaConsoleApi } from './brahma-wire';

const api = (setModel = vi.fn(async () => null)) =>
  ({ setModel }) as unknown as BrahmaConsoleApi & { setModel: typeof setModel };

describe('createBrahmaConsoleSession', () => {
  afterEach(() => localStorage.clear());

  it('names v4’s two keys', () => {
    expect(STORAGE_KEY_FLOATING_LAST_CHAT).toBe('quilltap:brahma-floating-last-id');
    expect(STORAGE_KEY_TAB_LAST_CHAT).toBe('quilltap:brahma-console-last-id');
  });

  it('two sessions under the two keys never share a pointer', () => {
    localStorage.setItem(STORAGE_KEY_TAB_LAST_CHAT, 'tab-chat');
    const floating = createBrahmaConsoleSession(STORAGE_KEY_FLOATING_LAST_CHAT, api());
    const tab = createBrahmaConsoleSession(STORAGE_KEY_TAB_LAST_CHAT, api());
    expect(floating.currentChatId()).toBeNull();
    expect(tab.currentChatId()).toBe('tab-chat');
    floating.setCurrentChatId('float-chat');
    expect(tab.currentChatId()).toBe('tab-chat');
    expect(localStorage.getItem(STORAGE_KEY_TAB_LAST_CHAT)).toBe('tab-chat');
  });

  it('setCurrentChatId writes and removes its OWN key', () => {
    const s = createBrahmaConsoleSession(STORAGE_KEY_FLOATING_LAST_CHAT, api());
    s.setCurrentChatId('c1');
    expect(localStorage.getItem(STORAGE_KEY_FLOATING_LAST_CHAT)).toBe('c1');
    s.setCurrentChatId(null);
    expect(localStorage.getItem(STORAGE_KEY_FLOATING_LAST_CHAT)).toBeNull();
  });

  it('setModel with no chat open sets the local id and dispatches NOTHING', async () => {
    const a = api();
    const s = createBrahmaConsoleSession(STORAGE_KEY_TAB_LAST_CHAT, a);
    await s.setModel('p1');
    expect(s.activeConnectionProfileId()).toBe('p1');
    expect(a.setModel).not.toHaveBeenCalled();
  });

  it('setModel with a chat open sets the local id, then PATCHes', async () => {
    const a = api();
    const s = createBrahmaConsoleSession(STORAGE_KEY_TAB_LAST_CHAT, a);
    s.setCurrentChatId('c3');
    await s.setModel('p2');
    expect(s.activeConnectionProfileId()).toBe('p2');
    expect(a.setModel).toHaveBeenCalledWith('c3', 'p2');
  });

  it('a failed PATCH logs v4’s line and keeps the optimistic id', async () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const s = createBrahmaConsoleSession(
      STORAGE_KEY_TAB_LAST_CHAT,
      api(vi.fn(async () => Promise.reject(new Error('boom')))),
    );
    s.setCurrentChatId('c4');
    await s.setModel('p3');
    expect(s.activeConnectionProfileId()).toBe('p3');
    expect(error).toHaveBeenCalledWith('Failed to switch Brahma Console model:', expect.any(Error));
    error.mockRestore();
  });
});
