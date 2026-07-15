import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ActivatedRoute, convertToParamMap, provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { Subject, of } from 'rxjs';
import { beforeAll, describe, expect, it, vi } from 'vitest';

/**
 * The message list is virtualized (`@tanstack/angular-virtual`). The virtualizer
 * measures the scroll container via `offsetHeight`, and forces an empty window
 * when it reads 0 — which is exactly what JSDOM reports (no layout). Stub a
 * non-zero `offsetHeight`/`offsetWidth` so the container has a viewport and the
 * (small) fixture's rows fall inside the window and render into the DOM.
 */
beforeAll(() => {
  const proto = globalThis.HTMLElement?.prototype;
  if (proto && !('__qtSizeStubbed' in proto)) {
    Object.defineProperty(proto, '__qtSizeStubbed', { value: true });
    Object.defineProperty(proto, 'offsetHeight', { configurable: true, get: () => 800 });
    Object.defineProperty(proto, 'offsetWidth', { configurable: true, get: () => 800 });
  }
});

import { CoreClient } from '../../core/core-client';
import type {
  ChatDetail,
  CoreRequest,
  CoreResponse,
  MessageDto,
  ParticipantDetail,
  ScopedEvent,
} from '../../core/core-contract';
import { SalonConversation } from './salon-conversation';

function participant(over: Partial<ParticipantDetail>): ParticipantDetail {
  return {
    id: 'p1',
    type: 'CHARACTER',
    displayOrder: 0,
    isActive: true,
    controlledBy: 'llm',
    status: 'active',
    character: {
      id: 'char1',
      name: 'Friday',
      title: 'The Butler',
      avatarUrl: null,
      defaultImageId: null,
      defaultImage: null,
    },
    connectionProfile: null,
    imageProfile: null,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    ...over,
  };
}

function message(over: Partial<MessageDto>): MessageDto {
  return {
    id: 'm',
    role: 'ASSISTANT',
    content: '',
    tokenCount: null,
    promptTokens: null,
    completionTokens: null,
    createdAt: '2024-01-01T00:00:00.000Z',
    swipeGroupId: null,
    swipeIndex: null,
    participantId: null,
    attachments: [],
    provider: null,
    modelName: null,
    targetParticipantIds: null,
    isSilentMessage: null,
    systemSender: null,
    systemKind: null,
    hostEvent: null,
    customAnnouncer: null,
    carinaMeta: null,
    pendingExternalPrompt: null,
    pendingExternalPromptFull: null,
    pendingExternalAttachments: null,
    reasoningContent: null,
    reasoningSegments: null,
    ...over,
  };
}

function chatDetail(): ChatDetail {
  return {
    id: 'chat-1',
    title: 'Tea Time',
    contextSummary: null,
    roleplayTemplateId: null,
    chatType: 'salon',
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    isPaused: false,
    isManuallyRenamed: false,
    participants: [
      participant({
        id: 'pu',
        controlledBy: 'user',
        character: {
          id: 'u',
          name: 'Bertie',
          title: null,
          avatarUrl: null,
          defaultImageId: null,
          defaultImage: null,
        },
      }),
      participant({ id: 'p1' }),
    ],
    user: { id: 'user1', name: 'Bertie', image: null },
    messages: [
      message({
        id: 'u1',
        role: 'USER',
        participantId: 'pu',
        content: 'Good morning, Friday.',
        createdAt: '2024-01-01T00:00:01.000Z',
      }),
      message({
        id: 'a1',
        role: 'ASSISTANT',
        participantId: 'p1',
        content: 'A fine morning it is.',
        reasoningContent: 'Consider the weather.',
        createdAt: '2024-01-01T00:00:02.000Z',
      }),
      message({
        id: 'w1',
        role: 'ASSISTANT',
        participantId: 'p1',
        content: 'psst, a private word',
        targetParticipantIds: ['pu'],
        createdAt: '2024-01-01T00:00:03.000Z',
      }),
      message({
        id: 'h1',
        role: 'ASSISTANT',
        systemSender: 'host',
        systemKind: 'turn-pass',
        content: 'Friday has nothing to add.',
        createdAt: '2024-01-01T00:00:04.000Z',
      }),
    ],
    projectId: null,
    projectName: null,
    turnSkippingEnabled: null,
    agentModeEnabled: false,
    resolvedAgentModeEnabled: false,
    agentModeSource: 'global',
    isDangerousChat: false,
    dangerCategories: [],
    conciergeOverride: null,
    offSceneCharacters: [],
    lastTurnParticipantId: null,
  };
}

