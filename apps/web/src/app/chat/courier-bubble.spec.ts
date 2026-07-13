import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { CoreResponse, MessageDto, PendingExternalAttachment } from '../core/core-contract';
import { CourierBubble } from './courier-bubble';

/** A pending-courier message in the DTO shape emitted by `project_message`. */
function courierMessage(overrides: Partial<MessageDto> = {}): MessageDto {
  return {
    id: 'm-courier',
    role: 'ASSISTANT',
    content: '',
    tokenCount: null,
    promptTokens: null,
    completionTokens: null,
    createdAt: '2026-07-13T12:00:00.000Z',
    swipeGroupId: null,
    swipeIndex: null,
    participantId: 'p-1',
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
    pendingExternalPrompt: 'DELTA PROMPT BODY',
    pendingExternalPromptFull: null,
    pendingExternalAttachments: null,
    reasoningContent: null,
    reasoningSegments: null,
    ...overrides,
  };
}

interface DispatchLog {
  requests: { type: string; [k: string]: unknown }[];
}

function stubClient(log: DispatchLog, response?: CoreResponse): Partial<CoreClient> {
  return {
    dispatch: (async (req: { type: string; [k: string]: unknown }) => {
      log.requests.push(req);
      return response ?? { type: 'ack', data: {} };
    }) as CoreClient['dispatch'],
  };
}

function render(
  message: MessageDto,
  client: Partial<CoreClient>,
): ComponentFixture<CourierBubble> {
  TestBed.configureTestingModule({
    imports: [CourierBubble],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(CourierBubble);
  fixture.componentRef.setInput('chatId', 'chat-1');
  fixture.componentRef.setInput('message', message);
  fixture.componentRef.setInput('characterName', 'Lorian');
  fixture.detectChanges();
  return fixture;
}

describe('CourierBubble', () => {
  beforeEach(() => {
    // jsdom has no clipboard by default; give copy a stub to resolve against.
    Object.assign(navigator, {
      clipboard: { writeText: vi.fn().mockResolvedValue(undefined) },
    });
  });
  afterEach(() => TestBed.resetTestingModule());

  it('renders the delta prompt and the awaiting-carrier title', () => {
    const fixture = render(courierMessage(), stubClient({ requests: [] }));
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Lorian — awaiting carrier');
    expect(fixture.nativeElement.querySelector('.qt-courier-bubble-prompt-body').textContent).toBe(
      'DELTA PROMPT BODY',
    );
  });

  it('omits the mode badge + toggle when there is no full-context fallback', () => {
    const fixture = render(courierMessage(), stubClient({ requests: [] }));
    expect(fixture.nativeElement.querySelector('.qt-courier-bubble-mode-badge')).toBeNull();
    expect(fixture.nativeElement.textContent).toContain('Copy the bundle below');
  });

  it('toggles between delta and full-context bundles', () => {
    const fixture = render(
      courierMessage({ pendingExternalPromptFull: 'FULL CONTEXT BODY' }),
      stubClient({ requests: [] }),
    );
    expect(fixture.nativeElement.querySelector('.qt-courier-bubble-mode-badge').textContent.trim()).toBe(
      'delta',
    );
    const toggle = fixture.nativeElement.querySelector(
      '.qt-courier-bubble-prompt-toolbar button',
    ) as HTMLButtonElement;
    toggle.click();
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('.qt-courier-bubble-prompt-body').textContent).toBe(
      'FULL CONTEXT BODY',
    );
    expect(fixture.nativeElement.querySelector('.qt-courier-bubble-mode-badge').textContent.trim()).toBe(
      'full context',
    );
  });

  it('renders attachment download links with filename + meta', () => {
    const att: PendingExternalAttachment = {
      fileId: 'f-1',
      filename: 'sketch.png',
      mimeType: 'image/png',
      sizeBytes: 2048,
      downloadUrl: '/api/v1/files/f-1',
    };
    const fixture = render(
      courierMessage({ pendingExternalAttachments: [att] }),
      stubClient({ requests: [] }),
    );
    const link = fixture.nativeElement.querySelector(
      '.qt-courier-bubble-attachments-list a',
    ) as HTMLAnchorElement;
    expect(link.getAttribute('href')).toBe('/api/v1/files/f-1');
    expect(link.getAttribute('download')).toBe('sketch.png');
    expect(link.textContent).toBe('sketch.png');
    expect(fixture.nativeElement.textContent).toContain('image/png, 2.0 KB');
  });

  it('disables submit while the reply is blank (v4 blank-reply guard)', () => {
    const fixture = render(courierMessage(), stubClient({ requests: [] }));
    expect(submitButton(fixture).disabled).toBe(true);
    typeReply(fixture, 'a real reply');
    expect(submitButton(fixture).disabled).toBe(false);
  });

  it('dispatches messageResolveExternalTurn with the trimmed reply and emits settled', async () => {
    const log: DispatchLog = { requests: [] };
    const settled = vi.fn();
    const fixture = render(courierMessage(), stubClient(log));
    fixture.componentInstance.settled.subscribe(settled);
    typeReply(fixture, '  the pasted reply  ');
    submitButton(fixture).click();
    await settle(fixture);
    expect(log.requests[0]).toEqual({
      type: 'messageResolveExternalTurn',
      chatId: 'chat-1',
      messageId: 'm-courier',
      replyContent: 'the pasted reply',
    });
    expect(settled).toHaveBeenCalledWith('m-courier');
  });

  it('dispatches messageCancelExternalTurn on cancel and emits settled', async () => {
    const log: DispatchLog = { requests: [] };
    const settled = vi.fn();
    const fixture = render(courierMessage(), stubClient(log));
    fixture.componentInstance.settled.subscribe(settled);
    cancelButton(fixture).click();
    await settle(fixture);
    expect(log.requests[0]).toEqual({
      type: 'messageCancelExternalTurn',
      chatId: 'chat-1',
      messageId: 'm-courier',
    });
    expect(settled).toHaveBeenCalledWith('m-courier');
  });

  it('surfaces a server error without emitting settled', async () => {
    const log: DispatchLog = { requests: [] };
    const settled = vi.fn();
    const fixture = render(
      courierMessage(),
      stubClient(log, {
        type: 'error',
        data: { kind: 'internal', message: 'boom' },
      } as CoreResponse),
    );
    fixture.componentInstance.settled.subscribe(settled);
    typeReply(fixture, 'reply');
    submitButton(fixture).click();
    await settle(fixture);
    expect(settled).not.toHaveBeenCalled();
    expect(fixture.nativeElement.querySelector('.qt-courier-bubble-error').textContent).toContain(
      'boom',
    );
  });
});

function typeReply(fixture: ComponentFixture<CourierBubble>, value: string): void {
  const textarea = fixture.nativeElement.querySelector('textarea') as HTMLTextAreaElement;
  textarea.value = value;
  textarea.dispatchEvent(new Event('input'));
  fixture.detectChanges();
}

function submitButton(fixture: ComponentFixture<CourierBubble>): HTMLButtonElement {
  return fixture.nativeElement.querySelectorAll('.qt-courier-bubble-actions button')[1];
}

function cancelButton(fixture: ComponentFixture<CourierBubble>): HTMLButtonElement {
  return fixture.nativeElement.querySelectorAll('.qt-courier-bubble-actions button')[0];
}

async function settle(fixture: ComponentFixture<CourierBubble>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}
