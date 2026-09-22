import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ActivatedRoute, convertToParamMap, provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { Subject, of } from 'rxjs';
import { beforeAll, describe, expect, it, vi } from 'vitest';

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
import { chatKeys } from '../../chat/chat-keys';
import { SalonConversation } from './salon-conversation';
import { ToastService } from '../../ui/toast.service';

/**
 * `AutoScrollController.performScrollToBottom` schedules four deferred
 * corrections (50/150/300 ms), and the last of them calls
 * `container.scrollTo` — which JSDOM's `HTMLElement` does not have. A timer
 * that fires after its fixture is gone therefore throws into the RUN rather
 * than into a test, and vitest exits non-zero on the unhandled error while
 * every test still reports green (440/440 files, 7,465/7,465 tests, exit 1 —
 * the most confusing shape a suite can take).
 *
 * Two tests in `salon-conversation.spec.ts` already install exactly this stub
 * around themselves; P4.D206 lifts it to file scope in every spec that mounts
 * the Salon, because the arming is not per-test — it depends on what the
 * message list renders, which is why the stray timers appeared when the
 * regeneration row and the status strip joined it. The per-test copies still
 * work: they only add a key that is `!(k in proto)`, and now it always is.
 */
beforeAll(() => {
  const proto = globalThis.HTMLElement?.prototype as unknown as Record<string, unknown>;
  if (!proto) return;
  for (const key of ['scrollTo', 'scrollIntoView'] as const) {
    if (!(key in proto)) proto[key] = () => {};
  }
});


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
    routeTrail: null,
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

/**
 * The shape that produced v4 bug 146: TWO seats the human drives (pU and pV)
 * alongside one LLM seat, with the composer deliberately pointed at pU. The
 * rotation and the speaking-as can then disagree, which is the whole feature.
 */
