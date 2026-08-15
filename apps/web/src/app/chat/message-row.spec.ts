import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type {
  ChatDetail,
  ChatSettingsDto,
  MessageAttachment,
  MessageDto,
  ParticipantDetail,
} from '../core/core-contract';
import { MessageRow, type ImageClickEvent } from './message-row';

function participant(over: Partial<ParticipantDetail>): ParticipantDetail {
  return {
    id: 'p1',
    type: 'CHARACTER',
    displayOrder: 0,
    isActive: true,
    controlledBy: 'llm',
    status: 'active',
    character: { id: 'char1', name: 'Lorian', title: null, avatarUrl: null, defaultImageId: null, defaultImage: null },
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
    createdAt: '2026-01-01T00:00:02.000Z',
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
    title: 'Tea',
    contextSummary: null,
    roleplayTemplateId: null,
    chatType: 'salon',
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    isPaused: false,
    isManuallyRenamed: false,
    participants: [participant({ id: 'p1' })],
    user: { id: 'user1', name: 'Bertie', image: null },
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

function render(msg: MessageDto): ComponentFixture<MessageRow> {
  TestBed.configureTestingModule({
    imports: [MessageRow],
    providers: [{ provide: CoreClient, useValue: { dispatch: vi.fn() } }],
  });
  const fixture = TestBed.createComponent(MessageRow);
  fixture.componentRef.setInput('message', msg);
  fixture.componentRef.setInput('chat', chatDetail());
  fixture.detectChanges();
  return fixture;
}

describe('MessageRow — courier branch', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders the courier bubble and skips the action bar for a pending turn', () => {
    const fixture = render(message({ pendingExternalPrompt: 'PROMPT' }));
    expect(fixture.nativeElement.querySelector('qt-courier-bubble')).not.toBeNull();
    expect(fixture.nativeElement.querySelector('.qt-chat-message-row-courier')).not.toBeNull();
    // v4 skips the normal action bar / danger chrome on the courier row.
    expect(fixture.nativeElement.querySelector('.qt-chat-message-action-bar')).toBeNull();
  });

  it('renders the normal body (action bar, no bubble) for an ordinary message', () => {
    const fixture = render(message({ content: 'A fine morning.' }));
    expect(fixture.nativeElement.querySelector('qt-courier-bubble')).toBeNull();
    expect(fixture.nativeElement.querySelector('.qt-chat-message-action-bar')).not.toBeNull();
  });
});

