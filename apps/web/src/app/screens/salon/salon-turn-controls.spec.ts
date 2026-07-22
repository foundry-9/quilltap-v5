import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ActivatedRoute, convertToParamMap, provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { Subject, of } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

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

/**
 * Deliverable 9: the tier-2 Salon controls (Skip banner, Speaking-As,
 * pause/resume, nudge) driven through the component over a mocked `CoreClient`
 * — the `salon-conversation.spec.ts` pattern.
 */

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
      title: null,
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

/** A 2-LLM + 1-user group chat (qualifies for turn-skipping). */
function groupChat(over: Partial<ChatDetail> = {}): ChatDetail {
  return {
    id: 'chat-1',
    title: 'Group Expedition',
    contextSummary: null,
    roleplayTemplateId: null,
    chatType: 'salon',
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    isPaused: false,
    isManuallyRenamed: false,
    participants: [
      participant({ id: 'pA', character: charOf('cA', 'Aaron') }),
      participant({ id: 'pB', character: charOf('cB', 'Beatrice') }),
      participant({
        id: 'pU',
        controlledBy: 'user',
        character: charOf('cU', 'Bertie'),
      }),
    ],
    user: { id: 'user1', name: 'Bertie', image: null },
    messages: [
      message({
        id: 'a1',
        role: 'ASSISTANT',
        participantId: 'pA',
        content: 'I stride toward the ridge.',
        createdAt: '2024-01-01T00:00:01.000Z',
      }),
      message({
        id: 'b1',
        role: 'ASSISTANT',
        participantId: 'pB',
        content: 'I follow.',
        createdAt: '2024-01-01T00:00:02.000Z',
      }),
    ],
    projectId: null,
    projectName: null,
    turnSkippingEnabled: true,
    agentModeEnabled: false,
    resolvedAgentModeEnabled: false,
    agentModeSource: 'global',
    isDangerousChat: false,
    dangerCategories: [],
    conciergeOverride: null,
    offSceneCharacters: [],
    lastTurnParticipantId: null,
    ...over,
  };
}

function charOf(id: string, name: string): ParticipantDetail['character'] {
  return { id, name, title: null, avatarUrl: null, defaultImageId: null, defaultImage: null };
}

interface StubOptions {
  /** The `turn` payload the `query` action returns. */
  query?: { nextSpeakerId: string | null; nextSpeakerControlledBy: string | null };
  /** The `skipUserTurn` response — a `turnAction` turn, or an error message. */
  skip?:
    | { turn: { nextSpeakerId: string | null; nextSpeakerControlledBy: string | null } }
    | { error: string };
}

