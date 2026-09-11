import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ActivatedRoute, convertToParamMap, provideRouter } from '@angular/router';
import {
  QueryClient,
  injectQueryClient,
  provideTanStackQuery,
} from '@tanstack/angular-query-experimental';
import { signal } from '@angular/core';
import { Subject, of } from 'rxjs';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';

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
import { chatSettingsKeys, updateChatSettings } from '../settings/chat/chat-settings.api';
import { SalonConversation } from './salon-conversation';

/**
 * **Bug 134 (v4 `f4ad2c8d1`), measured then pinned.**
 *
 * v4's Salon read its settings from `useChatData`, which fetched
 * `/api/v1/settings/chat` ONCE from the mount effect into `useState`. The tabbed
 * workspace renders every tab at once and hides the inactive ones with
 * `display: none`, so a backgrounded Salon never unmounted, never re-ran the
 * effect, and never saw a dial flipped in the Settings tab. v4 fixed it by moving
 * all nine reads onto `useChatSettingsQuery` and deleting the mount-only fetch.
 *
 * **v5 never had that mechanism** — its Salon has read through the
 * `['chatSettings']` TanStack query since P4.6f — so the port is not the
 * deletion; it is this proof, that every derived read follows the cache LIVE, and
 * the write side (the memory-cascade preference) that v4 fixed in the same
 * commit.
 *
 * v4 pins the same facts with a `readFileSync` source scan
 * (`useImpersonationVoice.test.ts:33-52`). A vitest spec cannot read source
 * (`spa-spec-cannot-read-source`), so this asserts the BEHAVIOUR that scan was
 * standing in for: flip the cache, and the derived reads move — with the Salon
 * never remounted and its chat query never refetched.
 */

beforeAll(() => {
  const proto = globalThis.HTMLElement?.prototype;
  if (proto && !('__qtSizeStubbed' in proto)) {
    Object.defineProperty(proto, '__qtSizeStubbed', { value: true });
    Object.defineProperty(proto, 'offsetHeight', { configurable: true, get: () => 800 });
    Object.defineProperty(proto, 'offsetWidth', { configurable: true, get: () => 800 });
  }
});

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
      participant({ id: 'p1' }),
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
    activeTypingParticipantId: 'p1',
    impersonatingParticipantIds: ['p1'],
  } as ChatDetail;
}

/** Every derived settings read the Salon exposes, by the name it exposes it as. */
interface SettingsHost {
  storyBackgroundsEnabled(): boolean;
  textReplacementsEnabled(): boolean;
  composerSpellcheck(): boolean;
  llmLoggingEnabled(): boolean;
  impersonationVoiceArmed(): boolean;
}

interface Rig {
  fixture: ComponentFixture<SalonConversation>;
  host: SettingsHost;
  client: QueryClient;
  /** Every request the Salon dispatched, by type, in order. */
  types: string[];
  core: CoreClient;
}

