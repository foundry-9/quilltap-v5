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

// --- Roleplay template seeding (v4 `4bbeab47` useNewChat) --------------------

/**
 * A CoreClient whose per-verb answers are scripted, so a single source can be
 * made to FAIL while the rest answer — the shape v4 gets for free from `fetch`
 * resolving on a non-2xx.
 */
function scriptedCore(over: {
  templates?: unknown[] | 'fail';
  chatSettings?: Record<string, unknown> | 'fail';
  project?: Record<string, unknown> | 'fail';
}): CoreClient {
  return {
    dispatchData: async (req: { type: string }) => {
      switch (req.type) {
        case 'characterList':
          return { characters: [] };
        case 'scenarioList':
        case 'projectScenarioList':
          return { scenarios: [] };
        case 'projectList':
          return { projects: [] };
        case 'roleplayTemplateList':
          if (over.templates === 'fail') throw new Error('templates offline');
          return over.templates ?? [];
        case 'projectGet':
          if (over.project === 'fail') throw new Error('project offline');
          return { project: over.project ?? { id: 'pr1', name: 'A Project' } };
        default:
          return {};
      }
    },
    dispatchExpect: async (req: { type: string }) => {
      if (req.type === 'chatSettings') {
        if (over.chatSettings === 'fail') throw new Error('settings offline');
        return { type: 'chatSettings', data: over.chatSettings ?? {} };
      }
      return { type: 'connectionProfiles', data: { profiles: [] } };
    },
  } as unknown as CoreClient;
}

const TPL = [
  { id: 'tpl-classic', name: 'Classic Roleplay', isBuiltIn: true },
  { id: 'tpl-house', name: 'House Style', isBuiltIn: false },
];

describe('NewChatState roleplay template default', () => {
  it('seeds the user/global default when no project is chosen', async () => {
    const state = new NewChatState(
      scriptedCore({ templates: TPL, chatSettings: { defaultRoleplayTemplateId: 'tpl-house' } }),
      {},
    );
    await state.load();
    expect(state.defaultRoleplayTemplateId()).toBe('tpl-house');
    expect(state.form().roleplayTemplateId).toBe('tpl-house');
  });

  it('prefers the project default over the user default', async () => {
    const state = new NewChatState(
      scriptedCore({
        templates: TPL,
        chatSettings: { defaultRoleplayTemplateId: 'tpl-house' },
        project: { id: 'pr1', name: 'A Project', defaultRoleplayTemplateId: 'tpl-classic' },
      }),
      { projectId: 'pr1' },
    );
    await state.load();
    expect(state.defaultRoleplayTemplateId()).toBe('tpl-classic');
    expect(state.form().roleplayTemplateId).toBe('tpl-classic');
  });

  it('seeds nothing when the default points at a template that no longer exists', async () => {
    const state = new NewChatState(
      scriptedCore({ templates: TPL, chatSettings: { defaultRoleplayTemplateId: 'tpl-gone' } }),
      {},
    );
    await state.load();
    expect(state.defaultRoleplayTemplateId()).toBeNull();
    expect(state.form().roleplayTemplateId).toBeNull();
  });

  it('a reference-data reload re-seeds the default — until the user has picked', async () => {
    const state = new NewChatState(
      scriptedCore({ templates: TPL, chatSettings: { defaultRoleplayTemplateId: 'tpl-house' } }),
      {},
    );
    await state.load();
    expect(state.form().roleplayTemplateId).toBe('tpl-house');

    // An untouched form re-seeds freely.
    state.patchForm({ roleplayTemplateId: null });
    await state.load();
    expect(state.form().roleplayTemplateId).toBe('tpl-house');

    // A hand pick latches, and survives the next reload.
    state.patchForm({ roleplayTemplateId: 'tpl-classic', roleplayTemplateTouched: true });
    await state.load();
    expect(state.form().roleplayTemplateId).toBe('tpl-classic');
  });
});

describe('NewChatState roleplayTemplateId omit-on-failed-fetch', () => {
  /** Capture the create body the state dispatches. */
  function captureCore(base: CoreClient, sink: { body?: Record<string, unknown> }): CoreClient {
    const b = base as unknown as {
      dispatchData: (r: { type: string }) => Promise<unknown>;
      dispatchExpect: (r: { type: string }, k: string) => Promise<unknown>;
    };
    return {
      dispatchData: b.dispatchData,
      dispatchExpect: async (req: Record<string, unknown>, key: string) => {
        if (req['type'] === 'chatCreate') {
          sink.body = req;
          return { type: 'chatCreate', data: { chat: { id: 'new-chat' } } };
        }
        return b.dispatchExpect(req as { type: string }, key);
      },
    } as unknown as CoreClient;
  }

  async function createWith(
    over: Parameters<typeof scriptedCore>[0],
  ): Promise<Record<string, unknown>> {
    const sink: { body?: Record<string, unknown> } = {};
    const state = new NewChatState(captureCore(scriptedCore(over), sink), {});
    await state.load();
    state.profiles.set([{ id: 'p1', name: 'Anthropic' }]);
    state.selectedCharacters.set([selected(char('a', 'Alice'))]);
    await state.handleCreate();
    return sink.body ?? {};
  }

  it('sends the key when every default source answered', async () => {
    const body = await createWith({
      templates: TPL,
      chatSettings: { defaultRoleplayTemplateId: 'tpl-house' },
    });
    expect(body['roleplayTemplateId']).toBe('tpl-house');
  });

  it('sends an explicit null when the user picked "No Template"', async () => {
    const sink: { body?: Record<string, unknown> } = {};
    const state = new NewChatState(
      captureCore(
        scriptedCore({ templates: TPL, chatSettings: { defaultRoleplayTemplateId: 'tpl-house' } }),
        sink,
      ),
      {},
    );
    await state.load();
    state.profiles.set([{ id: 'p1', name: 'Anthropic' }]);
    state.selectedCharacters.set([selected(char('a', 'Alice'))]);
    state.patchForm({ roleplayTemplateId: null, roleplayTemplateTouched: true });
    await state.handleCreate();
    expect(sink.body).toHaveProperty('roleplayTemplateId', null);
  });

  it('OMITS the key when the templates read failed', async () => {
    const body = await createWith({
      templates: 'fail',
      chatSettings: { defaultRoleplayTemplateId: 'tpl-house' },
    });
    expect(body).not.toHaveProperty('roleplayTemplateId');
  });

  it('OMITS the key when the chat-settings read failed', async () => {
    const body = await createWith({ templates: TPL, chatSettings: 'fail' });
    expect(body).not.toHaveProperty('roleplayTemplateId');
  });

  it('OMITS the key when a chosen project failed to load', async () => {
    const sink: { body?: Record<string, unknown> } = {};
    const state = new NewChatState(
      captureCore(
        scriptedCore({
          templates: TPL,
          chatSettings: { defaultRoleplayTemplateId: 'tpl-house' },
          project: 'fail',
        }),
        sink,
      ),
      { projectId: 'pr1' },
    );
    await state.load();
    state.profiles.set([{ id: 'p1', name: 'Anthropic' }]);
    state.selectedCharacters.set([selected(char('a', 'Alice'))]);
    await state.handleCreate();
    expect(sink.body).not.toHaveProperty('roleplayTemplateId');
  });
});
