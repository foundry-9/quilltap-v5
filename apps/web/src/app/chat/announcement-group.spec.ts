import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import type { ChatDetail, MessageDto, PascalMeta } from '../core/core-contract';
import { AnnouncementGroup } from './announcement-group';
import type { AnnouncementChip } from './chat-view-model';

/**
 * The collapsed-chip row. v5 renders Pascal's roll outcomes as their own full
 * row (`chat-view-model`'s `isAnnouncementChip` carves them out), so the accent
 * here is DEFENSIVE — exactly as it is in v4, which added it "defensively, a
 * collapsed chip". It is asserted anyway: the day a Pascal announcement does
 * collapse, it must not wear the red dot a deleted file gets.
 */
function message(over: Partial<MessageDto> = {}): MessageDto {
  return {
    id: 'm1',
    role: 'SYSTEM',
    content: 'The lock gives way.',
    tokenCount: null,
    promptTokens: null,
    completionTokens: null,
    createdAt: '2026-07-25T12:00:00.000Z',
    swipeGroupId: null,
    swipeIndex: null,
    participantId: null,
    attachments: [],
    provider: null,
    modelName: null,
    targetParticipantIds: null,
    isSilentMessage: null,
    systemSender: 'host',
    systemKind: 'add',
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

const rollMeta = (state: PascalMeta['state']): PascalMeta => ({
  tool: 'pick_lock',
  toolTitle: 'Pick Lock',
  definitionTier: 'global',
  definitionMountId: 'm1',
  params: {},
  rollForm: 'range',
  raw: 12,
  value: 12,
  state,
  outcomeIndex: 0,
  invokedBy: 'user',
});

function chip(over: Partial<AnnouncementChip> = {}): AnnouncementChip {
  return {
    id: 'm1',
    sender: 'The Host',
    kind: 'add',
    importance: 'high',
    createdAt: '2026-07-25T12:00:00.000Z',
    message: message(),
    ...over,
  };
}

function render(
  chips: AnnouncementChip[],
  chat: ChatDetail = {} as ChatDetail,
): ComponentFixture<AnnouncementGroup> {
  TestBed.configureTestingModule({ imports: [AnnouncementGroup] });
  const fixture = TestBed.createComponent(AnnouncementGroup);
  fixture.componentRef.setInput('chips', chips);
  fixture.componentRef.setInput('chatId', 'chat-1');
  // Read only by an expanded TOOL chip's card, and by the whisper-tag name
  // lookup; neither runs unless a test needs it.
  fixture.componentRef.setInput('chat', chat);
  fixture.detectChanges();
  return fixture;
}

describe('AnnouncementGroup — Pascal outcome accent (P4.d21)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('gives an ordinary Staff chip the importance dot and no accent', () => {
    const button = render([chip()]).nativeElement.querySelector('.qt-chat-announcement-chip');
    expect(button.classList.contains('qt-pascal-result')).toBe(false);
    expect(
      button
        .querySelector('.qt-chat-announcement-dot')
        .classList.contains('qt-chat-announcement-dot-high'),
    ).toBe(true);
    expect(button.querySelector('.sr-only')).toBeNull();
  });

  it('gives a roll chip the outcome dot, the accent, and the visually-hidden state', () => {
    const button = render([
      chip({
        sender: 'Pascal',
        kind: 'Pick Lock',
        message: message({
          systemSender: 'pascal',
          systemKind: 'custom-tool-result',
          pascalMeta: rollMeta('partial'),
        }),
      }),
    ]).nativeElement.querySelector('.qt-chat-announcement-chip');

    expect(button.classList.contains('qt-pascal-result')).toBe(true);
    expect(button.classList.contains('qt-pascal-result--partial')).toBe(true);
    const dot = button.querySelector('.qt-chat-announcement-dot');
    expect(dot.classList.contains('qt-chat-announcement-dot-outcome-partial')).toBe(true);
    expect(dot.classList.contains('qt-chat-announcement-dot-high')).toBe(false);
    expect(button.querySelector('.sr-only').textContent.trim()).toBe('partial');
  });

  it("leaves Prospero's custom-tool-error chip alone — it carries no roll record", () => {
    const button = render([
      chip({
        sender: 'Prospero',
        kind: "the table couldn't deal",
        importance: 'low',
        message: message({ systemSender: 'prospero', systemKind: 'custom-tool-error' }),
      }),
    ]).nativeElement.querySelector('.qt-chat-announcement-chip');

    expect(button.classList.contains('qt-pascal-result')).toBe(false);
    expect(
      button
        .querySelector('.qt-chat-announcement-dot')
        .classList.contains('qt-chat-announcement-dot-low'),
    ).toBe(true);
  });

  it('shows a collapsed chip a right-chevron, and swaps it on expand (v4 AnnouncementChip)', () => {
    const fixture = render([chip()]);
    const button = fixture.nativeElement.querySelector('.qt-chat-announcement-chip');

    // v4 renders AnnouncementChip with no `expanded` prop, so a chip in a group
    // always points right until the reader opens it.
    expect(button.querySelector('qt-icon [data-icon]').getAttribute('data-icon')).toBe(
      'chevron-right',
    );
    expect(button.getAttribute('aria-expanded')).toBe('false');
    expect(button.getAttribute('aria-label')).toBe('Expand The Host add message');

    button.click();
    fixture.detectChanges();

    const chevron = button.querySelector('qt-icon [data-icon]');
    expect(chevron.getAttribute('data-icon')).toBe('chevron-down');
    expect(chevron.classList.contains('qt-chat-system-bar-chevron-down')).toBe(true);
    expect(button.getAttribute('aria-expanded')).toBe('true');
    expect(button.getAttribute('aria-label')).toBe('Collapse The Host add message');
  });

  it('carries the message identity v4 keeps for deep-link scroll-to-message', () => {
    const button = render([chip({ id: 'abc123' })]).nativeElement.querySelector(
      '.qt-chat-announcement-chip',
    );

    // v4's comment on AnnouncementChip: the id/data-message-id are kept "so
    // deep-link and delete-next-focus scroll-to-message still resolve".
    expect(button.getAttribute('id')).toBe('message-abc123');
    expect(button.getAttribute('data-message-id')).toBe('abc123');
  });
});

