/**
 * BrahmaConsoleService — asserted against v4 `brahma-console-provider.tsx`'s
 * observable behavior: the exact localStorage key + persistence rules, the
 * launcher-reset on open, the optimistic-then-PATCH setModel (which no-ops
 * without a current chat), and `isEligible = profiles.length > 0`.
 */

import { TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { BrahmaConsoleService, STORAGE_KEY_LAST_CHAT } from './brahma-console.service';
import { BrahmaConsoleApi } from './brahma-wire';

interface Profile {
  id: string;
  name: string;
  provider: string;
  modelName: string;
}

function stubCore(profiles: Profile[]): Partial<CoreClient> {
  return {
    dispatchExpect: (async () => ({
      type: 'connectionProfiles',
      data: { profiles, count: profiles.length },
    })) as unknown as CoreClient['dispatchExpect'],
  };
}

function makeService(profiles: Profile[] = [], apiStub: Partial<BrahmaConsoleApi> = {}) {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: stubCore(profiles) },
      { provide: BrahmaConsoleApi, useValue: apiStub },
    ],
  });
  return TestBed.inject(BrahmaConsoleService);
}

async function settle(ticks = 6): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await new Promise((r) => setTimeout(r, 0));
  }
}

describe('BrahmaConsoleService (v4 brahma-console-provider.tsx)', () => {
  afterEach(() => {
    localStorage.clear();
    TestBed.resetTestingModule();
  });

  it('uses the exact v4 localStorage key', () => {
    expect(STORAGE_KEY_LAST_CHAT).toBe('quilltap:brahma-console-last-id');
  });

  it('hydrates currentChatId from localStorage on construction', () => {
    localStorage.setItem(STORAGE_KEY_LAST_CHAT, 'chat-9');
    const svc = makeService();
    expect(svc.currentChatId()).toBe('chat-9');
  });

  it('openConsole resets to the launcher (currentChatId null) and opens', () => {
    localStorage.setItem(STORAGE_KEY_LAST_CHAT, 'chat-9');
    const svc = makeService();
    svc.openConsole();
    expect(svc.isOpen()).toBe(true);
    expect(svc.currentChatId()).toBeNull();
    // Persistence cleared to match.
    expect(localStorage.getItem(STORAGE_KEY_LAST_CHAT)).toBeNull();
  });

  it('closeConsole closes without touching the chat id', () => {
    const svc = makeService();
    svc.setCurrentChatId('chat-1');
    svc.isOpen.set(true);
    svc.closeConsole();
    expect(svc.isOpen()).toBe(false);
    expect(svc.currentChatId()).toBe('chat-1');
  });

  it('setCurrentChatId persists an id and removes it on null', () => {
    const svc = makeService();
    svc.setCurrentChatId('chat-2');
    expect(localStorage.getItem(STORAGE_KEY_LAST_CHAT)).toBe('chat-2');
    svc.setCurrentChatId(null);
    expect(localStorage.getItem(STORAGE_KEY_LAST_CHAT)).toBeNull();
  });

  it('setModel optimistically sets the active profile even without a current chat, and does NOT PATCH', async () => {
    const setModel = vi.fn(async () => null);
    const svc = makeService([], { setModel });
    await svc.setModel('prof-1');
    expect(svc.activeConnectionProfileId()).toBe('prof-1');
    expect(setModel).not.toHaveBeenCalled();
  });

  it('setModel PATCHes when a chat is active (same chat continues)', async () => {
    const setModel = vi.fn(async () => null);
    const svc = makeService([], { setModel });
    svc.setCurrentChatId('chat-3');
    await svc.setModel('prof-2');
    expect(svc.activeConnectionProfileId()).toBe('prof-2');
    expect(setModel).toHaveBeenCalledWith('chat-3', 'prof-2');
  });

  it('isEligible is false with no profiles and true once ≥1 loads', async () => {
    const svc = makeService([]);
    expect(svc.isEligible()).toBe(false);

    const svc2 = makeService([{ id: 'p', name: 'GPT', provider: 'OPENAI', modelName: 'gpt-4o' }]);
    // touch the query to trigger the fetch
    expect(svc2.profiles()).toEqual([]);
    await settle();
    expect(svc2.profiles()).toHaveLength(1);
    expect(svc2.isEligible()).toBe(true);
  });
});
