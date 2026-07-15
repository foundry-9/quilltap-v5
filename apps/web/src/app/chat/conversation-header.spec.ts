import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { ChatDetail, ChatSettingsDto } from '../core/core-contract';
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

describe('ConversationHeader — the chat-totals summary gate (v4 SalonView.tsx:990-1027)', () => {
  afterEach(() => TestBed.resetTestingModule());

  function settingsRow(showChatTotals: boolean): ChatSettingsDto {
    return {
      avatarDisplayMode: 'ALWAYS',
      avatarDisplayStyle: 'CIRCULAR',
      tokenDisplaySettings: {
        showPerMessageTokens: false,
        showPerMessageCost: false,
        showChatTotals,
        showSystemEvents: false,
      },
    } as ChatSettingsDto;
  }

  function renderWith(settings: ChatSettingsDto | null): ComponentFixture<ConversationHeader> {
    TestBed.configureTestingModule({
      imports: [ConversationHeader],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        // The summary only mounts when the gate opens; when it does, it needs a
        // CoreClient. A never-resolving fetch is enough — this asserts the GATE,
        // not the summary's own rendering (chat-cost-summary.spec covers that).
        { provide: CoreClient, useValue: { dispatchData: vi.fn(() => new Promise(() => {})) } },
      ],
    });
    const fixture = TestBed.createComponent(ConversationHeader);
    fixture.componentRef.setInput('chat', chatDetail());
    fixture.componentRef.setInput('settings', settings);
    fixture.componentRef.setInput('messageCount', 7);
    fixture.detectChanges();
    return fixture;
  }

  it('mounts the summary when showChatTotals is on', () => {
    const fixture = renderWith(settingsRow(true));
    expect(fixture.nativeElement.querySelector('qt-chat-cost-summary')).not.toBeNull();
  });

  it('omits the summary when showChatTotals is off', () => {
    const fixture = renderWith(settingsRow(false));
    expect(fixture.nativeElement.querySelector('qt-chat-cost-summary')).toBeNull();
  });

  it('omits the summary when settings have not loaded (v4 default false)', () => {
    const fixture = renderWith(null);
    expect(fixture.nativeElement.querySelector('qt-chat-cost-summary')).toBeNull();
  });

  it('keeps the gallery and copy-id entries alongside the summary', () => {
    // The summary joins the right cluster; it must not displace what was there.
    const fixture = renderWith(settingsRow(true));
    expect(fixture.nativeElement.querySelector('button[aria-label="View chat photos"]')).not.toBeNull();
    expect(fixture.nativeElement.querySelector('qt-copy-chat-id-button')).not.toBeNull();
  });
});