/**
 * P4.26 — the bar contents and the expanded body, adjudicated against v4
 * `AnnouncementBarContents` + `MessageRow`'s expanded block (`ff12f491`).
 */
describe('AnnouncementGroup — v4 bar/body parity (P4.26)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('omits the kind span entirely when the row resolves to no kind', () => {
    // v4 renders `{kindLabel && <span …>}`. An always-present span spends the
    // chip's 0.5rem flex gap on nothing, so the sender drifts off its timestamp.
    const button = render([chip({ kind: '' })]).nativeElement.querySelector(
      '.qt-chat-announcement-chip',
    );
    expect(button.querySelector('.qt-chat-system-bar-kind')).toBeNull();
    expect(button.querySelector('.qt-chat-system-bar-sender')).not.toBeNull();
    expect(button.querySelector('.qt-chat-system-bar-time')).not.toBeNull();
  });

  it('hides the importance dot from assistive tech, as v4 does', () => {
    const dot = render([chip()]).nativeElement.querySelector('.qt-chat-announcement-dot');
    expect(dot.getAttribute('aria-hidden')).toBe('true');
  });

  it('carries no tooltip — v4’s chip has none', () => {
    const button = render([chip()]).nativeElement.querySelector('.qt-chat-announcement-chip');
    expect(button.getAttribute('title')).toBeNull();
  });

  it('renders an expanded body as an ordinary assistant bubble, never the system slab', () => {
    const fixture = render([chip()]);
    fixture.nativeElement.querySelector('.qt-chat-announcement-chip').click();
    fixture.detectChanges();

    const bubble = fixture.nativeElement.querySelector('.qt-chat-message-row .qt-chat-message');
    expect(bubble).not.toBeNull();
    expect(bubble.classList.contains('qt-chat-message-assistant')).toBe(true);
    // THE GUARD: `.qt-chat-message-system` is `text-sm italic text-center py-2`
    // on a muted slab. v4's stylesheet defines it; no v4 markup wears it.
    expect(bubble.classList.contains('qt-chat-message-system')).toBe(false);
    expect(fixture.nativeElement.querySelector('qt-message-content')).not.toBeNull();
  });
});

/** P4.D38 (v4 `WhisperTag`, `a163862c`) — the chip's whisper audience tag. */
describe('AnnouncementGroup — whisper tag', () => {
  afterEach(() => TestBed.resetTestingModule());

  const chatWithParticipants = (): ChatDetail =>
    ({
      participants: [
        { id: 'p-cleo', character: { name: 'Cleo' } },
        { id: 'p-dax', character: { name: 'Dax' } },
      ],
    }) as unknown as ChatDetail;

  it('renders no whisper tag or border class on a public announcement', () => {
    const button = render([chip()], chatWithParticipants()).nativeElement.querySelector(
      '.qt-chat-announcement-chip',
    );
    expect(button.querySelector('.qt-chat-system-bar-whisper')).toBeNull();
    expect(button.classList.contains('qt-chat-announcement-chip-whisper')).toBe(false);
  });

  it('shows "to <name>" for a single-target whisper, and the whisper border class', () => {
    const button = render(
      [chip({ message: message({ targetParticipantIds: ['p-cleo'] }) })],
      chatWithParticipants(),
    ).nativeElement.querySelector('.qt-chat-announcement-chip');
    const tag = button.querySelector('.qt-chat-system-bar-whisper');
    expect(tag.textContent.replace(/\s+/g, ' ').trim()).toBe('whispered to Cleo');
    expect(tag.getAttribute('title')).toBe('Whispered to Cleo');
    expect(button.classList.contains('qt-chat-announcement-chip-whisper')).toBe(true);
  });

  it('comma-joins multiple targets, in targetParticipantIds order', () => {
    const button = render(
      [chip({ message: message({ targetParticipantIds: ['p-dax', 'p-cleo'] }) })],
      chatWithParticipants(),
    ).nativeElement.querySelector('.qt-chat-announcement-chip');
    expect(button.querySelector('.qt-chat-system-bar-whisper').textContent).toContain(
      'to Dax, Cleo',
    );
  });

  it('falls back to "unknown" for a target id absent from the chat roster', () => {
    const button = render(
      [chip({ message: message({ targetParticipantIds: ['p-ghost'] }) })],
      chatWithParticipants(),
    ).nativeElement.querySelector('.qt-chat-announcement-chip');
    expect(button.querySelector('.qt-chat-system-bar-whisper').textContent).toContain(
      'to unknown',
    );
  });

  it('carries an sr-only "whispered" prefix ahead of the visible tag', () => {
    const button = render(
      [chip({ message: message({ targetParticipantIds: ['p-cleo'] }) })],
      chatWithParticipants(),
    ).nativeElement.querySelector('.qt-chat-announcement-chip');
    const srOnly = button.querySelector('.qt-chat-system-bar-whisper .sr-only');
    expect(srOnly.textContent).toBe('whispered ');
  });
});
