/**
 * WorkspaceService — store semantics (v4 `workspace-provider.tsx`).
 *
 * The reducer/persistence behaviour is corpus-diffed in
 * `core/workspace-core.spec.ts`; this covers the store's own additions: uuid
 * minting + de-dupe-resolved openTab id, refreshTab (the focus:false payload
 * refresh), debounced persistence gated on hydration, and one-shot hydrate with
 * the chat-existence prune seam.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../core/core-client';
import { PERSIST_DEBOUNCE_MS, WorkspaceService } from './workspace.service';
import { WORKSPACE_STORAGE_KEY_BASE } from './core/persistence';

function makeCore(chatIds: string[] = []): CoreClient {
  return {
    dispatchExpect: vi.fn(async () => chatIds.map((id) => ({ id }))),
  } as unknown as CoreClient;
}

function make(chatIds: string[] = []): WorkspaceService {
  return new WorkspaceService(makeCore(chatIds));
}

describe('WorkspaceService', () => {
  beforeEach(() => {
    localStorage.clear();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('starts with a single home tab, not yet hydrated', () => {
    const svc = make();
    expect(svc.hydrated()).toBe(false);
    expect(Object.keys(svc.state().tabs)).toEqual(['home']);
    expect(svc.state().panes.left.order).toEqual(['home']);
  });

  it('openTab mints a fresh id for a new tab and returns the existing id on de-dupe', () => {
    const svc = make();
    const first = svc.openTab('aurora');
    expect(first).not.toBe('home');
    expect(svc.state().panes.left.order).toEqual(['home', first]);
    // Re-opening the singleton returns the SAME id and adds no tab.
    const again = svc.openTab('aurora');
    expect(again).toBe(first);
    expect(svc.state().panes.left.order).toEqual(['home', first]);
  });

  it('de-dupes salon tabs by chatId', () => {
    const svc = make();
    const a = svc.openTab('salon', { chatId: 'c1' });
    const b = svc.openTab('salon', { chatId: 'c1' });
    expect(b).toBe(a);
    const c = svc.openTab('salon', { chatId: 'c2' });
    expect(c).not.toBe(a);
    expect(svc.state().panes.left.order).toEqual(['home', a, c]);
  });

  it('refreshTab updates payload/title in place without activating or focusing', () => {
    const svc = make();
    const settings = svc.openTab('settings', { tab: 'system' }, { title: 'System' });
    svc.openTab('aurora'); // now aurora is active
    const activeBefore = svc.state().panes.left.activeTabId;
    svc.refreshTab('settings', { tab: 'memory' }, 'Memory');
    expect(svc.state().tabs[settings].payload).toEqual({ tab: 'memory' });
    expect(svc.state().tabs[settings].title).toBe('Memory');
    // Active tab unchanged (refresh does not focus).
    expect(svc.state().panes.left.activeTabId).toBe(activeBefore);
  });

  it('does not persist before hydration', () => {
    vi.useFakeTimers();
    const svc = make();
    svc.openTab('aurora');
    vi.advanceTimersByTime(PERSIST_DEBOUNCE_MS + 10);
    expect(localStorage.getItem(WORKSPACE_STORAGE_KEY_BASE)).toBeNull();
  });

  it('persists (debounced) after hydration', async () => {
    const svc = make();
    await svc.hydrateOnce();
    expect(svc.hydrated()).toBe(true);
    vi.useFakeTimers();
    svc.openTab('aurora');
    expect(localStorage.getItem(WORKSPACE_STORAGE_KEY_BASE)).toBeNull(); // not yet
    vi.advanceTimersByTime(PERSIST_DEBOUNCE_MS + 10);
    const raw = localStorage.getItem(WORKSPACE_STORAGE_KEY_BASE);
    expect(raw).toBeTruthy();
    expect(JSON.parse(raw as string).tabs.home).toBeDefined();
  });

  it('hydrateOnce restores a stored layout and prunes tabs whose chat is gone', async () => {
    // Seed a layout: home + salon(c1) + salon(cGONE).
    const seed = {
      tabs: {
        home: { id: 'home', kind: 'home', title: 'Home', icon: 'sparkles' },
        s1: { id: 's1', kind: 'salon', title: 'Chat 1', payload: { chatId: 'c1' } },
        s2: { id: 's2', kind: 'salon', title: 'Chat 2', payload: { chatId: 'cGONE' } },
      },
      panes: { left: { order: ['home', 's1', 's2'], activeTabId: 'home' }, right: null },
      focusedPane: 'left',
      splitRatio: 0.5,
    };
    localStorage.setItem(WORKSPACE_STORAGE_KEY_BASE, JSON.stringify(seed));
    const svc = make(['c1']); // only c1 still exists
    await svc.hydrateOnce();
    expect(svc.state().tabs.s1).toBeDefined();
    expect(svc.state().tabs.s2).toBeUndefined(); // dead chat pruned
    expect(svc.state().panes.left.order).toEqual(['home', 's1']);
  });

  it('hydrateOnce is idempotent', async () => {
    const svc = make();
    const p1 = svc.hydrateOnce();
    const p2 = svc.hydrateOnce();
    expect(p1).toBe(p2);
    await p1;
  });
});
