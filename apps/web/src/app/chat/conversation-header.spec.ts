import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { afterEach, describe, expect, it } from 'vitest';

import type { ChatDetail } from '../core/core-contract';
import { ConversationHeader } from './conversation-header';

function chatDetail(overrides: Partial<ChatDetail> = {}): ChatDetail {
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
    participants: [],
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
    ...overrides,
  };
}

function render(chat: ChatDetail): ComponentFixture<ConversationHeader> {
  TestBed.configureTestingModule({
    imports: [ConversationHeader],
    providers: [provideRouter([])],
  });
  const fixture = TestBed.createComponent(ConversationHeader);
  fixture.componentRef.setInput('chat', chat);
  fixture.detectChanges();
  return fixture;
}

describe('ConversationHeader — Edit-Enclave gate', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders the Edit-Enclave button for an autonomous room', () => {
    const fixture = render(chatDetail({ chatType: 'autonomous' }));
    const button = fixture.nativeElement.querySelector('button[aria-label="Edit Enclave"]');
    expect(button).not.toBeNull();
    expect(button.getAttribute('title')).toBe(
      'Edit this enclave’s schedule, budget, and visibility',
    );
  });

  it('omits the Edit-Enclave button for a non-autonomous chat', () => {
    const fixture = render(chatDetail({ chatType: 'salon' }));
    expect(fixture.nativeElement.querySelector('button[aria-label="Edit Enclave"]')).toBeNull();
  });

  it('emits editEnclave when the button is clicked', () => {
    const fixture = render(chatDetail({ chatType: 'autonomous' }));
    let fired = false;
    fixture.componentInstance.editEnclave.subscribe(() => (fired = true));
    (
      fixture.nativeElement.querySelector(
        'button[aria-label="Edit Enclave"]',
      ) as HTMLButtonElement
    ).click();
    expect(fired).toBe(true);
  });
});
