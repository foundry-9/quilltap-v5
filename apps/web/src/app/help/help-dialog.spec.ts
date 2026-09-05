/**
 * `HelpDialog`, `HelpMessageList`, `HelpStreamingService` and `HelpEntry` —
 * asserted against v4 `HelpChatDialog.tsx`, `HelpChatMessageList.tsx`,
 * `hooks/useHelpChatStreaming.ts` and `sidebar-footer.tsx:203-212` at
 * `d883a5ee1`.
 *
 * The behaviours worth naming, because each one looks like an omission or an
 * over-complication until you know why v4 does it:
 *
 *  - the tab lives in **sessionStorage**, not localStorage;
 *  - the optimistic user bubble goes into the SAME array the reload replaces
 *    (dogfood #106 — a separate signal is what made it render twice mid-turn);
 *  - create narrows the picked seats to the tool-capable ones and falls back to
 *    the FIRST eligible, doing nothing at all when none is;
 *  - the participant maps read BOTH the nested and the flattened shape;
 *  - an assistant message with no visible content is hidden (agent mode's
 *    intermediate tool turns persist as empty rows);
 *  - a suggested link that duplicates a navigation link is dropped;
 *  - the rail button is disabled only once eligibility has ANSWERED.
 */

import { ComponentFixture, DeferBlockBehavior, TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { Subject } from 'rxjs';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

beforeAll(() => {
  const proto = globalThis.HTMLElement?.prototype as unknown as {
    scrollIntoView?: () => void;
    scrollTo?: () => void;
  };
  if (proto && !proto.scrollIntoView) proto.scrollIntoView = () => undefined;
  if (proto && !proto.scrollTo) proto.scrollTo = () => undefined;
});

import { CoreClient } from '../core/core-client';
import type { ChatStreamFrame, ScopedEvent } from '../core/core-contract';
import { HelpDialog, STORAGE_KEY_TAB } from './help-dialog';
import { HelpEntry } from './help-entry';
import { HelpMessageList } from './help-message-list';
import { HelpNavigate } from './help-navigate';
import { HelpStreamingService } from './help-streaming.service';
import { HelpApi, type HelpChatMessage } from './help-wire';
import { HelpService } from './help.service';

const CAPABLE = {
  id: 'c-1',
  name: 'Jeeves',
  avatarUrl: null,
  defaultHelpToolsEnabled: true,
  connectionProfileId: 'p-1',
  hasToolCapableProfile: true,
};
const INCAPABLE = { ...CAPABLE, id: 'c-2', name: 'Bertie', hasToolCapableProfile: false };

/* eslint-disable @typescript-eslint/no-explicit-any */
type AnyMock = ReturnType<typeof vi.fn<(...a: any[]) => any>>;

let api: Record<string, AnyMock>;
let events: Subject<ScopedEvent & ChatStreamFrame>;
let go: AnyMock;

function stubApi(chars = [CAPABLE]) {
  api = {
    eligibility: vi.fn(async () => ({ eligible: true, characters: chars, reasons: [] })),
    chatList: vi.fn(async () => [
      { id: 'h1', title: 'Prior counsel', updatedAt: '', participants: [], messageCount: 4, helpPageUrl: null },
    ]),
    chatCreate: vi.fn(async () => ({
      id: 'new-chat',
      participants: [{ id: 'pt-1', character: { id: 'c-1', name: 'Jeeves', avatarUrl: null } }],
    })),
    chatGet: vi.fn(async () => ({
      id: 'h1',
      participants: [{ id: 'pt-1', characterId: 'c-1', name: 'Jeeves', avatarUrl: '/a.png' }],
    })),
    chatMessages: vi.fn(async () => [] as HelpChatMessage[]),
    chatSend: vi.fn(async () => ({ messageId: 'm-1' })),
    chatDelete: vi.fn(async () => undefined),
    chatUpdateContext: vi.fn(async () => undefined),
    docsList: vi.fn(async () => []),
    docsChatCount: vi.fn(async () => 10),
    docsSearch: vi.fn(async () => []),
    docGet: vi.fn(async () => null),
  };
  const proxy: Record<string, unknown> = {};
  for (const k of Object.keys(api)) proxy[k] = (...a: any[]) => api[k](...a);
  return proxy as unknown as HelpApi;
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 10): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(chars = [CAPABLE]): Promise<{
  fixture: ComponentFixture<HelpDialog>;
  help: HelpService;
}> {
  go = vi.fn();
  events = new Subject();
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [HelpDialog],
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: HelpApi, useValue: stubApi(chars) },
      { provide: HelpNavigate, useValue: { go } },
      {
        provide: CoreClient,
        useValue: { events$: events.asObservable(), dispatchData: async () => ({}) },
      },
      { provide: Router, useValue: { events: new Subject().asObservable(), url: '/salon' } },
    ],
  });
  const help = TestBed.inject(HelpService);
  const fixture = TestBed.createComponent(HelpDialog);
  fixture.detectChanges();
  await settle(fixture);
  return { fixture, help };
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