describe('MessageRow — Pascal roll outcome (P4.6ba)', () => {
  afterEach(() => TestBed.resetTestingModule());

  const pascalMsg = (over: Partial<MessageDto> = {}) =>
    message({
      systemSender: 'pascal',
      systemKind: 'custom-tool-result',
      participantId: null,
      content: '🎲 The lock gives way.',
      pascalMeta: {
        tool: 'pick_lock',
        toolTitle: 'Pick Lock',
        definitionTier: 'global',
        definitionMountId: 'm1',
        params: {},
        rollForm: 'range',
        raw: 12,
        value: 12,
        state: 'success',
        outcomeIndex: 0,
        invokedBy: 'user',
      },
      ...over,
    });

  it('renders the header bar with "Pascal" and the tool title, and the markdown body', () => {
    const fixture = render(pascalMsg());
    const bar = fixture.nativeElement.querySelector('.qt-chat-system-bar');
    expect(bar).not.toBeNull();
    expect(bar.querySelector('.qt-chat-system-bar-sender').textContent.trim()).toBe('Pascal');
    expect(bar.querySelector('.qt-chat-system-bar-kind').textContent.trim()).toBe('Pick Lock');
    // The body still runs through the normal markdown pipeline.
    expect(fixture.nativeElement.querySelector('qt-message-content')).not.toBeNull();
    // The character author header is suppressed (Pascal has no participant).
    expect(fixture.nativeElement.querySelector('.qt-chat-message-header')).toBeNull();
  });

  it('falls back to the declaration name for a legacy row with no toolTitle', () => {
    const fixture = render(
      pascalMsg({
        pascalMeta: {
          tool: 'pick_lock',
          definitionTier: 'global',
          definitionMountId: 'm1',
          params: {},
          rollForm: 'range',
          raw: 1,
          value: 1,
          state: 'failure',
          outcomeIndex: 0,
          invokedBy: 'user',
        },
      }),
    );
    expect(
      fixture.nativeElement.querySelector('.qt-chat-system-bar-kind').textContent.trim(),
    ).toBe('pick_lock');
  });

  /**
   * P4.26 CORRECTION. This case used to assert NO avatar: v5 had no Staff
   * portraits, so `resolveMessageAuthor` fell through to the role fallback and
   * would have hung the first cast character's face on the croupier — suppressing
   * the whole avatar was the lesser wrong. v4 has always drawn Pascal's own
   * portrait beside the bar (`getMessageAvatar` :1127), and now so does v5.
   */
  it('renders Pascal’s own portrait beside the bar (v4 getMessageAvatar)', () => {
    const fixture = render(pascalMsg());
    const avatar = fixture.nativeElement.querySelector('.qt-chat-desktop-avatar');
    expect(avatar).not.toBeNull();
    const img = avatar.querySelector('img');
    expect(img.getAttribute('src')).toBe('/images/avatars/pascal-avatar.webp');
    expect(img.getAttribute('alt')).toBe('Pascal');
  });

  // --- P4.d21 (v4 231be14c): the bar wears the outcome's own state -----------

  it('accents the bar and its dot with the outcome the roll landed on', () => {
    const fixture = render(pascalMsg());
    const bar = fixture.nativeElement.querySelector('.qt-chat-system-bar');
    expect(bar.classList.contains('qt-pascal-result')).toBe(true);
    expect(bar.classList.contains('qt-pascal-result--success')).toBe(true);
    const dot = bar.querySelector('.qt-chat-announcement-dot');
    expect(dot.classList.contains('qt-chat-announcement-dot-outcome-success')).toBe(true);
    // The importance dot ("high", i.e. the same red a deleted file gets) is gone.
    expect(dot.classList.contains('qt-chat-announcement-dot-high')).toBe(false);
  });

  it('names the state in visually-hidden text, so it is not carried by colour alone', () => {
    const fixture = render(pascalMsg());
    const sr = fixture.nativeElement.querySelector('.qt-chat-system-bar .sr-only');
    expect(sr).not.toBeNull();
    expect(sr.textContent.trim()).toBe('success');
  });

  it('falls back to the importance dot when the record carries no usable state', () => {
    const fixture = render(
      pascalMsg({
        pascalMeta: {
          tool: 'pick_lock',
          definitionTier: 'global',
          definitionMountId: 'm1',
          params: {},
          rollForm: 'range',
          raw: 1,
          value: 1,
          // A state a future build introduces, which this one can't colour.
          state: 'triumph' as 'success',
          outcomeIndex: 0,
          invokedBy: 'user',
        },
      }),
    );
    const bar = fixture.nativeElement.querySelector('.qt-chat-system-bar');
    expect(bar.classList.contains('qt-pascal-result')).toBe(false);
    expect(
      bar.querySelector('.qt-chat-announcement-dot').classList.contains(
        'qt-chat-announcement-dot-high',
      ),
    ).toBe(true);
    expect(bar.querySelector('.sr-only')).toBeNull();
  });
});

describe('MessageRow — image thumbnails', () => {
  afterEach(() => TestBed.resetTestingModule());

  const imageAtt: MessageAttachment = {
    id: 'file-9',
    filename: 'sketch.png',
    filepath: 'uploads/sketch.png',
    mimeType: 'image/png',
  };

  it('renders a thumbnail button per image attachment using the id-keyed thumbnail route', () => {
    const fixture = render(message({ attachments: [imageAtt] }));
    const img = fixture.nativeElement.querySelector('.qt-chat-attachment-image') as HTMLImageElement;
    expect(img).not.toBeNull();
    expect(img.getAttribute('src')).toBe('/api/v1/files/file-9?action=thumbnail&size=80');
    expect(img.getAttribute('alt')).toBe('sketch.png');
  });

  it('filters out non-image attachments', () => {
    const pdf: MessageAttachment = { id: 'p', filename: 'doc.pdf', filepath: 'x/doc.pdf', mimeType: 'application/pdf' };
    const fixture = render(message({ attachments: [pdf] }));
    expect(fixture.nativeElement.querySelector('.qt-chat-attachment-image')).toBeNull();
  });

  it('emits imageClick with the full-image src, filename, and fileId on click', () => {
    const fixture = render(message({ attachments: [imageAtt] }));
    let event: ImageClickEvent | undefined;
    fixture.componentInstance.imageClick.subscribe((e) => (event = e));
    (fixture.nativeElement.querySelector('.qt-chat-attachment-button') as HTMLButtonElement).click();
    expect(event).toEqual({ src: '/api/v1/files/file-9', filename: 'sketch.png', fileId: 'file-9' });
  });
});

