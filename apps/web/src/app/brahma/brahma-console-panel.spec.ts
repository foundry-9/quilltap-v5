/**
 * BrahmaConsolePanel — asserted against v4 `BrahmaConsolePanel.tsx` at
 * `236c04e35` (`99463fc56`). The eight cases that lived in
 * `brahma-console-dialog.spec.ts` MOVED here (the panel is the console now; the
 * dialog is a frame around it), plus `99463fc56`'s new behaviours: the picker
 * with NO chat open pre-selecting the default profile, `Past conversations`,
 * the deleted-elsewhere fallback, the launcher composer's refocus.
 */

import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { Subject } from 'rxjs';

beforeAll(() => {
  const proto = globalThis.HTMLElement?.prototype as unknown as { scrollIntoView?: () => void };
  if (proto && !proto.scrollIntoView) proto.scrollIntoView = () => undefined;
});

import { CoreClient } from '../core/core-client';
import { CoreDispatchError, type ScopedEvent } from '../core/core-contract';
import { BrahmaConsoleHeaderActions, BrahmaConsolePanel } from './brahma-console-panel';
import {
  STORAGE_KEY_TAB_LAST_CHAT,
  createBrahmaConsoleSession,
  type BrahmaConsoleSession,
} from './brahma-console-session';
import { BrahmaConsoleApi, type BrahmaConsoleMessage, type BrahmaPastChat } from './brahma-wire';

/** A surface around the panel: the header chrome above it, one session. */
@Component({
  selector: 'qt-panel-host',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [BrahmaConsolePanel, BrahmaConsoleHeaderActions],
  template: `
    <qt-brahma-console-header-actions [panel]="panel" />
    <qt-brahma-console-panel #panel [session]="session()" [active]="active()" />
  `,
})
class PanelHost {
  readonly session = input.required<BrahmaConsoleSession>();
  readonly active = input(true);
}

interface Req {
  type: string;
  [k: string]: unknown;
}

interface StubOpts {
  profiles?: {
    id: string;
    name: string;
    provider: string;
    modelName: string;
    isDefault?: boolean;
  }[];
  pastChats?: BrahmaPastChat[];
  messages?: BrahmaConsoleMessage[];
  onDispatch?: (req: Req) => void;
  messagesThrow?: Error;
}

function stubCore(opts: StubOpts, events: Subject<ScopedEvent>): Partial<CoreClient> {
  const profiles = opts.profiles ?? [];
  return {
    events$: events.asObservable(),
    dispatchExpect: (async () => ({
      type: 'connectionProfiles',
      data: { profiles, count: profiles.length },
    })) as unknown as CoreClient['dispatchExpect'],
    dispatchData: (async (req: Req) => {
      opts.onDispatch?.(req);
      switch (req.type) {
        case 'brahmaConsoleList':
          return { chats: opts.pastChats ?? [] };
        case 'brahmaConsoleMessages':
          if (opts.messagesThrow) throw opts.messagesThrow;
          return { messages: opts.messages ?? [] };
        case 'brahmaConsoleGet':
          return { chat: { id: req['chatId'], consoleConnectionProfileId: null } };
        case 'brahmaConsoleCreate':
          return { chat: { id: 'new-chat', consoleConnectionProfileId: 'p1' } };
        case 'brahmaConsoleDelete':
          return { message: 'deleted' };
        case 'brahmaConsoleSend':
          return { messageId: 'msg-1' };
        default:
          return {};
      }
    }) as CoreClient['dispatchData'],
  };
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 8): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(
  opts: StubOpts = {},
  active = true,
): Promise<{ fixture: ComponentFixture<PanelHost>; session: BrahmaConsoleSession }> {
  const events = new Subject<ScopedEvent>();
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [PanelHost],
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: stubCore(opts, events) },
    ],
  });
  const session = createBrahmaConsoleSession(
    STORAGE_KEY_TAB_LAST_CHAT,
    TestBed.inject(BrahmaConsoleApi),
  );
  const fixture = TestBed.createComponent(PanelHost);
  fixture.componentRef.setInput('session', session);
  fixture.componentRef.setInput('active', active);
  fixture.detectChanges();
  await settle(fixture);
  return { fixture, session };
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

const PROFILES = [{ id: 'p1', name: 'Everyday', provider: 'OPENAI', modelName: 'gpt-4o' }];

