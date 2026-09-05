/**
 * `HelpService` — asserted against v4 `components/providers/help-chat-provider.tsx`
 * at `d883a5ee1`.
 *
 * The storage semantics are the load-bearing part: two keys with DIFFERENT
 * encodings (a JSON array and a plain string), and a one-shot repair for values
 * a prior v4 build double-quoted. Getting either wrong is invisible until an
 * operator's last chat stops reopening, so both are pinned, the legacy repair
 * with its own mutation.
 */

import { TestBed } from '@angular/core/testing';
import { NavigationEnd, Router } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { Subject } from 'rxjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { HelpApi, type HelpEligibility } from './help-wire';
import {
  HELP_ELIGIBILITY_QUERY_KEY,
  HelpService,
  STORAGE_KEY_LAST_CHAT,
  STORAGE_KEY_SELECTED,
} from './help.service';

const CAPABLE = {
  id: 'c-1',
  name: 'Jeeves',
  avatarUrl: null,
  defaultHelpToolsEnabled: true,
  connectionProfileId: 'p-1',
  hasToolCapableProfile: true,
};
const INCAPABLE = { ...CAPABLE, id: 'c-2', name: 'Bertie', hasToolCapableProfile: false };

function eligibility(characters: (typeof CAPABLE)[], flag?: boolean): HelpEligibility {
  return {
    eligible: flag ?? characters.some((c) => c.hasToolCapableProfile),
    characters,
    reasons: [],
  };
}

let routerEvents: Subject<unknown>;
let updateContext: ReturnType<typeof vi.fn>;

function setup(chars: (typeof CAPABLE)[] = [CAPABLE], flag?: boolean): HelpService {
  routerEvents = new Subject<unknown>();
  updateContext = vi.fn(async () => undefined);
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      {
        provide: HelpApi,
        useValue: {
          eligibility: async () => eligibility(chars, flag),
          chatUpdateContext: updateContext,
        } as Partial<HelpApi>,
      },
      { provide: Router, useValue: { events: routerEvents.asObservable(), url: '/salon' } },
    ],
  });
  return TestBed.inject(HelpService);
}

async function settle(ticks = 6): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await new Promise((r) => setTimeout(r, 0));
    TestBed.tick();
  }
}

describe('HelpService — storage semantics', () => {
  beforeEach(() => localStorage.clear());
  afterEach(() => {
    localStorage.clear();
    TestBed.resetTestingModule();
  });

  it('reads the last chat id as a PLAIN string', () => {
    localStorage.setItem(STORAGE_KEY_LAST_CHAT, 'chat-abc');
    expect(setup().currentChatId()).toBe('chat-abc');
  });

  it('repairs a legacy double-quoted value AND rewrites the key', () => {
    // A prior v4 build wrote this key through JSON.stringify. The repair is
    // one-shot: it strips the quotes and puts the clean value back, so the
    // next read needs no repair at all. Dropping it silently strands every
    // instance written by that build.
    localStorage.setItem(STORAGE_KEY_LAST_CHAT, '"chat-legacy"');
    expect(setup().currentChatId()).toBe('chat-legacy');
    expect(localStorage.getItem(STORAGE_KEY_LAST_CHAT)).toBe('chat-legacy');
  });

  it('leaves a value quoted on only one side alone', () => {
    localStorage.setItem(STORAGE_KEY_LAST_CHAT, '"chat-odd');
    expect(setup().currentChatId()).toBe('"chat-odd');
  });

  it('writes the last chat id plain, and removes the key on null', () => {
    const svc = setup();
    svc.setCurrentChatId('chat-x');
    expect(localStorage.getItem(STORAGE_KEY_LAST_CHAT)).toBe('chat-x');
    svc.setCurrentChatId(null);
    expect(localStorage.getItem(STORAGE_KEY_LAST_CHAT)).toBeNull();
  });

  it('reads and writes the selection as a JSON array', () => {
    localStorage.setItem(STORAGE_KEY_SELECTED, JSON.stringify(['a', 'b']));
    const svc = setup();
    expect(svc.selectedCharacterIds()).toEqual(['a', 'b']);
    svc.toggleCharacter('c');
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY_SELECTED)!)).toEqual(['a', 'b', 'c']);
    svc.toggleCharacter('a');
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY_SELECTED)!)).toEqual(['b', 'c']);
  });

  it('survives an unreadable selection value', () => {
    localStorage.setItem(STORAGE_KEY_SELECTED, '{not json');
    expect(setup().selectedCharacterIds()).toEqual([]);
  });
});