describe('MessageRow — the per-message token badge (v4 MessageActionBar.tsx:195-206)', () => {
  afterEach(() => TestBed.resetTestingModule());

  function tokenSettings(over: Partial<Record<string, boolean>> = {}): ChatSettingsDto {
    return {
      avatarDisplayMode: 'ALWAYS',
      avatarDisplayStyle: 'CIRCULAR',
      tokenDisplaySettings: {
        showPerMessageTokens: false,
        showPerMessageCost: false,
        showChatTotals: false,
        showSystemEvents: false,
        ...over,
      },
    } as ChatSettingsDto;
  }

  function renderWith(msg: MessageDto, settings: ChatSettingsDto | null): ComponentFixture<MessageRow> {
    TestBed.configureTestingModule({
      imports: [MessageRow],
      providers: [{ provide: CoreClient, useValue: { dispatch: vi.fn() } }],
    });
    const fixture = TestBed.createComponent(MessageRow);
    fixture.componentRef.setInput('message', msg);
    fixture.componentRef.setInput('chat', chatDetail());
    fixture.componentRef.setInput('settings', settings);
    fixture.detectChanges();
    return fixture;
  }

  const withTokens = { promptTokens: 1200, completionTokens: 340, tokenCount: 1540 };

  it('mounts the badge in the timestamp row when the flag is on and a count is non-zero', () => {
    const fixture = renderWith(
      message(withTokens),
      tokenSettings({ showPerMessageTokens: true }),
    );
    const row = fixture.nativeElement.querySelector('.qt-chat-message-action-timestamp');
    const badge = row.querySelector('qt-token-badge');
    expect(badge).not.toBeNull();
    expect(badge.textContent.replace(/\s+/g, ' ').trim()).toBe('1.2K/340tokens');
  });

  it('passes tokenCount as the badge total (not the prompt+completion sum)', () => {
    // A server-supplied total that disagrees with the parts must win — v4 threads
    // `message.tokenCount` straight through as `totalTokens`.
    const fixture = renderWith(
      message({ promptTokens: 10, completionTokens: 5, tokenCount: 0 }),
      tokenSettings({ showPerMessageTokens: true }),
    );
    // total === 0 → the badge's own v4 guard suppresses the render.
    expect(fixture.nativeElement.querySelector('qt-token-badge div')).toBeNull();
  });

  it('does not mount the badge when the flag is off', () => {
    const fixture = renderWith(message(withTokens), tokenSettings());
    expect(fixture.nativeElement.querySelector('qt-token-badge')).toBeNull();
  });

  it('does not mount the badge when settings have not loaded (flags default false)', () => {
    const fixture = renderWith(message(withTokens), null);
    expect(fixture.nativeElement.querySelector('qt-token-badge')).toBeNull();
  });

  it('does not mount the badge when both counts are null, flag on (v4 JS-truthy gate)', () => {
    const fixture = renderWith(
      message({ promptTokens: null, completionTokens: null }),
      tokenSettings({ showPerMessageTokens: true }),
    );
    expect(fixture.nativeElement.querySelector('qt-token-badge')).toBeNull();
  });

  it('does not mount the badge when both counts are 0, flag on — and leaks no "0" glyph', () => {
    // v4's JSX `&&`-chain evaluates to the number 0 here and React renders a
    // literal "0" next to the timestamp. That is a React-idiom bug; `@if` renders
    // nothing. This pins the divergence so it is a decision, not a surprise.
    const fixture = renderWith(
      message({ promptTokens: 0, completionTokens: 0 }),
      tokenSettings({ showPerMessageTokens: true }),
    );
    expect(fixture.nativeElement.querySelector('qt-token-badge')).toBeNull();
    // The row contains the timestamp and NOTHING else — no stray text node.
    const row = fixture.nativeElement.querySelector('.qt-chat-message-action-timestamp');
    const timestamp = row.querySelector('span').textContent.replace(/\s+/g, ' ').trim();
    expect(row.textContent.replace(/\s+/g, ' ').trim()).toBe(timestamp);
  });

  it('mounts when only ONE count is non-zero (v4 `promptTokens || completionTokens`)', () => {
    const fixture = renderWith(
      message({ promptTokens: 0, completionTokens: 77, tokenCount: 77 }),
      tokenSettings({ showPerMessageTokens: true }),
    );
    expect(fixture.nativeElement.querySelector('qt-token-badge')).not.toBeNull();
  });

  it('never mounts on the cost flag alone — v4 gates on the TOKENS flag only', () => {
    // The first of the two reasons per-message cost is unreachable in v4.
    const fixture = renderWith(
      message(withTokens),
      tokenSettings({ showPerMessageCost: true }),
    );
    expect(fixture.nativeElement.querySelector('qt-token-badge')).toBeNull();
  });

  it('renders no cost text even with both flags on (no per-message cost source exists)', () => {
    // The second reason: v4 passes no `estimatedCostUSD` — there is no such field
    // on the Message type. Ported dead rather than invented.
    const fixture = renderWith(
      message(withTokens),
      tokenSettings({ showPerMessageTokens: true, showPerMessageCost: true }),
    );
    const badge = fixture.nativeElement.querySelector('qt-token-badge');
    expect(badge.textContent.replace(/\s+/g, ' ').trim()).toBe('1.2K/340tokens');
    expect(badge.querySelector('[title="Estimated cost"]')).toBeNull();
  });
});