function twoUserSeatChat(over: Partial<ChatDetail> = {}): ChatDetail {
  return groupChat({
    participants: [
      participant({ id: 'pA', character: charOf('cA', 'Aaron') }),
      participant({ id: 'pU', controlledBy: 'user', character: charOf('cU', 'Bertie') }),
      participant({ id: 'pV', controlledBy: 'user', character: charOf('cV', 'Violet') }),
    ],
    activeTypingParticipantId: 'pU',
    ...over,
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
  /**
   * What `chatSetActiveSpeaker` answers. The default replaces the overlay with
   * the picked seat; a case that must keep an impersonation alive across a
   * deliberate pick says so here (the real server returns the UPDATED list, and
   * picking an owner seat does not end an impersonation of somebody else).
   */
  setSpeakerReply?: (participantId: string) => Record<string, unknown>;
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
      ? (opts.setSpeakerReply?.(req.participantId as string) ?? {
          impersonatingParticipantIds: [req.participantId as string],
          activeTypingParticipantId: req.participantId as string,
        })
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
  // The banner keyed on the FLOOR, not on the composer (v4 bug 146,
  // `2075242f9`, `SalonView.tsx:1531-1596`)
  // -------------------------------------------------------------------------

  /** The `skipUserTurn` dispatches this render made, in order. */
  function skipPosts(dispatch: ReturnType<typeof vi.fn>): string[] {
    return calls(dispatch, 'chatTurnAction')
      .filter((r) => (r as { action?: string }).action === 'skipUserTurn')
      .map((r) => (r as { participantId?: string }).participantId ?? '(none)');
  }

  /**
   * v4's own named route to a composer that is elsewhere: "the deliberate
   * same-turn SpeakerSelector choice" (`SalonView.tsx:1559-1562`). On a fresh
   * render bug 49's turn-follow has ALREADY moved the composer onto the floor,
   * so the two agree and the fourth sentence is unreachable — picking another
   * seat on the same turn is what parts them, and the follow's latch (keyed on
   * the turn SEAT) leaves the pick alone.
   */
  async function pickSpeaker(
    fixture: ComponentFixture<SalonConversation>,
    participantId: string,
  ): Promise<void> {
    await (
      fixture.componentInstance as unknown as {
        onSelectSpeaker(id: string): Promise<void>;
      }
    ).onSelectSpeaker(participantId);
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
  }

  async function clickSkip(fixture: ComponentFixture<SalonConversation>): Promise<void> {
    skipButton(fixture)!.click();
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
  }

  // (a) The heart of bug 146: the rotation has handed the floor to Violet while
  // the composer is still pointed at Bertie. Before the fix the banner named
  // Bertie and Skip passed BERTIE'S turn — a Host turn-pass against a seat that
  // never held the floor, with Violet's turn still outstanding.
  it('names the FLOOR’s seat and skips it, not the composer’s', async () => {
    const { client, dispatch } = stubClient(twoUserSeatChat(), {
      query: { nextSpeakerId: 'pV', nextSpeakerControlledBy: 'user' },
      skip: { turn: { nextSpeakerId: 'pA', nextSpeakerControlledBy: 'llm' } },
    });
    const fixture = await render(client);
    // The turn-follow has already moved the composer to the floor, so the two
    // agree and the banner reads the plain turn sentence…
    expect(banner(fixture)).toBe(
      "Violet's turn — type as them, or skip to let someone else respond.",
    );
    // …until the human deliberately points the composer back at Bertie.
    await pickSpeaker(fixture, 'pU');
    expect(banner(fixture)).toBe(
      "Violet's turn — switch the speaker to them to type, or skip to let someone else respond.",
    );
    await clickSkip(fixture);
    expect(skipPosts(dispatch)).toEqual(['pV']);
  });

  // (b) Bug 123 preserved by construction: the floor belongs to an LLM, so the
  // resolver falls back to the composer's seat and nothing about the off-turn
  // affordance moves.
  it('keeps the composer’s seat — and its wording — when the floor is an LLM’s', async () => {
    const { client, dispatch } = stubClient(twoUserSeatChat(), {
      query: { nextSpeakerId: 'pA', nextSpeakerControlledBy: 'llm' },
      skip: { turn: { nextSpeakerId: 'pA', nextSpeakerControlledBy: 'llm' } },
    });
    const fixture = await render(client);
    expect(banner(fixture)).toBe(
      'Speaking as Bertie — type, or skip to let someone else take the floor.',
    );
    await clickSkip(fixture);
    expect(skipPosts(dispatch)).toEqual(['pU']);
  });

  // (c) The two agree — the unchanged second sentence, with no "switch the
  // speaker" invitation to a composer that is already there.
  it('words it as a plain turn when the floor and the composer agree', async () => {
    const { client, dispatch } = stubClient(twoUserSeatChat(), {
      query: { nextSpeakerId: 'pU', nextSpeakerControlledBy: 'user' },
      skip: { turn: { nextSpeakerId: 'pA', nextSpeakerControlledBy: 'llm' } },
    });
    const fixture = await render(client);
    expect(banner(fixture)).toBe(
      "Bertie's turn — type as them, or skip to let someone else respond.",
    );
    await clickSkip(fixture);
    expect(skipPosts(dispatch)).toEqual(['pU']);
  });

  // (d) Bug 44's overlay preserved: the floor is an LLM seat the human is
  // impersonating, whose durable `controlledBy` is still 'llm'. A reader that
  // consulted the column would hand the floor back to the composer and pass the
  // wrong turn.
  it('follows the floor onto an impersonated LLM seat', async () => {
    const chat = twoUserSeatChat({
      participants: [
        participant({ id: 'pA', character: charOf('cA', 'Aaron') }),
        participant({ id: 'pL', character: charOf('cL', 'Lorian') }),
        participant({ id: 'pU', controlledBy: 'user', character: charOf('cU', 'Bertie') }),
      ],
      activeTypingParticipantId: 'pU',
      impersonatingParticipantIds: ['pL'],
    });
    const { client, dispatch } = stubClient(chat, {
      query: { nextSpeakerId: 'pL', nextSpeakerControlledBy: 'llm' },
      skip: { turn: { nextSpeakerId: 'pA', nextSpeakerControlledBy: 'llm' } },
      // Picking Bertie does not end the impersonation of Lorian.
      setSpeakerReply: (id) => ({
        impersonatingParticipantIds: ['pL'],
        activeTypingParticipantId: id,
      }),
    });
    const fixture = await render(client);
    await pickSpeaker(fixture, 'pU');
    expect(banner(fixture)).toBe(
      "Lorian's turn — switch the speaker to them to type, or skip to let someone else respond.",
    );
    await clickSkip(fixture);
    expect(skipPosts(dispatch)).toEqual(['pL']);
  });

  // (e) Must-speak is computed over the banner's seat, which is now the FLOOR's
  // — so it is Violet the floor falls to, and her copy wins over the
  // composer-elsewhere sentence. No Skip button at all.
  it('lets must-speak over the FLOOR’s seat win over the composer-elsewhere copy', async () => {
    const chat = twoUserSeatChat({
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
          id: 'p-u',
          role: 'ASSISTANT',
          participantId: null,
          systemSender: 'host',
          systemKind: 'turn-pass',
          hostEvent: { participantId: 'pU' },
          content: 'Bertie has nothing to add.',
          createdAt: '2024-01-01T00:00:03.000Z',
        }),
      ],
    });
    const { client } = stubClient(chat, {
      query: { nextSpeakerId: 'pV', nextSpeakerControlledBy: 'user' },
    });
    const fixture = await render(client);
    // Point the composer away, so `composerElsewhere` is genuinely TRUE and the
    // precedence is exercised rather than assumed — without this the turn-follow
    // leaves the two agreeing and the case says nothing about the arm order.
    await pickSpeaker(fixture, 'pU');
    expect(
      (fixture.componentInstance as unknown as { composerElsewhere(): boolean }).composerElsewhere(),
    ).toBe(true);
    expect(banner(fixture)).toBe(
      'Everyone else has passed — it falls to Violet to say something.',
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
  it('skips WITHOUT lifting the pause (bug 137)', async () => {
    // INVERTED at P4.D187. This used to pin the opposite — a skip lifted the
    // pause first, silently, because `triggerContinueMode` refused outright
    // while paused and the skip would otherwise appear to do nothing. v4
    // deleted that guard (`31436bae4`) and the two unpause-first seams with
    // it: a skip is one explicit turn and nothing more, so the room answers
    // with the seat the skip hands the floor to and then falls quiet again.
    // The pause the operator set is theirs, and nothing but Resume lifts it.
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
    // The skip action alone: no `chatUpdate` at all, in either position.
    expect(order).toEqual(['chatTurnAction']);
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
    // The CLAIM is pinned, not just the opening words — this sentence has been
    // wrong twice in opposite directions (dogfood #83, then #118), and both
    // times an opening-words assertion stayed green through it.
    //
    // Since v4 bug 137 (`31436bae4`, ported as P4.D186) the pause is consulted
    // on the SEND path too: a message typed into a paused room is recorded and
    // answered by nobody. The notice must say so, and must name the two ways
    // out, because it is the only persistent explanation on screen.
    expect(text).toContain(
      "Auto-responses are paused — characters won't carry on by themselves, " +
        'and a message you send is recorded without an answer. ' +
        'Nudge a character for a single turn, or press Resume.',
    );
    // The guard: the retired promise must not come back. A port that reverts to
    // the #83 wording would still pass an "Auto-responses are paused" check.
    expect(text).not.toContain('will still answer a message you send');
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

  // ---------------------------------------------------------------------
  // P4.84 — v4 `triggerContinueMode`'s two error toasts
  // (`useSSEStreaming.ts:997-1012`), ported at `runTurn`'s continue entrance.
  //
  // Both arms are reachable only through a STALE roster (the continue controls
  // are disabled by the same predicate that gates the second arm), so each spec
  // drives the gesture against a chat whose participants have moved under it —
  // the P4.D90 refresh class. Driving the RENDERED control, not `runTurn`
  // directly, is deliberate: a handler-direct spec would leave the template
  // wiring unpinned.
  // ---------------------------------------------------------------------

  it('refuses a continue for a seat that is no longer in the chat (v4 :1002-1006)', async () => {
    // Aaron is on the roster when the sidebar renders, and gone by the click.
    const before = groupChat();
    const after = groupChat({
      participants: before.participants.filter((p) => p.id !== 'pA'),
    });
    const { client, dispatch } = stubClient(before, {
      query: { nextSpeakerId: 'pA', nextSpeakerControlledBy: 'llm' },
      chatAfterSend: after,
    });
    const fixture = await render(client);
    const nudgeBtn = [...fixture.nativeElement.querySelectorAll('button')].find((b) =>
      (b as HTMLButtonElement).textContent?.includes('Nudge Aaron'),
    ) as HTMLButtonElement;

    // The roster moves under the rendered card, exactly as another tab's
    // removal would land through the chat query.
    before.participants = after.participants;
    nudgeBtn.click();
    await new Promise((r) => setTimeout(r, 0));

    expect(toasts().at(-1)).toEqual({
      type: 'error',
      message: 'This participant is no longer available in the chat.',
    });
    // v4 returns before the request: nothing was sent.
    expect(calls(dispatch, 'chatSend')).toHaveLength(0);
  });

  it('refuses a continue for a seat that is present but INACTIVE (v4 `&& p.isActive`)', async () => {
    const chat = groupChat();
    const { client, dispatch } = stubClient(chat, {
      query: { nextSpeakerId: 'pA', nextSpeakerControlledBy: 'llm' },
    });
    const fixture = await render(client);
    const nudgeBtn = [...fixture.nativeElement.querySelectorAll('button')].find((b) =>
      (b as HTMLButtonElement).textContent?.includes('Nudge Aaron'),
    ) as HTMLButtonElement;

    // Deactivated, not removed — v4's find filters on `isActive` too, so this
    // arm reddens if the port drops that half of the predicate.
    chat.participants = chat.participants.map((p) =>
      p.id === 'pA' ? { ...p, isActive: false } : p,
    );
    nudgeBtn.click();
    await new Promise((r) => setTimeout(r, 0));

    expect(toasts().at(-1)).toEqual({
      type: 'error',
      message: 'This participant is no longer available in the chat.',
    });
    expect(calls(dispatch, 'chatSend')).toHaveLength(0);
  });

  it("the sidebar nudge no longer swallows an absent seat in silence (v4 `handleNudge` has no such return)", async () => {
    // v4's `handleNudge` (`useTurnManagement.ts:126-155`) guards ONLY
    // `controlledBy === 'user'`; an id it cannot find falls straight through to
    // `triggerContinueMode`, which is where the sentence lives. v5 used to
    // `return` here in silence, which made the seat arm unreachable from the
    // sidebar entirely.
    const { client, dispatch } = stubClient(groupChat(), {
      query: { nextSpeakerId: 'pA', nextSpeakerControlledBy: 'llm' },
    });
    const fixture = await render(client);

    await (
      fixture.componentInstance as unknown as { onSidebarNudge(id: string): Promise<void> }
    ).onSidebarNudge('a-seat-that-is-not-here');
    await new Promise((r) => setTimeout(r, 0));

    expect(toasts().at(-1)).toEqual({
      type: 'error',
      message: 'This participant is no longer available in the chat.',
    });
    expect(calls(dispatch, 'chatSend')).toHaveLength(0);
  });

  it("checks the seat arm FIRST — v4's order, when both arms would fire", async () => {
    // Rendered with EVERY character seat inactive, so the roster arm is already
    // false when the call is made (an in-place mutation cannot move it: it is a
    // memoized `computed` over the chat signal — the trap that made an earlier
    // draft of this case vacuous). The named seat is inactive too, so both arms
    // would refuse; v4 checks the participant first, so its sentence is the one
    // that shows. Swap the two arms and this reddens.
    //
    // Driven through `onSidebarNudge`, the handler the sidebar's Nudge binds
    // (that binding is pinned by "nudges the next LLM speaker" above): the
    // button itself is not rendered for an inactive seat.
    const chat = groupChat({
      participants: groupChat().participants.map((p) =>
        p.type === 'CHARACTER' ? { ...p, isActive: false } : p,
      ),
    });
    const { client, dispatch } = stubClient(chat, {
      query: { nextSpeakerId: null, nextSpeakerControlledBy: null },
    });
    const fixture = await render(client);

    await (
      fixture.componentInstance as unknown as { onSidebarNudge(id: string): Promise<void> }
    ).onSidebarNudge('pA');
    await new Promise((r) => setTimeout(r, 0));

    expect(toasts().at(-1)).toEqual({
      type: 'error',
      message: 'This participant is no longer available in the chat.',
    });
    expect(calls(dispatch, 'chatSend')).toHaveLength(0);
  });

  it("raises the roster sentence when the seat survives but the room's characters do not", async () => {
    // The named seat stays active; the OTHER character goes. The wide predicate
    // (`type === 'CHARACTER' && isActive`, no `controlledBy` filter) is still
    // satisfied by the seat itself — so this must SEND, not toast. The narrow
    // `controlledBy === 'llm'` twin would also pass here; the discriminator for
    // wide-vs-narrow is the next case.
    const chat = groupChat();
    const { client, dispatch } = stubClient(chat, {
      query: { nextSpeakerId: 'pA', nextSpeakerControlledBy: 'llm' },
    });
    const fixture = await render(client);
    const nudgeBtn = [...fixture.nativeElement.querySelectorAll('button')].find((b) =>
      (b as HTMLButtonElement).textContent?.includes('Nudge Aaron'),
    ) as HTMLButtonElement;
    chat.participants = chat.participants.map((p) =>
      p.id === 'pB' ? { ...p, isActive: false } : p,
    );
    nudgeBtn.click();
    await new Promise((r) => setTimeout(r, 0));

    expect(toasts().filter((t) => t.type === 'error')).toHaveLength(0);
    expect(calls(dispatch, 'chatSend')).toHaveLength(1);
  });

  it('reads the WIDE predicate — a user-driven character still counts as present', async () => {
    // The composer's Continue names NO seat, so only the roster arm can fire.
    // Both LLM characters go inactive; the user-controlled CHARACTER remains.
    // v4's `useParticipants.hasActiveCharacters` counts it (no `controlledBy`
    // filter), so the continue proceeds — spelling this arm
    // `controlledBy === 'llm'`, this component's OTHER and narrower twin (which
    // belongs to `onSidebarSkip`), would raise the roster sentence instead.
    const chat = groupChat();
    const { client, dispatch } = stubClient(chat, {
      query: { nextSpeakerId: 'pA', nextSpeakerControlledBy: 'llm' },
    });
    const fixture = await render(client);
    chat.participants = chat.participants.map((p) =>
      p.controlledBy === 'llm' ? { ...p, isActive: false } : p,
    );
    const continueBtn = fixture.nativeElement.querySelector(
      'button[aria-label="Continue"]',
    ) as HTMLButtonElement;
    expect(continueBtn).toBeTruthy();
    continueBtn.click();
    await new Promise((r) => setTimeout(r, 0));

    expect(toasts().filter((t) => t.type === 'error')).toHaveLength(0);
    const sends = calls(dispatch, 'chatSend');
    expect(sends).toHaveLength(1);
    expect((sends[0] as { respondingParticipantId?: string }).respondingParticipantId).toBeUndefined();
  });

  it('raises the roster sentence when no active character remains (v4 :1008-1011)', async () => {
    // Rendered with the room already emptied of active characters, which is the
    // ONLY state this arm fires in. The composer's Continue is correctly
    // DISABLED there (`chat-composer.ts:381`, the same wide predicate), so the
    // gesture cannot be clicked — asserting the disabled button pins that
    // wiring, and the handler call pins the guard behind it. That pairing is
    // the honest shape: the guard exists for the stale-roster case (P4.D90),
    // where the roster empties AFTER the click has already been dispatched, and
    // a jsdom spec cannot land a click inside that window.
    const chat = groupChat({
      participants: groupChat().participants.map((p) =>
        p.type === 'CHARACTER' ? { ...p, isActive: false } : p,
      ),
    });
    const { client, dispatch } = stubClient(chat, {
      query: { nextSpeakerId: null, nextSpeakerControlledBy: null },
    });
    const fixture = await render(client);

    const continueBtn = fixture.nativeElement.querySelector(
      'button[aria-label="Continue"]',
    ) as HTMLButtonElement;
    expect(continueBtn).toBeTruthy();
    expect(continueBtn.disabled).toBe(true);

    (fixture.componentInstance as unknown as { continueTurn(): void }).continueTurn();
    await new Promise((r) => setTimeout(r, 0));

    expect(toasts().at(-1)).toEqual({
      type: 'error',
      message: 'No characters available. Add a character to continue the conversation.',
    });
    expect(calls(dispatch, 'chatSend')).toHaveLength(0);
  });

  it('a paused chat generates ONE turn from the sidebar Skip, and stays paused', async () => {
    // RETIRED DIVERGENCE (P4.D187). This was a recorded divergence: v4's
    // `triggerContinueMode` opened `if (isPaused) return`, so a paused chat
    // reached through the user card's Continue generated nothing, where v5
    // sent. v4 deleted that guard (bug 137) precisely because it was what
    // forced Nudge and Skip to clear the pause to work at all — so v5's
    // behaviour is now v4's, and the pin becomes a plain equality.
    //
    // What it holds now is the whole of bug 137's client half in one gesture:
    // the turn goes out, and the pause is not touched on the way.
    const { client, dispatch } = stubClient(groupChat({ isPaused: true }), {
      query: { nextSpeakerId: 'pA', nextSpeakerControlledBy: 'llm' },
    });
    const fixture = await render(client);

    await (
      fixture.componentInstance as unknown as { onSidebarSkip(): Promise<void> }
    ).onSidebarSkip();
    await new Promise((r) => setTimeout(r, 0));

    expect(calls(dispatch, 'chatSend')).toHaveLength(1);
    // Nothing rewrote `isPaused` — the room is still the operator's to resume.
    expect(calls(dispatch, 'chatUpdate')).toHaveLength(0);
    expect(toasts().filter((t) => t.type === 'error')).toHaveLength(0);
  });

  it('leaves a plain send alone — the guards are the CONTINUE entrance only', async () => {
    const chat = groupChat();
    const { client, dispatch } = stubClient(chat, {
      query: { nextSpeakerId: 'pA', nextSpeakerControlledBy: 'llm' },
    });
    const fixture = await render(client);
    chat.participants = chat.participants.map((p) =>
      p.type === 'CHARACTER' ? { ...p, isActive: false } : p,
    );
    (fixture.componentInstance as unknown as { send(p: { content: string; fileIds: string[] }): void }).send({
      content: 'Even with nobody active, a send is a send.',
      fileIds: [],
    });
    await new Promise((r) => setTimeout(r, 0));

    expect(toasts().filter((t) => t.type === 'error')).toHaveLength(0);
    expect(calls(dispatch, 'chatSend')).toHaveLength(1);
  });
});

/**
 * Bugs 137-139, the client half — a paused chat generates nothing on its own.
 *
 * Pause stopped the turn chain, not the chat: a paused room still drew exactly
 * one reply per message, and Nudge and Skip silently cleared the pause to work
 * at all (v4 `31436bae4`). The rule has two halves — nothing may follow a turn,
 * and nothing may start one — and only the first was written.
 *
 * The client's share: neither summons lifts the pause any more, the first held
 * message of each pause explains the silence, and the all-LLM modal's Continue
 * resumes BEFORE it asks for the next speaker.
 */
describe('SalonConversation — a paused room grants one turn and no more (bugs 137-139)', () => {
  it('nudges without lifting the pause', async () => {
    // The sidebar nudge used to go through `onTogglePause()`, which not only
    // cleared the pause but ANNOUNCED a resume the operator never asked for.
    const { client, dispatch } = stubClient(groupChat({ isPaused: true }));
    const fixture = await render(client);

    await (
      fixture.componentInstance as unknown as { onSidebarNudge(id: string): Promise<void> }
    ).onSidebarNudge('pA');
    await new Promise((r) => setTimeout(r, 0));

    expect(calls(dispatch, 'chatSend')).toHaveLength(1);
    expect(calls(dispatch, 'chatUpdate')).toHaveLength(0);
    expect(toasts().map((t) => t.message)).not.toContain('Auto-responses resumed');
  });

  it('still forwards `nudge: true` on both nudge paths (bug 138 — v5 never had it)', async () => {
    // v4's `stableTriggerContinueMode` declared ONE parameter around a ref that
    // took two, so `nudge` never reached the server and a summoned character
    // could pass the turn — in a paused chat, no turn at all. v5's two nudge
    // paths both pass the flag through `runTurn`'s one dispatch, so this is a
    // NO-COUNTERPART pinned rather than a fix ported.
    const { client, dispatch } = stubClient(groupChat({ isPaused: true }));
    const fixture = await render(client);

    await (
      fixture.componentInstance as unknown as { onSidebarNudge(id: string): Promise<void> }
    ).onSidebarNudge('pA');
    await new Promise((r) => setTimeout(r, 0));

    const send = calls(dispatch, 'chatSend')[0] as unknown as {
      nudge?: boolean;
      respondingParticipantId?: string;
    };
    expect(send.nudge).toBe(true);
    expect(send.respondingParticipantId).toBe('pA');
  });

  it('the all-LLM Continue RESUMES first, then asks for the next speaker (bug 139)', async () => {
    // Closing the modal alone left the room paused, and every later turn
    // stopped dead after one reply. The order is the fix: the server reads
    // `isPaused` when the continue-mode turn arrives, so the resume has to be
    // persisted before the request goes out.
    // v4 then calls `handleContinue` — ASK the server who is up, and name that
    // seat (`useTurnManagement.ts:178-205`); its v5 twin is `onSidebarSkip`, so
    // the resume is followed by the turn query, then the named send.
    const { client, dispatch } = stubClient(groupChat({ isPaused: true }), {
      query: { nextSpeakerId: 'pA', nextSpeakerControlledBy: 'llm' },
    });
    const fixture = await render(client);
    // The render's own mount-time turn query precedes everything; only the
    // calls the Continue itself makes are the order under test.
    const mounted = dispatch.mock.calls.length;

    await (
      fixture.componentInstance as unknown as { onAllLLMContinue(): Promise<void> }
    ).onAllLLMContinue();
    await new Promise((r) => setTimeout(r, 0));

    const order = dispatch.mock.calls
      .slice(mounted)
      .map((c) => c[0] as CoreRequest)
      .filter((r) => r.type === 'chatUpdate' || r.type === 'chatTurnAction' || r.type === 'chatSend')
      .map((r) => r.type);
    expect(order).toEqual(['chatUpdate', 'chatTurnAction', 'chatSend']);
    const send = dispatch.mock.calls
      .map((c) => c[0] as CoreRequest)
      .find((r) => r.type === 'chatSend') as { respondingParticipantId?: string };
    expect(send.respondingParticipantId).toBe('pA');
    const update = dispatch.mock.calls
      .map((c) => c[0] as CoreRequest)
      .find((r) => r.type === 'chatUpdate') as { chat?: { isPaused?: boolean } };
    expect(update.chat?.isPaused).toBe(false);
  });
});

/**
 * The held-turn notice (bug 137, §C.4) — once per pause, and never otherwise.
 *
 * The server emits `heldUserTurn: true` on the chain-complete frame of EVERY
 * held turn; the once-per-pause throttle is the client's, so a long dictation
 * into a paused room does not raise a toast a paragraph. The latch clears when
 * the pause lifts, so the first message into each NEW pause explains itself.
 */
describe('SalonConversation — the held-turn notice is once per pause (bug 137)', () => {
  const SENTENCE =
    'Your remark is in the record. The room stays paused — nudge a character for a single turn, or press Resume.';

  type Host = {
    announceChainPause(
      reason: string,
      paused: boolean,
      pausedBefore: boolean,
      heldUserTurn?: boolean,
    ): void;
  };

  async function pausedRoom(): Promise<ComponentFixture<SalonConversation>> {
    const { client } = stubClient(groupChat({ isPaused: true }));
    return render(client);
  }

  it('announces the first held turn of a pause', async () => {
    const fixture = await pausedRoom();
    const before = toasts().length;

    (fixture.componentInstance as unknown as Host).announceChainPause('paused', true, true, true);

    expect(toasts().slice(before)).toEqual([{ type: 'info', message: SENTENCE }]);
  });

  it('says nothing on the second held turn of the SAME pause', async () => {
    const fixture = await pausedRoom();
    const inst = fixture.componentInstance as unknown as Host;
    inst.announceChainPause('paused', true, true, true);
    const before = toasts().length;

    inst.announceChainPause('paused', true, true, true);

    expect(toasts().slice(before)).toEqual([]);
  });

  it('announces again once the pause has lifted and a new one begun', async () => {
    const { client } = stubClient(groupChat({ isPaused: true }));
    const fixture = await render(client);
    const inst = fixture.componentInstance as unknown as Host;
    inst.announceChainPause('paused', true, true, true);

    // The pause lifts FOR REAL — through the chat the component reads, so the
    // reset effect is what clears the latch. Poking the field directly would
    // pin the latch and leave its wiring untested, and the wiring is the half
    // that can silently rot.
    TestBed.inject(QueryClient).setQueryData(chatKeys.detail('chat-1'), (prev: unknown) => ({
      ...(prev as ChatDetail),
      isPaused: false,
    }));
    fixture.detectChanges();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    const before = toasts().length;
    inst.announceChainPause('paused', true, true, true);

    expect(toasts().slice(before)).toEqual([{ type: 'info', message: SENTENCE }]);
  });

  it('says nothing for a heldUserTurn frame that is not a pause', async () => {
    // `heldUserTurn` never appears without `paused: true` on the wire, and the
    // `!paused` gate still runs first — so a malformed frame raises nothing.
    const fixture = await pausedRoom();
    const before = toasts().length;

    (fixture.componentInstance as unknown as Host).announceChainPause('paused', false, true, true);

    expect(toasts().slice(before)).toEqual([]);
  });

  it('is checked BEFORE the pausedBefore and all-LLM gates', async () => {
    // v4 checks `heldUserTurn` first. Both later gates are wide open here
    // (`pausedBefore` true would silence an ordinary pause notice), so only the
    // ordering can produce this toast.
    const fixture = await pausedRoom();
    const before = toasts().length;

    (fixture.componentInstance as unknown as Host).announceChainPause('error', true, true, true);

    // The held sentence, NOT the chain-error one.
    expect(toasts().slice(before)).toEqual([{ type: 'info', message: SENTENCE }]);
  });
});