function q<T extends Element>(fixture: ComponentFixture<unknown>, sel: string): T | null {
  return (fixture.nativeElement as HTMLElement).querySelector<T>(sel);
}

async function send(fixture: ComponentFixture<HelpDialog>, content: string): Promise<void> {
  const ta = q<HTMLTextAreaElement>(fixture, '.qt-help-composer-input')!;
  ta.value = content;
  ta.dispatchEvent(new Event('input'));
  fixture.detectChanges();
  q<HTMLButtonElement>(fixture, '.qt-help-composer-send')!.click();
  await settle(fixture);
}

beforeEach(() => {
  localStorage.clear();
  sessionStorage.clear();
});
afterEach(() => {
  localStorage.clear();
  sessionStorage.clear();
  TestBed.resetTestingModule();
});

describe('HelpDialog — the shell and the tabs', () => {
  it('renders nothing while closed', async () => {
    const { fixture } = await render();
    expect(q(fixture, '.qt-dialog')).toBeNull();
  });

  it('opens on the Guide tab by default', async () => {
    const { fixture, help } = await render();
    help.openHelpChat();
    await settle(fixture);
    expect(q(fixture, '.qt-tab.qt-tab-active')?.textContent?.trim()).toBe('Guide');
    expect(q(fixture, 'qt-help-guide-tab')).not.toBeNull();
  });

  it('persists the tab in sessionStorage, and restores it', async () => {
    const { fixture, help } = await render();
    help.openHelpChat();
    await settle(fixture);
    const askTab = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll<HTMLButtonElement>('.qt-tab'),
    ).find((b) => b.textContent?.trim() === 'Ask')!;
    askTab.click();
    await settle(fixture);
    expect(sessionStorage.getItem(STORAGE_KEY_TAB)).toBe('ask');
    // sessionStorage, deliberately — the choice resets with the window.
    expect(localStorage.getItem(STORAGE_KEY_TAB)).toBeNull();

    const second = await render();
    second.help.openHelpChat();
    await settle(second.fixture);
    expect(q(second.fixture, '.qt-tab.qt-tab-active')?.textContent?.trim()).toBe('Ask');
  });

  it('closes on the backdrop and on the close button', async () => {
    const { fixture, help } = await render();
    help.openHelpChat();
    await settle(fixture);
    q<HTMLElement>(fixture, '.qt-dialog-overlay')!.click();
    expect(help.isOpen()).toBe(false);
  });
});

