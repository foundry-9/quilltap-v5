import { describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../core/core-client';
import type { CoreRequest } from '../core/core-contract';
import { resolveDefaultCharacterForChat, SALON_CHAT_PATH_RE } from './default-character';

function stubCore(chat: Record<string, unknown> | Error): CoreClient {
  return {
    dispatchData: vi.fn(async (req: CoreRequest) => {
      if (chat instanceof Error) throw chat;
      expect(req).toEqual({ type: 'chatGet', chatId: 'chat-1' });
      return { chat };
    }),
  } as unknown as CoreClient;
}

const participants = [
  { id: 'p-user', controlledBy: 'user', displayOrder: 0, character: { id: 'c-user' } },
  { id: 'p-b', controlledBy: 'llm', displayOrder: 2, character: { id: 'c-b' } },
  { id: 'p-a', controlledBy: 'llm', displayOrder: 1, character: { id: 'c-a' } },
];

describe('SALON_CHAT_PATH_RE (v4 sidebar-footer.tsx:39)', () => {
  it('matches only a strict UUID salon path and captures the id', () => {
    const m = '/salon/123e4567-e89b-12d3-a456-426614174000'.match(SALON_CHAT_PATH_RE);
    expect(m?.[1]).toBe('123e4567-e89b-12d3-a456-426614174000');
    expect('/salon/new'.match(SALON_CHAT_PATH_RE)).toBeNull();
    expect('/characters/123e4567-e89b-12d3-a456-426614174000'.match(SALON_CHAT_PATH_RE)).toBeNull();
  });
});

describe('resolveDefaultCharacterForChat (v4 sidebar-footer.tsx:60-101)', () => {
  it('priority 1: the most recent assistant message by an eligible participant', async () => {
    const core = stubCore({
      participants,
      messages: [
        { role: 'ASSISTANT', participantId: 'p-a' },
        { role: 'USER', participantId: 'p-user' },
        { role: 'ASSISTANT', participantId: 'p-b' },
        { role: 'USER', participantId: 'p-user' },
      ],
    });
    expect(await resolveDefaultCharacterForChat(core, 'chat-1')).toBe('c-b');
  });

  it('priority 2: falls back to the first eligible participant by displayOrder', async () => {
    const core = stubCore({ participants, messages: [{ role: 'USER', participantId: 'p-user' }] });
    expect(await resolveDefaultCharacterForChat(core, 'chat-1')).toBe('c-a');
  });

  it('user-controlled participants and character-less ones are never eligible', async () => {
    const core = stubCore({
      participants: [
        { id: 'p-user', controlledBy: 'user', displayOrder: 0, character: { id: 'c-user' } },
        { id: 'p-x', controlledBy: 'llm', displayOrder: 1, character: null },
      ],
      messages: [{ role: 'ASSISTANT', participantId: 'p-user' }],
    });
    expect(await resolveDefaultCharacterForChat(core, 'chat-1')).toBeNull();
  });

  it('a failed fetch resolves null (v4 :98-100)', async () => {
    expect(await resolveDefaultCharacterForChat(stubCore(new Error('boom')), 'chat-1')).toBeNull();
  });
});
