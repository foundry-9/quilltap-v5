import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';

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
import type { DialogueDetection, RenderingPattern } from './render/roleplay-rendering';

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
  function render(
    messages: MessageDto[],
    stream: ChatStreamState | null,
    userParticipantIds?: ReadonlySet<string>,
  ): ComponentFixture<MessageList> {
    TestBed.configureTestingModule({
      imports: [MessageList],
      providers: [{ provide: CoreClient, useValue: { dispatch: vi.fn(), events$: { subscribe: () => ({ unsubscribe() {} }) } } }],
    });
    const fixture = TestBed.createComponent(MessageList);
    fixture.componentRef.setInput('messages', messages);
    fixture.componentRef.setInput('chat', chatDetail());
    fixture.componentRef.setInput('stream', stream);
    if (userParticipantIds) fixture.componentRef.setInput('userParticipantIds', userParticipantIds);
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

/**
 * P4.D38 tier 2 — the overheard-dim wiring (v4 `VirtualizedMessageList.tsx
 * :358-373`). `userParticipantIds` is threaded straight to `isOverheardWhisper`
 * per message; the pure function's own case inventory lives in
 * `whisper-visibility.spec.ts`.
 */
describe('MessageList — the overheard-whisper dim (P4.D38 tier 2)', () => {
  function render(
    messages: MessageDto[],
    userParticipantIds: ReadonlySet<string>,
  ): ComponentFixture<MessageList> {
    TestBed.configureTestingModule({
      imports: [MessageList],
      providers: [{ provide: CoreClient, useValue: { dispatch: vi.fn(), events$: { subscribe: () => ({ unsubscribe() {} }) } } }],
    });
    const fixture = TestBed.createComponent(MessageList);
    fixture.componentRef.setInput('messages', messages);
    fixture.componentRef.setInput('chat', chatDetail());
    fixture.componentRef.setInput('userParticipantIds', userParticipantIds);
    fixture.detectChanges();
    return fixture;
  }

  it('dims a character-to-character whisper the operator has no part in', () => {
    const fixture = render(
      [
        message({
          id: 'w1',
          participantId: 'p1',
          targetParticipantIds: ['p2'],
          content: 'psst, only for Bob',
        }),
      ],
      new Set(['p3']),
    );
    const bubble = fixture.nativeElement.querySelector('.qt-chat-message-whisper-overheard');
    expect(bubble).not.toBeNull();
    expect(bubble.textContent).toContain('psst, only for Bob');
  });

  it('does not dim a whisper the operator authored', () => {
    const fixture = render(
      [
        message({
          id: 'w1',
          participantId: 'p1',
          targetParticipantIds: ['p2'],
          content: 'from me',
        }),
      ],
      new Set(['p1']),
    );
    expect(fixture.nativeElement.querySelector('.qt-chat-message-whisper-overheard')).toBeNull();
  });

  it('does not dim an ordinary public message', () => {
    const fixture = render(
      [message({ id: 'm1', participantId: 'p1', content: 'in the open' })],
      new Set(['p3']),
    );
    expect(fixture.nativeElement.querySelector('.qt-chat-message-whisper-overheard')).toBeNull();
  });
});
/**
 * Dogfood #51, the sharper half. `copy` is a native clipboard event that
 * BUBBLES, so with the output named `copy`, any Cmd+C over selected text in a
 * message reached this binding with a `ClipboardEvent` — and the Salon's
 * handler does `writeText(message.content)`, i.e. `writeText(undefined)`,
 * overwriting what the user had just copied and toasting success. The output is
 * `copyMessage` now; a native copy must emit nothing.
 */
describe('MessageList — a native copy must not fire the copy action', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('selecting text and copying inside a row emits nothing', () => {
    TestBed.configureTestingModule({
      imports: [MessageList],
      providers: [
        {
          provide: CoreClient,
          useValue: { dispatch: vi.fn(), events$: { subscribe: () => ({ unsubscribe() {} }) } },
        },
      ],
    });
    const fixture = TestBed.createComponent(MessageList);
    fixture.componentRef.setInput('messages', [message({ id: 'm1', content: 'selectable prose' })]);
    fixture.componentRef.setInput('chat', chatDetail());
    fixture.detectChanges();

    const seen: unknown[] = [];
    fixture.componentInstance.copyMessage.subscribe((c) => seen.push(c));
    const row = fixture.nativeElement.querySelector('qt-message-row') as HTMLElement;
    row.dispatchEvent(new Event('copy', { bubbles: true }));

    expect(seen).toEqual([]);
  });
});