describe('MessageRow — the per-message LLM-logs entry (v4 MessageActionBar.tsx:149-157)', () => {
  afterEach(() => TestBed.resetTestingModule());

  function renderWith(msg: MessageDto, hasLlmLogs: boolean): ComponentFixture<MessageRow> {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [MessageRow],
      providers: [{ provide: CoreClient, useValue: { dispatch: vi.fn() } }],
    });
    const fixture = TestBed.createComponent(MessageRow);
    fixture.componentRef.setInput('message', msg);
    fixture.componentRef.setInput('chat', chatDetail());
    fixture.componentRef.setInput('hasLlmLogs', hasLlmLogs);
    fixture.detectChanges();
    return fixture;
  }

  function entry(fixture: ComponentFixture<MessageRow>): HTMLButtonElement | null {
    return fixture.nativeElement.querySelector(
      'button[aria-label="View LLM request/response logs"]',
    );
  }

  it('renders the cpu entry for an ASSISTANT message that has logs', () => {
    const fixture = renderWith(message({ role: 'ASSISTANT' }), true);
    expect(entry(fixture)).not.toBeNull();
    expect(entry(fixture)!.querySelector('qt-icon')!.getAttribute('name')).toBe('cpu');
  });

  it('carries MessageActionBar’s copy, not MessageDesktopActions’ (v4 :153 vs :73)', () => {
    // v4 has TWO action bars whose titles for this one button DIFFER. v5 has a
    // single bar and it is MessageActionBar's — same qt-chat-message-action-bar /
    // qt-chat-message-action-icon classes, same hover-reveal placement — so it
    // takes MessageActionBar's string.
    const fixture = renderWith(message({ role: 'ASSISTANT' }), true);
    expect(entry(fixture)!.getAttribute('title')).toBe('View LLM request/response logs');
    expect(entry(fixture)!.className).toBe('qt-chat-message-action-icon');
  });

  it('omits the entry when the message has no logs (v4 hasLLMLogs gate)', () => {
    expect(entry(renderWith(message({ role: 'ASSISTANT' }), false))).toBeNull();
  });

  it('omits the entry on a USER message even when the id somehow has logs (v4 role gate)', () => {
    // Only an assistant turn has a request/response pair to inspect.
    expect(entry(renderWith(message({ role: 'USER' }), true))).toBeNull();
  });

  it('emits the message id when clicked (v4 onViewLLMLogs(message.id))', () => {
    const fixture = renderWith(message({ id: 'm-42', role: 'ASSISTANT' }), true);
    const seen: string[] = [];
    fixture.componentInstance.viewLlmLogs.subscribe((id) => seen.push(id));
    entry(fixture)!.click();
    expect(seen).toEqual(['m-42']);
  });

  it('keeps the sibling action icons alongside it', () => {
    const fixture = renderWith(message({ role: 'ASSISTANT' }), true);
    expect(fixture.nativeElement.querySelector('button[aria-label="Copy message"]')).not.toBeNull();
    expect(
      fixture.nativeElement.querySelector('button[aria-label="Delete message"]'),
    ).not.toBeNull();
    expect(
      fixture.nativeElement.querySelector('button[aria-label="Regenerate response"]'),
    ).not.toBeNull();
  });
});

