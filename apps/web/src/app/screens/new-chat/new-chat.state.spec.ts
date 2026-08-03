import { describe, expect, it } from 'vitest';

import type { CoreClient } from '../../core/core-client';
import type { CharacterListItem } from '../../core/core-contract';
import { NewChatState } from './new-chat.state';
import type { NewChatSelectedCharacter } from './new-chat.types';

function char(id: string, name: string, over: Partial<CharacterListItem> = {}): CharacterListItem {
  return {
    id,
    name,
    title: null,
    description: null,
    defaultImageId: null,
    defaultImage: null,
    isFavorite: false,
    controlledBy: 'llm',
    canBeCarina: false,
    defaultConnectionProfileId: null,
    defaultPartnerId: null,
    defaultPartnerName: null,
    defaultTimestampConfig: null,
    defaultScenarioId: null,
    defaultSystemPromptId: null,
    defaultImageProfileId: null,
    npc: false,
    createdAt: '',
    tags: [],
    updatedAt: '',
    systemPrompts: [],
    scenarios: [],
    _count: { chats: 0 },
    ...over,
  };
}

function selected(
  c: CharacterListItem,
  over: Partial<NewChatSelectedCharacter> = {},
): NewChatSelectedCharacter {
  return {
    character: c,
    connectionProfileId: 'p1',
    selectedSystemPromptId: null,
    controlledBy: 'llm',
    ...over,
  };
}

function fakeToasts(): {
  showSuccess: (m: string) => string;
  showError: (m: string) => string;
  calls: { type: 'success' | 'error'; message: string }[];
} {
  const calls: { type: 'success' | 'error'; message: string }[] = [];
  return {
    calls,
    showSuccess: (m: string) => {
      calls.push({ type: 'success', message: m });
      return 'id';
    },
    showError: (m: string) => {
      calls.push({ type: 'error', message: m });
      return 'id';
    },
  };
}

/** v4 `useNewChat.ts` (9 rows, P4.29 unit 10) — all toast only, no v5 banner. */
describe('NewChatState toasts', () => {
  it('toasts "Failed to load chat creation data" on a load failure', async () => {
    const toasts = fakeToasts();
    const core = {
      dispatchData: async () => {
        throw new Error('offline');
      },
      dispatchExpect: async () => {
        throw new Error('offline');
      },
    } as unknown as CoreClient;
    const state = new NewChatState(core, {}, null, toasts);
    await state.load();
    expect(toasts.calls).toEqual([{ type: 'error', message: 'Failed to load chat creation data' }]);
  });

  it('refuses with no character selected', async () => {
    const toasts = fakeToasts();
    const state = new NewChatState({} as CoreClient, {}, null, toasts);
    const outcome = await state.handleCreate();
    expect(outcome).toBeNull();
    expect(toasts.calls).toEqual([
      { type: 'error', message: 'Please select at least one character' },
    ]);
  });

  it('refuses an autonomous room with fewer than two LLM characters', async () => {
    const toasts = fakeToasts();
    const state = new NewChatState({} as CoreClient, {}, null, toasts);
    state.form.update((f) => ({ ...f, autonomous: { ...f.autonomous, enabled: true } }));
    state.selectedCharacters.set([selected(char('a', 'Alice'))]);
    const outcome = await state.handleCreate();
    expect(outcome).toBeNull();
    expect(toasts.calls).toEqual([
      {
        type: 'error',
        message: 'Autonomous rooms need at least two LLM-controlled characters',
      },
    ]);
  });

  it('refuses an autonomous room with a user-controlled participant', async () => {
    const toasts = fakeToasts();
    const state = new NewChatState({} as CoreClient, {}, null, toasts);
    state.form.update((f) => ({ ...f, autonomous: { ...f.autonomous, enabled: true } }));
    state.selectedCharacters.set([
      selected(char('a', 'Alice')),
      selected(char('b', 'Bob')),
      selected(char('c', 'Carol'), { controlledBy: 'user', connectionProfileId: '' }),
    ]);
    const outcome = await state.handleCreate();
    expect(outcome).toBeNull();
    expect(toasts.calls).toEqual([
      {
        type: 'error',
        message: 'Autonomous rooms have no user — remove user-controlled characters',
      },
    ]);
  });

  it('names every LLM character still missing a connection profile', async () => {
    const toasts = fakeToasts();
    const state = new NewChatState({} as CoreClient, {}, null, toasts);
    state.selectedCharacters.set([
      selected(char('a', 'Alice'), { connectionProfileId: '' }),
      selected(char('b', 'Bob'), { connectionProfileId: '' }),
    ]);
    const outcome = await state.handleCreate();
    expect(outcome).toBeNull();
    expect(toasts.calls).toEqual([
      {
        type: 'error',
        message: 'Please select a connection profile for: Alice, Bob',
      },
    ]);
  });

  it('refuses an all-user-controlled cast (no LLM character)', async () => {
    const toasts = fakeToasts();
    const state = new NewChatState({} as CoreClient, {}, null, toasts);
    state.selectedCharacters.set([
      selected(char('a', 'Alice'), { controlledBy: 'user', connectionProfileId: '' }),
    ]);
    const outcome = await state.handleCreate();
    expect(outcome).toBeNull();
    expect(toasts.calls).toEqual([
      { type: 'error', message: 'At least one character must be LLM-controlled' },
    ]);
  });

  it('toasts "Chat created!" on a successful non-autonomous create', async () => {
    const toasts = fakeToasts();
    const core = {
      dispatchExpect: async () => ({ type: 'chatCreate', data: { chat: { id: 'chat-1' } } }),
    } as unknown as CoreClient;
    const state = new NewChatState(core, {}, null, toasts);
    state.selectedCharacters.set([selected(char('a', 'Alice'))]);
    const outcome = await state.handleCreate();
    expect(outcome).toEqual({ chatId: 'chat-1', isAutonomous: false });
    expect(toasts.calls).toEqual([{ type: 'success', message: 'Chat created!' }]);
  });

  it('toasts "Autonomous room created!" on a successful autonomous create', async () => {
    const toasts = fakeToasts();
    const core = {
      dispatchExpect: async () => ({ type: 'chatCreate', data: { chat: { id: 'chat-2' } } }),
    } as unknown as CoreClient;
    const state = new NewChatState(core, {}, null, toasts);
    state.form.update((f) => ({ ...f, autonomous: { ...f.autonomous, enabled: true } }));
    state.selectedCharacters.set([selected(char('a', 'Alice')), selected(char('b', 'Bob'))]);
    const outcome = await state.handleCreate();
    expect(outcome).toEqual({ chatId: 'chat-2', isAutonomous: true });
    expect(toasts.calls).toEqual([{ type: 'success', message: 'Autonomous room created!' }]);
  });

  it('toasts the server message on a failed create', async () => {
    const toasts = fakeToasts();
    const core = {
      dispatchExpect: async () => {
        throw new Error('the registry rejected the cast');
      },
    } as unknown as CoreClient;
    const state = new NewChatState(core, {}, null, toasts);
    state.selectedCharacters.set([selected(char('a', 'Alice'))]);
    const outcome = await state.handleCreate();
    expect(outcome).toBeNull();
    expect(toasts.calls).toEqual([{ type: 'error', message: 'the registry rejected the cast' }]);
  });
});
