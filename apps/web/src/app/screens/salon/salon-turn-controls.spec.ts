import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ActivatedRoute, convertToParamMap, provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { Subject, of } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { coreStreamStub } from '../../core/core-client.testing';
import type {
  ChatDetail,
  CoreRequest,
  CoreResponse,
  ChatStreamFrame,
  MessageDto,
  ParticipantDetail,
  ScopedEvent,
} from '../../core/core-contract';
import { SalonConversation } from './salon-conversation';
import { ToastService } from '../../ui/toast.service';

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

/**
 * An all-LLM room whose persisted speaking-as points at an LLM seat nobody is
 * impersonating. `findActiveUserParticipant` rejects the selection and finds no
 * user seat to fall back to, so the resolved seat is `pA` — present, but NOT
 * user-driven. The one shape that reaches v4's Skip guard.
 */
function llmSeatChat(): ChatDetail {
  return groupChat({
    participants: [
      participant({ id: 'pA', character: charOf('cA', 'Aaron') }),
      participant({ id: 'pB', character: charOf('cB', 'Beatrice') }),
    ],
    activeTypingParticipantId: 'pA',
    impersonatingParticipantIds: ['pB'],
  });
}

/** A history where both LLM characters have passed since the last substantive
 *  message, so the floor falls to the user seat. */