/**
 * P4.30 — the chat's roleplay template reaching EVERY rendered row.
 *
 * v4 threads `renderingPatterns` / `dialogueDetection` from `SalonView`'s
 * template fetch into its two `VirtualizedMessageList` call sites (`:314-315`
 * per row, `:387-388` for the streaming bubble); `MessageRow` fans them out to
 * the body AND to each spliced reasoning block, and an EXPANDED announcement is
 * a normal `MessageRow` in v4, so it gets them too. v5 renders four of those
 * five surfaces through its own components, so each is asserted here.
 *
 * The custom set below shares no delimiter with the defaults (`@@…@@`), so each
 * assertion is paired with its no-template twin: the class appearing is only
 * meaningful next to the same content NOT wearing it.
 */
describe('MessageList — the chat template threads into every rendered row (P4.30)', () => {
  afterEach(() => TestBed.resetTestingModule());

  const CUSTOM: RenderingPattern[] = [{ pattern: '@@[^@]+@@', className: 'qt-chat-emote' }];
  const CUSTOM_DIALOGUE: DialogueDetection = {
    openingChars: ['«'],
    closingChars: ['»'],
    className: 'qt-chat-custom-dialogue',
  };

  function render(opts: {
    messages: MessageDto[];
    stream?: ChatStreamState | null;
    patterns?: RenderingPattern[];
    dialogue?: DialogueDetection | null;
  }): ComponentFixture<MessageList> {
    // Several cases render the SAME content twice — once with a template, once
    // without — and the pair is the whole proof, so the module has to be torn
    // down between the two.
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [MessageList],
      providers: [
        {
          provide: CoreClient,
          useValue: { dispatch: vi.fn(), events$: { subscribe: () => ({ unsubscribe() {} }) } },
        },
      ],
    });
    const fixture = TestBed.createComponent(MessageList);
    fixture.componentRef.setInput('messages', opts.messages);
    fixture.componentRef.setInput('chat', chatDetail());
    fixture.componentRef.setInput('stream', opts.stream ?? null);
    if (opts.patterns) fixture.componentRef.setInput('renderingPatterns', opts.patterns);
    if (opts.dialogue !== undefined) fixture.componentRef.setInput('dialogueDetection', opts.dialogue);
    fixture.detectChanges();
    return fixture;
  }

  function emotes(fixture: ComponentFixture<MessageList>): number {
    return fixture.nativeElement.querySelectorAll('.qt-chat-emote').length;
  }

  it('styles a settled message body with the template patterns', () => {
    const messages = [message({ id: 'm1', content: 'She paused. @@leans in@@ and whispered.' })];
    expect(emotes(render({ messages, patterns: CUSTOM }))).toBe(1);
  });

  it('leaves the same body unstyled with no template (the reset arm)', () => {
    const messages = [message({ id: 'm1', content: 'She paused. @@leans in@@ and whispered.' })];
    expect(emotes(render({ messages }))).toBe(0);
  });

  it('styles a reasoning block with the template patterns (v4 MessageRow:367-372)', () => {
    const messages = [
      message({ id: 'm1', content: 'plain body', reasoningContent: 'I should @@pause@@ here.' }),
    ];
    expect(emotes(render({ messages, patterns: CUSTOM }))).toBe(1);
    expect(emotes(render({ messages }))).toBe(0);
  });

  it('styles the live streaming bubble (v4 VirtualizedMessageList:387-388)', () => {
    const stream = reduceChatFrame(initialChatStreamState(), {
      content: 'Ada @@steps closer@@.',
    });
    expect(emotes(render({ messages: [], stream, patterns: CUSTOM }))).toBe(1);
    expect(emotes(render({ messages: [], stream }))).toBe(0);
  });

  it('styles an EXPANDED announcement body — v4 renders it as an ordinary row', () => {
    const messages = [
      message({
        id: 'a1',
        role: 'SYSTEM',
        systemSender: 'host',
        systemKind: 'add',
        participantId: null,
        content: 'The lock @@gives way@@.',
      }),
    ];
    const fixture = render({ messages, patterns: CUSTOM });
    // Collapsed: only the chip is on screen, so nothing is styled yet.
    expect(emotes(fixture)).toBe(0);
    const chipButton = fixture.nativeElement.querySelector(
      '.qt-chat-announcement-chip',
    ) as HTMLButtonElement;
    expect(chipButton).not.toBeNull();
    chipButton.click();
    fixture.detectChanges();
    expect(emotes(fixture)).toBe(1);
  });

  it('threads the dialogue detection as well as the patterns', () => {
    const messages = [message({ id: 'm1', content: '«Bonjour, mon ami»' })];
    const withTemplate = render({ messages, dialogue: CUSTOM_DIALOGUE });
    expect(
      withTemplate.nativeElement.querySelectorAll('.qt-chat-custom-dialogue').length,
    ).toBe(1);
    const without = render({ messages });
    expect(without.nativeElement.querySelectorAll('.qt-chat-custom-dialogue').length).toBe(0);
  });

  it('an EMPTY patterns array renders exactly as no template does (v4 fallback arm)', () => {
    const messages = [message({ id: 'm1', content: 'A hush falls. [the door creaks open]' })];
    const empty = render({ messages, patterns: [] });
    const none = render({ messages });
    const html = (f: ComponentFixture<MessageList>) =>
      (f.nativeElement.querySelector('.qt-chat-message-content') as HTMLElement).innerHTML;
    expect(html(empty)).toBe(html(none));
    expect(html(empty)).toContain('qt-chat-narration');
  });
});