function stubClient(
  chat: ChatDetail,
  opts: StubOptions = {},
): { client: Partial<CoreClient>; dispatch: ReturnType<typeof vi.fn> } {
  const dispatch = vi.fn(async (req: CoreRequest): Promise<CoreResponse> => {
    switch (req.type) {
      case 'chatGet':
        return { type: 'chat', data: { chat } };
      case 'chatSettings':
        return {
          type: 'chatSettings',
          data: { avatarDisplayMode: 'ALWAYS', avatarDisplayStyle: 'CIRCULAR' },
        };
      case 'chatTurnAction':
        if (req.action === 'query') {
          return {
            type: 'turnAction',
            data: { turn: opts.query ?? { nextSpeakerId: null, nextSpeakerControlledBy: null } },
          };
        }
        if (req.action === 'skipUserTurn') {
          if (opts.skip && 'error' in opts.skip) {
            return { type: 'error', data: { kind: 'bad-request', message: opts.skip.error } };
          }
          return {
            type: 'turnAction',
            data: {
              turn: opts.skip?.turn ?? { nextSpeakerId: null, nextSpeakerControlledBy: null },
            },
          };
        }
        return { type: 'turnAction', data: {} };
      case 'chatSend':
        return {
          type: 'chatSend',
          data: { messageId: 'x', hasContent: true, isMultiCharacter: true, isPaused: false },
        };
      default:
        return { type: 'ack', data: {} };
    }
  });
  return {
    dispatch,
    client: {
      events$: new Subject<ScopedEvent>().asObservable(),
      dispatch,
      dispatchExpect: (async (req: CoreRequest, expect: string) => {
        const resp = await dispatch(req);
        if (resp.type !== expect) throw new Error(`unexpected ${resp.type}`);
        return resp;
      }) as CoreClient['dispatchExpect'],
    },
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<SalonConversation>> {
  // The sidebar defaults to its mini strip; these cases drive its drawers.
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
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

function calls(dispatch: ReturnType<typeof vi.fn>, type: string): CoreRequest[] {
  return dispatch.mock.calls.map((c) => c[0] as CoreRequest).filter((r) => r.type === type);
}

describe('Salon turn controls', () => {
  it('shows the user-turn banner + Skip when it is a user-controlled turn', async () => {
    const { client, dispatch } = stubClient(groupChat(), {
      query: { nextSpeakerId: 'pU', nextSpeakerControlledBy: 'user' },
      skip: { turn: { nextSpeakerId: 'pA', nextSpeakerControlledBy: 'llm' } },
    });
    const fixture = await render(client);
    const text = fixture.nativeElement.textContent as string;

    expect(text).toContain("Bertie's turn — type as them, or skip to let someone else respond.");
    const skipBtn = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b) => (b as HTMLButtonElement).textContent?.trim() === 'Skip',
    ) as HTMLButtonElement;
    expect(skipBtn).toBeTruthy();

    skipBtn.click();
    await new Promise((r) => setTimeout(r, 0));
    const skips = calls(dispatch, 'chatTurnAction').filter(
      (r) => (r as { action?: string }).action === 'skipUserTurn',
    );
    expect(skips).toHaveLength(1);
    expect((skips[0] as { participantId?: string }).participantId).toBe('pU');
  });

  it('shows the must-speak copy and NO Skip when everyone else has passed', async () => {
    // pB substantive, then pA passed — so for pU's turn, pA & pB are... build a
    // history where the only other active characters (pA, pB) have both passed
    // since the last substantive message so pU must speak.
    const chat = groupChat({
      messages: [
        message({
          id: 's0',
          role: 'USER',
          participantId: 'pU',
          content: 'Off we go.',
          createdAt: '2024-01-01T00:00:00.500Z',
        }),
        message({
          id: 'a1',
          role: 'ASSISTANT',
          participantId: 'pA',
          content: 'I lead.',
          createdAt: '2024-01-01T00:00:01.000Z',
        }),
        // pA and pB then both pass:
        message({
          id: 'p-a',
          role: 'ASSISTANT',
          participantId: null,
          systemSender: 'host',
          systemKind: 'turn-pass',
          hostEvent: { participantId: 'pA' },
          content: 'Aaron has nothing to add.',
          createdAt: '2024-01-01T00:00:02.000Z',
        }),
        message({
          id: 'p-b',
          role: 'ASSISTANT',
          participantId: null,
          systemSender: 'host',
          systemKind: 'turn-pass',
          hostEvent: { participantId: 'pB' },
          content: 'Beatrice has nothing to add.',
          createdAt: '2024-01-01T00:00:03.000Z',
        }),
      ],
    });
    const { client } = stubClient(chat, {
      query: { nextSpeakerId: 'pU', nextSpeakerControlledBy: 'user' },
    });
    const fixture = await render(client);
    const text = fixture.nativeElement.textContent as string;

    expect(text).toContain('Everyone else has passed — it falls to Bertie to say something.');
    // The BANNER drops its Skip. (The sidebar card's Skip is a different control:
    // v4 renders it always for the user seat and merely DISABLES it when it is
    // not the user's turn — `ParticipantCard.tsx:541-548`.)
    const skipBtn = [
      ...fixture.nativeElement.querySelectorAll('qt-turn-controls button'),
    ].find((b) => (b as HTMLButtonElement).textContent?.trim() === 'Skip');
    expect(skipBtn).toBeFalsy();
    const cardSkip = [...fixture.nativeElement.querySelectorAll('qt-chat-sidebar button')].find(
      (b) => (b as HTMLButtonElement).textContent?.trim() === 'Skip',
    ) as HTMLButtonElement;
    expect(cardSkip.disabled).toBe(true);
  });

  it('surfaces the server refusal copy when a skip is rejected', async () => {
    const { client } = stubClient(groupChat(), {
      query: { nextSpeakerId: 'pU', nextSpeakerControlledBy: 'user' },
      skip: { error: 'Everyone else has passed — it falls to Bertie to say something.' },
    });
    const fixture = await render(client);
    const skipBtn = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b) => (b as HTMLButtonElement).textContent?.trim() === 'Skip',
    ) as HTMLButtonElement;
    skipBtn.click();
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Everyone else has passed — it falls to Bertie to say something.');
  });

  it('renders the Speaking-As selector with two user-controlled characters', async () => {
    const chat = groupChat({
      participants: [
        participant({ id: 'pA', character: charOf('cA', 'Aaron') }),
        participant({ id: 'pU1', controlledBy: 'user', character: charOf('cU1', 'Bertie') }),
        participant({ id: 'pU2', controlledBy: 'user', character: charOf('cU2', 'Jeeves') }),
      ],
    });
    const { client, dispatch } = stubClient(chat, {
      query: { nextSpeakerId: 'pA', nextSpeakerControlledBy: 'llm' },
    });
    const fixture = await render(client);

    // The selector shows the active/first speaker and opens on click.
    const selectorBtn = fixture.nativeElement.querySelector(
      'qt-speaker-selector button',
    ) as HTMLButtonElement;
    expect(selectorBtn).toBeTruthy();
    selectorBtn.click();
    fixture.detectChanges();

    const option = [...fixture.nativeElement.querySelectorAll('[role="option"]')].find((o) =>
      (o as HTMLElement).textContent?.includes('Jeeves'),
    ) as HTMLButtonElement;
    expect(option).toBeTruthy();
    option.click();
    await new Promise((r) => setTimeout(r, 0));

    const setSpeaker = calls(dispatch, 'chatSetActiveSpeaker');
    expect(setSpeaker).toHaveLength(1);
    expect((setSpeaker[0] as { participantId?: string }).participantId).toBe('pU2');
  });

  it('toggles pause via chatUpdate — from the sidebar, v4\u2019s home for the button', async () => {
    const { client, dispatch } = stubClient(groupChat(), {
      query: { nextSpeakerId: 'pA', nextSpeakerControlledBy: 'llm' },
    });
    const fixture = await render(client);

    // P4.9H1 moved the button out of the turn-controls bar into the sidebar.
    expect(
      fixture.nativeElement.querySelector('qt-turn-controls .qt-chat-pause-button'),
    ).toBeNull();

    const pauseBtn = fixture.nativeElement.querySelector(
      'qt-chat-sidebar .qt-chat-pause-button',
    ) as HTMLButtonElement;
    expect(pauseBtn.textContent).toContain('Pause');
    pauseBtn.click();
    for (let i = 0; i < 5; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    const updates = calls(dispatch, 'chatUpdate');
    expect((updates[0] as { chat?: { isPaused?: boolean } }).chat?.isPaused).toBe(true);
  });

  it('shows the paused notice when the chat is paused', async () => {
    const { client } = stubClient(groupChat({ isPaused: true }), {
      query: { nextSpeakerId: 'pA', nextSpeakerControlledBy: 'llm' },
    });
    const fixture = await render(client);
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Auto-responses are paused');
    const resumeBtn = fixture.nativeElement.querySelector(
      'qt-chat-sidebar .qt-chat-pause-button',
    ) as HTMLButtonElement;
    expect(resumeBtn.textContent).toContain('Resume');
  });

  it('nudges the next LLM speaker with nudge: true', async () => {
    const { client, dispatch } = stubClient(groupChat(), {
      query: { nextSpeakerId: 'pA', nextSpeakerControlledBy: 'llm' },
    });
    const fixture = await render(client);
    const nudgeBtn = [...fixture.nativeElement.querySelectorAll('button')].find((b) =>
      (b as HTMLButtonElement).textContent?.includes('Nudge Aaron'),
    ) as HTMLButtonElement;
    expect(nudgeBtn).toBeTruthy();
    nudgeBtn.click();
    await new Promise((r) => setTimeout(r, 0));

    const sends = calls(dispatch, 'chatSend');
    expect(sends).toHaveLength(1);
    expect((sends[0] as { nudge?: boolean }).nudge).toBe(true);
    expect((sends[0] as { respondingParticipantId?: string }).respondingParticipantId).toBe('pA');
    expect((sends[0] as { continueMode?: boolean }).continueMode).toBe(true);
  });
});