function allOthersPassedChat(): ChatDetail {
  return groupChat({
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
  /**
   * Stream frames a `chatSend` emits before it resolves — the wire seam for the
   * bug-123 chain-pause announcement. `runTurn` subscribes to `events$` before
   * it dispatches, so a synchronous emit from inside the mock is folded exactly
   * as a real frame would be.
   */
  chainFrames?: ChatStreamFrame[];
  /**
   * The chat a `chatGet` returns once a `chatSend` has run — the real
   * chain-pause shape, where the server paused the chat during the chain and
   * the post-turn reconcile brings that back.
   */
  chatAfterSend?: ChatDetail;
  /** A `chatSend` that never resolves — holds the component in `busy()`. */
  hangSend?: boolean;
}

function stubClient(
  chat: ChatDetail,
  opts: StubOptions = {},
): {
  client: Partial<CoreClient>;
  dispatch: ReturnType<typeof vi.fn>;
  dispatchData: ReturnType<typeof vi.fn>;
  stream: ReturnType<typeof coreStreamStub>;
} {
  const stream = coreStreamStub();
  let sent = false;
  // onSelectSpeaker reads the reply through dispatchData (v4 handleSetActiveSpeaker
  // applies it — the chat GET projects no activeTypingParticipantId).
  const dispatchData = vi.fn(async (req: CoreRequest) =>
    req.type === 'chatSetActiveSpeaker'
      ? {
          impersonatingParticipantIds: [req.participantId as string],
          activeTypingParticipantId: req.participantId as string,
        }
      : {},
  );
  const dispatch = vi.fn(async (req: CoreRequest): Promise<CoreResponse> => {
    switch (req.type) {
      case 'chatGet':
        return { type: 'chat', data: { chat: sent && opts.chatAfterSend ? opts.chatAfterSend : chat } };
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
        sent = true;
        if (opts.hangSend) return new Promise<CoreResponse>(() => {});
        for (const frame of opts.chainFrames ?? []) {
          stream.frames.next({ chatId: 'chat-1', ...frame } as ScopedEvent);
        }
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
    dispatchData,
    stream,
    client: {
      ...stream,
      dispatch,
      dispatchData: dispatchData as unknown as CoreClient['dispatchData'],
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

/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
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

  // v4's `callTurnAction` swallows the server's sentence into a console.error
  // and toasts a fixed line (`useTurnManagement.ts:212-215`); v5's inline skip
  // banner, which showed the server message, was an invention of the no-toast
  // era and is retired with it.
  it('reports v4’s fixed refusal line when a skip is rejected', async () => {
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
    expect(toasts()).toEqual([
      { type: 'error', message: 'Failed to skip turn. Please try again.' },
    ]);
  });

  // -------------------------------------------------------------------------
  // The Skip banner keyed on the speaking seat (v4 bug 123, `SalonView.tsx:1457-1515`)
  // -------------------------------------------------------------------------

  /** The banner's rendered sentence, or null when the banner is hidden. */
  function banner(fixture: ComponentFixture<SalonConversation>): string | null {
    const el = fixture.nativeElement.querySelector('.qt-chat-user-turn-banner span');
    return el ? (el.textContent as string).trim() : null;
  }

  function skipButton(fixture: ComponentFixture<SalonConversation>): HTMLButtonElement | undefined {
    return [...fixture.nativeElement.querySelectorAll('qt-turn-controls button')].find(
      (b) => (b as HTMLButtonElement).textContent?.trim() === 'Skip',
    ) as HTMLButtonElement | undefined;
  }

  // The heart of bug 123: the rotation has landed on an LLM, but the composer
  // will still take words as Bertie — so Skip is offered, worded for a pass
  // rather than for a turn.
  it('offers the banner off-turn, worded "Speaking as", when the seat is not next', async () => {
    const { client } = stubClient(groupChat(), {
      query: { nextSpeakerId: 'pA', nextSpeakerControlledBy: 'llm' },
    });
    const fixture = await render(client);
    expect(banner(fixture)).toBe(
      'Speaking as Bertie — type, or skip to let someone else take the floor.',
    );
    expect(skipButton(fixture)).toBeTruthy();
  });

  it('words it as a turn when the rotation HAS landed on the seat', async () => {
    const { client } = stubClient(groupChat(), {
      query: { nextSpeakerId: 'pU', nextSpeakerControlledBy: 'user' },
    });
    const fixture = await render(client);
    expect(banner(fixture)).toBe(
      "Bertie's turn — type as them, or skip to let someone else respond.",
    );
  });

  // Gate 1 — the composer must be writable. v4 added `sending` to the
  // streaming/waiting pair it already tested; v5's `busy()` is all three.
  it('hides the banner while a send is in flight', async () => {
    const { client } = stubClient(groupChat(), {
      query: { nextSpeakerId: 'pU', nextSpeakerControlledBy: 'user' },
      hangSend: true,
    });
    const fixture = await render(client);
    expect(banner(fixture)).not.toBeNull();
    await sendAndSettle(fixture);
    expect(banner(fixture)).toBeNull();
  });

  // Gate 2 — v4's `useParticipants.hasActiveCharacters`: ANY active CHARACTER,
  // whoever drives it. The seat itself is still resolvable here, so this case
  // sees gate 2 and nothing else.
  it('hides the banner when the room has no active character', async () => {
    const chat = groupChat({
      participants: [
        participant({ id: 'pA', isActive: false, character: charOf('cA', 'Aaron') }),
        participant({
          id: 'pU',
          isActive: false,
          controlledBy: 'user',
          character: charOf('cU', 'Bertie'),
        }),
      ],
    });
    const { client } = stubClient(chat, {
      query: { nextSpeakerId: 'pU', nextSpeakerControlledBy: 'user' },
    });
    const fixture = await render(client);
    expect(banner(fixture)).toBeNull();
  });

  // The gate-2 predicate is v4's WIDER `useParticipants` twin. Here the only
  // active character is the user's own: v4 shows the banner (an active
  // CHARACTER exists), while this component's same-named `hasActiveCharacters`
  // — v4's `useTurnManagement` twin, `controlledBy !== 'user'` — reads false.
  // Reading the narrow one here would hide the banner from a solo user seat.
  it('shows the banner when the only active character is the user’s own', async () => {
    const chat = groupChat({
      participants: [
        participant({ id: 'pA', isActive: false, character: charOf('cA', 'Aaron') }),
        participant({ id: 'pU', controlledBy: 'user', character: charOf('cU', 'Bertie') }),
      ],
    });
    const { client } = stubClient(chat, {
      query: { nextSpeakerId: 'pU', nextSpeakerControlledBy: 'user' },
    });
    const fixture = await render(client);
    expect(banner(fixture)).toBe(
      "Bertie's turn — type as them, or skip to let someone else respond.",
    );
  });

  // Gate 3 — the seat resolves, but the human does not speak as it (Bug 44: an
  // LLM seat nobody is impersonating).
  it('hides the banner for a seat that is not user-driven', async () => {
    const { client } = stubClient(llmSeatChat());
    const fixture = await render(client);
    expect(banner(fixture)).toBeNull();
  });

  // The must-speak guard moved onto the SEAT with bug 123. Keyed on the
  // rotation's next speaker (an LLM here) it would read false and offer Skip.
  it('computes must-speak over the SEAT, not the rotation’s next speaker', async () => {
    const { client } = stubClient(allOthersPassedChat(), {
      query: { nextSpeakerId: 'pA', nextSpeakerControlledBy: 'llm' },
    });
    const fixture = await render(client);
    expect(banner(fixture)).toBe(
      'Everyone else has passed — it falls to Bertie to say something.',
    );
    expect(skipButton(fixture)).toBeFalsy();
  });

  // -------------------------------------------------------------------------
  // Skip honours the overlay, lifts a pause, and withholds the auto-continue
  // (v4 bug 123, `useTurnManagement.handleSkipUserTurn`)
  // -------------------------------------------------------------------------

  /** v4's jest suite calls the hook handler directly; so does this. */
  function skipDirectly(fixture: ComponentFixture<SalonConversation>): Promise<void> {
    return (
      fixture.componentInstance as unknown as { onSkipUserTurn(): Promise<void> }
    ).onSkipUserTurn();
  }

  it('refuses a seat the human is not speaking as, with v4’s new sentence', async () => {
    const { client, dispatch } = stubClient(llmSeatChat());
    const fixture = await render(client);
    await skipDirectly(fixture);
    expect(toasts()).toContainEqual({
      type: 'error',
      message: 'Only a character you are speaking as can be skipped.',
    });
    expect(
      calls(dispatch, 'chatTurnAction').filter(
        (r) => (r as { action?: string }).action === 'skipUserTurn',
      ),
    ).toHaveLength(0);
  });

  it('skips an impersonated seat whose durable controlledBy is still llm', async () => {
    const chat = groupChat({
      participants: [
        participant({ id: 'pA', character: charOf('cA', 'Aaron') }),
        participant({ id: 'pB', character: charOf('cB', 'Beatrice') }),
      ],
      activeTypingParticipantId: 'pA',
      impersonatingParticipantIds: ['pA'],
    });
    const { client, dispatch } = stubClient(chat, {
      skip: { turn: { nextSpeakerId: 'pB', nextSpeakerControlledBy: 'llm' } },
    });
    const fixture = await render(client);
    await skipDirectly(fixture);
    const skips = calls(dispatch, 'chatTurnAction').filter(
      (r) => (r as { action?: string }).action === 'skipUserTurn',
    );
    expect(skips).toHaveLength(1);
    expect((skips[0] as { participantId?: string }).participantId).toBe('pA');
    expect(toasts()).toEqual([]);
  });

  // v4 `:216-221`: "like a nudge, it lifts a pause first, or the next speaker it
  // hands the floor to would be refused by the pause guard". Through
  // `unpauseChat` — SILENT, because v4's toast lives in `togglePause`.
  it('lifts a pause BEFORE skipping, and says nothing about it', async () => {
    const { client, dispatch } = stubClient(groupChat({ isPaused: true }), {
      query: { nextSpeakerId: 'pU', nextSpeakerControlledBy: 'user' },
      skip: { turn: { nextSpeakerId: 'pA', nextSpeakerControlledBy: 'llm' } },
    });
    const fixture = await render(client);
    await skipDirectly(fixture);

    const order = dispatch.mock.calls
      .map((c) => c[0] as CoreRequest)
      .filter(
        (r) =>
          r.type === 'chatUpdate' ||
          (r.type === 'chatTurnAction' && (r as { action?: string }).action === 'skipUserTurn'),
      )
      .map((r) => r.type);
    expect(order).toEqual(['chatUpdate', 'chatTurnAction']);
    const update = dispatch.mock.calls
      .map((c) => c[0] as CoreRequest)
      .find((r) => r.type === 'chatUpdate') as { chat?: { isPaused?: boolean } };
    expect(update.chat?.isPaused).toBe(false);
    expect(toasts().map((t) => t.message)).not.toContain('Auto-responses resumed');
  });

  it('does not lift a pause that is not there', async () => {
    const { client, dispatch } = stubClient(groupChat({ isPaused: false }), {
      query: { nextSpeakerId: 'pU', nextSpeakerControlledBy: 'user' },
      skip: { turn: { nextSpeakerId: 'pA', nextSpeakerControlledBy: 'llm' } },
    });
    const fixture = await render(client);
    await skipDirectly(fixture);
    expect(calls(dispatch, 'chatUpdate')).toHaveLength(0);
  });

  it('hands the floor on to an LLM next speaker', async () => {
    const { client, dispatch } = stubClient(groupChat(), {
      query: { nextSpeakerId: 'pU', nextSpeakerControlledBy: 'user' },
      skip: { turn: { nextSpeakerId: 'pA', nextSpeakerControlledBy: 'llm' } },
    });
    const fixture = await render(client);
    await skipDirectly(fixture);
    expect(calls(dispatch, 'chatSend')).toHaveLength(1);
  });

  // Bug 123's overlay half of the auto-continue test: an impersonated seat keeps
  // `controlledBy: 'llm'`, so without the overlay check the client would
  // generate a reply for a seat the human means to type as.
  it('withholds the auto-continue when the next speaker is an impersonated seat', async () => {
    const chat = groupChat({
      activeTypingParticipantId: 'pU',
      impersonatingParticipantIds: ['pA'],
    });
    const { client, dispatch } = stubClient(chat, {
      query: { nextSpeakerId: 'pU', nextSpeakerControlledBy: 'user' },
      skip: { turn: { nextSpeakerId: 'pA', nextSpeakerControlledBy: 'llm' } },
    });
    const fixture = await render(client);
    await skipDirectly(fixture);
    expect(calls(dispatch, 'chatSend')).toHaveLength(0);
  });

  // -------------------------------------------------------------------------
  // The chain-pause announcement (v4 bug 123, `useSSEStreaming.announceChainPause`)
  // -------------------------------------------------------------------------

  /** Send a message and drain the turn. */
  async function sendAndSettle(fixture: ComponentFixture<SalonConversation>): Promise<void> {
    (
      fixture.componentInstance as unknown as {
        send(p: { content: string; fileIds: string[] }): void;
      }
    ).send({ content: 'carry on', fileIds: [] });
    for (let i = 0; i < 8; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
  }

  it('warns when a chain stops paused because a turn FAILED', async () => {
    const { client } = stubClient(groupChat(), {
      chainFrames: [{ chainComplete: true, reason: 'error', paused: true }],
    });
    const fixture = await render(client);
    await sendAndSettle(fixture);
    expect(toasts()).toContainEqual({
      type: 'warning',
      message:
        "A character's turn failed, so auto-responses are paused. Press Resume in the sidebar to carry on.",
    });
  });

  it('informs when a chain stops paused for any other reason', async () => {
    const { client } = stubClient(groupChat(), {
      chainFrames: [{ chainComplete: true, reason: 'paused', paused: true }],
    });
    const fixture = await render(client);
    await sendAndSettle(fixture);
    expect(toasts()).toContainEqual({
      type: 'info',
      message: 'Auto-responses are paused. Press Resume in the sidebar to let the others answer.',
    });
  });

  it('says nothing when the chainComplete carries no `paused` key at all', async () => {
    const { client } = stubClient(groupChat(), {
      chainFrames: [{ chainComplete: true, reason: 'no_next_speaker' }],
    });
    const fixture = await render(client);
    await sendAndSettle(fixture);
    expect(toasts()).toEqual([]);
  });

  it('says nothing when the chainComplete carries `paused: false`', async () => {
    // The empty-response stop: v4 emits reason 'error' with paused FALSE, so the
    // reason alone must never be enough to warn.
    const { client } = stubClient(groupChat(), {
      chainFrames: [{ chainComplete: true, reason: 'error', paused: false }],
    });
    const fixture = await render(client);
    await sendAndSettle(fixture);
    expect(toasts()).toEqual([]);
  });

  // Gate 2: the user paused it themselves, so they already had the toggle's own
  // toast — judged against what the client believed BEFORE the reconcile.
  it('says nothing when the client already believed the chat was paused', async () => {
    const { client } = stubClient(groupChat({ isPaused: true }), {
      chainFrames: [{ chainComplete: true, reason: 'error', paused: true }],
    });
    const fixture = await render(client);
    await sendAndSettle(fixture);
    expect(toasts()).toEqual([]);
  });

  // Gate 3: an all-LLM room keeps AllLLMPauseModal, which explains the stop. v4
  // reads the BARE `isAllLLMChat` here — a room of LLMs that HAS been typed into
  // (so the composite `isAllLLM` is false) still suppresses the toast.
  it('says nothing in an all-LLM room, even one that has been typed into', async () => {
    const allLLM = groupChat({
      participants: [
        participant({ id: 'pA', character: charOf('cA', 'Aaron') }),
        participant({ id: 'pB', character: charOf('cB', 'Beatrice') }),
      ],
      messages: [
        message({
          id: 'u0',
          role: 'USER',
          content: 'Begin.',
          createdAt: '2024-01-01T00:00:00.500Z',
        }),
      ],
    });
    const { client } = stubClient(allLLM, {
      chainFrames: [{ chainComplete: true, reason: 'error', paused: true }],
    });
    const fixture = await render(client);
    await sendAndSettle(fixture);
    expect(toasts()).toEqual([]);
  });

  /**
   * A FORWARD guard, deliberately not a discriminator — and the distinction is
   * recorded rather than papered over.
   *
   * The real shape of bug 123 is the server pausing the chat mid-chain, so the
   * post-turn reconcile brings `isPaused: true` back. If gate 2 ever came to be
   * judged against the RECONCILED chat, it would swallow every genuine
   * announcement and the fix would be dead. This case pins that it does not.
   *
   * It cannot, today, tell v4's ordering (snapshot before the reconcile) from
   * the opposite: measured 2026-09-06 with a probe, `chat()` still reads the
   * PRE-reconcile value immediately after `await invalidateQueries` — the
   * refetch has not settled into the resource yet — so both spellings read
   * `false` here and the obvious mutation stays green. v4's ordering is kept
   * because it is faithful and correct whenever the resource DOES settle
   * sooner; this case is what would redden if that day came.
   */
  it('a reconcile that brings back `isPaused: true` does not swallow the news', async () => {
    const { client } = stubClient(groupChat({ isPaused: false }), {
      chainFrames: [{ chainComplete: true, reason: 'error', paused: true }],
      chatAfterSend: groupChat({ isPaused: true }),
    });
    const fixture = await render(client);
    await sendAndSettle(fixture);
    expect(toasts()).toContainEqual({
      type: 'warning',
      message:
        "A character's turn failed, so auto-responses are paused. Press Resume in the sidebar to carry on.",
    });
  });

  /**
   * v4 bug 123's pause-sync half has NO v5 counterpart, and this case is the
   * executable form of the reason — not a prose claim in a lane record.
   *
   * v4 kept TWO sources of truth for the pause: a local `isPaused` useState in
   * `useChatControls` and the fetched `chat.isPaused`. They drifted because the
   * sync effect keyed on a *transition* of the fetched value while Resume
   * flipped only the local one — so server-paused → Resume → server-paused
   * again was paused→paused, no transition, no sync, and the Salon reported
   * "not paused" forever. v4's fix re-keys the effect on `chat` and has
   * `setPauseState` write the fetched object too.
   *
   * v5 has ONE source: `chat()` is `computed(() => chatQuery.data())`, every
   * reader derives from it, and every writer dispatches `chatUpdate` and
   * invalidates. There is no effect to re-key and no second object to write.
   *
   * The guard: with a server that never actually pauses, a toggle must leave
   * the UI reading "not paused". Any local latch — the thing that made v4's
   * drift possible — would light the notice here and redden this.
   */
  it('holds no local pause latch: the server is the only source of truth', async () => {
    const { client } = stubClient(groupChat({ isPaused: false }));
    const fixture = await render(client);
    expect(fixture.nativeElement.querySelector('.qt-chat-paused-banner')).toBeFalsy();

    await (
      fixture.componentInstance as unknown as { onTogglePause(): Promise<void> }
    ).onTogglePause();
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }

    // The stub's chat still says `isPaused: false`, so the notice must stay
    // away however loudly the toggle was pressed.
    expect(fixture.nativeElement.querySelector('.qt-chat-paused-banner')).toBeFalsy();
  });

  it('renders the Speaking-As selector with two user-controlled characters', async () => {
    const chat = groupChat({
      participants: [
        participant({ id: 'pA', character: charOf('cA', 'Aaron') }),
        participant({ id: 'pU1', controlledBy: 'user', character: charOf('cU1', 'Bertie') }),
        participant({ id: 'pU2', controlledBy: 'user', character: charOf('cU2', 'Jeeves') }),
      ],
    });
    const { client, dispatchData } = stubClient(chat, {
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

    const setSpeaker = calls(dispatchData, 'chatSetActiveSpeaker');
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
    const text = (fixture.nativeElement.textContent as string).replace(/\s+/g, ' ');
    // The CLAIM is pinned, not just the opening words (dogfood #83): the notice
    // promised that no character would speak until you resumed, which is not
    // what pause does in either app — a message you send is still answered
    // once, by whoever's turn it is. Only the chain stops.
    expect(text).toContain(
      "Auto-responses are paused — characters won't carry on by themselves, " +
        'but whoever\'s turn it is will still answer a message you send.',
    );
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
