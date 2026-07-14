import { ComponentFixture, TestBed } from '@angular/core/testing';
import { beforeAll, describe, expect, it, vi } from 'vitest';

/**
 * MessageList is virtualized; the virtualizer forces an empty window when the
 * scroll container reports offsetHeight 0 (which is what JSDOM reports). Stub a
 * non-zero size so the canonical rows fall inside the window and render (the
 * salon-conversation.spec precedent). The stream-accumulated bubbles are NOT
 * virtualized, so they render regardless — but the canonical flow needs a
 * viewport to assert the dedup handoff.
 */
beforeAll(() => {
  const proto = globalThis.HTMLElement?.prototype;
  if (proto && !('__qtSizeStubbed' in proto)) {
    Object.defineProperty(proto, '__qtSizeStubbed', { value: true });
    Object.defineProperty(proto, 'offsetHeight', { configurable: true, get: () => 800 });
    Object.defineProperty(proto, 'offsetWidth', { configurable: true, get: () => 800 });
  }
});

import { CoreClient } from '../core/core-client';
import type { ChatDetail, MessageDto, ParticipantDetail } from '../core/core-contract';
import {
  FIXTURE_CHAT_ID,
  multiTurnChainTrace,
  skipDoneTrace,
} from '../core/__fixtures__/frame-trace';
import {
  foldChatFrames,
  initialChatStreamState,
  reduceChatFrame,
  type ChatStreamState,
} from '../core/chat-stream.reducer';
import { MessageList, buildStreamRenderItems } from './message-list';

/** Fold a scoped trace filtered to the subject chat — the consumer's own step. */
function foldChain(count = Infinity): ChatStreamState {
  const frames = multiTurnChainTrace
    .filter((e) => e.chatId === FIXTURE_CHAT_ID)
    .map(({ chatId: _c, roomId: _r, progressId: _p, ...frame }) => frame)
    .slice(0, count === Infinity ? undefined : count);
  return foldChatFrames(frames);
}

function foldSkip(): ChatStreamState {
  const frames = skipDoneTrace
    .filter((e) => e.chatId === FIXTURE_CHAT_ID)
    .map(({ chatId: _c, roomId: _r, progressId: _p, ...frame }) => frame);
  return foldChatFrames(frames);
}

function participant(over: Partial<ParticipantDetail>): ParticipantDetail {
  return {
    id: 'p1',
    type: 'CHARACTER',
    displayOrder: 0,
    isActive: true,
    controlledBy: 'llm',
    status: 'active',
    character: { id: 'c1', name: 'Ada', title: null, avatarUrl: null, defaultImageId: null, defaultImage: null },
    connectionProfile: null,
    imageProfile: null,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    ...over,
  };
}