async function rig(seed: Partial<ChatSettingsDto> = {}): Promise<Rig> {
  const chat = chatDetail();
  const types: string[] = [];
  let current: ChatSettingsDto = {
    avatarDisplayMode: 'ALWAYS',
    avatarDisplayStyle: 'CIRCULAR',
    ...seed,
  } as ChatSettingsDto;

  const dispatch = vi.fn(async (req: CoreRequest): Promise<CoreResponse> => {
    types.push(req.type);
    if (req.type === 'chatGet') return { type: 'chat', data: { chat } };
    if (req.type === 'chatSettings') return { type: 'chatSettings', data: current };
    if (req.type === 'chatSettingsUpdate') {
      // `failUpdate` is a seed-only test flag, never a real settings key.
      if (current['failUpdate']) throw new Error('boom');
      current = { ...current, ...(req.settings as Record<string, unknown>) } as ChatSettingsDto;
      return { type: 'chatSettings', data: current };
    }
    return { type: 'ack', data: {} };
  });

  const client: Partial<CoreClient> = {
    events$: new Subject<ScopedEvent>().asObservable(),
    connection: signal<ConnectionState>('idle'),
    resyncCounter: signal(0),
    dispatch,
    dispatchData: (async () => ({
      backgroundUrl: null,
      fileId: null,
      filename: null,
      sha256: null,
      linkSummary: null,
    })) as unknown as CoreClient['dispatchData'],
    dispatchExpect: (async (req: CoreRequest, expected: string) => {
      const resp = await dispatch(req);
      if (resp.type !== expected) throw new Error(`unexpected ${resp.type}`);
      return resp;
    }) as CoreClient['dispatchExpect'],
  };

  const queryClient = new QueryClient();
  localStorage.setItem('quilltap.chat-sidebar.collapsed', 'false');
  TestBed.configureTestingModule({
    imports: [SalonConversation],
    providers: [
      provideRouter([]),
      provideTanStackQuery(queryClient),
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
    host: fixture.componentInstance as unknown as SettingsHost,
    client: queryClient,
    types,
    core: client as CoreClient,
  };
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

afterEach(() => {
  vi.unstubAllGlobals();
  TestBed.resetTestingModule();
  localStorage.clear();
});

describe('SalonConversation — every chat setting it reads is LIVE (v4 bug 134)', () => {
  it('every derived read follows a cache flip, with no remount and no chat refetch', async () => {
    const r = await rig({
      llmLoggingSettings: { enabled: true },
      composerSpellcheck: true,
      textReplacementsEnabled: true,
      storyBackgroundsSettings: { enabled: false },
      impersonationVoiceRewrite: false,
    } as Partial<ChatSettingsDto>);

    expect(r.host.llmLoggingEnabled()).toBe(true);
    expect(r.host.composerSpellcheck()).toBe(true);
    expect(r.host.textReplacementsEnabled()).toBe(true);
    expect(r.host.storyBackgroundsEnabled()).toBe(false);
    expect(r.host.impersonationVoiceArmed()).toBe(false);

    const chatGetsBefore = r.types.filter((t) => t === 'chatGet').length;

    // What a save in ANOTHER tab does to this one: the shared key is seeded, and
    // every mounted observer — including a tab hidden behind `display: none` —
    // is told. v4's pre-fix Salon saw none of this.
    r.client.setQueryData(chatSettingsKeys.all, {
      avatarDisplayMode: 'ALWAYS',
      avatarDisplayStyle: 'CIRCULAR',
      llmLoggingSettings: { enabled: false },
      composerSpellcheck: false,
      textReplacementsEnabled: false,
      storyBackgroundsSettings: { enabled: true },
      impersonationVoiceRewrite: true,
    } as ChatSettingsDto);
    await settle(r.fixture);

    expect(r.host.llmLoggingEnabled()).toBe(false);
    expect(r.host.composerSpellcheck()).toBe(false);
    expect(r.host.textReplacementsEnabled()).toBe(false);
    expect(r.host.storyBackgroundsEnabled()).toBe(true);
    expect(r.host.impersonationVoiceArmed()).toBe(true);

    // The chat itself was never refetched: a live setting must not disturb a
    // stream (v4's own verification note).
    expect(r.types.filter((t) => t === 'chatGet').length).toBe(chatGetsBefore);
  });

  it('the inspector button appears and disappears with the live flag', async () => {
    const r = await rig({ llmLoggingSettings: { enabled: false } } as Partial<ChatSettingsDto>);
    const inspector = () =>
      (r.fixture.nativeElement as HTMLElement).querySelector('[aria-label="Toggle LLM Inspector"]');
    expect(inspector()).toBeNull();

    r.client.setQueryData(chatSettingsKeys.all, {
      avatarDisplayMode: 'ALWAYS',
      avatarDisplayStyle: 'CIRCULAR',
      llmLoggingSettings: { enabled: true },
    } as ChatSettingsDto);
    await settle(r.fixture);
    expect(inspector()).not.toBeNull();
  });

  it('a settings-card SAVE reaches the open Salon (the write side)', async () => {
    const r = await rig({ impersonationVoiceRewrite: false } as Partial<ChatSettingsDto>);
    expect(r.host.impersonationVoiceArmed()).toBe(false);

    // Exactly what `ChatSettingsCard.save` does: PUT, then seed the shared key
    // from the echo.
    const updated = await updateChatSettings(r.core, { impersonationVoiceRewrite: true });
    r.client.setQueryData(chatSettingsKeys.all, updated);
    await settle(r.fixture);

    expect(r.host.impersonationVoiceArmed()).toBe(true);
  });

  it('the Salon reads the SHARED key, not a second spelling of it', async () => {
    // Two literals would be two caches, and the publish would reach neither the
    // Salon nor the card. Proven by flipping through the constant only.
    const r = await rig({ composerSpellcheck: true } as Partial<ChatSettingsDto>);
    r.client.setQueryData(chatSettingsKeys.all, {
      avatarDisplayMode: 'ALWAYS',
      avatarDisplayStyle: 'CIRCULAR',
      composerSpellcheck: false,
    } as ChatSettingsDto);
    await settle(r.fixture);
    expect(r.host.composerSpellcheck()).toBe(false);
  });
});

describe('SalonConversation — the memory-cascade remembered choice (v4 bug 134, write side)', () => {
  interface CascadeHost {
    cascade: {
      set(v: { messageId: string; memoryCount: number; isSwipeGroup: boolean } | null): void;
    };
    onCascadeConfirm(choice: { action: string; remember: boolean }): Promise<void>;
  }

  it('writes the preference AND publishes it, so the next dialog sees it', async () => {
    const r = await rig({
      memoryCascadePreferences: {
        onMessageDelete: 'ASK_EVERY_TIME',
        onSwipeRegenerate: 'KEEP_MEMORIES',
      },
    } as Partial<ChatSettingsDto>);
    const host = r.fixture.componentInstance as unknown as CascadeHost;
    host.cascade.set({ messageId: 'm-1', memoryCount: 2, isSwipeGroup: false });
    await settle(r.fixture);

    const getsBefore = r.types.filter((t) => t === 'chatSettings').length;
    await host.onCascadeConfirm({ action: 'KEEP_MEMORIES', remember: true });
    await settle(r.fixture);

    // The PUT carries the WHOLE bag with the sibling key preserved (v4 spreads
    // `onSwipeRegenerate` through, because the server replaces the column).
    const put = (r.core.dispatch as unknown as { mock: { calls: [CoreRequest][] } }).mock.calls
      .map((c) => c[0])
      .find((req) => req.type === 'chatSettingsUpdate') as
      { settings: Record<string, unknown> } | undefined;
    expect(put?.settings).toEqual({
      memoryCascadePreferences: {
        onMessageDelete: 'KEEP_MEMORIES',
        onSwipeRegenerate: 'KEEP_MEMORIES',
      },
    });

    // …and the shared key was INVALIDATED, which is the half that publishes it:
    // without it the PUT lands and every mounted observer keeps the stale row —
    // bug 134's write side exactly. The discriminator is the refetch the
    // invalidation forces on this active observer; reading the cached data back
    // proves nothing, because the seed already looks like the answer (that
    // vacuity is what the M19 mutation caught).
    expect(r.types.filter((t) => t === 'chatSettings').length).toBeGreaterThan(getsBefore);
    expect(
      (r.client.getQueryData(chatSettingsKeys.all) as ChatSettingsDto | undefined)?.[
        'memoryCascadePreferences'
      ],
    ).toEqual({ onMessageDelete: 'KEEP_MEMORIES', onSwipeRegenerate: 'KEEP_MEMORIES' });
  });

  it('defaults the sibling key when the stored bag has none (v4 `|| DELETE_MEMORIES`)', async () => {
    const r = await rig();
    const host = r.fixture.componentInstance as unknown as CascadeHost;
    host.cascade.set({ messageId: 'm-1', memoryCount: 1, isSwipeGroup: false });
    await settle(r.fixture);
    await host.onCascadeConfirm({ action: 'REGENERATE_MEMORIES', remember: true });
    await settle(r.fixture);

    const put = (r.core.dispatch as unknown as { mock: { calls: [CoreRequest][] } }).mock.calls
      .map((c) => c[0])
      .find((req) => req.type === 'chatSettingsUpdate') as
      { settings: Record<string, unknown> } | undefined;
    expect(put?.settings).toEqual({
      memoryCascadePreferences: {
        onMessageDelete: 'REGENERATE_MEMORIES',
        onSwipeRegenerate: 'DELETE_MEMORIES',
      },
    });
  });

  it('writes nothing when the box is left unticked', async () => {
    const r = await rig();
    const host = r.fixture.componentInstance as unknown as CascadeHost;
    host.cascade.set({ messageId: 'm-1', memoryCount: 1, isSwipeGroup: false });
    await settle(r.fixture);
    await host.onCascadeConfirm({ action: 'KEEP_MEMORIES', remember: false });
    await settle(r.fixture);
    expect(r.types).not.toContain('chatSettingsUpdate');
    // …but the delete still went out.
    expect(r.types).toContain('messageDelete');
  });

  it('a failed preference write never costs the delete (v4 swallows it)', async () => {
    const r = await rig({ failUpdate: true } as Partial<ChatSettingsDto>);
    const host = r.fixture.componentInstance as unknown as CascadeHost;
    host.cascade.set({ messageId: 'm-1', memoryCount: 1, isSwipeGroup: false });
    await settle(r.fixture);
    await host.onCascadeConfirm({ action: 'KEEP_MEMORIES', remember: true });
    await settle(r.fixture);
    expect(r.types).toContain('chatSettingsUpdate');
    expect(r.types).toContain('messageDelete');
  });
});