describe('MessageRow — whisper label (P4.17 unit 3)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('names the whisper targets (v4 "whispered to <names>", MessageRow.tsx:321-327)', () => {
    const fixture = render(message({ targetParticipantIds: ['p1'] }));
    const label = fixture.nativeElement.querySelector('.qt-chat-whisper-label');
    expect(label).not.toBeNull();
    expect(label.textContent.trim()).toBe('whispered to Lorian');
  });

  it('falls back to "unknown" for an unresolved target id', () => {
    const fixture = render(message({ targetParticipantIds: ['p1', 'ghost'] }));
    const label = fixture.nativeElement.querySelector('.qt-chat-whisper-label');
    expect(label.textContent.trim()).toBe('whispered to Lorian, unknown');
  });

  it('resolves the operator\'s own userId to "you" (Bug 30)', () => {
    // A private user-initiated run whispers to the operator's userId (chat.user.id
    // = 'user1'), which is never a participant id — it read "unknown" before.
    const fixture = render(message({ targetParticipantIds: ['user1'] }));
    const label = fixture.nativeElement.querySelector('.qt-chat-whisper-label');
    expect(label.textContent.trim()).toBe('whispered to you');
  });

  /** P4.D38 tier 2 (v4 `isOverheardWhisper`) — the row only APPLIES the class;
   *  the list computes it (see `message-list.spec.ts`). */
  it('applies the overheard-dim class only when the input is set', () => {
    const fixture = render(message({ targetParticipantIds: ['p1'] }));
    const bubble = () => fixture.nativeElement.querySelector('.qt-chat-message');
    expect(bubble().classList.contains('qt-chat-message-whisper-overheard')).toBe(false);

    fixture.componentRef.setInput('isOverheardWhisper', true);
    fixture.detectChanges();
    expect(bubble().classList.contains('qt-chat-message-whisper-overheard')).toBe(true);
    // The whisper border/label stay — dimming is opacity only, not a
    // replacement for the whisper variant.
    expect(bubble().classList.contains('qt-chat-message-whisper')).toBe(true);
    expect(fixture.nativeElement.querySelector('.qt-chat-whisper-label')).not.toBeNull();
  });
});

/**
 * P4.26 — v4 gives EVERY Staff row that renders in full the expanded
 * systemSender header bar (`MessageRow.tsx:277-294`), not Pascal alone. The
 * three that reach `MessageRow` in v5 are the three `isAnnouncementChip`
 * exempts: a Carina reference answer, a Suparṇā letter, a Pascal roll.
 */