function message(over: Partial<MessageDto>): MessageDto {
  return {
    id: 'm',
    role: 'ASSISTANT',
    content: 'hello',
    tokenCount: null,
    promptTokens: null,
    completionTokens: null,
    createdAt: '2026-01-01T00:00:01.000Z',
    swipeGroupId: null,
    swipeIndex: null,
    participantId: 'p1',
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
    title: 'Chain',
    contextSummary: null,
    roleplayTemplateId: null,
    chatType: 'salon',
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    isPaused: false,
    isManuallyRenamed: false,
    participants: [
      participant({ id: 'p1', character: { id: 'c1', name: 'Ada', title: null, avatarUrl: null, defaultImageId: null, defaultImage: null } }),
      participant({ id: 'p2', character: { id: 'c2', name: 'Bob', title: null, avatarUrl: null, defaultImageId: null, defaultImage: null } }),
    ],
    user: { id: 'u', name: 'Bertie', image: null },
    messages: [],
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

describe('buildStreamRenderItems (dogfood finding #7)', () => {
  it('returns nothing when the stream is null or has no messages', () => {
    expect(buildStreamRenderItems(null, [])).toEqual([]);
    expect(buildStreamRenderItems(initialChatStreamState(), [])).toEqual([]);
  });

  it('renders one row per finished chained turn, in order', () => {
    const items = buildStreamRenderItems(foldChain(), []);
    expect(items.map((i) => (i.type === 'message' ? i.message.id : 'grp'))).toEqual(['m10', 'm11']);
    expect(items[0].type === 'message' && items[0].message.content).toBe('Hi, I am Ada.');
    expect(items[0].type === 'message' && items[0].message.participantId).toBe('p1');
  });

  it('surfaces each turn AS it completes — only the first bubble after the first done', () => {
    // Frames 0..2 = turnStart, content, done(m10) — the first turn is finished
    // but the chain has not moved on yet.
    const items = buildStreamRenderItems(foldChain(3), []);
    expect(items.map((i) => (i.type === 'message' ? i.message.id : 'grp'))).toEqual(['m10']);
  });

  it('dedupes by id against the canonical flow (post-reconcile rows never double)', () => {
    // The canonical refetch already holds m10; its streamed twin must drop out.
    const canonical = [message({ id: 'm10', content: 'Canonical Ada line' })];
    const items = buildStreamRenderItems(foldChain(), canonical);
    expect(items.map((i) => (i.type === 'message' ? i.message.id : 'grp'))).toEqual(['m11']);
  });

  it('a skipped turn renders nothing but its Host note DOES render as a chip', () => {
    const items = buildStreamRenderItems(foldSkip(), []);
    // No assistant row for the skipped participant.
    expect(items.filter((i) => i.type === 'message')).toHaveLength(0);
    // The Host announcement collapses to one chip group.
    expect(items).toHaveLength(1);
    expect(items[0].type === 'announcement-group' && items[0].chips[0].id).toBe('host-1');
  });

  it('renders a mid-turn carina answer as its own row (never a chip)', () => {
    const state = reduceChatFrame(initialChatStreamState(), {
      carinaAnswer: { id: 'carina-1', role: 'ASSISTANT', content: 'Reference answer.', systemSender: 'carina', participantId: 'p1' },
    });
    const items = buildStreamRenderItems(state, []);
    expect(items).toHaveLength(1);
    expect(items[0].type).toBe('message');
    expect(items[0].type === 'message' && items[0].message.id).toBe('carina-1');
  });
});

describe('MessageList — chained stream render in the DOM', () => {
  function render(messages: MessageDto[], stream: ChatStreamState | null): ComponentFixture<MessageList> {
    TestBed.configureTestingModule({
      imports: [MessageList],
      providers: [{ provide: CoreClient, useValue: { dispatch: vi.fn(), events$: { subscribe: () => ({ unsubscribe() {} }) } } }],
    });
    const fixture = TestBed.createComponent(MessageList);
    fixture.componentRef.setInput('messages', messages);
    fixture.componentRef.setInput('chat', chatDetail());
    fixture.componentRef.setInput('stream', stream);
    fixture.detectChanges();
    return fixture;
  }

  it('shows both chained replies as they finish, above the live bubble', () => {
    const fixture = render([message({ id: 'u1', role: 'USER', content: 'Hello both.' })], foldChain());
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Hi, I am Ada.');
    expect(text).toContain('And I am Bob.');
  });

  it('hands off without duplication — a canonical id renders once, its stream twin drops', () => {
    const canonical = [
      message({ id: 'u1', role: 'USER', content: 'Hello both.' }),
      message({ id: 'm10', participantId: 'p1', content: 'Canonical Ada line' }),
    ];
    const fixture = render(canonical, foldChain());
    const text = fixture.nativeElement.textContent as string;
    // The canonical copy of m10 shows; the streamed twin's content does not.
    expect(text).toContain('Canonical Ada line');
    expect(text).not.toContain('Hi, I am Ada.');
    // m11 has no canonical copy yet, so it still shows from the stream.
    expect(text).toContain('And I am Bob.');
  });
});
