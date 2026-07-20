/**
 * BrahmaConsoleDialog — asserted against v4 `BrahmaConsoleDialog.tsx`: the asTab
 * bare render vs the floating overlay (gated on `isOpen`), the eligibility gate
 * on the launcher, the past-chats launcher (loaded only when
 * `(isOpen || asTab) && !currentChatId`), select-to-load, and the conversation
 * view's header (model picker + new conversation) + message list.
 */

import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import { Subject } from 'rxjs';

beforeAll(() => {
  const proto = globalThis.HTMLElement?.prototype as unknown as { scrollIntoView?: () => void };
  if (proto && !proto.scrollIntoView) proto.scrollIntoView = () => undefined;
});

import { CoreClient } from '../core/core-client';
import type { ScopedEvent } from '../core/core-contract';
import { BrahmaConsoleDialog } from './brahma-console-dialog';
import { BrahmaConsoleService } from './brahma-console.service';
import type { BrahmaConsoleMessage, BrahmaPastChat } from './brahma-wire';

interface Req {
  type: string;
  [k: string]: unknown;
}

interface StubOpts {
  profiles?: { id: string; name: string; provider: string; modelName: string }[];
  pastChats?: BrahmaPastChat[];
  messages?: BrahmaConsoleMessage[];
  onDispatch?: (req: Req) => void;
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
  asTab: boolean,
  opts: StubOpts = {},
): Promise<{ fixture: ComponentFixture<BrahmaConsoleDialog>; service: BrahmaConsoleService }> {
  const events = new Subject<ScopedEvent>();
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [BrahmaConsoleDialog],
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: stubCore(opts, events) },
    ],
  });
  const service = TestBed.inject(BrahmaConsoleService);
  const fixture = TestBed.createComponent(BrahmaConsoleDialog);
  fixture.componentRef.setInput('asTab', asTab);
  fixture.detectChanges();
  await settle(fixture);
  return { fixture, service };
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

const PROFILES = [{ id: 'p1', name: 'Everyday', provider: 'OPENAI', modelName: 'gpt-4o' }];

describe('BrahmaConsoleDialog (v4 BrahmaConsoleDialog.tsx)', () => {
  afterEach(() => {
    localStorage.clear();
    TestBed.resetTestingModule();
  });

  it('asTab renders bare (no overlay) with the launcher composer', async () => {
    const { fixture } = await render(true, { profiles: PROFILES });
    expect(fixture.nativeElement.querySelector('.qt-dialog-overlay')).toBeNull();
    expect(fixture.nativeElement.querySelector('qt-help-composer')).not.toBeNull();
  });

  it('gates the launcher on eligibility (no profiles → the wants-for-an-engine notice)', async () => {
    const { fixture } = await render(true, { profiles: [] });
    expect(text(fixture)).toContain('The Console wants for an engine');
  });

  it('floating mode is hidden until isOpen, then shows the overlay', async () => {
    const { fixture, service } = await render(false, { profiles: PROFILES });
    expect(fixture.nativeElement.querySelector('.qt-dialog-overlay')).toBeNull();

    service.openConsole();
    await settle(fixture);
    const overlay = fixture.nativeElement.querySelector('.qt-dialog-overlay');
    expect(overlay).not.toBeNull();
    expect(text(fixture)).toContain('Brahma Console');
  });

  it('lists past chats in the launcher and selecting one loads its transcript', async () => {
    const past: BrahmaPastChat[] = [
      {
        id: 'c1',
        title: 'A prior audience',
        updatedAt: '2026-01-01T00:00:00.000Z',
        lastMessageAt: null,
        messageCount: 3,
        consoleConnectionProfileId: 'p1',
      },
    ];
    const messages: BrahmaConsoleMessage[] = [
      { id: 'm1', role: 'USER', content: 'Ahoy', createdAt: '2026-01-01T00:00:00.000Z' },
    ];
    const { fixture, service } = await render(true, { profiles: PROFILES, pastChats: past, messages });
    expect(text(fixture)).toContain('Recent Console Conversations');
    expect(text(fixture)).toContain('A prior audience');

    // Select it → currentChatId set, transcript loads.
    (fixture.nativeElement.querySelector('.qt-help-past-chat button') as HTMLButtonElement).click();
    await settle(fixture);
    expect(service.currentChatId()).toBe('c1');
    expect(text(fixture)).toContain('Ahoy');
  });

  it('with a chat open shows the header (model picker + new conversation) and the message list', async () => {
    localStorage.setItem('quilltap:brahma-console-last-id', 'c9');
    const { fixture } = await render(true, {
      profiles: PROFILES,
      messages: [{ id: 'm1', role: 'ASSISTANT', content: 'At your service.', createdAt: 'x' }],
    });
    // Header chrome present.
    expect(fixture.nativeElement.querySelector('qt-brahma-model-picker')).not.toBeNull();
    expect(fixture.nativeElement.querySelector('[title="New conversation"]')).not.toBeNull();
    // The transcript rendered.
    expect(fixture.nativeElement.querySelector('qt-brahma-console-message-list')).not.toBeNull();
    expect(text(fixture)).toContain('At your service.');
  });

  it('folds the live stream frames then reconciles against the reloaded transcript', async () => {
    localStorage.setItem('quilltap:brahma-console-last-id', 'c9');
    const events = new Subject<ScopedEvent>();

    // A deferred send lets us observe the live overlay before the run completes.
    let resolveSend: (v: unknown) => void = () => undefined;
    const sendGate = new Promise((r) => (resolveSend = r));
    // The transcript the reconcile reload returns once the turn settles.
    let transcript: BrahmaConsoleMessage[] = [];

    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [BrahmaConsoleDialog],
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
    TestBed.inject(BrahmaConsoleService);
    const fixture = TestBed.createComponent(BrahmaConsoleDialog);
    fixture.componentRef.setInput('asTab', true);
    fixture.detectChanges();
    await settle(fixture);

    // Send a message; the subscription is live before the dispatch awaits.
    (fixture.componentInstance as unknown as { handleSend(c: string): void }).handleSend('ping');
    await settle(fixture, 2);

    // A content frame on this chat's scope shows live in the message list.
    events.next({ chatId: 'c9', content: 'pong' } as ScopedEvent);
    await settle(fixture, 2);
    expect(text(fixture)).toContain('pong');

    // The turn settles: the persisted transcript now carries the assistant bubble.
    transcript = [{ id: 'a1', role: 'ASSISTANT', content: 'pong!', createdAt: 'x' }];
    events.next({ chatId: 'c9', done: true, messageId: 'a1' } as ScopedEvent);
    resolveSend(undefined);
    await settle(fixture, 8);

    // The live overlay cleared; the reloaded transcript is what renders.
    expect(text(fixture)).toContain('pong!');
    expect(
      (fixture.componentInstance as unknown as { isStreaming(): boolean }).isStreaming(),
    ).toBe(false);
  });

  it('does NOT fetch past chats while a chat is open (v4 enabled gate)', async () => {
    localStorage.setItem('quilltap:brahma-console-last-id', 'c9');
    const seen: string[] = [];
    await render(true, {
      profiles: PROFILES,
      onDispatch: (req) => seen.push(req.type),
    });
    expect(seen).not.toContain('brahmaConsoleList');
  });
});
