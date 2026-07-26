import { describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../core/core-client';
import {
  addParticipant,
  fetchChatAvatars,
  rebuildSystemPrompt,
  removeChatAvatar,
  removeParticipant,
  rollRng,
  setChatAvatar,
  toggleAvatarGeneration,
  updateParticipant,
} from './chat-cast.api';

function coreStub(data: unknown): { core: CoreClient; dispatchData: ReturnType<typeof vi.fn> } {
  const dispatchData = vi.fn(async () => data as Record<string, unknown>);
  return { core: { dispatchData } as unknown as CoreClient, dispatchData };
}

/** The body a helper actually put on the wire. */
function sentBody(dispatchData: ReturnType<typeof vi.fn>, call = 0): Record<string, unknown> {
  return dispatchData.mock.calls[call]![0] as Record<string, unknown>;
}

describe('chat-cast.api — chatAddParticipant', () => {
  it('sends the character id and OMITS every unset optional (v4 :186-209)', async () => {
    const { core, dispatchData } = coreStub({ participant: {} });
    await addParticipant(core, 'chat-1', { characterId: 'char-7' });
    expect(dispatchData).toHaveBeenCalledWith({
      type: 'chatAddParticipant',
      chatId: 'chat-1',
      characterId: 'char-7',
    });
  });

  it('carries no `type` field — v4’s literal rides the verb name (E1A tier-2 item 6)', async () => {
    const { core, dispatchData } = coreStub({});
    await addParticipant(core, 'chat-1', { characterId: 'char-7', controlledBy: 'llm' });
    const body = sentBody(dispatchData);
    // The only `type` on the wire is the request union's own tag.
    expect(body['type']).toBe('chatAddParticipant');
    expect(Object.keys(body).filter((k) => k === 'characterType')).toEqual([]);
  });

  it('omits connectionProfileId for a user-controlled seat, sends it for an LLM one', async () => {
    const { core, dispatchData } = coreStub({});
    await addParticipant(core, 'chat-1', { characterId: 'c', controlledBy: 'user' });
    expect('connectionProfileId' in sentBody(dispatchData, 0)).toBe(false);

    await addParticipant(core, 'chat-1', {
      characterId: 'c',
      controlledBy: 'llm',
      connectionProfileId: 'prof-1',
    });
    expect(sentBody(dispatchData, 1)['connectionProfileId']).toBe('prof-1');
  });

  it('passes an explicit joinScenario null through, and omits it when unset', async () => {
    const { core, dispatchData } = coreStub({});
    await addParticipant(core, 'chat-1', { characterId: 'c', joinScenario: null });
    expect('joinScenario' in sentBody(dispatchData, 0)).toBe(true);
    expect(sentBody(dispatchData, 0)['joinScenario']).toBeNull();

    await addParticipant(core, 'chat-1', { characterId: 'c' });
    expect('joinScenario' in sentBody(dispatchData, 1)).toBe(false);
  });

  it('forwards the outfit selection verbatim when the selector produced one', async () => {
    const { core, dispatchData } = coreStub({});
    await addParticipant(core, 'chat-1', {
      characterId: 'c',
      outfitSelection: { characterId: 'c', mode: 'llm_choose' },
    });
    expect(sentBody(dispatchData)['outfitSelection']).toEqual({
      characterId: 'c',
      mode: 'llm_choose',
    });
  });
});

describe('chat-cast.api — chatUpdateParticipant and the three-valued fields', () => {
  it('sends only the keys the patch actually carries', async () => {
    const { core, dispatchData } = coreStub({});
    await updateParticipant(core, 'chat-1', 'p-1', { status: 'silent' });
    expect(dispatchData).toHaveBeenCalledWith({
      type: 'chatUpdateParticipant',
      chatId: 'chat-1',
      participantId: 'p-1',
      status: 'silent',
    });
  });

  it('treats an EXPLICIT undefined as an absent key, not as a null', async () => {
    const { core, dispatchData } = coreStub({});
    await updateParticipant(core, 'chat-1', 'p-1', {
      talkativeness: undefined,
      isActive: true,
    });
    const body = sentBody(dispatchData);
    expect('talkativeness' in body).toBe(false);
    expect(body['isActive']).toBe(true);
  });

  // The tri-state proof, field by field: v4 `helpers.ts:159-160,180` branch on
  // `!== undefined`, so absent (leave alone) and null (clear) MUST differ on the
  // wire. A client that always sends the key defeats the server's `Option<Option<T>>`.
  for (const field of [
    'imageProfileId',
    'selectedSystemPromptId',
    'joinScenario',
    'talkativeness',
  ] as const) {
    it(`${field}: absent when unset, explicit null when cleared, value when set`, async () => {
      const { core, dispatchData } = coreStub({});

      await updateParticipant(core, 'chat-1', 'p-1', {});
      expect(field in sentBody(dispatchData, 0)).toBe(false);

      await updateParticipant(core, 'chat-1', 'p-1', { [field]: null });
      expect(field in sentBody(dispatchData, 1)).toBe(true);
      expect(sentBody(dispatchData, 1)[field]).toBeNull();

      const value = field === 'talkativeness' ? 0.7 : 'v-1';
      await updateParticipant(core, 'chat-1', 'p-1', { [field]: value });
      expect(sentBody(dispatchData, 2)[field]).toBe(value);
    });
  }

  it('the connection-profile flip sends controlledBy with the profile (v4 :500-515)', async () => {
    const { core, dispatchData } = coreStub({});
    await updateParticipant(core, 'chat-1', 'p-1', {
      connectionProfileId: 'prof-2',
      controlledBy: 'llm',
    });
    expect(sentBody(dispatchData)).toEqual({
      type: 'chatUpdateParticipant',
      chatId: 'chat-1',
      participantId: 'p-1',
      connectionProfileId: 'prof-2',
      controlledBy: 'llm',
    });
  });

  it('flipping to user control omits connectionProfileId entirely (v4 `? undefined`)', async () => {
    const { core, dispatchData } = coreStub({});
    await updateParticipant(core, 'chat-1', 'p-1', { controlledBy: 'user' });
    expect('connectionProfileId' in sentBody(dispatchData)).toBe(false);
  });
});

describe('chat-cast.api — the thin cast verbs', () => {
  it('chatRemoveParticipant sends the pair', async () => {
    const { core, dispatchData } = coreStub({ success: true });
    await removeParticipant(core, 'chat-1', 'p-1');
    expect(dispatchData).toHaveBeenCalledWith({
      type: 'chatRemoveParticipant',
      chatId: 'chat-1',
      participantId: 'p-1',
    });
  });

  it('chatRebuildSystemPrompt sends the pair', async () => {
    const { core, dispatchData } = coreStub({ ok: true });
    await rebuildSystemPrompt(core, 'chat-1', 'p-1');
    expect(dispatchData).toHaveBeenCalledWith({
      type: 'chatRebuildSystemPrompt',
      chatId: 'chat-1',
      participantId: 'p-1',
    });
  });
});

describe('chat-cast.api — the avatar overrides', () => {
  it('chatGetAvatars reads the list and degrades an odd body to empty', async () => {
    const listed = coreStub({ avatars: [{ characterId: 'c1', imageId: 'i1' }] });
    await expect(fetchChatAvatars(listed.core, 'chat-1')).resolves.toEqual([
      { characterId: 'c1', imageId: 'i1' },
    ]);
    const odd = coreStub({});
    await expect(fetchChatAvatars(odd.core, 'chat-1')).resolves.toEqual([]);
  });

  it('chatSetAvatar / chatRemoveAvatar send v4’s fields', async () => {
    const { core, dispatchData } = coreStub({});
    await setChatAvatar(core, 'chat-1', 'c1', 'img-9');
    expect(dispatchData).toHaveBeenCalledWith({
      type: 'chatSetAvatar',
      chatId: 'chat-1',
      characterId: 'c1',
      imageId: 'img-9',
    });
    await removeChatAvatar(core, 'chat-1', 'c1');
    expect(sentBody(dispatchData, 1)).toEqual({
      type: 'chatRemoveAvatar',
      chatId: 'chat-1',
      characterId: 'c1',
    });
  });

  it('chatToggleAvatarGeneration returns v4’s echoed flag, else null', async () => {
    const echoed = coreStub({ avatarGenerationEnabled: true });
    await expect(toggleAvatarGeneration(echoed.core, 'chat-1')).resolves.toBe(true);
    const quiet = coreStub({ success: true });
    await expect(toggleAvatarGeneration(quiet.core, 'chat-1')).resolves.toBeNull();
  });
});

describe('chat-cast.api — chatRng', () => {
  it('sends `kind`, NOT `type` — the E3A §1 rename', async () => {
    const { core, dispatchData } = coreStub({ success: true, preview: true, result: {} });
    await rollRng(core, 'chat-1', 20, 2, true);
    const body = sentBody(dispatchData);
    expect(body['kind']).toBe(20);
    expect(body['type']).toBe('chatRng');
    expect(body['rolls']).toBe(2);
    expect(body['preview']).toBe(true);
  });

  it('carries the two named kinds unchanged', async () => {
    const { core, dispatchData } = coreStub({ preview: true, result: {} });
    await rollRng(core, 'chat-1', 'flip_coin', 1, true);
    expect(sentBody(dispatchData, 0)['kind']).toBe('flip_coin');
    await rollRng(core, 'chat-1', 'spin_the_bottle', 1, true);
    expect(sentBody(dispatchData, 1)['kind']).toBe('spin_the_bottle');
  });

  it('maps v4’s preview body into the pending-chip shape', async () => {
    const { core } = coreStub({
      success: true,
      preview: true,
      result: {
        type: 20,
        rollCount: 1,
        results: [17],
        sum: 17,
        formattedText: '🎲 Rolled 1d20: **17**',
        summary: 'd20: 17',
        requestPrompt: 'Roll a d20',
        arguments: { type: 20, rolls: 1 },
      },
    });
    await expect(rollRng(core, 'chat-1', 20, 1, true)).resolves.toEqual({
      summary: 'd20: 17',
      formattedText: '🎲 Rolled 1d20: **17**',
      requestPrompt: 'Roll a d20',
      // v4's own `{type, rolls}` bag, passed through opaque and NOT re-keyed to `kind`.
      arguments: { type: 20, rolls: 1 },
    });
  });

  it('returns null for a non-preview roll (the server already wrote the message)', async () => {
    const { core } = coreStub({ success: true, message: { id: 'm1' }, result: {} });
    await expect(rollRng(core, 'chat-1', 6, 1, false)).resolves.toBeNull();
  });
});
