import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ActivatedRoute, convertToParamMap, provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { signal } from '@angular/core';
import { By } from '@angular/platform-browser';
import { Subject, of } from 'rxjs';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';

import { ChatComposer } from '../../chat/chat-composer';
import { ImpersonationVoiceState } from '../../chat/impersonation-voice/impersonation-voice.state';
import { CoreClient } from '../../core/core-client';
import type {
  ChatDetail,
  ChatSettingsDto,
  CoreRequest,
  CoreResponse,
  ParticipantDetail,
  ScopedEvent,
} from '../../core/core-contract';
import type { ConnectionState } from '../../core/core-transport';
import { SalonConversation } from './salon-conversation';

/**
 * In Their Own Words at the SALON level (P4.D181 unit 3) — the intercept sitting
 * at the top of `send()`, and the invariant it exists for: an intercepted submit
 * clears NOTHING and sends NOTHING, so "Edit original" and "Cancel" have a draft
 * and a tray to come back to. Both of the dialog's Send doors then post through
 * the SAME path the direct submit uses, so all three leave the composer in the
 * same state.
 *
 * v4's equivalent lives in `SalonView.tsx:1615-1642` (the `onSubmit` that returns
 * when `intercept` answers true) and in the fact that v4's composer clears
 * nothing at all — only `sendMessage` does.
 */

beforeAll(() => {
  const proto = globalThis.HTMLElement?.prototype;
  if (proto && !('__qtSizeStubbed' in proto)) {
    Object.defineProperty(proto, '__qtSizeStubbed', { value: true });
    Object.defineProperty(proto, 'offsetHeight', { configurable: true, get: () => 800 });
    Object.defineProperty(proto, 'offsetWidth', { configurable: true, get: () => 800 });
  }
});

const SEAT = 'p1';

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
      title: 'The Butler',
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

function chatDetail(): ChatDetail {
  return {
    id: 'chat-1',
    title: 'Tea Time',
    contextSummary: null,
    roleplayTemplateId: null,
    chatType: 'salon',
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    isPaused: false,
    isManuallyRenamed: false,
    participants: [
      participant({
        id: 'pu',
        controlledBy: 'user',
        character: {
          id: 'u',
          name: 'Bertie',
          title: null,
          avatarUrl: null,
          defaultImageId: null,
          defaultImage: null,
        },
      }),
      participant({ id: SEAT }),
    ],
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
    // The human is wearing Friday's seat (the Bug 44 overlay).
    activeTypingParticipantId: SEAT,
    impersonatingParticipantIds: [SEAT],
  } as ChatDetail;
}

interface Rig {
  fixture: ComponentFixture<SalonConversation>;
  voice: ImpersonationVoiceState;
  composer: ChatComposer;
  /** Every `chatSend` the Salon dispatched, in order. */
  sends: Record<string, unknown>[];
  previews: Record<string, unknown>[];
}

async function rig(
  settings: Partial<ChatSettingsDto> = { impersonationVoiceRewrite: true },
): Promise<Rig> {
  const sends: Record<string, unknown>[] = [];
  const previews: Record<string, unknown>[] = [];
  const chat = chatDetail();

  const dispatch = vi.fn(async (req: CoreRequest): Promise<CoreResponse> => {
    if (req.type === 'chatGet') return { type: 'chat', data: { chat } };
    if (req.type === 'chatSettings') {
      return {
        type: 'chatSettings',
        data: {
          avatarDisplayMode: 'ALWAYS',
          avatarDisplayStyle: 'CIRCULAR',
          ...settings,
        } as ChatSettingsDto,
      };
    }
    if (req.type === 'chatSend') {
      sends.push(req as unknown as Record<string, unknown>);
      return { type: 'ack', data: {} };
    }
    return { type: 'ack', data: {} };
  });

  const dispatchData = vi.fn(async (req: Record<string, unknown>) => {
    if (req['type'] === 'chatImpersonationVoicePreview') {
      previews.push(req);
      return {
        success: true,
        proposedMarkdown: 'Very good, sir. I shall see to it.',
        profileName: 'The butler’s desk',
        modelName: 'gpt-butler',
      };
    }
    return { backgroundUrl: null, fileId: null, filename: null, sha256: null, linkSummary: null };
  });

  const client: Partial<CoreClient> = {
    events$: new Subject<ScopedEvent>().asObservable(),
    connection: signal<ConnectionState>('idle'),
    resyncCounter: signal(0),
    dispatch,
    dispatchData: dispatchData as unknown as CoreClient['dispatchData'],
    dispatchExpect: (async (req: CoreRequest, expected: string) => {
      const resp = await dispatch(req);
      if (resp.type !== expected) throw new Error(`unexpected ${resp.type}`);
      return resp;
    }) as CoreClient['dispatchExpect'],
  };

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
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return {
    fixture,
    voice: fixture.debugElement.injector.get(ImpersonationVoiceState),
    composer: fixture.debugElement.query(By.directive(ChatComposer))
      .componentInstance as ChatComposer,
    sends,
    previews,
  };
}