describe('BrahmaConsolePanel (v4 BrahmaConsolePanel.tsx)', () => {
  afterEach(() => {
    localStorage.clear();
    TestBed.resetTestingModule();
  });

  it('renders the body bare with the launcher composer', async () => {
    const { fixture } = await render({ profiles: PROFILES });
    expect(fixture.nativeElement.querySelector('.qt-dialog-overlay')).toBeNull();
    expect(fixture.nativeElement.querySelector('qt-help-composer')).not.toBeNull();
  });

  it('gates the launcher on eligibility (no profiles → the wants-for-an-engine notice, no chrome)', async () => {
    const { fixture } = await render({ profiles: [] });
    expect(text(fixture)).toContain('The Console wants for an engine');
    expect(fixture.nativeElement.querySelector('qt-brahma-model-picker')).toBeNull();
  });

  it('lists past chats in the launcher and selecting one loads its transcript', async () => {
    const past: BrahmaPastChat[] = [
      {
        id: 'c1',
        title: 'A prior audience',
        createdAt: '2025-12-01T00:00:00.000Z',
        updatedAt: '2026-01-01T00:00:00.000Z',
        lastMessageAt: null,
        messageCount: 3,
        consoleConnectionProfileId: 'p1',
      },
    ];
    const messages: BrahmaConsoleMessage[] = [
      { id: 'm1', role: 'USER', content: 'Ahoy', createdAt: '2026-01-01T00:00:00.000Z' },
    ];
    const { fixture, session } = await render({ profiles: PROFILES, pastChats: past, messages });
    expect(text(fixture)).toContain('Recent Console Conversations');
    expect(text(fixture)).toContain('A prior audience');

    (fixture.nativeElement.querySelector('.qt-help-past-chat button') as HTMLButtonElement).click();
    await settle(fixture);
    expect(session.currentChatId()).toBe('c1');
    expect(text(fixture)).toContain('Ahoy');
  });

  it('with a chat open shows the picker, Past conversations then New conversation, and the transcript', async () => {
    localStorage.setItem(STORAGE_KEY_TAB_LAST_CHAT, 'c9');
    const { fixture } = await render({
      profiles: PROFILES,
      messages: [{ id: 'm1', role: 'ASSISTANT', content: 'At your service.', createdAt: 'x' }],
    });
    expect(fixture.nativeElement.querySelector('qt-brahma-model-picker')).not.toBeNull();
    const titles = [
      ...fixture.nativeElement.querySelectorAll('qt-brahma-console-header-actions button[title]'),
    ].map((b) => (b as HTMLElement).getAttribute('title'));
    expect(titles.slice(-2)).toEqual(['Past conversations', 'New conversation']);
    expect(fixture.nativeElement.querySelector('qt-brahma-console-message-list')).not.toBeNull();
    expect(text(fixture)).toContain('At your service.');
  });

  it('folds the live stream frames then reconciles against the reloaded transcript', async () => {
    localStorage.setItem(STORAGE_KEY_TAB_LAST_CHAT, 'c9');
    const events = new Subject<ScopedEvent>();
    let resolveSend: (v: unknown) => void = () => undefined;
    const sendGate = new Promise((r) => (resolveSend = r));
    let transcript: BrahmaConsoleMessage[] = [];

    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [PanelHost],
      providers: [
        provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
        {
          provide: CoreClient,
          useValue: {
            events$: events.asObservable(),
            dispatchExpect: (async () => ({
              type: 'connectionProfiles',
              data: { profiles: PROFILES, count: 1 },
            })) as unknown as CoreClient['dispatchExpect'],
            dispatchData: (async (req: Req) => {
              switch (req.type) {
                case 'brahmaConsoleMessages':
                  return { messages: transcript };
                case 'brahmaConsoleGet':
                  return { chat: { id: req['chatId'], consoleConnectionProfileId: null } };
                case 'brahmaConsoleSend':
                  await sendGate;
                  return { messageId: 'msg-1' };
                default:
                  return {};
              }
            }) as CoreClient['dispatchData'],
          } as Partial<CoreClient>,
        },
      ],
    });
    const session = createBrahmaConsoleSession(
      STORAGE_KEY_TAB_LAST_CHAT,
      TestBed.inject(BrahmaConsoleApi),
    );
    const fixture = TestBed.createComponent(PanelHost);
    fixture.componentRef.setInput('session', session);
    fixture.detectChanges();
    await settle(fixture);
    const panel = fixture.debugElement.query((d) => d.name === 'qt-brahma-console-panel')
      .componentInstance as unknown as { handleSend(c: string): void; isStreaming(): boolean };

    panel.handleSend('ping');
    await settle(fixture, 2);
    events.next({ chatId: 'c9', content: 'pong' } as ScopedEvent);
    await settle(fixture, 2);
    expect(text(fixture)).toContain('pong');

    transcript = [{ id: 'a1', role: 'ASSISTANT', content: 'pong!', createdAt: 'x' }];
    events.next({ chatId: 'c9', done: true, messageId: 'a1' } as ScopedEvent);
    resolveSend(undefined);
    await settle(fixture, 8);
    expect(text(fixture)).toContain('pong!');
    expect(panel.isStreaming()).toBe(false);
  });

  it('does NOT fetch past chats while a chat is open, nor while inactive (v4 enabled gate)', async () => {
    localStorage.setItem(STORAGE_KEY_TAB_LAST_CHAT, 'c9');
    const seen: string[] = [];
    await render({ profiles: PROFILES, onDispatch: (req) => seen.push(req.type) });
    expect(seen).not.toContain('brahmaConsoleList');
    localStorage.clear();
    const inactive: string[] = [];
    await render({ profiles: PROFILES, onDispatch: (req) => inactive.push(req.type) }, false);
    expect(inactive).not.toContain('brahmaConsoleList');
  });

  it('shows the picker with NO chat open when eligible, pre-selecting the isDefault profile (v4 :220-223)', async () => {
    const profiles = [
      { id: 'p1', name: 'Everyday', provider: 'OPENAI', modelName: 'gpt-4o', isDefault: false },
      { id: 'p2', name: 'Deep', provider: 'ANTHROPIC', modelName: 'opus', isDefault: true },
    ];
    const { fixture } = await render({ profiles });
    const picker = fixture.debugElement.query((d) => d.name === 'qt-brahma-model-picker');
    expect(picker).not.toBeNull();
    expect(picker.componentInstance.activeId()).toBe('p2');
    expect(fixture.nativeElement.querySelector('[title="Past conversations"]')).toBeNull();
  });

  it('Past conversations returns to the launcher, clears the model and refetches the roster', async () => {
    localStorage.setItem(STORAGE_KEY_TAB_LAST_CHAT, 'c9');
    const seen: string[] = [];
    const { fixture, session } = await render({
      profiles: PROFILES,
      onDispatch: (r) => seen.push(r.type),
    });
    session.setActiveConnectionProfileId('p1');
    (
      fixture.nativeElement.querySelector('[title="Past conversations"]') as HTMLButtonElement
    ).click();
    await settle(fixture);
    expect(session.currentChatId()).toBeNull();
    expect(session.activeConnectionProfileId()).toBeNull();
    expect(localStorage.getItem(STORAGE_KEY_TAB_LAST_CHAT)).toBeNull();
    expect(seen).toContain('brahmaConsoleList');
  });

  it('a conversation deleted elsewhere (not-found) falls back to the launcher and forgets the id', async () => {
    localStorage.setItem(STORAGE_KEY_TAB_LAST_CHAT, 'gone');
    const error = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const { fixture, session } = await render({
      profiles: PROFILES,
      messagesThrow: new CoreDispatchError({ kind: 'not-found', message: 'Chat not found' }),
    });
    expect(session.currentChatId()).toBeNull();
    expect(localStorage.getItem(STORAGE_KEY_TAB_LAST_CHAT)).toBeNull();
    expect(
      (fixture.nativeElement.querySelector('qt-help-composer textarea') as HTMLTextAreaElement)
        .placeholder,
    ).toBe('What shall we put to the engine?');
    expect(fixture.nativeElement.querySelector('qt-brahma-console-message-list')).toBeNull();
    expect(error).not.toHaveBeenCalled();
    error.mockRestore();
  });

  it('any other transcript failure is logged with v4’s line and keeps the conversation', async () => {
    localStorage.setItem(STORAGE_KEY_TAB_LAST_CHAT, 'c9');
    const error = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const { session } = await render({
      profiles: PROFILES,
      messagesThrow: new CoreDispatchError({ kind: 'internal', message: 'boom' }),
    });
    expect(session.currentChatId()).toBe('c9');
    expect(error).toHaveBeenCalledWith(
      'Failed to load Brahma Console messages:',
      expect.any(Error),
    );
    error.mockRestore();
  });

  it('the launcher composer takes focus 50 ms after New conversation', async () => {
    localStorage.setItem(STORAGE_KEY_TAB_LAST_CHAT, 'c9');
    const { fixture } = await render({ profiles: PROFILES });
    vi.useFakeTimers();
    try {
      (
        fixture.nativeElement.querySelector('[title="New conversation"]') as HTMLButtonElement
      ).click();
      fixture.detectChanges();
      const textarea = fixture.nativeElement.querySelector(
        'qt-help-composer textarea',
      ) as HTMLTextAreaElement;
      expect(textarea.placeholder).toBe('What shall we put to the engine?');
      vi.advanceTimersByTime(49);
      expect(document.activeElement).not.toBe(textarea);
      vi.advanceTimersByTime(1);
      expect(document.activeElement).toBe(textarea);
    } finally {
      vi.useRealTimers();
    }
  });
});
