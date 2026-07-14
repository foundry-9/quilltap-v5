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
    expect(layout.style.getPropertyValue('--story-background-url')).toBe("url('/api/v1/files/bg-7')");
    expect(layout.getAttribute('style') ?? '').toContain('--story-background-url');
  });
});