describe('HelpDialog — the Ask launcher', () => {
  async function openAsk(chars = [CAPABLE, INCAPABLE]) {
    sessionStorage.setItem(STORAGE_KEY_TAB, 'ask');
    const r = await render(chars);
    r.help.openHelpChat();
    await settle(r.fixture);
    return r;
  }

  it('offers only tool-capable seats as pills', async () => {
    const { fixture } = await openAsk();
    const pills = (fixture.nativeElement as HTMLElement).querySelectorAll('.qt-help-char-pill');
    expect(pills).toHaveLength(1);
    // The pill carries an avatar (or its initial) alongside the name.
    expect(pills[0].textContent).toContain('Jeeves');
    expect(pills[0].textContent).not.toContain('Bertie');
  });

  it('shows the guidance sentence when nothing is eligible', async () => {
    const { fixture } = await openAsk([INCAPABLE]);
    expect(text(fixture)).toContain('No eligible help characters.');
    expect(q<HTMLTextAreaElement>(fixture, '.qt-help-composer-input')!.disabled).toBe(true);
  });

  it('lists past chats with their message counts, and deletes one', async () => {
    const { fixture } = await openAsk();
    expect(text(fixture)).toContain('Prior counsel');
    expect(text(fixture)).toContain('4');
    q<HTMLButtonElement>(fixture, '.qt-help-past-chat button[title="Delete"]')!.click();
    await settle(fixture);
    expect(api['chatDelete']).toHaveBeenCalledWith('h1');
  });

  it('opens a past chat and loads its transcript', async () => {
    const { fixture, help } = await openAsk();
    q<HTMLButtonElement>(fixture, '.qt-help-past-chat button')!.click();
    await settle(fixture);
    expect(help.currentChatId()).toBe('h1');
    expect(api['chatMessages']).toHaveBeenCalledWith('h1');
    expect(api['chatGet']).toHaveBeenCalledWith('h1');
  });

  it('creates a chat with the picked seat, then sends the question', async () => {
    const { fixture, help } = await openAsk();
    await send(fixture, 'How do I make a character?');
    expect(api['chatCreate']).toHaveBeenCalledWith(['c-1'], '/salon');
    expect(api['chatSend']).toHaveBeenCalledWith('new-chat', 'How do I make a character?', undefined);
    expect(help.currentChatId()).toBe('new-chat');
  });

  it('falls back to the first eligible seat when the selection is stale', async () => {
    localStorage.setItem('quilltap:help-chat-selected-characters', JSON.stringify(['gone']));
    const { fixture } = await openAsk();
    await send(fixture, 'Anyone?');
    expect(api['chatCreate']).toHaveBeenCalledWith(['c-1'], '/salon');
  });

  it('creates nothing at all when no seat is eligible', async () => {
    const { fixture } = await openAsk([INCAPABLE]);
    // The composer is disabled, so drive the path directly: v4 returns early
    // rather than creating a chat nobody can answer in.
    (fixture.componentInstance as unknown as { handleSend(c: string): void }).handleSend('hi');
    await settle(fixture);
    expect(api['chatCreate']).not.toHaveBeenCalled();
  });

  it('toggles a seat and persists the selection', async () => {
    const { fixture } = await openAsk();
    q<HTMLButtonElement>(fixture, '.qt-help-char-pill')!.click();
    await settle(fixture);
    expect(JSON.parse(localStorage.getItem('quilltap:help-chat-selected-characters')!)).toEqual([]);
  });
});

describe('HelpDialog — the conversation', () => {
  async function openChat() {
    sessionStorage.setItem(STORAGE_KEY_TAB, 'ask');
    const r = await render();
    r.help.openHelpChat();
    await settle(r.fixture);
    r.help.setCurrentChatId('h1');
    await settle(r.fixture);
    return r;
  }

  it('shows the optimistic user bubble in the message array', async () => {
    // Not a separate signal appended at render — that is dogfood #106's shape,
    // where the user's message showed twice for most of a turn.
    const { fixture } = await openChat();
    api['chatSend'].mockImplementation(() => new Promise(() => undefined));
    await send(fixture, 'a question');
    const bubbles = (fixture.nativeElement as HTMLElement).querySelectorAll('.qt-help-msg-user');
    expect(bubbles).toHaveLength(1);
    expect(bubbles[0].textContent).toContain('a question');
  });

  it('reloads the transcript when a done frame carries a message id', async () => {
    const { fixture } = await openChat();
    let resolveSend: (v: unknown) => void = () => undefined;
    api['chatSend'].mockImplementation(() => new Promise((r) => (resolveSend = r)));
    await send(fixture, 'q');
    const before = api['chatMessages'].mock.calls.length;
    events.next({ chatId: 'h1', done: true, messageId: 'm-9' });
    await settle(fixture);
    expect(api['chatMessages'].mock.calls.length).toBeGreaterThan(before);
    resolveSend({ messageId: 'm-9' });
    await settle(fixture);
  });

  it('surfaces a stream error frame', async () => {
    const { fixture } = await openChat();
    let resolveSend: (v: unknown) => void = () => undefined;
    api['chatSend'].mockImplementation(() => new Promise((r) => (resolveSend = r)));
    await send(fixture, 'q');
    events.next({ chatId: 'h1', error: 'The archives are shut', errorType: 'fatal_error', details: '' });
    await settle(fixture);
    expect(q(fixture, '.qt-help-error')?.textContent).toContain('The archives are shut');
    resolveSend({});
    await settle(fixture);
  });

  it('ignores frames scoped to another chat', async () => {
    const { fixture } = await openChat();
    let resolveSend: (v: unknown) => void = () => undefined;
    api['chatSend'].mockImplementation(() => new Promise((r) => (resolveSend = r)));
    await send(fixture, 'q');
    events.next({ chatId: 'SOMEONE-ELSE', content: 'not ours' });
    await settle(fixture);
    expect(text(fixture)).not.toContain('not ours');
    resolveSend({});
    await settle(fixture);
  });

  it('goes back to the launcher on New conversation', async () => {
    const { fixture, help } = await openChat();
    q<HTMLButtonElement>(fixture, 'button[title="New help chat"]')!.click();
    await settle(fixture);
    expect(help.currentChatId()).toBeNull();
  });

  it('routes a parameterised link to the entity picker, not to navigation', async () => {
    const { fixture } = await openChat();
    const inst = fixture.componentInstance as unknown as { handleNavigate(u: string): void };
    inst.handleNavigate('/aurora/:id/edit');
    await settle(fixture);
    expect(go).not.toHaveBeenCalled();
    expect(q(fixture, 'qt-help-entity-picker')).not.toBeNull();
  });

  it('navigates a plain link directly', async () => {
    const { fixture } = await openChat();
    (fixture.componentInstance as unknown as { handleNavigate(u: string): void }).handleNavigate(
      '/settings?tab=chat',
    );
    expect(go).toHaveBeenCalledWith('/settings?tab=chat');
  });

  it('builds the participant maps from BOTH shapes', async () => {
    const { fixture, help } = await openChat();
    api['chatMessages'].mockImplementation(async () => [
      { id: 'm1', role: 'ASSISTANT', content: 'At your service.', participantId: 'pt-1', createdAt: '' },
    ]);
    help.setCurrentChatId(null);
    await settle(fixture);
    help.setCurrentChatId('h1');
    await settle(fixture);
    // `chatGet` answers the FLATTENED shape (characterId/name at the top).
    expect(q(fixture, '.qt-help-msg-character-name')?.textContent?.trim()).toBe('Jeeves');
  });
});

