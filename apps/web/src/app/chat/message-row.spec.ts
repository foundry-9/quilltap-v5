import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { ChatDetail, MessageAttachment, MessageDto, ParticipantDetail } from '../core/core-contract';
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