describe('streamMessageToMessageDto — the confirmation family (P4.D132)', () => {
  it('carries all five confirmation fields through the stream→bubble hop', () => {
    // `confirmationOriginalContent` is absent on every LIVE frame (v4's
    // confirmationResult never carries the pre-revision text), so this pin
    // hand-builds the StreamMessage: what it guards is the MAPPER — the badge
    // reads a uniform five-field family whichever flow produced the row.
    const state: ChatStreamState = {
      ...initialChatStreamState(),
      messages: [
        {
          kind: 'assistant',
          id: 'm-conf',
          content: 'Altitude is reported in feet.',
          participantId: 'p1',
          provider: null,
          modelName: null,
          reasoningContent: null,
          reasoningSegments: null,
          intermediate: false,
          confirmed: true,
          confirmationChecked: true,
          confirmationRevised: true,
          confirmationNotes: 'The ledger excerpt shows a metric column.',
          confirmationOriginalContent: 'Altitude is reported in metres.',
        },
      ],
    };
    const items = buildStreamRenderItems(state, []);
    expect(items).toHaveLength(1);
    const msg = items[0].type === 'message' ? items[0].message : null;
    expect(msg!.confirmed).toBe(true);
    expect(msg!.confirmationChecked).toBe(true);
    expect(msg!.confirmationRevised).toBe(true);
    expect(msg!.confirmationNotes).toBe('The ledger excerpt shows a metric column.');
    expect(msg!.confirmationOriginalContent).toBe('Altitude is reported in metres.');
  });
});