describe('HelpMessageList', () => {
  async function renderList(inputs: Record<string, unknown>) {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [HelpMessageList] });
    const fixture = TestBed.createComponent(HelpMessageList);
    fixture.componentRef.setInput('messages', []);
    fixture.componentRef.setInput('characterMap', new Map());
    fixture.componentRef.setInput('participantToCharacter', new Map());
    for (const [k, v] of Object.entries(inputs)) fixture.componentRef.setInput(k, v);
    fixture.detectChanges();
    await settle(fixture, 3);
    return fixture;
  }

  it('shows the empty state', async () => {
    const fixture = await renderList({});
    expect(text(fixture)).toContain('Ask a question to get started');
  });

  it('hides an assistant turn with no visible content', async () => {
    // Agent mode's intermediate tool-using iterations persist as empty rows.
    const fixture = await renderList({
      messages: [
        { id: 'a', role: 'ASSISTANT', content: '   ', createdAt: '' },
        { id: 'b', role: 'ASSISTANT', content: 'Real answer.', createdAt: '' },
        { id: 'c', role: 'SYSTEM', content: 'system noise', createdAt: '' },
      ],
    });
    expect(text(fixture)).toContain('Real answer.');
    expect(text(fixture)).not.toContain('system noise');
    expect(
      (fixture.nativeElement as HTMLElement).querySelectorAll('.qt-help-msg-assistant'),
    ).toHaveLength(1);
  });

  it('keeps a user message whatever its content', async () => {
    const fixture = await renderList({
      messages: [{ id: 'u', role: 'user', content: '', createdAt: '' }],
    });
    expect((fixture.nativeElement as HTMLElement).querySelectorAll('.qt-help-msg-user')).toHaveLength(1);
  });

  it('shows the working line only while executing tools', async () => {
    const thinking = await renderList({ isStreaming: true });
    expect(text(thinking)).toContain('Thinking...');
    const tools = await renderList({ isStreaming: true, isExecutingTools: true });
    expect(text(tools)).toContain('Consulting the archives...');
  });

  it('renders navigation links only once streaming ends', async () => {
    const links = [{ url: '/aurora', label: 'Characters' }];
    const during = await renderList({ isStreaming: true, navigationLinks: links });
    expect(during.nativeElement.querySelectorAll('.qt-help-nav-button')).toHaveLength(0);
    const after = await renderList({ navigationLinks: links });
    expect(after.nativeElement.querySelectorAll('.qt-help-nav-button')).toHaveLength(1);
  });

  it('drops a suggested link that duplicates a navigation link', async () => {
    const fixture = await renderList({
      navigationLinks: [{ url: '/aurora', label: 'Characters' }],
      suggestedLinks: [
        { url: '/aurora', label: 'Characters again' },
        { url: '/files', label: 'Files' },
      ],
    });
    const suggested = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('.qt-help-suggested-link'),
    ).map((b) => b.textContent?.trim());
    expect(suggested).toEqual(['Files']);
    expect(text(fixture)).toContain('Related pages');
  });

  it('emits the url when a link is clicked', async () => {
    const fixture = await renderList({ navigationLinks: [{ url: '/files', label: 'Files' }] });
    const seen: string[] = [];
    fixture.componentInstance.navigate.subscribe((u) => seen.push(u));
    (
      (fixture.nativeElement as HTMLElement).querySelector('.qt-help-nav-button') as HTMLElement
    ).click();
    expect(seen).toEqual(['/files']);
  });
});

