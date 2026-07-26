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

function render(chips: AnnouncementChip[]): ComponentFixture<AnnouncementGroup> {
  TestBed.configureTestingModule({ imports: [AnnouncementGroup] });
  const fixture = TestBed.createComponent(AnnouncementGroup);
  fixture.componentRef.setInput('chips', chips);
  fixture.componentRef.setInput('chatId', 'chat-1');
  // Read only by an expanded TOOL chip's card; nothing here expands.
  fixture.componentRef.setInput('chat', {} as ChatDetail);
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
});