function stubClient(
  chat: ChatDetail,
  events$: Subject<ScopedEvent>,
  background: Record<string, unknown> | null = null,
): Partial<CoreClient> {
  const dispatch = vi.fn(async (req: CoreRequest): Promise<CoreResponse> => {
    if (req.type === 'chatGet') return { type: 'chat', data: { chat } };
    if (req.type === 'chatSettings') {
      return {
        type: 'chatSettings',
        data: { avatarDisplayMode: 'ALWAYS', avatarDisplayStyle: 'CIRCULAR' },
      };
    }
    return { type: 'ack', data: {} };
  });
  // The story-background resolver reads through dispatchData (finding #9). Return
  // the all-null body unless a test seeds a background.
  const dispatchData = vi.fn(
    async () =>
      background ?? {
        backgroundUrl: null,
        fileId: null,
        filename: null,
        sha256: null,
        linkSummary: null,
      },
  );
  return {
    events$: events$.asObservable(),
    dispatch,
    dispatchData: dispatchData as unknown as CoreClient['dispatchData'],
    dispatchExpect: (async (req: CoreRequest, expect: string) => {
      const resp = await dispatch(req);
      if (resp.type !== expect) throw new Error(`unexpected ${resp.type}`);
      return resp;
    }) as CoreClient['dispatchExpect'],
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<SalonConversation>> {
  TestBed.configureTestingModule({
    imports: [SalonConversation],
    providers: [
      provideRouter([]),
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: client },
      { provide: ActivatedRoute, useValue: { paramMap: of(convertToParamMap({ id: 'chat-1' })) } },
    ],
  });
  const fixture = TestBed.createComponent(SalonConversation);
  fixture.detectChanges();
  // Let the TanStack queries' async settle (zoneless whenStable doesn't track them).
  for (let i = 0; i < 5; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

describe('SalonConversation (read path)', () => {
  it('renders the baked messages, the whisper label, the reasoning block, and the staff chip', async () => {
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    const text = fixture.nativeElement.textContent as string;

    // Messages + authors.
    expect(text).toContain('Good morning, Friday.');
    expect(text).toContain('A fine morning it is.');
    expect(text).toContain('Friday');

    // Whisper label.
    expect(text).toContain('Private whisper');

    // Reasoning block (collapsible).
    expect(fixture.nativeElement.querySelector('.qt-chat-thinking')).toBeTruthy();

    // Staff announcement chip: sender display name + kind label.
    expect(text).toContain('The Host');
    expect(text).toContain('nothing to add');

    // Header title + copy button.
    expect(text).toContain('Tea Time');
    expect(fixture.nativeElement.querySelector('[aria-label="Copy conversation ID"]')).toBeTruthy();
  });

  it('does not set the story-background var when the chat has no background (finding #9)', async () => {
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    const layout = fixture.nativeElement.querySelector('.qt-chat-layout') as HTMLElement;
    // No --story-background-url in the inline style → the ::before layer stays hidden.
    expect(layout.getAttribute('style') ?? '').not.toContain('--story-background-url');
  });

  it('applies --story-background-url from the resolved file id (finding #9)', async () => {
    const fixture = await render(
      stubClient(chatDetail(), new Subject<ScopedEvent>(), {
        backgroundUrl: '/v4/path/bg.webp',
        fileId: 'bg-7',
        filename: 'bg.webp',
        sha256: 's',
        linkSummary: null,
      }),
    );
    const layout = fixture.nativeElement.querySelector('.qt-chat-layout') as HTMLElement;
    // The store-backed byte route, wrapped as a CSS url(), lands on the layout root.
    expect(layout.style.getPropertyValue('--story-background-url')).toBe(
      "url('/api/v1/files/bg-7')",
    );
    expect(layout.getAttribute('style') ?? '').toContain('--story-background-url');
  });
});

describe('SalonConversation — story-background regeneration (v4 useChatControls.ts:397-416)', () => {
  interface RegenStub {
    calls: string[];
    client: Partial<CoreClient>;
  }

  /**
   * A stub that ROUTES dispatchData by request type — the shared `stubClient`
   * answers every dispatchData call with the background body, which would make a
   * regenerate look like a background read.
   */
  function regenStub(opts: {
    enabled: boolean;
    fileId?: string | null;
    regenerate?: () => Record<string, unknown>;
  }): RegenStub {
    const calls: string[] = [];
    const chat = chatDetail();

    const dispatch = vi.fn(async (req: CoreRequest): Promise<CoreResponse> => {
      calls.push(req.type);
      if (req.type === 'chatGet') return { type: 'chat', data: { chat } };
      if (req.type === 'chatSettings') {
        return {
          type: 'chatSettings',
          data: {
            avatarDisplayMode: 'ALWAYS',
            avatarDisplayStyle: 'CIRCULAR',
            storyBackgroundsSettings: { enabled: opts.enabled, defaultImageProfileId: null },
          },
        };
      }
      return { type: 'ack', data: {} };
    });

    const dispatchData = vi.fn(async (req: { type: string }) => {
      calls.push(req.type);
      if (req.type === 'chatRegenerateBackground') {
        return (
          opts.regenerate?.() ?? {
            message: 'Story background regeneration queued',
            queued: true,
            jobId: 'job-1',
          }
        );
      }
      return {
        backgroundUrl: null,
        fileId: opts.fileId ?? null,
        filename: null,
        sha256: null,
        linkSummary: null,
      };
    });

    return {
      calls,
      client: {
        events$: new Subject<ScopedEvent>().asObservable(),
        dispatch,
        dispatchData: dispatchData as unknown as CoreClient['dispatchData'],
        dispatchExpect: (async (req: CoreRequest, expected: string) => {
          const resp = await dispatch(req);
          if (resp.type !== expected) throw new Error(`unexpected ${resp.type}`);
          return resp;
        }) as CoreClient['dispatchExpect'],
      },
    };
  }

  async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
    for (let i = 0; i < 6; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
  }

  function regenEntry(fixture: ComponentFixture<SalonConversation>): HTMLButtonElement | null {
    return fixture.nativeElement.querySelector('button[aria-label="Regenerate Background"]');
  }

  it('shows the entry only when storyBackgroundsSettings.enabled is on', async () => {
    const off = await render(regenStub({ enabled: false }).client);
    expect(regenEntry(off)).toBeNull();
    TestBed.resetTestingModule();

    const on = await render(regenStub({ enabled: true }).client);
    expect(regenEntry(on)).not.toBeNull();
  });

  it('dispatches the regenerate and flashes the server success message', async () => {
    const s = regenStub({ enabled: true });
    const fixture = await render(s.client);

    regenEntry(fixture)!.click();
    await settle(fixture);

    expect(s.calls).toContain('chatRegenerateBackground');
    const flash = fixture.nativeElement.querySelector('.qt-alert-success');
    expect(flash).not.toBeNull();
    expect(flash.textContent).toContain('Story background regeneration queued');
  });

  it('flashes the "already in progress" arm verbatim too (both §2 arms are successes)', async () => {
    const s = regenStub({
      enabled: true,
      regenerate: () => ({
        message: 'Story background generation already in progress',
        queued: true,
        jobId: 'job-2',
      }),
    });
    const fixture = await render(s.client);

    regenEntry(fixture)!.click();
    await settle(fixture);

    const flash = fixture.nativeElement.querySelector('.qt-alert-success');
    expect(flash.textContent).toContain('Story background generation already in progress');
  });

  it('surfaces the server error verbatim (the §2 badRequest strings)', async () => {
    const s = regenStub({
      enabled: true,
      regenerate: () => {
        throw new Error('No characters in chat to generate background for.');
      },
    });
    const fixture = await render(s.client);

    regenEntry(fixture)!.click();
    await settle(fixture);

    const flash = fixture.nativeElement.querySelector('.qt-alert-error');
    expect(flash).not.toBeNull();
    expect(flash.textContent).toContain('No characters in chat to generate background for.');
  });

  it('keeps DISPLAYING a background while generation is switched off (display is unconditional)', async () => {
    // The flag gates polling, never display — the P4.6am survey rule.
    const fixture = await render(regenStub({ enabled: false, fileId: 'file-77' }).client);
    const layout = fixture.nativeElement.querySelector('.qt-chat-layout');
    expect(layout.getAttribute('style')).toContain("url('/api/v1/files/file-77')");
  });
});

describe('SalonConversation — the LLM Inspector (v4 useLLMLogs.ts + SalonView.tsx:796-811, :1696-1705)', () => {
  function inspectorLog(over: Record<string, unknown> = {}): Record<string, unknown> {
    return {
      id: 'log-1',
      userId: 'user-1',
      type: 'CHAT_MESSAGE',
      messageId: 'a1',
      provider: 'openai',
      modelName: 'gpt-4o',
      request: { messageCount: 1, messages: [], toolCount: 0 },
      response: { content: 'A fine morning it is.', contentLength: 21 },
      createdAt: '2024-01-01T00:00:02.000Z',
      updatedAt: '2024-01-01T00:00:02.000Z',
      ...over,
    };
  }

  function inspectorClient(
    opts: { logs?: Record<string, unknown>[]; loggingEnabled?: boolean | null } = {},
  ): { client: Partial<CoreClient>; listCalls: () => CoreRequest[] } {
    const calls: CoreRequest[] = [];
    const chat = chatDetail();
    const settings: Record<string, unknown> = {
      avatarDisplayMode: 'ALWAYS',
      avatarDisplayStyle: 'CIRCULAR',
    };
    if (opts.loggingEnabled !== undefined) {
      settings['llmLoggingSettings'] = { enabled: opts.loggingEnabled };
    }
    const dispatch = vi.fn(async (req: CoreRequest): Promise<CoreResponse> => {
      if (req.type === 'chatGet') return { type: 'chat', data: { chat } };
      if (req.type === 'chatSettings')
        return { type: 'chatSettings', data: settings } as CoreResponse;
      return { type: 'ack', data: {} };
    });
    const dispatchData = vi.fn(async (req: CoreRequest) => {
      calls.push(req);
      if ((req as { type: string }).type === 'llmLogsList') {
        return { logs: opts.logs ?? [], count: 0, total: 0, limit: 100, offset: 0 };
      }
      return { backgroundUrl: null, fileId: null, filename: null, sha256: null, linkSummary: null };
    });
    return {
      listCalls: () => calls.filter((c) => (c as { type: string }).type === 'llmLogsList'),
      client: {
        events$: new Subject<ScopedEvent>().asObservable(),
        dispatch,
        dispatchData: dispatchData as unknown as CoreClient['dispatchData'],
        dispatchExpect: (async (req: CoreRequest, expected: string) => {
          const resp = await dispatch(req);
          if (resp.type !== expected) throw new Error(`unexpected ${resp.type}`);
          return resp;
        }) as CoreClient['dispatchExpect'],
      },
    };
  }

  function panel(fixture: ComponentFixture<SalonConversation>): HTMLElement | null {
    return fixture.nativeElement.querySelector('.qt-slide-over-panel');
  }

  function toolbarButton(fixture: ComponentFixture<SalonConversation>): HTMLButtonElement {
    return fixture.nativeElement.querySelector('button[aria-label="Toggle LLM Inspector"]');
  }

  function pressShortcut(key = 'L'): void {
    document.dispatchEvent(
      new KeyboardEvent('keydown', { key, metaKey: true, shiftKey: true, cancelable: true }),
    );
  }

  it('mounts the panel CLOSED and fetches the chat’s logs', async () => {
    const { client, listCalls } = inspectorClient({ logs: [inspectorLog()] });
    const fixture = await render(client);
    // Always mounted — the slide-over animates on data-open (v4 renders it
    // outside every gate).
    expect(panel(fixture)).not.toBeNull();
    expect(panel(fixture)!.getAttribute('data-open')).toBe('false');
    expect(listCalls()).toEqual([{ type: 'llmLogsList', chatId: 'chat-1', includeMessages: true }]);
  });

  it('opens and closes on the toolbar button', async () => {
    const { client } = inspectorClient();
    const fixture = await render(client);
    toolbarButton(fixture).click();
    fixture.detectChanges();
    expect(panel(fixture)!.getAttribute('data-open')).toBe('true');
    toolbarButton(fixture).click();
    fixture.detectChanges();
    expect(panel(fixture)!.getAttribute('data-open')).toBe('false');
  });

  it('toggles on Cmd+Shift+L (v4 :796-811)', async () => {
    const { client } = inspectorClient();
    const fixture = await render(client);
    pressShortcut();
    fixture.detectChanges();
    expect(panel(fixture)!.getAttribute('data-open')).toBe('true');
    pressShortcut();
    fixture.detectChanges();
    expect(panel(fixture)!.getAttribute('data-open')).toBe('false');
  });

  it('preventDefaults the shortcut (v4 :806)', async () => {
    const { client } = inspectorClient();
    await render(client);
    const event = new KeyboardEvent('keydown', {
      key: 'L',
      metaKey: true,
      shiftKey: true,
      cancelable: true,
    });
    document.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
  });

  it('matches the UPPERCASE L only — Shift is held, so the browser reports the capital', async () => {
    const { client } = inspectorClient();
    const fixture = await render(client);
    pressShortcut('l');
    fixture.detectChanges();
    expect(panel(fixture)!.getAttribute('data-open')).toBe('false');
  });

  it('does not attach the shortcut when logging is disabled (v4’s effect returns early)', async () => {
    const { client } = inspectorClient({ loggingEnabled: false });
    const fixture = await render(client);
    // The toolbar button is gone AND the key is dead — v4 gates both on the
    // same flag.
    expect(toolbarButton(fixture)).toBeNull();
    pressShortcut();
    fixture.detectChanges();
    expect(panel(fixture)!.getAttribute('data-open')).toBe('false');
  });

  it('renders the per-message cpu entry for a message that has logs, and opens scrolled', async () => {
    const { client } = inspectorClient({ logs: [inspectorLog({ messageId: 'a1' })] });
    const fixture = await render(client);
    const entries = fixture.nativeElement.querySelectorAll(
      'button[aria-label="View LLM request/response logs"]',
    );
    // Exactly the ASSISTANT messages that have logs — 'a1' has one; the whisper
    // and the user message do not.
    expect(entries.length).toBe(1);
    (entries[0] as HTMLButtonElement).click();
    fixture.detectChanges();
    expect(panel(fixture)!.getAttribute('data-open')).toBe('true');
    // The target entry carries the highlight (v4 threads scrollToMessageId).
    expect(fixture.nativeElement.querySelector('.qt-inspector-entry-highlight')).not.toBeNull();
  });

  it('renders no cpu entries when the chat has no logs', async () => {
    const { client } = inspectorClient({ logs: [] });
    const fixture = await render(client);
    expect(
      fixture.nativeElement.querySelectorAll('button[aria-label="View LLM request/response logs"]')
        .length,
    ).toBe(0);
  });

  it('clears the scroll target when the TOGGLE opens, but not when it closes (v4 :50-58)', async () => {
    const { client } = inspectorClient({ logs: [inspectorLog({ messageId: 'a1' })] });
    const fixture = await render(client);

    // Open scrolled from the message entry → highlighted.
    (
      fixture.nativeElement.querySelector(
        'button[aria-label="View LLM request/response logs"]',
      ) as HTMLButtonElement
    ).click();
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('.qt-inspector-entry-highlight')).not.toBeNull();

    // Toggle CLOSED: the target survives — the panel is still animating out and
    // must not lose the highlight mid-transition.
    toolbarButton(fixture).click();
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('.qt-inspector-entry-highlight')).not.toBeNull();

    // Toggle OPEN again: now it clears — a toolbar open is not about any message.
    toolbarButton(fixture).click();
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('.qt-inspector-entry-highlight')).toBeNull();
  });

  it('clears BOTH on an explicit close (v4 closeInspector :61-64)', async () => {
    const { client } = inspectorClient({ logs: [inspectorLog({ messageId: 'a1' })] });
    const fixture = await render(client);
    (
      fixture.nativeElement.querySelector(
        'button[aria-label="View LLM request/response logs"]',
      ) as HTMLButtonElement
    ).click();
    fixture.detectChanges();

    (
      fixture.nativeElement.querySelector('button[aria-label="Close panel"]') as HTMLButtonElement
    ).click();
    fixture.detectChanges();
    expect(panel(fixture)!.getAttribute('data-open')).toBe('false');
    expect(fixture.nativeElement.querySelector('.qt-inspector-entry-highlight')).toBeNull();
  });

  it('refetches on the panel’s refresh button (v4 refreshLogs :67-69)', async () => {
    const { client, listCalls } = inspectorClient({ logs: [inspectorLog()] });
    const fixture = await render(client);
    expect(listCalls().length).toBe(1);
    (
      fixture.nativeElement.querySelector('button[aria-label="Refresh logs"]') as HTMLButtonElement
    ).click();
    await new Promise((r) => setTimeout(r, 0));
    expect(listCalls().length).toBe(2);
  });
});