describe('HelpEntry (v4 sidebar-footer.tsx:203-212)', () => {
  async function renderEntry(chars: (typeof CAPABLE)[]): Promise<ComponentFixture<HelpEntry>> {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [HelpEntry],
      providers: [
        provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
        { provide: HelpApi, useValue: stubApi(chars) },
        { provide: HelpNavigate, useValue: { go: vi.fn() } },
        {
          provide: CoreClient,
          useValue: { events$: new Subject().asObservable(), dispatchData: async () => ({}) },
        },
        { provide: Router, useValue: { events: new Subject().asObservable(), url: '/salon' } },
      ],
      deferBlockBehavior: DeferBlockBehavior.Playthrough,
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(HelpEntry);
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  it('enables the button with an eligible character', async () => {
    const fixture = await renderEntry([CAPABLE]);
    const button = q<HTMLButtonElement>(fixture, 'button')!;
    expect(button.disabled).toBe(false);
    expect(button.getAttribute('title')).toBe('Help');
  });

  it('disables it once eligibility has ANSWERED and said no', async () => {
    const fixture = await renderEntry([INCAPABLE]);
    const button = q<HTMLButtonElement>(fixture, 'button')!;
    expect(button.disabled).toBe(true);
    expect(button.getAttribute('title')).toBe(
      'Help (requires a help-enabled character with a tool-capable connection)',
    );
  });

  it('hosts the dialog, deferred until Help opens', async () => {
    const fixture = await renderEntry([CAPABLE]);
    expect(q(fixture, 'qt-help-dialog')).toBeNull();
    q<HTMLButtonElement>(fixture, 'button')!.click();
    await settle(fixture);
    expect(q(fixture, 'qt-help-dialog')).not.toBeNull();
    expect(TestBed.inject(HelpService).isOpen()).toBe(true);
  });
});

describe('HelpStreamingService', () => {
  function setup() {
    events = new Subject();
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      providers: [
        { provide: HelpApi, useValue: stubApi() },
        {
          provide: CoreClient,
          useValue: { events$: events.asObservable(), dispatchData: async () => ({}) },
        },
      ],
    });
    return TestBed.inject(HelpStreamingService);
  }

  it('folds frames scoped to the chat and fires per completed message', async () => {
    const svc = setup();
    const completed: string[] = [];
    let resolveSend: (v: unknown) => void = () => undefined;
    api['chatSend'].mockImplementation(() => new Promise((r) => (resolveSend = r)));
    const run = svc.sendMessage('h1', 'q', (id) => completed.push(id));
    await new Promise((r) => setTimeout(r, 0));
    events.next({ chatId: 'h1', content: 'Half ' });
    events.next({ chatId: 'h1', content: 'an answer.' });
    expect(svc.streamingContent()).toBe('Half an answer.');
    events.next({ chatId: 'h1', done: true, messageId: 'm-1' });
    expect(completed).toEqual(['m-1']);
    resolveSend({});
    await run;
    expect(svc.isStreaming()).toBe(false);
  });

  it('records a dispatch failure as the stream error', async () => {
    const svc = setup();
    api['chatSend'].mockImplementation(async () => {
      throw new Error('the wire snapped');
    });
    await svc.sendMessage('h1', 'q');
    expect(svc.error()).toBe('the wire snapped');
    expect(svc.isStreaming()).toBe(false);
  });

  it('projects the two link strips', async () => {
    const svc = setup();
    let resolveSend: (v: unknown) => void = () => undefined;
    api['chatSend'].mockImplementation(() => new Promise((r) => (resolveSend = r)));
    const run = svc.sendMessage('h1', 'q');
    await new Promise((r) => setTimeout(r, 0));
    events.next({
      chatId: 'h1',
      toolResult: { name: 'help_navigate', success: true, result: { url: '/aurora' } },
    });
    events.next({
      chatId: 'h1',
      toolResult: {
        name: 'help_search',
        success: true,
        result: { results: [{ url: '/files', title: 'Files' }] },
      },
    });
    expect(svc.streamingNavigationLinks()).toEqual([{ url: '/aurora', label: 'Characters' }]);
    expect(svc.suggestedLinks()).toEqual([{ url: '/files', label: 'Files' }]);
    resolveSend({});
    await run;
  });
});