async function settle(fixture: ComponentFixture<SalonConversation>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

/** Type the line into the composer the way the editor would, then submit. */
function typeAndSubmit(r: Rig, text: string): void {
  const composer = r.composer as unknown as { text: { set(v: string): void } };
  composer.text.set(text);
  r.composer.send.emit({ content: text, fileIds: [] });
}

afterEach(() => {
  vi.unstubAllGlobals();
  TestBed.resetTestingModule();
  localStorage.clear();
});

describe('SalonConversation — the In Their Own Words intercept', () => {
  it('takes the submit over: nothing is sent, and the dialog opens on the draft', async () => {
    const r = await rig();
    typeAndSubmit(r, 'Tell him I accept.');
    await settle(r.fixture);

    expect(r.sends).toEqual([]);
    expect(r.voice.isOpen()).toBe(true);
    expect(r.voice.seed()).toBe('Tell him I accept.');
    expect(r.previews).toHaveLength(1);
    expect(r.previews[0]['participantId']).toBe(SEAT);
  });

  it('an intercepted submit does NOT clear the composer', async () => {
    const r = await rig();
    const cleared = vi.spyOn(r.composer, 'clearAfterSend');
    typeAndSubmit(r, 'Tell him I accept.');
    await settle(r.fixture);
    expect(cleared).not.toHaveBeenCalled();
  });

  it('an intercepted submit keeps the pending tool results in state', async () => {
    const r = await rig();
    const host = r.fixture.componentInstance as unknown as {
      pendingToolResults: { set(v: unknown[]): void; (): unknown[] };
    };
    host.pendingToolResults.set([{ id: 'roll-1', label: '1d20', value: '17' }]);
    typeAndSubmit(r, 'I roll and say so.');
    await settle(r.fixture);
    expect(host.pendingToolResults()).toHaveLength(1);
  });

  it('the setting OFF lets the submit through untouched', async () => {
    const r = await rig({ impersonationVoiceRewrite: false });
    const cleared = vi.spyOn(r.composer, 'clearAfterSend');
    typeAndSubmit(r, 'Tell him I accept.');
    await settle(r.fixture);

    expect(r.voice.isOpen()).toBe(false);
    expect(r.previews).toEqual([]);
    expect(r.sends).toHaveLength(1);
    expect(cleared).toHaveBeenCalledTimes(1);
  });

  it('an UNSET setting is off (v4 `?? false`)', async () => {
    const r = await rig({});
    typeAndSubmit(r, 'Tell him I accept.');
    await settle(r.fixture);
    expect(r.voice.isOpen()).toBe(false);
    expect(r.sends).toHaveLength(1);
  });

  it('Send posts the PROPOSAL through the same path and clears the composer', async () => {
    const r = await rig();
    const cleared = vi.spyOn(r.composer, 'clearAfterSend');
    typeAndSubmit(r, 'Tell him I accept.');
    await settle(r.fixture);

    r.voice.send(r.voice.proposal());
    await settle(r.fixture);

    expect(r.sends).toHaveLength(1);
    expect(r.sends[0]['content']).toBe('Very good, sir. I shall see to it.');
    expect(cleared).toHaveBeenCalledTimes(1);
    expect(r.voice.isOpen()).toBe(false);
  });

  it('Send as written posts the operator’s OWN bytes', async () => {
    const r = await rig();
    typeAndSubmit(r, 'Tell him I accept.');
    await settle(r.fixture);

    r.voice.sendAsWritten();
    await settle(r.fixture);
    expect(r.sends[0]['content']).toBe('Tell him I accept.');
  });

  it('Edit original sends nothing and never clears', async () => {
    const r = await rig();
    const cleared = vi.spyOn(r.composer, 'clearAfterSend');
    typeAndSubmit(r, 'Tell him I accept.');
    await settle(r.fixture);

    r.voice.editOriginal();
    await settle(r.fixture);
    expect(r.sends).toEqual([]);
    expect(cleared).not.toHaveBeenCalled();
    expect(r.voice.isOpen()).toBe(false);
  });

  it('a dialog Send carries the stashed attachments and the stashed rolls', async () => {
    const r = await rig();
    const host = r.fixture.componentInstance as unknown as {
      pendingToolResults: { set(v: unknown[]): void; (): unknown[] };
    };
    host.pendingToolResults.set([{ id: 'roll-1', label: '1d20', value: '17' }]);
    const composer = r.composer as unknown as { text: { set(v: string): void } };
    composer.text.set('I roll and say so.');
    r.composer.send.emit({ content: 'I roll and say so.', fileIds: ['f-9'] });
    await settle(r.fixture);

    r.voice.send('A seventeen, sir.');
    await settle(r.fixture);

    expect(r.sends).toHaveLength(1);
    expect(r.sends[0]['fileIds']).toEqual(['f-9']);
    // The rolls rode the send, and the Salon's own state is empty again.
    expect(host.pendingToolResults()).toEqual([]);
  });

  it('the armed cue follows the setting, the seat and the overlay', async () => {
    const r = await rig();
    const host = r.fixture.componentInstance as unknown as {
      impersonationVoiceArmed(): boolean;
      impersonatingLocal: { set(v: string[]): void };
    };
    expect(host.impersonationVoiceArmed()).toBe(true);
    // Drop the seat out of the overlay and the cue goes dark, with no text
    // anywhere in the decision.
    host.impersonatingLocal.set([]);
    r.fixture.detectChanges();
    expect(host.impersonationVoiceArmed()).toBe(false);
  });
});