describe('HelpService — eligibility', () => {
  beforeEach(() => localStorage.clear());
  afterEach(() => {
    localStorage.clear();
    TestBed.resetTestingModule();
  });

  it('derives isEligible from the LIST, not the payload flag', async () => {
    // The `eligible` flag is forced TRUE against a list nothing in which is
    // tool-capable. v4 reads the list, so the button must stay disabled: an
    // enabled Help button over an empty character picker is the failure this
    // guards. (Measured: with the flag and the list agreeing by construction, a
    // mutation reading the flag survived all seventeen cases.)
    const svc = setup([INCAPABLE], true);
    await settle();
    expect(svc.eligibleCharacters()).toHaveLength(1);
    expect(svc.isEligible()).toBe(false);
    expect(svc.toolCapableCharacters()).toEqual([]);
  });

  it('auto-selects the first tool-capable character when nothing is picked', async () => {
    const svc = setup([INCAPABLE, CAPABLE]);
    await settle();
    expect(svc.selectedCharacterIds()).toEqual(['c-1']);
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY_SELECTED)!)).toEqual(['c-1']);
  });

  it('never overrides an existing selection', async () => {
    localStorage.setItem(STORAGE_KEY_SELECTED, JSON.stringify(['c-9']));
    const svc = setup([CAPABLE]);
    await settle();
    expect(svc.selectedCharacterIds()).toEqual(['c-9']);
  });

  it('uses the v4 eligibility query key', () => {
    expect(HELP_ELIGIBILITY_QUERY_KEY).toEqual(['help-chat', 'eligibility']);
  });
});

describe('HelpService — open, close, and the page watcher', () => {
  beforeEach(() => localStorage.clear());
  afterEach(() => {
    localStorage.clear();
    TestBed.resetTestingModule();
  });

  it('opening ALWAYS resets to the launcher', () => {
    const svc = setup();
    svc.setCurrentChatId('chat-x');
    svc.openHelpChat();
    expect(svc.isOpen()).toBe(true);
    expect(svc.currentChatId()).toBeNull();
    expect(localStorage.getItem(STORAGE_KEY_LAST_CHAT)).toBeNull();
  });

  it('re-anchors an open chat when the route changes', () => {
    const svc = setup();
    svc.isOpen.set(true);
    svc.setCurrentChatId('chat-x');
    routerEvents.next(new NavigationEnd(1, '/aurora', '/aurora'));
    expect(updateContext).toHaveBeenCalledWith('chat-x', '/aurora');
    expect(svc.currentPageUrl()).toBe('/aurora');
  });

  it('does not re-anchor while the dialog is closed', () => {
    const svc = setup();
    svc.setCurrentChatId('chat-x');
    routerEvents.next(new NavigationEnd(1, '/aurora', '/aurora'));
    expect(updateContext).not.toHaveBeenCalled();
    // The page still tracks — only the PATCH is gated.
    expect(svc.currentPageUrl()).toBe('/aurora');
  });

  it('does not re-anchor with no chat in hand', () => {
    const svc = setup();
    svc.isOpen.set(true);
    routerEvents.next(new NavigationEnd(1, '/aurora', '/aurora'));
    expect(updateContext).not.toHaveBeenCalled();
  });

  it('ignores a navigation that does not change the path', () => {
    const svc = setup();
    svc.isOpen.set(true);
    svc.setCurrentChatId('chat-x');
    routerEvents.next(new NavigationEnd(1, svc.currentPageUrl(), svc.currentPageUrl()));
    expect(updateContext).not.toHaveBeenCalled();
  });

  it('carries the PATH ONLY, never the query string', () => {
    // v4 reads `usePathname()`, which Next's own typings document as dropping
    // the query. The consequence is a real v4 defect this port reproduces
    // rather than repairs: `getCategoryForUrl('/settings')` can never reach the
    // seven `?tab=` rows of URL_CATEGORY_MAP, so the Guide always expands
    // Settings & System from any settings screen. See `help.service.ts`
    // `currentPath` — a candidate upstream filing, pinned here so neither side
    // drifts silently.
    const svc = setup();
    svc.isOpen.set(true);
    svc.setCurrentChatId('chat-x');
    routerEvents.next(new NavigationEnd(1, '/settings?tab=memory', '/settings?tab=memory'));
    expect(svc.currentPageUrl()).toBe('/settings');
    expect(updateContext).toHaveBeenCalledWith('chat-x', '/settings');
  });

  it('swallows a failed context PATCH', async () => {
    const svc = setup();
    const spy = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    updateContext.mockRejectedValueOnce(new Error('nope'));
    svc.isOpen.set(true);
    svc.setCurrentChatId('chat-x');
    routerEvents.next(new NavigationEnd(1, '/aurora', '/aurora'));
    await settle(2);
    expect(spy).toHaveBeenCalledWith('Failed to update help chat context:', expect.anything());
    spy.mockRestore();
  });
});
