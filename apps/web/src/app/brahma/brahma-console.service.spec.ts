/**
 * BrahmaConsoleService — asserted against v4 `brahma-console-provider.tsx`'s
 * observable behavior at `236c04e35`: open / close / toggle (no conversation
 * state — that moved to `brahma-console-session.ts`), `isDefault` on the
 * picker profile, and `isEligible = profiles.length > 0`.
 */

import { TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { ConnectionProfileDto } from '../core/core-contract';
import { BrahmaConsoleService, mapBrahmaConnectionProfiles } from './brahma-console.service';
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

describe('BrahmaConsoleService (v4 brahma-console-provider.tsx at 236c04e35)', () => {
  afterEach(() => {
    localStorage.clear();
    TestBed.resetTestingModule();
  });

  it('openConsole only opens — it no longer resets a conversation (v4 99463fc56)', () => {
    localStorage.setItem('quilltap:brahma-floating-last-id', 'chat-9');
    localStorage.setItem('quilltap:brahma-console-last-id', 'chat-8');
    const svc = makeService();
    svc.openConsole();
    expect(svc.isOpen()).toBe(true);
    // The regression fence for the pre-99463fc56 reset: both surfaces' ids stand.
    expect(localStorage.getItem('quilltap:brahma-floating-last-id')).toBe('chat-9');
    expect(localStorage.getItem('quilltap:brahma-console-last-id')).toBe('chat-8');
  });

  it('closeConsole closes; toggleConsole flips', () => {
    const svc = makeService();
    svc.toggleConsole();
    expect(svc.isOpen()).toBe(true);
    svc.toggleConsole();
    expect(svc.isOpen()).toBe(false);
    svc.openConsole();
    svc.closeConsole();
    expect(svc.isOpen()).toBe(false);
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

  it('mapBrahmaConnectionProfiles carries isDefault (v4 99463fc56)', () => {
    const rows = [
      { id: 'a', name: 'A', provider: 'OPENAI', modelName: 'm', isDefault: true },
      { id: 'b', name: 'B', provider: 'OPENAI', modelName: 'm', isDefault: false },
      { id: 'c', name: 'C', provider: 'OPENAI', modelName: 'm' },
    ] as unknown as ConnectionProfileDto[];
    expect(mapBrahmaConnectionProfiles(rows).map((p) => p.isDefault)).toEqual([true, false, false]);
  });
});