/**
 * P4.69 — the list forwards the Salon's danger verdict to every row.
 *
 * v4 declares the prop at `VirtualizedMessageList.tsx:106`, defaults it false at
 * `:165` and passes it straight through at `:368` (→ `MessageRow`). v4 has ONE
 * MessageRow path; v5 has two — the virtualized rows and the
 * stream-accumulated finished bubbles (dogfood #7), whose v4 counterparts are
 * folded into `state.messages` and render through that same path — so BOTH must
 * carry it or a chained reply loses its ring mid-turn.
 *
 * `MessageRow`'s own three-site rule (assistant paints, user never) is pinned in
 * `message-row.spec.ts`; this file pins only the forwarding.
 */
describe('MessageList — forwarding the danger verdict (P4.69, v4 VirtualizedMessageList:106/:165/:368)', () => {
  afterEach(() => TestBed.resetTestingModule());

  function render(
    messages: MessageDto[],
    stream: ChatStreamState | null,
    isDangerousChat?: boolean,
  ): ComponentFixture<MessageList> {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [MessageList],
      providers: [
        {
          provide: CoreClient,
          useValue: { dispatch: vi.fn(), events$: { subscribe: () => ({ unsubscribe() {} }) } },
        },
      ],
    });
    const fixture = TestBed.createComponent(MessageList);
    fixture.componentRef.setInput('messages', messages);
    fixture.componentRef.setInput('chat', chatDetail());
    fixture.componentRef.setInput('stream', stream);
    if (isDangerousChat !== undefined) {
      fixture.componentRef.setInput('isDangerousChat', isDangerousChat);
    }
    fixture.detectChanges();
    return fixture;
  }

  const rings = (fixture: ComponentFixture<MessageList>): number =>
    fixture.nativeElement.querySelectorAll('.qt-chat-avatar-dangerous').length;

  const assistant = () => message({ id: 'm1', participantId: 'p1', content: 'A fine morning.' });

  it('rings a virtualized assistant row when flagged (v4 :368)', () => {
    expect(rings(render([assistant()], null, true))).toBe(1);
  });

  it('rings nothing on the same row unflagged — the reset arm', () => {
    expect(rings(render([assistant()], null, false))).toBe(0);
  });

  it('defaults to unflagged when the Salon passes nothing (v4 :165)', () => {
    expect(rings(render([assistant()], null))).toBe(0);
  });

  it('rings the stream-accumulated finished bubbles too (v5’s second MessageRow path)', () => {
    // foldChain() lands two finished chained replies in the stream state; only
    // those rows are on screen (no canonical messages), so every ring counted
    // here comes from the stream path.
    const flagged = render([], foldChain(), true);
    expect(flagged.nativeElement.textContent as string).toContain('Hi, I am Ada.');
    expect(rings(flagged)).toBeGreaterThan(0);
    expect(rings(render([], foldChain(), false))).toBe(0);
  });
});

/**
 * The avatar gate and the responding-character resolver — v4 `SalonView.tsx`,
 * ported by P4.75 so the streaming bubble can open with its avatar column.
 *
 * ⚠ **An order premise refuted by measurement.** The order described v5's
 * settled-row gate as a separate GROUP_ONLY-aware rule that must NOT be merged
 * with the streaming row's ALWAYS-only one. Measured at the `0b0617fee` pin, v4
 * consumes `avatarDisplayMode` at exactly ONE site — `shouldShowAvatars`
 * (`SalonView.tsx:1171-1174`) — and feeds it to BOTH the settled rows
 * (`VirtualizedMessageList.tsx:274`, `:305`) and the streaming bubble (`:383`);
 * `GROUP_ONLY` is never read anywhere in `app/` or `components/`, and v4's own
 * settings copy calls it "(will be implemented in the future)"
 * (`components/settings/chat-settings/types.ts:266`). v5's "≥2 characters" arm
 * implemented a feature v4 has not built, so it is gone and there is one rule.
 */
