import { signal } from '@angular/core';
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
import { coreStreamStub } from '../../core/core-client.testing';
import type { ConnectionState } from '../../core/core-transport';
import type {
  ChatDetail,
  ChatStreamFrame,
  CoreRequest,
  CoreResponse,
  MessageDto,
  ParticipantDetail,
  ScopedEvent,
} from '../../core/core-contract';
import {
  WORKSPACE_BACKDROP_REGISTRY,
  WORKSPACE_TAB_ID,
  type WorkspaceBackdropEntry,
  type WorkspaceBackdropRegistry,
} from '../../workspace/workspace-contract';
import {
  initialChatStreamState,
  reduceChatFrame,
  type ChatStreamState,
} from '../../core/chat-stream.reducer';
import type { ToolExecutionStatus } from '../../chat/chat-composer';
import { SalonConversation } from './salon-conversation';
import { ToastService } from '../../ui/toast.service';

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
      participant({ id: 'pu', controlledBy: 'user', character: { id: 'u', name: 'Bertie', title: null, avatarUrl: null, defaultImageId: null, defaultImage: null } }),
      participant({ id: 'p1' }),
    ],
    user: { id: 'user1', name: 'Bertie', image: null },
    messages: [
      message({ id: 'u1', role: 'USER', participantId: 'pu', content: 'Good morning, Friday.', createdAt: '2024-01-01T00:00:01.000Z' }),
      message({
        id: 'a1',
        role: 'ASSISTANT',
        participantId: 'p1',
        content: 'A fine morning it is.',
        reasoningContent: 'Consider the weather.',
        createdAt: '2024-01-01T00:00:02.000Z',
      }),
      message({ id: 'w1', role: 'ASSISTANT', participantId: 'p1', content: 'psst, a private word', targetParticipantIds: ['pu'], createdAt: '2024-01-01T00:00:03.000Z' }),
      message({ id: 'h1', role: 'ASSISTANT', systemSender: 'host', systemKind: 'turn-pass', content: 'Friday has nothing to add.', createdAt: '2024-01-01T00:00:04.000Z' }),
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
      return { type: 'chatSettings', data: { avatarDisplayMode: 'ALWAYS', avatarDisplayStyle: 'CIRCULAR' } };
    }
    return { type: 'ack', data: {} };
  });
  // The story-background resolver reads through dispatchData (finding #9). Return
  // the all-null body unless a test seeds a background.
  const dispatchData = vi.fn(async () => background ?? { backgroundUrl: null, fileId: null, filename: null, sha256: null, linkSummary: null });
  return {
    events$: events$.asObservable(),
    connection: signal<ConnectionState>('idle'),
    resyncCounter: signal(0),
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
  // The chat sidebar (P4.9H1) reads its collapse preference from localStorage and
  // defaults to the mini strip; cases that reach into its drawers want it open.
  localStorage.setItem('quilltap.chat-sidebar.collapsed', 'false');
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

/**
 * Workspace-tab mode (p4.9j2): the chat id comes from the `chatId` input with NO
 * `ActivatedRoute`, and the id-dependent wiring (terminal/document configure +
 * the tool-result subscription) defers to a one-shot effect until the input
 * resolves. The read path must render identically.
 */
/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('SalonConversation (workspace-tab mode)', () => {
  async function renderTab(
    client: Partial<CoreClient>,
  ): Promise<ComponentFixture<SalonConversation>> {
    TestBed.configureTestingModule({
      imports: [SalonConversation],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: client },
      ],
    });
    const fixture = TestBed.createComponent(SalonConversation);
    fixture.componentRef.setInput('chatId', 'chat-1');
    fixture.detectChanges();
    for (let i = 0; i < 5; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    return fixture;
  }

  it('renders the chat from the chatId input with no ActivatedRoute', async () => {
    const fixture = await renderTab(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Tea Time');
    expect(text).toContain('A fine morning it is.');
  });

  it('fetches this chat’s LLM logs by the input id (one-shot wiring resolved)', async () => {
    const fixture = await renderTab(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    // The header title proves chatGet ran with the input id; logs list is scoped
    // to the same id.
    expect(fixture.nativeElement.querySelector('.qt-slide-over-panel')).not.toBeNull();
  });

  it('reports the story background to the backdrop registry (v4 useReportWorkspaceBackdrop)', async () => {
    const reports: { tabId: string; entry: WorkspaceBackdropEntry }[] = [];
    const cleared: string[] = [];
    const registry: WorkspaceBackdropRegistry = {
      report: (tabId, entry) => reports.push({ tabId, entry }),
      clear: (tabId) => cleared.push(tabId),
    };
    TestBed.configureTestingModule({
      imports: [SalonConversation],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        {
          provide: CoreClient,
          useValue: stubClient(chatDetail(), new Subject<ScopedEvent>(), {
            backgroundUrl: '/v4/path/bg.webp',
            fileId: 'bg-7',
            filename: 'bg.webp',
            sha256: 's',
            linkSummary: null,
          }),
        },
        { provide: WORKSPACE_TAB_ID, useValue: 'tab-1' },
        { provide: WORKSPACE_BACKDROP_REGISTRY, useValue: registry },
      ],
    });
    const fixture = TestBed.createComponent(SalonConversation);
    fixture.componentRef.setInput('chatId', 'chat-1');
    fixture.detectChanges();
    for (let i = 0; i < 5; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    // The raw file URL is reported with isSalon: true.
    const last = reports.at(-1);
    expect(last?.tabId).toBe('tab-1');
    expect(last?.entry).toEqual({ url: '/api/v1/files/bg-7', isSalon: true });

    // On destroy it clears its slot.
    fixture.destroy();
    expect(cleared).toContain('tab-1');
  });

  it('clears the backdrop for a background-less chat', async () => {
    const cleared: string[] = [];
    const registry: WorkspaceBackdropRegistry = {
      report: () => {},
      clear: (tabId) => cleared.push(tabId),
    };
    TestBed.configureTestingModule({
      imports: [SalonConversation],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: stubClient(chatDetail(), new Subject<ScopedEvent>()) },
        { provide: WORKSPACE_TAB_ID, useValue: 'tab-1' },
        { provide: WORKSPACE_BACKDROP_REGISTRY, useValue: registry },
      ],
    });
    const fixture = TestBed.createComponent(SalonConversation);
    fixture.componentRef.setInput('chatId', 'chat-1');
    fixture.detectChanges();
    for (let i = 0; i < 5; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(cleared).toContain('tab-1');
  });
});

describe('SalonConversation (read path)', () => {
  it('renders the baked messages, the whisper label, the reasoning block, and the staff chip', async () => {
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    const text = fixture.nativeElement.textContent as string;

    // Messages + authors.
    expect(text).toContain('Good morning, Friday.');
    expect(text).toContain('A fine morning it is.');
    expect(text).toContain('Friday');

    // Whisper label — names the target (v4 "whispered to <names>"; w1 → pu = Bertie).
    expect(text).toContain('whispered to Bertie');

    // Reasoning block (collapsible).
    expect(fixture.nativeElement.querySelector('.qt-chat-thinking')).toBeTruthy();

    // Staff announcement chip: sender display name + kind label.
    expect(text).toContain('The Host');
    expect(text).toContain('nothing to add');

    // Header title + copy button.
    expect(text).toContain('Tea Time');
    expect(fixture.nativeElement.querySelector('[aria-label="Copy conversation ID"]')).toBeTruthy();
  });

  it('filters operator-invisible whispers, keeps operator machinery, and reveals on toggle (P4.6ba)', async () => {
    // p1 is an LLM character (not a user participant); pu is the human.
    const chat: ChatDetail = {
      ...chatDetail(),
      messages: [
        message({ id: 'm0', role: 'USER', participantId: 'pu', content: 'Good morning.', createdAt: '2024-01-01T00:00:01.000Z' }),
        // A Commonplace Book recall whispered to a character — the operator is
        // not its audience; hidden when the toggle is off.
        message({
          id: 'cb',
          systemSender: 'commonplaceBook',
          systemKind: 'relevant-memories',
          participantId: null,
          targetParticipantIds: ['p1'],
          content: 'a private recall',
          createdAt: '2024-01-01T00:00:02.000Z',
        }),
        // A Pascal private roll to a character — operator machinery, always shown.
        message({
          id: 'px',
          systemSender: 'pascal',
          systemKind: 'custom-tool-result',
          participantId: null,
          targetParticipantIds: ['p1'],
          content: '🎲 pascal secret roll',
          createdAt: '2024-01-01T00:00:03.000Z',
        }),
      ],
    };
    const fixture = await render(stubClient(chat, new Subject<ScopedEvent>()));

    // Off by default: the Commonplace chip is hidden, the Pascal machinery shown.
    expect(fixture.nativeElement.textContent).not.toContain('The Commonplace Book');
    expect(fixture.nativeElement.textContent).toContain('pascal secret roll');

    // Flip "All Whispers" on → the hidden recall chip appears. Since P4.9H1 the
    // toggle lives in the sidebar's Visibility drawer (v4's home for it), so the
    // card must be opened first.
    const visibilityCard = (
      Array.from(
        fixture.nativeElement.querySelectorAll('.qt-collapsible-card-header'),
      ) as HTMLButtonElement[]
    ).find((h) => h.textContent?.trim().startsWith('Visibility'))!;
    visibilityCard.click();
    fixture.detectChanges();
    const toggle = fixture.nativeElement.querySelector(
      'button[aria-label="All Whispers"]',
    ) as HTMLButtonElement;
    expect(toggle).toBeTruthy();
    toggle.click();
    for (let i = 0; i < 5; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(fixture.nativeElement.textContent).toContain('The Commonplace Book');
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
    expect(layout.style.getPropertyValue('--story-background-url')).toBe("url('/api/v1/files/bg-7')");
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
        connection: signal<ConnectionState>('idle'),
        resyncCounter: signal(0),
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

  /**
   * The entry lives in the sidebar's Chat drawer since P4.9H1 (v4's own home for
   * it), so a case must expand the sidebar and open that card first. v4's palette
   * button carries a text label, not an aria-label — hence the text match.
   */
  function openChatDrawer(fixture: ComponentFixture<SalonConversation>): void {
    const headers = Array.from(
      fixture.nativeElement.querySelectorAll('.qt-collapsible-card-header'),
    ) as HTMLButtonElement[];
    const chatCard = headers.find((h) => h.textContent?.trim().startsWith('Chat'));
    chatCard?.click();
    fixture.detectChanges();
  }

  function regenEntry(fixture: ComponentFixture<SalonConversation>): HTMLButtonElement | null {
    openChatDrawer(fixture);
    return (
      (Array.from(fixture.nativeElement.querySelectorAll('button')).find(
        (b) => (b as HTMLButtonElement).textContent?.trim() === 'Regenerate Background',
      ) as HTMLButtonElement | undefined) ?? null
    );
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
    expect(toasts()).toEqual([
      { type: 'success', message: 'Story background regeneration queued' },
    ]);
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

    expect(toasts()).toEqual([
      { type: 'success', message: 'Story background generation already in progress' },
    ]);
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

    expect(toasts()).toEqual([
      { type: 'error', message: 'No characters in chat to generate background for.' },
    ]);
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
        connection: signal<ConnectionState>('idle'),
        resyncCounter: signal(0),
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

describe('SalonConversation — the standalone generate-image dialog (v4 ChatModals.tsx:269-)', () => {
  it('opens the STANDALONE dialog from the composer camera button, not the chat-profile one', async () => {
    const events$ = new Subject<ScopedEvent>();
    const fixture = await render(stubClient(chatDetail(), events$));
    const button = fixture.nativeElement.querySelector(
      'button[aria-label="Generate image"]',
    ) as HTMLButtonElement;
    // v4's ONLY opener chain is ComposerGutterTools :52 → the standalone dialog.
    expect(button).not.toBeNull();
    button.click();
    fixture.detectChanges();
    expect(
      fixture.nativeElement.querySelector('qt-standalone-generate-image-dialog'),
    ).not.toBeNull();
    // v4 mounts the chat-profile dialog but never opens it (useModalState:63 has
    // no caller); v5 keeps that faithful, so it must stay closed here.
    expect(fixture.nativeElement.querySelector('qt-generate-image-dialog')).toBeNull();
  });

  it('records the generate_image tool result with v4’s exact payload (v4 ChatModals :271-283)', async () => {
    const events$ = new Subject<ScopedEvent>();
    const client = stubClient(chatDetail(), events$);
    const fixture = await render(client);
    const inst = fixture.componentInstance as unknown as {
      onImagesGenerated(e: {
        images: { id: string; filename: string; filepath: string; mimeType: string }[];
        prompt: string;
      }): Promise<void>;
    };
    await inst.onImagesGenerated({
      images: [
        { id: 'i1', filename: 'one.png', filepath: 'tool/one.png', mimeType: 'image/png' },
        { id: 'i2', filename: 'two.png', filepath: 'tool/two.png', mimeType: 'image/png' },
      ],
      prompt: 'a cat',
    });
    const dispatch = client.dispatch as unknown as { mock: { calls: [CoreRequest][] } };
    const call = dispatch.mock.calls
      .map((c) => c[0] as Record<string, unknown>)
      .find((r) => r['type'] === 'chatAddToolResult');
    expect(call).toEqual({
      type: 'chatAddToolResult',
      chatId: 'chat-1',
      tool: 'generate_image',
      initiatedBy: 'user',
      prompt: 'a cat',
      // v4 maps to id + filename ONLY — filepath and mimeType are dropped.
      images: [
        { id: 'i1', filename: 'one.png' },
        { id: 'i2', filename: 'two.png' },
      ],
    });
  });
  /**
   * The cast controls (P4.9E1B). The wire shapes themselves are pinned in
   * `chat/chat-cast.api.spec.ts`; what these cases prove is what the SALON
   * decides before calling it — which key is omitted, which pair travels
   * together, and how many requests a slider drag becomes.
   */
  it('flipping a participant to the human OMITS connectionProfileId (v4 `? undefined`)', async () => {
    const events$ = new Subject<ScopedEvent>();
    const client = stubClient(chatDetail(), events$);
    const fixture = await render(client);
    const inst = fixture.componentInstance as unknown as {
      onParticipantProfileChange(e: {
        participantId: string;
        profileId: string | null;
        controlledBy: 'llm' | 'user';
      }): Promise<void>;
    };
    await inst.onParticipantProfileChange({
      participantId: 'p1',
      profileId: null,
      controlledBy: 'user',
    });
    const dispatchData = client.dispatchData as unknown as { mock: { calls: unknown[][] } };
    const call = dispatchData.mock.calls
      .map((c) => c[0] as Record<string, unknown>)
      .find((r) => r?.['type'] === 'chatUpdateParticipant')!;
    expect(call).toEqual({
      type: 'chatUpdateParticipant',
      chatId: 'chat-1',
      participantId: 'p1',
      controlledBy: 'user',
    });
  });

  it('the status select sends v4’s derived isActive alongside status (ChatSidebar :818)', async () => {
    const events$ = new Subject<ScopedEvent>();
    const client = stubClient(chatDetail(), events$);
    const fixture = await render(client);
    const inst = fixture.componentInstance as unknown as {
      onParticipantStatusChange(e: { participantId: string; status: string }): Promise<void>;
    };
    const dispatchData = client.dispatchData as unknown as { mock: { calls: unknown[][] } };
    const updates = () =>
      dispatchData.mock.calls
        .map((c) => c[0] as Record<string, unknown>)
        .filter((r) => r?.['type'] === 'chatUpdateParticipant');

    await inst.onParticipantStatusChange({ participantId: 'p1', status: 'silent' });
    // Silent still counts as present, so the legacy column stays true.
    expect(updates().at(-1)).toMatchObject({ status: 'silent', isActive: true });

    await inst.onParticipantStatusChange({ participantId: 'p1', status: 'absent' });
    expect(updates().at(-1)).toMatchObject({ status: 'absent', isActive: false });
  });

  it('a talkativeness drag becomes ONE request, carrying the last value (v4 400 ms)', async () => {
    const events$ = new Subject<ScopedEvent>();
    const client = stubClient(chatDetail(), events$);
    // Render on REAL timers — `render()` drains the queries with setTimeout(0) —
    // then freeze the clock to watch the debounce.
    const fixture = await render(client);
    vi.useFakeTimers();
    try {
      const inst = fixture.componentInstance as unknown as {
        onParticipantTalkativenessChange(e: { participantId: string; value: number }): void;
      };
      const dispatchData = client.dispatchData as unknown as { mock: { calls: unknown[][] } };
      const updates = () =>
        dispatchData.mock.calls
          .map((c) => c[0] as Record<string, unknown>)
          .filter((r) => r?.['type'] === 'chatUpdateParticipant');

      inst.onParticipantTalkativenessChange({ participantId: 'p1', value: 0.6 });
      inst.onParticipantTalkativenessChange({ participantId: 'p1', value: 0.8 });
      inst.onParticipantTalkativenessChange({ participantId: 'p1', value: 0.9 });
      await vi.advanceTimersByTimeAsync(399);
      expect(updates()).toHaveLength(0);
      await vi.advanceTimersByTimeAsync(1);
      expect(updates()).toEqual([
        {
          type: 'chatUpdateParticipant',
          chatId: 'chat-1',
          participantId: 'p1',
          talkativeness: 0.9,
        },
      ]);
    } finally {
      vi.useRealTimers();
    }
  });
  it('a pending roll rides the next send, then the list is cleared (v4 :606-666)', async () => {
    const events$ = new Subject<ScopedEvent>();
    const client = stubClient(chatDetail(), events$);
    const fixture = await render(client);
    const inst = fixture.componentInstance as unknown as {
      onPendingToolResult(r: Record<string, unknown>): void;
      pendingToolResults(): unknown[];
      send(p: { content: string; fileIds: string[] }): void;
    };

    inst.onPendingToolResult({
      tool: 'rng',
      displayName: 'Random Number Generator',
      icon: '🎲',
      summary: 'd20: 17',
      formattedResult: '🎲 Rolled 1d20: **17**',
      requestPrompt: 'Roll a d20',
      arguments: { type: 20, rolls: 1 },
      success: true,
    });
    expect(inst.pendingToolResults()).toHaveLength(1);

    inst.send({ content: 'and so it was decided', fileIds: [] });
    // Cleared BEFORE the request resolves — a second send cannot carry it twice.
    expect(inst.pendingToolResults()).toHaveLength(0);
    for (let i = 0; i < 5; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }

    const dispatch = client.dispatch as unknown as { mock: { calls: [CoreRequest][] } };
    const sent = dispatch.mock.calls
      .map((c) => c[0] as Record<string, unknown>)
      .find((r) => r['type'] === 'chatSend')!;
    // v4 maps to SIX fields; the chip's id/displayName/icon never travel.
    expect(sent['pendingToolResults']).toEqual([
      {
        tool: 'rng',
        success: true,
        result: '🎲 Rolled 1d20: **17**',
        prompt: 'Roll a d20',
        arguments: { type: 20, rolls: 1 },
        createdAt: expect.any(String),
      },
    ]);
  });

  it('an ordinary send carries no pendingToolResults key at all', async () => {
    const events$ = new Subject<ScopedEvent>();
    const client = stubClient(chatDetail(), events$);
    const fixture = await render(client);
    (
      fixture.componentInstance as unknown as { send(p: { content: string; fileIds: string[] }): void }
    ).send({ content: 'hello', fileIds: [] });
    for (let i = 0; i < 5; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    const dispatch = client.dispatch as unknown as { mock: { calls: [CoreRequest][] } };
    const sent = dispatch.mock.calls
      .map((c) => c[0] as Record<string, unknown>)
      .find((r) => r['type'] === 'chatSend')!;
    expect(sent['pendingToolResults']).toBeUndefined();
  });
});

/**
 * The Speaking-As choice is an optimistic BRIDGE, never a latch. v4 only ever
 * holds what a server response gave it (`useImpersonation.ts:63,107,134`), so a
 * v5 override that outranks `chat.activeTypingParticipantId` indefinitely
 * misattributes every optimistic bubble once the server drops that speaker —
 * which it does the moment the participant stops being user-controlled.
 */
describe('SalonConversation — the Speaking-As override does not outlive its round trip', () => {
  type SpeakerHost = {
    onSelectSpeaker(participantId: string): Promise<void>;
    activeSpeakerId(): string | null;
  };

  async function settle(fixture: ComponentFixture<SalonConversation>): Promise<void> {
    for (let i = 0; i < 5; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
  }

  it('shows the choice immediately, and an ACCEPTED user seat persists (v4 setActiveTypingParticipantId)', async () => {
    const events$ = new Subject<ScopedEvent>();
    // Selecting a genuine user seat (pu) is ACCEPTED — the server adds it to the
    // impersonating list and returns it (v4 handleSetActiveSpeaker applies the
    // reply). The chat GET projects no activeTypingParticipantId, so persistence
    // rides the local mirror, not the refetch.
    const base = stubClient({ ...chatDetail(), activeTypingParticipantId: null }, events$);
    const dispatchData = vi.fn(async (req: CoreRequest) =>
      req.type === 'chatSetActiveSpeaker'
        ? { impersonatingParticipantIds: ['pu'], activeTypingParticipantId: 'pu' }
        : { backgroundUrl: null, fileId: null, filename: null, sha256: null, linkSummary: null },
    );
    const client = { ...base, dispatchData: dispatchData as unknown as CoreClient['dispatchData'] };
    const fixture = await render(client);
    const inst = fixture.componentInstance as unknown as SpeakerHost;

    void inst.onSelectSpeaker('pu');
    expect(inst.activeSpeakerId()).toBe('pu'); // the optimistic bridge

    await settle(fixture);
    // The accepted choice sticks — the optimistic latch retired but the local
    // mirror carries it.
    expect(inst.activeSpeakerId()).toBe('pu');
  });

  it('reverts when the server REFUSES the choice (the optimistic latch does not outlive)', async () => {
    const events$ = new Subject<ScopedEvent>();
    // Selecting a non-impersonated LLM seat (p1) is refused — dispatchData
    // rejects; the optimistic latch must clear and nothing persists.
    const base = stubClient({ ...chatDetail(), activeTypingParticipantId: null }, events$);
    const dispatchData = vi.fn(async (req: CoreRequest) => {
      if (req.type === 'chatSetActiveSpeaker') throw new Error('Participant is not being impersonated');
      return { backgroundUrl: null, fileId: null, filename: null, sha256: null, linkSummary: null };
    });
    const client = { ...base, dispatchData: dispatchData as unknown as CoreClient['dispatchData'] };
    const fixture = await render(client);
    const inst = fixture.componentInstance as unknown as SpeakerHost;

    void inst.onSelectSpeaker('p1');
    expect(inst.activeSpeakerId()).toBe('p1'); // the bridge

    await settle(fixture);
    // THE GUARD: a refused choice must not survive. Before the fix a stale latch
    // did, and every later optimistic bubble wore that participant's name.
    expect(inst.activeSpeakerId()).toBeNull();
  });

  it('keeps the speaker the server DID persist', async () => {
    const events$ = new Subject<ScopedEvent>();
    const client = stubClient({ ...chatDetail(), activeTypingParticipantId: 'pu' }, events$);
    const fixture = await render(client);
    const inst = fixture.componentInstance as unknown as SpeakerHost;

    inst.onSelectSpeaker('pu');
    await settle(fixture);
    // Cleared the override, but the chat carries the choice — no flicker back.
    expect(inst.activeSpeakerId()).toBe('pu');
  });
});

/**
 * The composer turn banner (v4 Bug 46(a), `1bed814f`). Since impersonation is a
 * pure overlay (Bug 44), an impersonated seat keeps `controlledBy: 'llm'` and its
 * id sits in `impersonatingParticipantIds`. v4's client banner now gates on
 * `isUserDrivenSeat` over the overlay (not the bare column), so an impersonated
 * seat's OWN turn is announced — matching the server's `reason: 'user_turn'`.
 * Proven at the computed level: the fixture's genuine user seat always wins
 * weighted-random turn selection, so an impersonated seat cannot be forced to be
 * the next speaker in an e2e beat (see the impersonation e2e note).
 */
describe('SalonConversation — the user-turn banner honours the impersonation overlay (v4 Bug 46a)', () => {
  type BannerHost = {
    userTurnName(): string | null;
    mustSpeak(): boolean;
    turnInfo: { set(v: { nextSpeakerId: string | null; nextSpeakerControlledBy: string | null }): void };
    impersonatingLocal: { set(v: string[]): void };
  };

  it("announces an impersonated LLM seat's own turn, where the bare column would not", async () => {
    const events$ = new Subject<ScopedEvent>();
    const fixture = await render(stubClient(chatDetail(), events$));
    const inst = fixture.componentInstance as unknown as BannerHost;
    // p1 is an LLM-controlled seat whose id is in the impersonation overlay.
    inst.impersonatingLocal.set(['p1']);
    inst.turnInfo.set({ nextSpeakerId: 'p1', nextSpeakerControlledBy: 'llm' });
    fixture.detectChanges();
    // The banner names the impersonated character — the bare column ('llm')
    // would have returned null.
    expect(inst.userTurnName()).toBe('Friday');
  });

  it('does NOT announce an ordinary LLM seat with no overlay', async () => {
    const events$ = new Subject<ScopedEvent>();
    const fixture = await render(stubClient(chatDetail(), events$));
    const inst = fixture.componentInstance as unknown as BannerHost;
    inst.impersonatingLocal.set([]);
    inst.turnInfo.set({ nextSpeakerId: 'p1', nextSpeakerControlledBy: 'llm' });
    fixture.detectChanges();
    expect(inst.userTurnName()).toBeNull();
    expect(inst.mustSpeak()).toBe(false);
  });

  it('still announces a genuine user-controlled seat (unchanged)', async () => {
    const events$ = new Subject<ScopedEvent>();
    const fixture = await render(stubClient(chatDetail(), events$));
    const inst = fixture.componentInstance as unknown as BannerHost;
    inst.turnInfo.set({ nextSpeakerId: 'pu', nextSpeakerControlledBy: 'user' });
    fixture.detectChanges();
    expect(inst.userTurnName()).toBe('Bertie');
  });
});

/**
 * The optimistic user bubble's author (v4 Bug 45, `1bed814f`). The bubble must
 * be attributed to the seat the SERVER will resolve the message onto —
 * `findActiveUserParticipant`, honouring the impersonation overlay — or it
 * flickers to the wrong author on refetch. The old code attributed to any
 * participant matching the raw `activeTypingParticipantId`, which diverged from
 * the server when that id was not itself a user-driven seat.
 */
describe('SalonConversation — the optimistic bubble matches server attribution (v4 Bug 45)', () => {
  type SendHost = {
    send(p: { content: string; fileIds: string[] }): void;
    optimisticUser(): MessageDto | null;
  };

  it('attributes to an impersonated LLM seat via the overlay (matches the server)', async () => {
    const events$ = new Subject<ScopedEvent>();
    // p1 is LLM-controlled but impersonated this session; the human speaks as it.
    const chat = { ...chatDetail(), activeTypingParticipantId: 'p1', impersonatingParticipantIds: ['p1'] };
    const fixture = await render(stubClient(chat, events$));
    const inst = fixture.componentInstance as unknown as SendHost;

    inst.send({ content: 'as Friday, then', fileIds: [] });
    expect(inst.optimisticUser()?.participantId).toBe('p1');
  });

  it('falls back to the owner user seat when the active id is not user-driven', async () => {
    const events$ = new Subject<ScopedEvent>();
    // The active-typing id points at an LLM seat NOT in the overlay (e.g. a stale
    // selection just after stopping an impersonation). The server would attribute
    // to the owner user seat; the old bubble wrongly used the LLM seat's id.
    const chat = { ...chatDetail(), activeTypingParticipantId: 'p1', impersonatingParticipantIds: [] };
    const fixture = await render(stubClient(chat, events$));
    const inst = fixture.componentInstance as unknown as SendHost;

    inst.send({ content: 'back to me', fileIds: [] });
    expect(inst.optimisticUser()?.participantId).toBe('pu');
  });
});

/**
 * The "Speaking As" composer portrait's seat (v4 Bug 46(b), `1bed814f`). It is
 * resolved via `findActiveUserParticipant` (overlay-aware) and hydrated with the
 * character's name + avatar so it matches server attribution.
 */
describe('SalonConversation — the speaking-as composer seat (v4 Bug 46b)', () => {
  type SeatHost = { speakingAsSeat(): { name: string; avatarUrl: string | null } | null };

  it('resolves the impersonated LLM seat when the human speaks as it', async () => {
    const events$ = new Subject<ScopedEvent>();
    const chat = { ...chatDetail(), activeTypingParticipantId: 'p1', impersonatingParticipantIds: ['p1'] };
    const fixture = await render(stubClient(chat, events$));
    const inst = fixture.componentInstance as unknown as SeatHost;
    expect(inst.speakingAsSeat()).toEqual({ name: 'Friday', avatarUrl: null });
  });

  it('falls back to the owner user seat when no valid selection', async () => {
    const events$ = new Subject<ScopedEvent>();
    // No active-typing id and no overlay → the genuine user seat 'pu'/'Bertie'.
    const chat = { ...chatDetail(), activeTypingParticipantId: null };
    const fixture = await render(stubClient(chat, events$));
    const inst = fixture.componentInstance as unknown as SeatHost;
    expect(inst.speakingAsSeat()).toEqual({ name: 'Bertie', avatarUrl: null });
  });
});

/**
 * The impersonation flow END TO END (v4 Bugs 45 + 46(b)), driven the way the app
 * actually runs it: `assemble_chat_get` projects NO `activeTypingParticipantId`
 * (nor `impersonatingParticipantIds`), so the impersonate REPLY is the only
 * source. `onImpersonate` must apply `data.activeTypingParticipantId` to a local
 * mirror (v4 `setActiveTypingParticipantId(data.activeTypingParticipantId ||
 * participantId)`), or the speaking-as portrait and the optimistic bubble fall
 * back to the owner seat while impersonating — dogfood #76.
 *
 * ⚠ The older Bug 45/46b specs seeded `activeTypingParticipantId` onto the chat
 * DTO — a value the real GET never sends — which is exactly why they stayed green
 * while the live flow failed. These tests carry NO such phantom field.
 */
describe('SalonConversation — impersonation applies the reply, not a phantom chat field (dogfood #76)', () => {
  type Host = {
    onImpersonate(id: string): Promise<void>;
    onStopImpersonate(id: string): Promise<void>;
    onConfirmHandOff(profileId: string): Promise<void>;
    onSelectSpeaker(id: string): Promise<void>;
    controlledCharacters(): ReadonlyArray<{ participantId: string; name: string }>;
    speakingAsSeat(): { name: string; avatarUrl: string | null } | null;
    send(p: { content: string; fileIds: string[] }): void;
    optimisticUser(): MessageDto | null;
  };

  function impersonationClient(chat: ChatDetail): Partial<CoreClient> {
    const base = stubClient(chat, new Subject<ScopedEvent>());
    // v4's server sets activeTypingParticipantId = it || participantId on start,
    // and reassigns/clears it on stop; the reply carries both keys.
    let impersonating: string[] = [];
    const dispatchData = vi.fn(async (req: CoreRequest) => {
      if (req.type === 'chatImpersonate') {
        impersonating = [req.participantId];
        return {
          impersonatingParticipantIds: [...impersonating],
          activeTypingParticipantId: req.participantId,
          characterName: 'Friday',
        };
      }
      if (req.type === 'chatStopImpersonate') {
        impersonating = [];
        return { impersonatingParticipantIds: [], activeTypingParticipantId: null, characterName: 'Friday' };
      }
      if (req.type === 'chatSetActiveSpeaker') {
        // The server adds a genuine user seat to the impersonating list (so it is
        // "user-driven") and returns the updated list + the selected active id.
        const pid = req.participantId as string;
        if (!impersonating.includes(pid)) impersonating.push(pid);
        return {
          impersonatingParticipantIds: [...impersonating],
          activeTypingParticipantId: pid,
          characterName: 'Bertie',
        };
      }
      return { backgroundUrl: null, fileId: null, filename: null, sha256: null, linkSummary: null };
    });
    return { ...base, dispatchData: dispatchData as unknown as CoreClient['dispatchData'] };
  }

  it('resolves the impersonated seat for the portrait AND the optimistic bubble — with no activeTypingParticipantId on the chat', async () => {
    const chat = chatDetail();
    // The realistic condition the old specs skipped: the GET omits the key.
    expect((chat as unknown as Record<string, unknown>)['activeTypingParticipantId']).toBeUndefined();
    const fixture = await render(impersonationClient(chat));
    const inst = fixture.componentInstance as unknown as Host;

    // Before impersonating, the human speaks as their own owner seat.
    expect(inst.speakingAsSeat()).toEqual({ name: 'Bertie', avatarUrl: null });

    // Impersonate the LLM seat p1 (Friday) — the reply is the only source.
    await inst.onImpersonate('p1');
    fixture.detectChanges();

    // Bug 46(b): the portrait now shows the impersonated seat.
    expect(inst.speakingAsSeat()).toEqual({ name: 'Friday', avatarUrl: null });
    // Bug 45: a just-sent message is optimistically authored as it (matching the
    // server), with no flicker to Bertie.
    inst.send({ content: 'as Friday', fileIds: [] });
    expect(inst.optimisticUser()?.participantId).toBe('p1');
  });

  it('reverts to the owner seat when impersonation stops (via the hand-off confirm)', async () => {
    const fixture = await render(impersonationClient(chatDetail()));
    const inst = fixture.componentInstance as unknown as Host;
    await inst.onImpersonate('p1');
    fixture.detectChanges();
    expect(inst.speakingAsSeat()).toEqual({ name: 'Friday', avatarUrl: null });

    // p1 has no connection profile, so stop diverts to the hand-off dialog; the
    // confirm dispatches the stop, whose reply clears the active-typing seat.
    await inst.onStopImpersonate('p1');
    await inst.onConfirmHandOff('cp1');
    fixture.detectChanges();
    expect(inst.speakingAsSeat()).toEqual({ name: 'Bertie', avatarUrl: null });
  });

  // dogfood #77: while impersonating, the human must still be able to speak as
  // their OWN character. v4's `controlledCharacters` includes both the genuine
  // user seat and the impersonated seat, so the Speaking-As selector shows and
  // switches between them; v5 had listed only `controlledBy === 'user'`, hiding
  // the selector (< 2 seats) and locking the human to the impersonated seat.
  it('lists both the owner and the impersonated seat, and switches back to the owner (dogfood #77)', async () => {
    const fixture = await render(impersonationClient(chatDetail()));
    const inst = fixture.componentInstance as unknown as Host;

    // Only the owner seat before impersonating (the selector stays hidden).
    expect(inst.controlledCharacters().map((c) => c.name)).toEqual(['Bertie']);

    await inst.onImpersonate('p1');
    fixture.detectChanges();
    // Now BOTH seats are offered (selector shows at length >= 2), and the human
    // is speaking as the impersonated seat.
    expect(inst.controlledCharacters().map((c) => c.name).sort()).toEqual(['Bertie', 'Friday']);
    expect(inst.speakingAsSeat()).toEqual({ name: 'Friday', avatarUrl: null });

    // Switch back to the owner seat through the selector — the reply is applied
    // to the local mirror, so it sticks (no phantom chat field, no refetch reset).
    await inst.onSelectSpeaker('pu');
    fixture.detectChanges();
    expect(inst.speakingAsSeat()).toEqual({ name: 'Bertie', avatarUrl: null });
    // A message typed now is authored as the owner seat, not the impersonated one.
    inst.send({ content: 'back as myself', fileIds: [] });
    expect(inst.optimisticUser()?.participantId).toBe('pu');
  });
});

/**
 * v4 Bug 51's CLIENT half (`useImpersonation.ts:34-48`, `f6eac168`). Once the
 * sibling server lane (P4.D60) projects `impersonatingParticipantIds` /
 * `activeTypingParticipantId` on the chat GET, those persisted values go STALE
 * between a mutation reply and the refetch settling. v4 holds the overlay LOCAL
 * and treats the persisted values as a SEED, not a live source:
 *  - the impersonating LIST is the local mirror; the record re-seeds it only when
 *    NON-EMPTY (`:34-36`), and every transition (including → empty) is owned by
 *    the reply handlers (the refetch after a stop arrives already-consistent);
 *  - the speaking-as is seeded ONCE, only while still unset
 *    (`prev => prev ?? activeTypingId ?? null`, `:43`) — a refetch must never snap
 *    the composer back to the stale persisted seat after each turn.
 *
 * These specs seed the chat DTO with the projected fields (which the GET WILL
 * send once P4.D60 lands) on purpose — the inverse of the pre-seeded-DTO
 * false-green trap: here the seed is exactly what makes a stale-record clobber
 * reproducible, and the fix is what keeps the local authoritative. The
 * clobber-guard spec is RED against the old `fromChat.length > 0 ? fromChat`
 * computed.
 */
describe('SalonConversation — the persisted impersonation overlay is seeded, not re-applied (v4 Bug 51 client)', () => {
  type Host = {
    onStopImpersonate(id: string): Promise<void>;
    impersonatingIds(): readonly string[];
    activeSpeakerId(): string | null;
    activeTypingLocal: { set(v: string | null): void };
  };

  function overlayClient(getChat: () => ChatDetail): Partial<CoreClient> {
    // A chatGet that returns a FRESH object each call, so an invalidation
    // genuinely changes `chat()` and re-fires the sync effect (that is the only
    // condition under which seed-once is observably different from a plain set).
    const dispatch = vi.fn(async (req: CoreRequest): Promise<CoreResponse> => {
      if (req.type === 'chatGet') return { type: 'chat', data: { chat: getChat() } };
      if (req.type === 'chatSettings') {
        return { type: 'chatSettings', data: { avatarDisplayMode: 'ALWAYS', avatarDisplayStyle: 'CIRCULAR' } };
      }
      return { type: 'ack', data: {} };
    });
    const dispatchData = vi.fn(async (req: CoreRequest) =>
      req.type === 'chatStopImpersonate'
        ? { impersonatingParticipantIds: [], activeTypingParticipantId: null }
        : { backgroundUrl: null, fileId: null, filename: null, sha256: null, linkSummary: null },
    );
    return {
      events$: new Subject<ScopedEvent>().asObservable(),
      connection: signal<ConnectionState>('idle'),
      resyncCounter: signal(0),
      dispatch,
      dispatchData: dispatchData as unknown as CoreClient['dispatchData'],
      dispatchExpect: (async (req: CoreRequest, expect: string) => {
        const resp = await dispatch(req);
        if (resp.type !== expect) throw new Error(`unexpected ${resp.type}`);
        return resp;
      }) as CoreClient['dispatchExpect'],
    };
  }

  it('re-seeds the impersonating list and speaking-as from a non-empty record (reload)', async () => {
    // A reloaded chat that (with P4.D60) carries the projected overlay.
    const chat = () => ({
      ...chatDetail(),
      impersonatingParticipantIds: ['p1'],
      activeTypingParticipantId: 'p1',
    });
    const fixture = await render(overlayClient(chat));
    const inst = fixture.componentInstance as unknown as Host;
    expect([...inst.impersonatingIds()]).toEqual(['p1']);
    expect(inst.activeSpeakerId()).toBe('p1'); // seeded once from the record
  });

  it('a stale non-empty record does NOT resurrect impersonation after a stop (the clobber guard)', async () => {
    // p1 has a connection profile so the stop goes direct (no hand-off dialog).
    const chat = () => ({
      ...chatDetail(),
      participants: [
        participant({ id: 'pu', controlledBy: 'user', character: { id: 'u', name: 'Bertie', title: null, avatarUrl: null, defaultImageId: null, defaultImage: null } }),
        participant({ id: 'p1', connectionProfile: { id: 'cp1', name: 'GPT' } as ParticipantDetail['connectionProfile'] }),
      ],
      impersonatingParticipantIds: ['p1'],
      activeTypingParticipantId: 'p1',
    });
    const fixture = await render(overlayClient(chat));
    const inst = fixture.componentInstance as unknown as Host;
    expect([...inst.impersonatingIds()]).toEqual(['p1']); // seeded

    // Stop impersonating. The reply clears the list, but the chat() STILL holds the
    // stale projected ['p1'] until the next refetch settles.
    await inst.onStopImpersonate('p1');
    fixture.detectChanges();

    // THE GUARD: the local mirror is authoritative — the stale record must not
    // resurrect the overlay. Before the fix, `fromChat.length > 0 ? fromChat` won.
    expect([...inst.impersonatingIds()]).toEqual([]);
  });

  it('seeds the speaking-as ONCE — a later refetch does not clobber a moved seat', async () => {
    // Each fetch returns a GENUINELY different object (a bumped `updatedAt`), so
    // TanStack's structural sharing yields a NEW reference and the sync effect
    // actually re-fires. A deep-equal stub would be silently kept by reference
    // (`replaceEqualDeep`), the effect would never re-run, and this spec would
    // pass even against an unconditional re-apply — the false green the §3
    // unification review caught.
    let fetchSeq = 0;
    const chat = () => ({
      ...chatDetail(),
      updatedAt: `2026-01-01T00:00:0${fetchSeq++ % 10}.000Z`,
      impersonatingParticipantIds: ['p1'],
      activeTypingParticipantId: 'p1',
    });
    const fixture = await render(overlayClient(chat));
    const inst = fixture.componentInstance as unknown as Host;
    expect(inst.activeSpeakerId()).toBe('p1'); // seeded once

    // The turn-follow / a manual pick moves the speaking-as to the owner seat.
    inst.activeTypingLocal.set('pu');
    fixture.detectChanges();
    expect(inst.activeSpeakerId()).toBe('pu');

    // Force a refetch: the bumped `updatedAt` changes chat(), so the sync effect
    // RE-FIRES over the still-non-empty list. Seed-once (`prev ?? …`) leaves the
    // moved seat alone — an unconditional re-apply would snap it back to the
    // persisted 'p1'.
    await TestBed.inject(QueryClient).invalidateQueries({ queryKey: ['chat', 'chat-1'] });
    for (let i = 0; i < 5; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(inst.activeSpeakerId()).toBe('pu');
  });
});

/**
 * v4 Bug 48 (`SalonView.tsx` `handleImpersonateAndTakeTurn`, `f6eac168`):
 * impersonating a character hands them the CURRENT turn — unless an LLM is
 * mid-generation — so the banner names them and a typed message lands in turn.
 * v5's turn is server-authoritative and auto-refreshed, so this is a client
 * `turnOverride` that layers above the server-queried turn (see the lane
 * record). Both v4 wiring sites route through `onImpersonate` here, so the
 * AllLLMPause take-over inherits it.
 */
describe('SalonConversation — impersonating takes the current turn (v4 Bug 48)', () => {
  type Host = {
    onImpersonate(id: string): Promise<void>;
    onAllLLMTakeOver(id: string): Promise<void>;
    userTurnName(): string | null;
    turnOverride(): { nextSpeakerId: string | null; reason: string; cycleComplete: boolean } | null;
    stream: { set(v: unknown): void };
  };

  function takeTurnClient(chat: ChatDetail): Partial<CoreClient> {
    const base = stubClient(chat, new Subject<ScopedEvent>());
    const dispatchData = vi.fn(async (req: CoreRequest) =>
      req.type === 'chatImpersonate'
        ? { impersonatingParticipantIds: [req.participantId as string], activeTypingParticipantId: req.participantId as string }
        : { backgroundUrl: null, fileId: null, filename: null, sha256: null, linkSummary: null },
    );
    return { ...base, dispatchData: dispatchData as unknown as CoreClient['dispatchData'] };
  }

  it('hands the turn to the impersonated seat when idle — the banner names it', async () => {
    const fixture = await render(takeTurnClient(chatDetail()));
    const inst = fixture.componentInstance as unknown as Host;
    expect(inst.userTurnName()).toBeNull(); // no user turn before impersonating

    await inst.onImpersonate('p1'); // p1 = Friday, an LLM seat
    fixture.detectChanges();

    expect(inst.turnOverride()?.nextSpeakerId).toBe('p1');
    expect(inst.turnOverride()?.reason).toBe('queue');
    expect(inst.turnOverride()?.cycleComplete).toBe(false);
    // The banner reads the override (via the overlay) and names the impersonated
    // seat, whose bare `controlledBy` is still 'llm'.
    expect(inst.userTurnName()).toBe('Friday');
  });

  it('leaves an in-flight generation untouched (no override while streaming)', async () => {
    const fixture = await render(takeTurnClient(chatDetail()));
    const inst = fixture.componentInstance as unknown as Host;
    // An LLM is mid-stream — busy() is true.
    inst.stream.set({ waitingForResponse: true });
    fixture.detectChanges();

    await inst.onImpersonate('p1');
    fixture.detectChanges();
    // The impersonation applied, but the current turn was NOT seized mid-stream.
    expect(inst.turnOverride()).toBeNull();
  });

  it('the AllLLMPause take-over path inherits the turn handoff', async () => {
    const fixture = await render(takeTurnClient(chatDetail()));
    const inst = fixture.componentInstance as unknown as Host;
    await inst.onAllLLMTakeOver('p1');
    fixture.detectChanges();
    expect(inst.turnOverride()?.nextSpeakerId).toBe('p1');
    expect(inst.userTurnName()).toBe('Friday');
  });
});

/**
 * v4 Bug 49 (`SalonView.tsx` turn-follow effect, `f6eac168`): the composer's
 * speaking-as follows the current user-driven turn. It is a latch keyed on the
 * turn SEAT — it reacts only when the user-driven seat CHANGES, clears on a
 * non-user seat or no seat, and never fights a deliberate same-turn pick.
 */
describe('SalonConversation — the speaking-as follows the user-driven turn (v4 Bug 49)', () => {
  type FollowHost = {
    activeSpeakerId(): string | null;
    turnInfo: { set(v: { nextSpeakerId: string | null; nextSpeakerControlledBy: string | null }): void };
    impersonatingLocal: { set(v: string[]): void };
    activeTypingLocal: { set(v: string | null): void };
    lastFollowedTurnSeat: string | null;
  };

  function threeSeatChat(): ChatDetail {
    return {
      ...chatDetail(),
      participants: [
        participant({ id: 'pu', controlledBy: 'user', character: { id: 'u', name: 'Bertie', title: null, avatarUrl: null, defaultImageId: null, defaultImage: null } }),
        participant({ id: 'p1', controlledBy: 'llm', character: { id: 'c1', name: 'Friday', title: null, avatarUrl: null, defaultImageId: null, defaultImage: null } }),
        participant({ id: 'p2', controlledBy: 'user', character: { id: 'u2', name: 'Jeeves', title: null, avatarUrl: null, defaultImageId: null, defaultImage: null } }),
      ],
    };
  }

  async function mount(): Promise<{ fixture: ComponentFixture<SalonConversation>; inst: FollowHost }> {
    const fixture = await render(stubClient(threeSeatChat(), new Subject<ScopedEvent>()));
    return { fixture, inst: fixture.componentInstance as unknown as FollowHost };
  }

  it('defaults the speaking-as to a genuine user-driven turn seat', async () => {
    const { fixture, inst } = await mount();
    inst.turnInfo.set({ nextSpeakerId: 'pu', nextSpeakerControlledBy: 'user' });
    fixture.detectChanges();
    expect(inst.activeSpeakerId()).toBe('pu');
  });

  it('follows an impersonated LLM seat via the overlay (its own turn)', async () => {
    const { fixture, inst } = await mount();
    inst.impersonatingLocal.set(['p1']);
    inst.turnInfo.set({ nextSpeakerId: 'p1', nextSpeakerControlledBy: 'llm' });
    fixture.detectChanges();
    expect(inst.activeSpeakerId()).toBe('p1');
  });

  it('reacts when the user-driven turn seat CHANGES', async () => {
    const { fixture, inst } = await mount();
    inst.turnInfo.set({ nextSpeakerId: 'pu', nextSpeakerControlledBy: 'user' });
    fixture.detectChanges();
    expect(inst.activeSpeakerId()).toBe('pu');
    inst.turnInfo.set({ nextSpeakerId: 'p2', nextSpeakerControlledBy: 'user' });
    fixture.detectChanges();
    expect(inst.activeSpeakerId()).toBe('p2');
  });

  it('does NOT fight a deliberate same-turn pick (the latch)', async () => {
    const { fixture, inst } = await mount();
    inst.turnInfo.set({ nextSpeakerId: 'pu', nextSpeakerControlledBy: 'user' });
    fixture.detectChanges();
    expect(inst.activeSpeakerId()).toBe('pu'); // followed

    // The human deliberately picks another seat on the SAME turn.
    inst.activeTypingLocal.set('p2');
    fixture.detectChanges();
    // Force the effect to re-run (a tracked dep changes) while the turn seat is
    // unchanged — the seat latch must leave the manual pick alone.
    inst.impersonatingLocal.set(['p1']);
    fixture.detectChanges();
    expect(inst.activeSpeakerId()).toBe('p2');
  });

  it('an LLM (non-impersonated) turn clears the latch and does not follow', async () => {
    const { fixture, inst } = await mount();
    inst.turnInfo.set({ nextSpeakerId: 'pu', nextSpeakerControlledBy: 'user' });
    fixture.detectChanges();
    expect(inst.lastFollowedTurnSeat).toBe('pu');

    inst.turnInfo.set({ nextSpeakerId: 'p1', nextSpeakerControlledBy: 'llm' });
    fixture.detectChanges();
    expect(inst.lastFollowedTurnSeat).toBeNull();
    // The speaking-as was not dragged onto the LLM seat.
    expect(inst.activeSpeakerId()).toBe('pu');
  });

  it('no next speaker clears the latch', async () => {
    const { fixture, inst } = await mount();
    inst.turnInfo.set({ nextSpeakerId: 'pu', nextSpeakerControlledBy: 'user' });
    fixture.detectChanges();
    expect(inst.lastFollowedTurnSeat).toBe('pu');

    inst.turnInfo.set({ nextSpeakerId: null, nextSpeakerControlledBy: null });
    fixture.detectChanges();
    expect(inst.lastFollowedTurnSeat).toBeNull();
  });
});

/**
 * The AllLLMPause live opener (v4 `bd419ae9`, the P4.D54 deferral) + its take-over
 * → Bug 48 handoff. The modal auto-opens when an all-LLM chat becomes paused
 * (`isPaused && isAllLLM`), and taking over a character from it starts
 * impersonating AND hands them the current turn.
 *
 * ⚠ Covered here at the UNIT level, deterministically, rather than as an e2e
 * live-opener beat: the committed salon fixture has NO all-LLM chat (both chats
 * carry a genuine user seat), the pause THRESHOLD needs real LLM turns the
 * key-less e2e instance cannot make, and creating an all-LLM chat in-walk
 * triggers auto-greeting streaming that makes any opener assertion
 * timing-dependent — a flaky beat the order says to avoid. Recorded in the lane
 * record.
 */
describe('SalonConversation — the AllLLMPause opener + take-over hands the turn (v4 bd419ae9 + Bug 48)', () => {
  type Host = {
    showAllLLMPause(): boolean;
    onAllLLMTakeOver(id: string): Promise<void>;
    turnOverride(): { nextSpeakerId: string | null } | null;
    userTurnName(): string | null;
  };

  function allLLMChat(): ChatDetail {
    return {
      ...chatDetail(),
      isPaused: true,
      allLLMPauseTurnCount: 3,
      participants: [
        participant({ id: 'p1', controlledBy: 'llm', character: { id: 'c1', name: 'Friday', title: null, avatarUrl: null, defaultImageId: null, defaultImage: null } }),
        participant({ id: 'p2', controlledBy: 'llm', character: { id: 'c2', name: 'Aria', title: null, avatarUrl: null, defaultImageId: null, defaultImage: null } }),
      ],
      // No USER message → `isAllLLM` stays true.
      messages: [
        message({ id: 'a1', role: 'ASSISTANT', participantId: 'p1', content: 'Shall we begin?', createdAt: '2024-01-01T00:00:01.000Z' }),
      ],
    };
  }

  function takeOverClient(chat: ChatDetail): Partial<CoreClient> {
    const base = stubClient(chat, new Subject<ScopedEvent>());
    const dispatchData = vi.fn(async (req: CoreRequest) =>
      req.type === 'chatImpersonate'
        ? { impersonatingParticipantIds: [req.participantId as string], activeTypingParticipantId: req.participantId as string }
        : { backgroundUrl: null, fileId: null, filename: null, sha256: null, linkSummary: null },
    );
    return { ...base, dispatchData: dispatchData as unknown as CoreClient['dispatchData'] };
  }

  it('auto-opens when an all-LLM chat is paused', async () => {
    const fixture = await render(takeOverClient(allLLMChat()));
    const inst = fixture.componentInstance as unknown as Host;
    expect(inst.showAllLLMPause()).toBe(true);
  });

  it('taking over a character closes the modal, impersonates, and hands them the turn (Bug 48)', async () => {
    const fixture = await render(takeOverClient(allLLMChat()));
    const inst = fixture.componentInstance as unknown as Host;
    expect(inst.showAllLLMPause()).toBe(true);

    await inst.onAllLLMTakeOver('p1');
    fixture.detectChanges();

    expect(inst.showAllLLMPause()).toBe(false);
    expect(inst.turnOverride()?.nextSpeakerId).toBe('p1');
    expect(inst.userTurnName()).toBe('Friday');
  });
});

describe('audienceCandidates (v4 ChatModals.tsx:325-332, a163862c)', () => {
  interface AnnouncementHost {
    showAnnouncement: { set(v: boolean): void };
    audienceCandidates(): ReadonlyArray<{ participantId: string; name: string; status?: string }>;
  }

  it('excludes non-characters, hard-removed, and soft-removed participants; includes silent/absent', async () => {
    const chat: ChatDetail = {
      ...chatDetail(),
      participants: [
        participant({ id: 'pu', controlledBy: 'user', character: { id: 'u', name: 'Bertie', title: null, avatarUrl: null, defaultImageId: null, defaultImage: null } }),
        participant({ id: 'p-active', character: { id: 'c-active', name: 'Aria', title: null, avatarUrl: null, defaultImageId: null, defaultImage: null } }),
        participant({ id: 'p-silent', status: 'silent', character: { id: 'c-silent', name: 'Dax', title: null, avatarUrl: null, defaultImageId: null, defaultImage: null } }),
        // Hard-removed: removedAt set. Excluded by `!p.removedAt`.
        participant({ id: 'p-hard-removed', removedAt: '2024-01-02T00:00:00.000Z', character: { id: 'c-gone', name: 'Ghost', title: null, avatarUrl: null, defaultImageId: null, defaultImage: null } }),
        // Soft-removed: status is 'removed' but removedAt is unset. Excluded by
        // `status !== 'removed'` — the filter v4 carries and v5 had been
        // missing until the sibling P4.9E1B cast-walk beat caught it.
        participant({ id: 'p-soft-removed', status: 'removed', character: { id: 'c-soft', name: 'Shade', title: null, avatarUrl: null, defaultImageId: null, defaultImage: null } }),
      ],
    };
    const events$ = new Subject<ScopedEvent>();
    const fixture = await render(stubClient(chat, events$));
    const inst = fixture.componentInstance as unknown as AnnouncementHost;

    const names = inst.audienceCandidates().map((c) => c.name).sort();
    // The human's own participant (`type: 'CHARACTER'`, `controlledBy: 'user'`)
    // IS a valid candidate in v4 too — nothing excludes it.
    expect(names).toEqual(['Aria', 'Bertie', 'Dax']);
    expect(inst.audienceCandidates().find((c) => c.name === 'Dax')?.status).toBe('silent');
  });

  it('threads the same candidates into the Insert Announcement dialog', async () => {
    const chat: ChatDetail = {
      ...chatDetail(),
      participants: [
        participant({ id: 'p-active', character: { id: 'c-active', name: 'Aria', title: null, avatarUrl: null, defaultImageId: null, defaultImage: null } }),
      ],
    };
    const events$ = new Subject<ScopedEvent>();
    const fixture = await render(stubClient(chat, events$));
    const inst = fixture.componentInstance as unknown as AnnouncementHost;
    inst.showAnnouncement.set(true);
    fixture.detectChanges();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    const text = (fixture.nativeElement.textContent ?? '').replace(/\s+/g, ' ');
    expect(text).toContain('Who hears it');
    expect(text).toContain('Aria');
  });
});

/**
 * P4.30 — the chat's roleplay template fetch (v4 `SalonView.tsx:745-776`).
 *
 * Before this, nothing in v5 ever read the chat's template, so every message in
 * every conversation rendered with the built-in defaults. The three reset arms
 * are ported as ONE reset on purpose: no template id, a non-ok response, and a
 * throw all land on `undefined`, which is what lets a chat pointing at a DELETED
 * template keep rendering rather than fail.
 *
 * These assert through the DOM rather than the private signals: the wire from
 * `chat.roleplayTemplateId` to the class on the rendered span is the deliverable,
 * and a spec that read the signal would pass with the binding removed.
 */
describe('SalonConversation — the roleplay template reaches the rendered rows (P4.30)', () => {
  const CUSTOM_TEMPLATE = {
    id: 'tpl-1',
    userId: null,
    name: 'Guillemets & Ampersands',
    systemPrompt: '',
    isBuiltIn: false,
    tags: [],
    delimiters: [],
    renderingPatterns: [{ pattern: '@@[^@]+@@', className: 'qt-chat-emote' }],
    dialogueDetection: { openingChars: ['«'], closingChars: ['»'], className: 'qt-chat-custom-dialogue' },
    narrationDelimiters: '*',
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
  };

  /** A chat whose one message carries both custom markers. */
  function templatedChat(roleplayTemplateId: string | null): ChatDetail {
    return {
      ...chatDetail(),
      roleplayTemplateId,
      messages: [
        message({
          id: 'a1',
          role: 'ASSISTANT',
          participantId: 'p1',
          content: 'She paused. @@leans in@@ and whispered.',
          createdAt: '2024-01-01T00:00:02.000Z',
        }),
      ],
    };
  }

  /**
   * A client whose `roleplayTemplateGet` is scripted. `template` may be a value
   * (the ok arm) or a thrown error (v4's `!res.ok` / catch arms — `dispatchData`
   * rejects on an error envelope).
   */
  function templateClient(
    chat: ChatDetail,
    template: Record<string, unknown> | Error | null,
    seen: string[] = [],
  ): Partial<CoreClient> {
    const base = stubClient(chat, new Subject<ScopedEvent>());
    const dispatchData = vi.fn(async (req: CoreRequest) => {
      if (req.type === 'roleplayTemplateGet') {
        seen.push(req.templateId);
        if (template instanceof Error) throw template;
        return (template ?? {}) as Record<string, unknown>;
      }
      return { backgroundUrl: null, fileId: null, filename: null, sha256: null, linkSummary: null };
    });
    return { ...base, dispatchData: dispatchData as unknown as CoreClient['dispatchData'] };
  }

  function emotes(fixture: ComponentFixture<SalonConversation>): number {
    return fixture.nativeElement.querySelectorAll('.qt-chat-emote').length;
  }

  it('renders with the defaults — and fetches nothing — when the chat has no template', async () => {
    const seen: string[] = [];
    const fixture = await render(templateClient(templatedChat(null), CUSTOM_TEMPLATE, seen));
    expect(seen).toEqual([]);
    expect(emotes(fixture)).toBe(0);
  });

  it("applies the fetched template's patterns to a rendered message", async () => {
    const seen: string[] = [];
    const fixture = await render(templateClient(templatedChat('tpl-1'), CUSTOM_TEMPLATE, seen));
    expect(seen).toEqual(['tpl-1']);
    expect(emotes(fixture)).toBe(1);
  });

  it('falls back to the defaults when the template GET fails (a deleted template)', async () => {
    const seen: string[] = [];
    const fixture = await render(
      templateClient(templatedChat('tpl-gone'), new Error('NOT_FOUND'), seen),
    );
    expect(seen).toEqual(['tpl-gone']);
    // v4's non-ok arm: reset to undefined, which IS the defaults — the room
    // still renders.
    expect(emotes(fixture)).toBe(0);
    expect(fixture.nativeElement.textContent).toContain('leans in');
  });

  it('falls back to the defaults when the template carries an EMPTY patterns array', async () => {
    const fixture = await render(
      templateClient(templatedChat('tpl-1'), { ...CUSTOM_TEMPLATE, renderingPatterns: [] }),
    );
    expect(emotes(fixture)).toBe(0);
    expect(fixture.nativeElement.textContent).toContain('leans in');
  });

  it('applies the fetched dialogue detection as well as the patterns', async () => {
    const chat: ChatDetail = {
      ...chatDetail(),
      roleplayTemplateId: 'tpl-1',
      messages: [
        message({
          id: 'a1',
          role: 'ASSISTANT',
          participantId: 'p1',
          content: '«Bonjour, mon ami»',
          createdAt: '2024-01-01T00:00:02.000Z',
        }),
      ],
    };
    const fixture = await render(templateClient(chat, CUSTOM_TEMPLATE));
    expect(fixture.nativeElement.querySelectorAll('.qt-chat-custom-dialogue').length).toBe(1);
  });

  /**
   * The reconcile point v4 gets from its effect's `[chat?.roleplayTemplateId]`
   * dep: change the template from the sidebar, the chat refetches, the id moves,
   * and the room re-renders through the new template WITHOUT a reload.
   */
  it('re-fetches and re-renders when the template changes mid-session', async () => {
    const seen: string[] = [];
    const live = { chat: templatedChat(null) };
    const events$ = new Subject<ScopedEvent>();
    const dispatch = vi.fn(async (req: CoreRequest): Promise<CoreResponse> => {
      if (req.type === 'chatGet') return { type: 'chat', data: { chat: live.chat } };
      if (req.type === 'chatSettings') {
        return {
          type: 'chatSettings',
          data: { avatarDisplayMode: 'ALWAYS', avatarDisplayStyle: 'CIRCULAR' },
        };
      }
      return { type: 'ack', data: {} };
    });
    const dispatchData = vi.fn(async (req: CoreRequest) => {
      if (req.type === 'roleplayTemplateGet') {
        seen.push(req.templateId);
        return CUSTOM_TEMPLATE as unknown as Record<string, unknown>;
      }
      return { backgroundUrl: null, fileId: null, filename: null, sha256: null, linkSummary: null };
    });
    const client: Partial<CoreClient> = {
      events$: events$.asObservable(),
      connection: signal<ConnectionState>('idle'),
      resyncCounter: signal(0),
      dispatch,
      dispatchData: dispatchData as unknown as CoreClient['dispatchData'],
      dispatchExpect: (async (req: CoreRequest, expected: string) => {
        const resp = await dispatch(req);
        if (resp.type !== expected) throw new Error(`unexpected ${resp.type}`);
        return resp;
      }) as CoreClient['dispatchExpect'],
    };

    const fixture = await render(client);
    expect(seen).toEqual([]);
    expect(emotes(fixture)).toBe(0);

    // The sidebar saves a template; the chat refetches carrying the new id.
    live.chat = templatedChat('tpl-1');
    await TestBed.inject(QueryClient).invalidateQueries({ queryKey: ['chat', 'chat-1'] });
    for (let i = 0; i < 6; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }

    expect(seen).toEqual(['tpl-1']);
    expect(emotes(fixture)).toBe(1);
  });
});

/**
 * Bug 77 — the tool-execution notice owns its own lifetime.
 *
 * v4's banner above the composer ("Generating image..." / "Successfully
 * generated N image(s)!") had exactly one teardown — a detached `setTimeout` at
 * the bottom of `sendMessage`'s terminal `onDone` — so every other route out of
 * a turn stranded it, and it carried no close control. v4 `25767c0f` moves
 * ownership onto the notice itself.
 *
 * ⚠ v5 never ported this surface at all: the settled outcomes landed as toasts
 * (which v4 raises TOO, with different sentences) and the `'pending'` half did
 * not exist. So this is v4's mechanism landed in its FIXED form, and these are
 * the first specs over `reportStreamTransitions` in either direction.
 *
 * No v4 test file shipped with the fix — the parity target is the surveyed v4
 * mechanism at `useSSEStreaming.ts:37-38,308-357,381,417,424,848,1016,1128`,
 * with the message bytes carried verbatim.
 */
describe('SalonConversation tool-execution notice (Bug 77)', () => {
  /** A stream state carrying exactly the given generate_image calls. */
  function state(
    calls: {
      id: string;
      status: 'pending' | 'success' | 'error';
      result?: unknown;
      errorText?: string;
    }[],
    over: Partial<ChatStreamState> = {},
  ): ChatStreamState {
    return {
      ...initialChatStreamState(),
      toolBatches: calls.length
        ? [{ offset: 0, calls: calls.map((c) => ({ name: 'generate_image', ...c })) }]
        : [],
      ...over,
    };
  }

  /** Drive one transition through the vertical's reporter. */
  function report(
    fixture: ComponentFixture<SalonConversation>,
    before: ChatStreamState,
    after: ChatStreamState,
  ): void {
    (
      fixture.componentInstance as unknown as {
        reportStreamTransitions(a: ChatStreamState, b: ChatStreamState): void;
      }
    ).reportStreamTransitions(before, after);
  }

  function notice(fixture: ComponentFixture<SalonConversation>): ToolExecutionStatus | null {
    return (
      fixture.componentInstance as unknown as {
        toolExecutionStatus(): ToolExecutionStatus | null;
      }
    ).toolExecutionStatus();
  }

  it('raises a pending notice the moment a generate_image batch is detected', async () => {
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    report(fixture, state([]), state([{ id: 't0', status: 'pending' }]));
    // v4 `trackToolsDetected:381` — the message bytes, verbatim.
    expect(notice(fixture)).toEqual({
      tool: 'generate_image',
      status: 'pending',
      message: 'Generating image...',
    });
  });

  it('raises it only ONCE per call, however many transitions carry it', async () => {
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    const pending = state([{ id: 't0', status: 'pending' }]);
    report(fixture, state([]), pending);
    (fixture.componentInstance as unknown as { dismissToolExecutionStatus(): void })[
      'dismissToolExecutionStatus'
    ]();
    // A transition where the call was already seen must not re-raise it — v4
    // raises from the detection EVENT, which fires once.
    report(fixture, pending, pending);
    expect(notice(fixture)).toBeNull();
  });

  it('supersedes the pending notice with the settled one, and toasts alongside it', async () => {
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    const pending = state([{ id: 't0', status: 'pending' }]);
    report(fixture, state([]), pending);
    report(
      fixture,
      pending,
      state([{ id: 't0', status: 'success', result: { images: [{}, {}] } }]),
    );
    // v4 `:417-421` raises the NOTICE and the toast both — different sentences,
    // neither standing in for the other.
    expect(notice(fixture)).toEqual({
      tool: 'generate_image',
      status: 'success',
      message: 'Successfully generated 2 images!',
    });
    expect(toasts().at(-1)).toEqual({
      type: 'success',
      message: 'Image generation complete! 2 images generated.',
    });
  });

  it('carries v4’s singular wording, and the generic bytes when a failure says nothing', async () => {
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    report(
      fixture,
      state([{ id: 't0', status: 'pending' }]),
      state([{ id: 't0', status: 'success', result: { images: [{}] } }]),
    );
    expect(notice(fixture)!.message).toBe('Successfully generated 1 image!');

    report(
      fixture,
      state([{ id: 't1', status: 'pending' }]),
      state([{ id: 't1', status: 'error', result: {} }]),
    );
    // v4 `:447,:453` — with nothing resolvable on the frame the notice falls
    // back to 'Failed to generate image' where the toast beside it says
    // 'Unknown error'; both bytes are v4's, and they SURVIVE the Bug 84 fix as
    // the fallback (this used to be all a failure could ever say).
    expect(notice(fixture)).toEqual({
      tool: 'generate_image',
      status: 'error',
      message: 'Failed to generate image',
    });
    expect(toasts().at(-1)).toEqual({
      type: 'error',
      message: 'Image generation failed: Unknown error',
    });
  });

  it("renders the failing tool's own sentence, prefix stripped (Bug 84)", async () => {
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    report(
      fixture,
      state([{ id: 't2', status: 'pending' }]),
      state([
        {
          id: 't2',
          status: 'error',
          result: null,
          // The real failure shape: `result` null, the sentence on the SIBLING
          // `error`, which the reducer carried onto the call as `errorText`.
          errorText: 'Error: Image generation is not enabled for this chat',
        },
      ]),
    );
    // v4 `:447,:453` — both surfaces render the SAME resolved sentence, with
    // the executor's own `Error: ` wrapper stripped so the toast doesn't read
    // 'Image generation failed: Error: …'.
    expect(notice(fixture)).toEqual({
      tool: 'generate_image',
      status: 'error',
      message: 'Image generation is not enabled for this chat',
    });
    expect(toasts().at(-1)).toEqual({
      type: 'error',
      message: 'Image generation failed: Image generation is not enabled for this chat',
    });
  });

  it('pins the WHOLE path: frame → reducer → reporter → notice (Bug 84)', async () => {
    // The specs above drive the reporter directly, which is exactly how a
    // reducer-level drop stayed invisible for a whole round. This one starts
    // from REAL frames, so reverting either layer — the reducer's `errorText`
    // carry or the render site's resolver read — turns it red.
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    const detected = reduceChatFrame(initialChatStreamState(), {
      toolsDetected: 1,
      toolNames: ['generate_image'],
    });
    const failed = reduceChatFrame(detected, {
      toolResult: {
        index: 0,
        name: 'generate_image',
        success: false,
        result: null,
        error: 'Error: No image profile is configured',
      },
    });
    report(fixture, detected, failed);
    expect(notice(fixture)!.message).toBe('No image profile is configured');
    expect(toasts().at(-1)).toEqual({
      type: 'error',
      message: 'Image generation failed: No image profile is configured',
    });
  });

  it('a settled notice dismisses itself after 6s, and not a tick before', async () => {
    // ⚠ Render BEFORE installing fake timers: the harness settles the TanStack
    // queries by awaiting real `setTimeout(0)` ticks, which fake timers freeze —
    // the test then hangs to its 5 s timeout, its `finally` never runs, and the
    // leaked fake clock hangs every spec after it too.
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    vi.useFakeTimers();
    try {
      report(
        fixture,
        state([{ id: 't0', status: 'pending' }]),
        state([{ id: 't0', status: 'success', result: { images: [{}] } }]),
      );
      vi.advanceTimersByTime(5999);
      expect(notice(fixture)).not.toBeNull();
      vi.advanceTimersByTime(1);
      expect(notice(fixture)).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it('a pending notice never expires on its own', async () => {
    // ⚠ Render BEFORE installing fake timers: the harness settles the TanStack
    // queries by awaiting real `setTimeout(0)` ticks, which fake timers freeze —
    // the test then hangs to its 5 s timeout, its `finally` never runs, and the
    // leaked fake clock hangs every spec after it too.
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    vi.useFakeTimers();
    try {
      report(fixture, state([]), state([{ id: 't0', status: 'pending' }]));
      vi.advanceTimersByTime(60_000);
      expect(notice(fixture)!.status).toBe('pending');
    } finally {
      vi.useRealTimers();
    }
  });

  it('a new publish supersedes the previous countdown rather than stacking timers', async () => {
    // ⚠ Render BEFORE installing fake timers: the harness settles the TanStack
    // queries by awaiting real `setTimeout(0)` ticks, which fake timers freeze —
    // the test then hangs to its 5 s timeout, its `finally` never runs, and the
    // leaked fake clock hangs every spec after it too.
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    vi.useFakeTimers();
    try {
      report(
        fixture,
        state([{ id: 't0', status: 'pending' }]),
        state([{ id: 't0', status: 'success', result: { images: [{}] } }]),
      );
      vi.advanceTimersByTime(4000);
      report(
        fixture,
        state([{ id: 't1', status: 'pending' }]),
        state([{ id: 't1', status: 'success', result: { images: [{}, {}] } }]),
      );
      // The first notice's remaining 2s must not take the SECOND one down.
      vi.advanceTimersByTime(2001);
      expect(notice(fixture)!.message).toBe('Successfully generated 2 images!');
      vi.advanceTimersByTime(4000);
      expect(notice(fixture)).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it('the turn-end boundary drops a STRANDED pending notice', async () => {
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    report(fixture, state([]), state([{ id: 't0', status: 'pending' }]));
    expect(notice(fixture)!.status).toBe('pending');
    (fixture.componentInstance as unknown as { clearPendingToolExecutionStatus(): void })[
      'clearPendingToolExecutionStatus'
    ]();
    expect(notice(fixture)).toBeNull();
  });

  it('a REAL send that completes clears a stranded pending notice (the wiring, not the method)', async () => {
    // The §3 unification catch: every other boundary spec invokes the private
    // clear method directly, so deleting the ONE production call in the send
    // flow's reconcile tail would red nothing — the exact bug-77 shape shipping
    // invisibly (the #58 PumpPause WIRING-test lesson). This spec drives
    // `send()` end to end instead: the notice is raised through the production
    // door, the turn completes, and the reconcile tail must be what clears it.
    const events$ = new Subject<ScopedEvent>();
    const fixture = await render(stubClient(chatDetail(), events$));
    const inst = fixture.componentInstance as unknown as {
      send(p: { content: string; fileIds: string[] }): void;
    };
    report(fixture, state([]), state([{ id: 't0', status: 'pending' }]));
    expect(notice(fixture)!.status).toBe('pending');
    inst.send({ content: 'paint the estate at dusk', fileIds: [] });
    for (let i = 0; i < 8; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(notice(fixture)).toBeNull();
  });

  it('the turn-end boundary does NOT cut a settled countdown short', async () => {
    // ⚠ The mutation proof of the stranded-vs-settled distinction: a
    // `clearPending` that cleared unconditionally (v4's own pre-fix shape, and
    // the obvious wrong spelling) would rob the user of the outcome the instant
    // the turn ended. Cutting the countdown short must red this.
    // ⚠ Render BEFORE installing fake timers: the harness settles the TanStack
    // queries by awaiting real `setTimeout(0)` ticks, which fake timers freeze —
    // the test then hangs to its 5 s timeout, its `finally` never runs, and the
    // leaked fake clock hangs every spec after it too.
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    vi.useFakeTimers();
    try {
      report(
        fixture,
        state([{ id: 't0', status: 'pending' }]),
        state([{ id: 't0', status: 'success', result: { images: [{}] } }]),
      );
      (fixture.componentInstance as unknown as { clearPendingToolExecutionStatus(): void })[
        'clearPendingToolExecutionStatus'
      ]();
      expect(notice(fixture)!.message).toBe('Successfully generated 1 image!');
      // ...and it still expires on its own schedule afterwards.
      vi.advanceTimersByTime(6000);
      expect(notice(fixture)).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it('a turn ending by the ERROR arm still clears a stranded pending notice', async () => {
    // The route v4's single `onDone` teardown missed entirely: an error arm left
    // the banner pinned above the composer for the rest of the session.
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    const pending = state([{ id: 't0', status: 'pending' }]);
    report(fixture, state([]), pending);
    report(fixture, pending, state([{ id: 't0', status: 'pending' }], { error: 'Stream failed' }));
    expect(notice(fixture)!.status).toBe('pending');
    (fixture.componentInstance as unknown as { clearPendingToolExecutionStatus(): void })[
      'clearPendingToolExecutionStatus'
    ]();
    expect(notice(fixture)).toBeNull();
  });

  it('stop() dismisses the notice at once, settled or not', async () => {
    // v4 `stopStreaming:1128` calls dismiss directly — aborting a turn clears it
    // immediately rather than leaving a settled one counting down.
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    report(
      fixture,
      state([{ id: 't0', status: 'pending' }]),
      state([{ id: 't0', status: 'success', result: { images: [{}] } }]),
    );
    (fixture.componentInstance as unknown as { stop(): void })['stop']();
    expect(notice(fixture)).toBeNull();
  });

  it('teardown cancels a live countdown, so nothing writes state after destroy', async () => {
    // ⚠ Render BEFORE installing fake timers: the harness settles the TanStack
    // queries by awaiting real `setTimeout(0)` ticks, which fake timers freeze —
    // the test then hangs to its 5 s timeout, its `finally` never runs, and the
    // leaked fake clock hangs every spec after it too.
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    vi.useFakeTimers();
    try {
      report(
        fixture,
        state([{ id: 't0', status: 'pending' }]),
        state([{ id: 't0', status: 'success', result: { images: [{}] } }]),
      );
      const settled = notice(fixture);
      fixture.destroy();
      // Had the countdown survived teardown it would blank the signal here; that
      // it still holds the settled notice is the cancellation, observed.
      vi.advanceTimersByTime(60_000);
      expect(notice(fixture)).toEqual(settled);
    } finally {
      vi.useRealTimers();
    }
  });

  it('draws the notice above the composer with v4’s a11y semantics and close control', async () => {
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    report(fixture, state([]), state([{ id: 't0', status: 'pending' }]));
    fixture.detectChanges();

    const alert = (fixture.nativeElement as HTMLElement).querySelector<HTMLElement>(
      '.qt-chat-composer-content [role="status"]',
    )!;
    expect(alert).not.toBeNull();
    expect(alert.getAttribute('aria-live')).toBe('polite');
    expect(alert.classList.contains('qt-alert-info')).toBe(true);
    expect(alert.textContent).toContain('Generating image...');

    const dismiss = alert.querySelector<HTMLButtonElement>('button[aria-label="Dismiss notice"]')!;
    expect(dismiss).not.toBeNull();
    expect(dismiss.getAttribute('title')).toBe('Dismiss');
    dismiss.click();
    fixture.detectChanges();
    expect(notice(fixture)).toBeNull();
    expect(
      (fixture.nativeElement as HTMLElement).querySelector(
        '.qt-chat-composer-content [role="status"]',
      ),
    ).toBeNull();
  });

  it('paints the settled notice by outcome, as v4’s class ladder does', async () => {
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    report(
      fixture,
      state([{ id: 't0', status: 'pending' }]),
      state([{ id: 't0', status: 'error', result: { error: 'quota exhausted' } }]),
    );
    fixture.detectChanges();
    const alert = (fixture.nativeElement as HTMLElement).querySelector<HTMLElement>(
      '.qt-chat-composer-content [role="status"]',
    )!;
    expect(alert.classList.contains('qt-alert-error')).toBe(true);
    expect(alert.textContent).toContain('quota exhausted');
  });
});

/**
 * Bug 94 — the attachment-failure ledger's reader.
 *
 * Provider plugins have always reported dropped attachments in
 * `attachmentResults`; the object rode the SSE done event to a client that
 * never read it, which is why bug 91 (images silently dropped for four
 * providers) lasted as long as it did. v4 `a14a1811` gives it a reader in
 * `useSSEStreaming.ts:601-616`: a WARNING toast on the done event, naming only
 * the FIRST error and counting the rest.
 *
 * v4 shipped no test for the hook, so the parity floor is v4's own
 * message-construction expression, transcribed and pinned arm by arm. Every
 * state here is built by the REAL reducer from a real done frame, so these
 * cases prove the carry and the door together.
 */
describe('SalonConversation attachment-failure warning (Bug 94)', () => {
  /** Fold a done frame carrying (or omitting) a ledger. */
  function done(attachmentResults?: ChatStreamFrame['attachmentResults']): ChatStreamState {
    return reduceChatFrame(initialChatStreamState(), {
      done: true,
      messageId: 'm1',
      ...(attachmentResults === undefined ? {} : { attachmentResults }),
    });
  }

  function report(
    fixture: ComponentFixture<SalonConversation>,
    before: ChatStreamState,
    after: ChatStreamState,
  ): void {
    (
      fixture.componentInstance as unknown as {
        reportStreamTransitions(a: ChatStreamState, b: ChatStreamState): void;
      }
    ).reportStreamTransitions(before, after);
  }

  it('warns with the plugin’s own sentence when ONE attachment failed', async () => {
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    report(
      fixture,
      initialChatStreamState(),
      done({ sent: [], failed: [{ id: 'f1', error: 'NanoGPT could not read image/heic' }] }),
    );
    // v4's singular arm, byte for byte.
    expect(toasts()).toEqual([
      {
        type: 'warning',
        message: 'An attachment was not sent to the model: NanoGPT could not read image/heic',
      },
    ]);
  });

  it('counts the rest and shows only the FIRST error when several failed', async () => {
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    report(
      fixture,
      initialChatStreamState(),
      done({
        failed: [
          { id: 'f1', error: 'first reason' },
          { id: 'f2', error: 'second reason' },
          { id: 'f3', error: 'third reason' },
        ],
      }),
    );
    // The `(and N more)` suffix sits BEFORE the colon, and N counts the ones
    // NOT shown — v4 `${failedAttachments.length - 1}`.
    expect(toasts()).toEqual([
      {
        type: 'warning',
        message: '3 attachments were not sent to the model (and 2 more): first reason',
      },
    ]);
  });

  it('falls back to `unknown reason` when the entry carries no error text', async () => {
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    // v4's `?? 'unknown reason'` — a plugin that reported a failure without
    // saying why still gets a sentence.
    report(
      fixture,
      initialChatStreamState(),
      done({ failed: [{ id: 'f1' } as unknown as { id: string; error: string }] }),
    );
    expect(toasts()).toEqual([
      { type: 'warning', message: 'An attachment was not sent to the model: unknown reason' },
    ]);
  });

  it('says nothing when the ledger is absent, null, or carries no failures', async () => {
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    report(fixture, initialChatStreamState(), done());
    report(fixture, initialChatStreamState(), done(null));
    report(fixture, initialChatStreamState(), done({ sent: ['f1', 'f2'], failed: [] }));
    report(fixture, initialChatStreamState(), done({ sent: ['f1'] }));
    expect(toasts()).toEqual([]);
  });

  it('warns ONCE per done, not once per transition carrying the same ledger', async () => {
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    const settled = done({ failed: [{ id: 'f1', error: 'dropped' }] });
    report(fixture, initialChatStreamState(), settled);
    // The Courier's parked-placeholder patch spreads the SAME ledger forward;
    // a later chainComplete leaves `finalDone` alone entirely. Neither is a new
    // done event, so neither may warn again.
    const parked = reduceChatFrame(settled, { pendingExternalTurn: true });
    report(fixture, settled, parked);
    report(fixture, parked, reduceChatFrame(parked, { chainComplete: true }));
    expect(toasts()).toHaveLength(1);
  });

  it('warns again for the NEXT done in a chain, as v4’s per-event read does', async () => {
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    const first = done({ failed: [{ id: 'f1', error: 'dropped once' }] });
    report(fixture, initialChatStreamState(), first);
    const second = reduceChatFrame(first, {
      done: true,
      messageId: 'm2',
      attachmentResults: { failed: [{ id: 'f2', error: 'dropped twice' }] },
    });
    report(fixture, first, second);
    expect(toasts().map((t) => t.message)).toEqual([
      'An attachment was not sent to the model: dropped once',
      'An attachment was not sent to the model: dropped twice',
    ]);
  });
});

/**
 * The story-background loops after `f3892158d` (P4.D125): both are the FALLBACK
 * now, gated on channel health, and the change callback belongs to the shared
 * transition effect — which sees the move whether it arrived by poll or by a
 * `chats:<id>` hint invalidating the background key.
 */
describe('SalonConversation — the story-background loops are the fallback now', () => {
  function backgroundClient(opts: { enabled: boolean; fileId: () => string | null }) {
    const stream = coreStreamStub();
    const chat = chatDetail();
    const calls: string[] = [];
    const dispatch = vi.fn(async (req: CoreRequest): Promise<CoreResponse> => {
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
        return { message: 'Story background regeneration queued', queued: true, jobId: 'job-1' };
      }
      return {
        backgroundUrl: null,
        fileId: opts.fileId(),
        filename: null,
        sha256: null,
        linkSummary: null,
      };
    });
    return {
      stream,
      calls,
      client: {
        ...stream,
        dispatch,
        dispatchData: dispatchData as unknown as CoreClient['dispatchData'],
        dispatchExpect: (async (req: CoreRequest, expected: string) => {
          const resp = await dispatch(req);
          if (resp.type !== expected) throw new Error(`unexpected ${resp.type}`);
          return resp;
        }) as CoreClient['dispatchExpect'],
      } as unknown as Partial<CoreClient>,
    };
  }

  async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
    for (let i = 0; i < 6; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
  }

  it('a `chats:<id>` hint re-reads the background without any timer', async () => {
    const s = backgroundClient({ enabled: true, fileId: () => null });
    const fixture = await render(s.client);
    s.stream.connection.set('open');
    await settle(fixture);
    const before = s.calls.filter((c) => c === 'chatGetBackground').length;
    s.stream.frames.next({
      v: 1,
      topic: 'chats',
      id: 'chat-1',
      at: 1,
    } as unknown as ScopedEvent);
    await settle(fixture);
    const after = s.calls.filter((c) => c === 'chatGetBackground').length;
    expect(after).toBeGreaterThan(before);
  });

  it('a hint for ANOTHER chat leaves this one alone (the row-scoped narrowing)', async () => {
    const s = backgroundClient({ enabled: true, fileId: () => null });
    const fixture = await render(s.client);
    s.stream.connection.set('open');
    await settle(fixture);
    const before = s.calls.filter((c) => c === 'chatGetBackground').length;
    s.stream.frames.next({
      v: 1,
      topic: 'chats',
      id: 'some-other-chat',
      at: 1,
    } as unknown as ScopedEvent);
    await settle(fixture);
    expect(s.calls.filter((c) => c === 'chatGetBackground').length).toBe(before);
  });

  it('the transition effect fires the change callback however the value arrived', async () => {
    // Seeded with no background; the next read finds one — the transition the
    // active watch used to be the only thing that could notice.
    //
    // The move is driven by invalidating ONLY the background key, never by a
    // `chats:<id>` hint: a hint invalidates `['chat', id]` itself through the
    // topic map, which would make this assertion vacuous (the hub, not the
    // effect, would be the one that produced the key).
    let fileId: string | null = null;
    const s = backgroundClient({ enabled: true, fileId: () => fileId });
    const fixture = await render(s.client);
    await settle(fixture);

    const queryClient = TestBed.inject(QueryClient);
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries');
    fileId = 'bg-9';
    await queryClient.invalidateQueries({ queryKey: ['chat', 'chat-1', 'background'] });
    await settle(fixture);

    // `onBackgroundChanged` invalidates the chat detail so the Lantern
    // announcement posted alongside the new backdrop lands.
    const keys = invalidate.mock.calls.map((c) =>
      JSON.stringify((c[0] as { queryKey: readonly unknown[] }).queryKey),
    );
    expect(keys).toContain(JSON.stringify(['chat', 'chat-1']));
    invalidate.mockRestore();
  });

  it('a transition RETIRES an armed active watch, without waiting out its next tick', async () => {
    vi.useFakeTimers();
    const proto = globalThis.HTMLElement.prototype as unknown as Record<string, unknown>;
    const added = (['scrollTo', 'scrollIntoView'] as const).filter((k) => !(k in proto));
    for (const k of added) proto[k] = () => {};
    try {
      let fileId: string | null = null;
      const s = backgroundClient({ enabled: true, fileId: () => fileId });
      localStorage.setItem('quilltap.chat-sidebar.collapsed', 'false');
      TestBed.configureTestingModule({
        imports: [SalonConversation],
        providers: [
          provideRouter([]),
          provideTanStackQuery(new QueryClient()),
          { provide: CoreClient, useValue: s.client },
          {
            provide: ActivatedRoute,
            useValue: { paramMap: of(convertToParamMap({ id: 'chat-1' })) },
          },
        ],
      });
      const fixture = TestBed.createComponent(SalonConversation);
      fixture.detectChanges();
      const flush = async () => {
        for (let i = 0; i < 8; i++) {
          await vi.advanceTimersByTimeAsync(0);
          fixture.detectChanges();
        }
      };
      await flush();
      // Channel up, so the 30 s passive sweep is off and the only remaining
      // repeat timer would be the active watch.
      s.stream.connection.set('open');
      fixture.detectChanges();
      await flush();

      // Arm the watch through the surface the user uses.
      const headers = Array.from(
        fixture.nativeElement.querySelectorAll('.qt-collapsible-card-header'),
      ) as HTMLButtonElement[];
      headers.find((h) => h.textContent?.trim().startsWith('Chat'))?.click();
      fixture.detectChanges();
      const button = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
        (b) => (b as HTMLButtonElement).textContent?.trim() === 'Regenerate Background',
      ) as HTMLButtonElement;
      expect(button).toBeTruthy();
      button.click();
      await flush();

      // The backdrop lands and a hint invalidates the key — the effect sees the
      // transition first. Without its `poller.stop()` the watch would still fire
      // its own 5 s tick before noticing.
      fileId = 'bg-9';
      s.stream.frames.next({
        v: 1,
        topic: 'chats',
        id: 'chat-1',
        at: 3,
      } as unknown as ScopedEvent);
      await flush();

      const reads = () => s.calls.filter((c) => c === 'chatGetBackground').length;
      const settled = reads();
      await vi.advanceTimersByTimeAsync(30_000);
      expect(reads()).toBe(settled);
    } finally {
      for (const k of added) delete proto[k];
      vi.useRealTimers();
    }
  });

  it('the 30 s passive sweep runs while the channel is down and NOT while it is up', async () => {
    vi.useFakeTimers();
    // Advancing the clock lets the auto-scroll's deferred passes actually run,
    // and JSDOM elements have neither `scrollTo` nor `scrollIntoView`.
    const proto = globalThis.HTMLElement.prototype as unknown as Record<string, unknown>;
    const added = (['scrollTo', 'scrollIntoView'] as const).filter((k) => !(k in proto));
    for (const k of added) proto[k] = () => {};
    try {
      const s = backgroundClient({ enabled: true, fileId: () => null });
      localStorage.setItem('quilltap.chat-sidebar.collapsed', 'false');
      TestBed.configureTestingModule({
        imports: [SalonConversation],
        providers: [
          provideRouter([]),
          provideTanStackQuery(new QueryClient()),
          { provide: CoreClient, useValue: s.client },
          {
            provide: ActivatedRoute,
            useValue: { paramMap: of(convertToParamMap({ id: 'chat-1' })) },
          },
        ],
      });
      const fixture = TestBed.createComponent(SalonConversation);
      fixture.detectChanges();
      const flush = async () => {
        for (let i = 0; i < 6; i++) {
          await vi.advanceTimersByTimeAsync(0);
          fixture.detectChanges();
        }
      };
      await flush();

      const reads = () => s.calls.filter((c) => c === 'chatGetBackground').length;
      const down = reads();
      await vi.advanceTimersByTimeAsync(30_100);
      expect(reads()).toBeGreaterThan(down);

      s.stream.connection.set('open');
      fixture.detectChanges();
      await flush();
      const up = reads();
      await vi.advanceTimersByTimeAsync(120_000);
      expect(reads()).toBe(up);
    } finally {
      for (const k of added) delete proto[k];
      vi.useRealTimers();
    }
  });
});

/**
 * The rescue-stage toasts (`retrying` / `failing-over`, v4
 * `useSSEStreaming.ts:508-519` at `65f5021c8`).
 *
 * v4 toasts on EVERY status event; v5 rides transitions with one narrowing,
 * settled at the round-1 unification (2026-09-01): a stage that stays put while
 * its MESSAGE changes still toasts — a chain walking two stand-ins names each,
 * and the second name is news — while a byte-identical repeat frame is
 * coalesced (the remaining recorded divergence).
 */
describe('SalonConversation rescue-stage toasts (65f5021c8)', () => {
  function withStatus(stage: string, message: string): ChatStreamState {
    return { ...initialChatStreamState(), status: { stage, message } };
  }

  function report(
    fixture: ComponentFixture<SalonConversation>,
    before: ChatStreamState,
    after: ChatStreamState,
  ): void {
    (
      fixture.componentInstance as unknown as {
        reportStreamTransitions(a: ChatStreamState, b: ChatStreamState): void;
      }
    ).reportStreamTransitions(before, after);
  }

  it('toasts the server sentence on entering failing-over', async () => {
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    const n = toasts().length;
    report(
      fixture,
      initialChatStreamState(),
      withStatus('failing-over', 'Understudy is standing in for Aria...'),
    );
    expect(toasts().slice(n)).toEqual([
      { type: 'warning', message: 'Understudy is standing in for Aria...' },
    ]);
  });

  it('toasts AGAIN when the stage stays failing-over but the message names a different stand-in', async () => {
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    const n = toasts().length;
    const first = withStatus('failing-over', 'Understudy is standing in for Aria...');
    report(fixture, initialChatStreamState(), first);
    report(fixture, first, withStatus('failing-over', 'Tier Spare is standing in for Aria...'));
    expect(toasts().slice(n)).toEqual([
      { type: 'warning', message: 'Understudy is standing in for Aria...' },
      { type: 'warning', message: 'Tier Spare is standing in for Aria...' },
    ]);
  });

  it('coalesces a byte-identical repeat frame (the recorded divergence from v4)', async () => {
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    const n = toasts().length;
    const frame = withStatus('retrying', 'Retrying with the same provider...');
    report(fixture, initialChatStreamState(), frame);
    report(fixture, frame, { ...initialChatStreamState(), status: { ...frame.status! } });
    expect(toasts().slice(n)).toEqual([
      { type: 'warning', message: 'Retrying with the same provider...' },
    ]);
  });

  it('never toasts a non-rescue stage', async () => {
    const fixture = await render(stubClient(chatDetail(), new Subject<ScopedEvent>()));
    const n = toasts().length;
    report(fixture, initialChatStreamState(), withStatus('thinking', 'Composing...'));
    expect(toasts().slice(n)).toEqual([]);
  });
});