describe('MessageRow — the Staff header bar (P4.26)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('gives a Carina reference answer the bar', () => {
    const fixture = render(
      message({
        systemSender: 'carina',
        systemKind: 'carina-response',
        participantId: null,
        content: 'Eighteen ninety-three.',
        carinaMeta: { answererId: 'char1', question: 'What year is it?' },
      }),
    );
    const bar = fixture.nativeElement.querySelector('.qt-chat-system-bar');
    expect(bar).not.toBeNull();
    // v4 names the DESK in the bar while the portrait beside it is the
    // answerer's own — that resolution is pinned in `chat-view-model.spec`.
    expect(bar.querySelector('.qt-chat-system-bar-sender').textContent.trim()).toBe('Carina');
    expect(bar.querySelector('.qt-chat-system-bar-kind').textContent.trim()).toBe(
      'reference answer',
    );
    // No roll record, so no accent — the bar stays plain (v4).
    expect(bar.classList.contains('qt-pascal-result')).toBe(false);
    // The ordinary author line is replaced by the bar.
    expect(fixture.nativeElement.querySelector('.qt-chat-message-header')).toBeNull();
  });

  it('gives a Suparṇā letter the bar with v4’s mail label', () => {
    const bar = render(
      message({
        systemSender: 'suparna',
        systemKind: 'mail-delivery',
        participantId: null,
        content: 'A letter arrives.',
      }),
    ).nativeElement.querySelector('.qt-chat-system-bar');
    expect(bar.querySelector('.qt-chat-system-bar-sender').textContent.trim()).toBe('Suparṇā');
    expect(bar.querySelector('.qt-chat-system-bar-kind').textContent.trim()).toBe('mail delivery');
  });

  it('leaves an ordinary character line with its author header and no bar', () => {
    const fixture = render(message({ content: 'Good evening.' }));
    expect(fixture.nativeElement.querySelector('.qt-chat-system-bar')).toBeNull();
    expect(fixture.nativeElement.querySelector('.qt-chat-message-header')).not.toBeNull();
  });

  /**
   * P4.D38 (v4 `AnnouncementBarContents`'s `WhisperTag`, `a163862c`) — a
   * targeted Carina answer or Suparṇā letter carries the same compact
   * whisper tag the collapsed chip does, since v4 shares one component
   * between the bar and the chip.
   */
  it('shows the compact whisper tag on a targeted Carina answer', () => {
    const bar = render(
      message({
        systemSender: 'carina',
        systemKind: 'carina-response',
        participantId: null,
        targetParticipantIds: ['p1'],
        content: 'Eighteen ninety-three.',
        carinaMeta: { answererId: 'char1', question: 'What year is it?' },
      }),
    ).nativeElement.querySelector('.qt-chat-system-bar');
    const tag = bar.querySelector('.qt-chat-system-bar-whisper');
    expect(tag.textContent.replace(/\s+/g, ' ').trim()).toBe('whispered to Lorian');
  });

  it('omits the whisper tag on a public Staff row', () => {
    const bar = render(
      message({ systemSender: 'suparna', systemKind: 'mail-delivery', participantId: null }),
    ).nativeElement.querySelector('.qt-chat-system-bar');
    expect(bar.querySelector('.qt-chat-system-bar-whisper')).toBeNull();
  });
});

/**
 * P4.D81 Tier 2 — the client half of v4 `23af7146`'s greeting-reasoning fix.
 * v4's `generateGreetingMessage` read `chunk.content` and nothing else, so a
 * thinking model's reasoning while composing an opening greeting was discarded
 * (one observed greeting cost 3,620 characters and rendered no fold). P4.D79
 * carries `reasoningContent` onto the stored greeting; the rendering side needs
 * nothing new — the fold is the same one every assistant message gets — and
 * this pins that, so the two halves cannot drift apart unnoticed.
 *
 * A greeting IS just the chat's first assistant message: no marker field, no
 * separate kind. The shape below is exactly that.
 */
describe('MessageRow — a greeting that carries reasoning', () => {
  afterEach(() => TestBed.resetTestingModule());

  const greeting = () =>
    message({
      id: 'greeting',
      role: 'ASSISTANT',
      content: 'The parlour door swings wide.',
      reasoningContent: 'They arrive mid-storm; open on the weather.',
      createdAt: '2026-01-01T00:00:00.000Z',
    });

  it('renders the thinking fold from reasoningContent alone', () => {
    const fixture = render(greeting());
    const folds = fixture.nativeElement.querySelectorAll('qt-thinking-block');
    expect(folds.length).toBe(1);
    expect(folds[0].textContent).toContain('They arrive mid-storm; open on the weather.');
    // The reply itself is untouched by the fold.
    expect(fixture.nativeElement.textContent).toContain('The parlour door swings wide.');
  });

  it('renders no fold when the greeting carries no reasoning', () => {
    const fixture = render(message({ id: 'greeting', role: 'ASSISTANT', content: 'Hello.' }));
    expect(fixture.nativeElement.querySelectorAll('qt-thinking-block').length).toBe(0);
  });
});