describe('MessageList — the avatar gate + responding character (v4 SalonView:1171,:1176)', () => {
  function render(
    stream: ChatStreamState | null,
    settings: { avatarDisplayMode: 'ALWAYS' | 'GROUP_ONLY' | 'NEVER' } | null,
    chat: ChatDetail = chatDetail(),
  ): ComponentFixture<MessageList> {
    TestBed.configureTestingModule({
      imports: [MessageList],
      providers: [
        {
          provide: CoreClient,
          useValue: { dispatch: vi.fn(), events$: { subscribe: () => ({ unsubscribe() {} }) } },
        },
      ],
    });
    const fixture = TestBed.createComponent(MessageList);
    fixture.componentRef.setInput('messages', []);
    fixture.componentRef.setInput('chat', chat);
    fixture.componentRef.setInput('stream', stream);
    if (settings) fixture.componentRef.setInput('settings', settings);
    fixture.detectChanges();
    return fixture;
  }

  /** The live bubble's avatar column, or null. */
  function column(fixture: ComponentFixture<MessageList>): HTMLElement | null {
    return fixture.nativeElement.querySelector('qt-streaming-message .qt-chat-desktop-avatar');
  }

  /**
   * A live turn. `respondingParticipantId` is set by the `turnStart` frame
   * (`chat-stream.reducer.ts:277`), which is where the server names the speaker;
   * a bare content frame carries no participant, so the fixture has to open the
   * turn the way the wire does.
   */
  const live = (participantId?: string): ChatStreamState =>
    foldChatFrames(
      participantId
        ? [{ turnStart: true, participantId }, { content: 'live' }]
        : [{ content: 'live' }],
    );

  it('shows avatars with no settings loaded (v4 :1172 — `if (!chatSettings) return true`)', () => {
    expect(column(render(live(), null))).not.toBeNull();
  });

  it("shows avatars on ALWAYS (v4 :1173 — `=== 'ALWAYS'`)", () => {
    expect(column(render(live(), { avatarDisplayMode: 'ALWAYS' }))).not.toBeNull();
  });

  it('hides avatars on NEVER (v4 :1173)', () => {
    expect(column(render(live(), { avatarDisplayMode: 'NEVER' }))).toBeNull();
  });

  it('hides avatars on GROUP_ONLY even with two characters in the cast (v4 :1173)', () => {
    // The fixture chat carries Ada AND Bob — the exact shape v5's retired "≥2"
    // rule would have shown avatars for.
    expect(chatDetail().participants.filter((p) => p.type === 'CHARACTER').length).toBe(2);
    expect(column(render(live(), { avatarDisplayMode: 'GROUP_ONLY' }))).toBeNull();
  });

  it('names the participant the stream is answering as (v4 :1177-1181)', () => {
    const col = column(render(live('p2'), null));
    expect(col?.textContent?.trim()).toBe('B'); // Bob's initial
  });

  it('falls back to the first ACTIVE character participant (v4 :1183 getFirstCharacter)', () => {
    const chat = chatDetail();
    chat.participants[0] = { ...chat.participants[0], isActive: false };
    // Ada is inactive, so v4's `find(p => p.type === 'CHARACTER' && p.isActive)`
    // skips her and lands on Bob — the arm a `filter`-free `[0]` would miss.
    expect(column(render(live(), null, chat))?.textContent?.trim()).toBe('B');
  });

  it('takes the named participant even when it is inactive (v4 :1178 checks neither)', () => {
    const chat = chatDetail();
    chat.participants[0] = { ...chat.participants[0], isActive: false };
    expect(column(render(live('p1'), null, chat))?.textContent?.trim()).toBe('A');
  });

  it("renders 'AI' when the cast holds no character at all (v4 :1183 → undefined)", () => {
    const chat = chatDetail();
    chat.participants = [];
    expect(column(render(live(), null, chat))?.textContent?.trim()).toBe('A');
  });

  it("forwards the chat's danger verdict to the live column (v4 VML:384)", () => {
    const fixture = render(live(), null);
    expect(column(fixture)?.classList.contains('qt-chat-avatar-dangerous')).toBe(false);
    fixture.componentRef.setInput('isDangerousChat', true);
    fixture.detectChanges();
    expect(column(fixture)?.classList.contains('qt-chat-avatar-dangerous')).toBe(true);
  });
});
