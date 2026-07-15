import { describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../core/core-client';
import {
  deriveMessagesWithLogs,
  fetchChatLlmLogs,
  llmLogKeys,
  type LlmLogDto,
} from './llm-logs.api';

function log(overrides: Partial<LlmLogDto> = {}): LlmLogDto {
  return {
    id: 'log-1',
    userId: 'user-1',
    type: 'CHAT_MESSAGE',
    provider: 'openai',
    modelName: 'gpt-4',
    request: { messageCount: 1, messages: [], toolCount: 0 },
    response: { content: 'hi', contentLength: 2 },
    createdAt: '2026-02-01T00:00:00.000Z',
    updatedAt: '2026-02-01T00:00:00.000Z',
    ...overrides,
  };
}

function coreStub(data: unknown): { core: CoreClient; dispatchData: ReturnType<typeof vi.fn> } {
  const dispatchData = vi.fn(async () => data as Record<string, unknown>);
  return { core: { dispatchData } as unknown as CoreClient, dispatchData };
}

describe('llm-logs.api — the request (v4 useLLMLogs.ts:25-30, Shared contract §1)', () => {
  it('sends exactly {type, chatId, includeMessages} — v4’s query string, nothing more', async () => {
    // v4 fetches `/api/v1/llm-logs?chatId=${chatId}&includeMessages=true`. The
    // dispatch verb takes the same two params; any extra field here would be a
    // filter v4 never applies.
    const { core, dispatchData } = coreStub({
      logs: [],
      count: 0,
      total: 0,
      limit: 100,
      offset: 0,
    });
    await fetchChatLlmLogs(core, 'chat-9');
    expect(dispatchData).toHaveBeenCalledWith({
      type: 'llmLogsList',
      chatId: 'chat-9',
      includeMessages: true,
    });
  });

  it('returns the envelope’s logs array', async () => {
    const rows = [log({ id: 'a' }), log({ id: 'b' })];
    const { core } = coreStub({ logs: rows, count: 2, total: 2, limit: 100, offset: 0 });
    expect(await fetchChatLlmLogs(core, 'chat-9')).toEqual(rows);
  });

  it('tolerates an envelope with no logs member', async () => {
    const { core } = coreStub({ count: 0, total: 0, limit: 100, offset: 0 });
    expect(await fetchChatLlmLogs(core, 'chat-9')).toEqual([]);
  });
});

describe('llm-logs.api — the query key (v4 queryKeys.llmLogs.byChat)', () => {
  it('is per-chat', () => {
    expect(llmLogKeys.byChat('chat-1')).toEqual(['llmLogs', 'byChat', 'chat-1']);
    expect(llmLogKeys.byChat('chat-2')).not.toEqual(llmLogKeys.byChat('chat-1'));
  });
});

describe('llm-logs.api — messagesWithLogs (v4 useLLMLogs.ts:35-41)', () => {
  it('collects the messageIds present on the logs', () => {
    const set = deriveMessagesWithLogs([
      log({ id: '1', messageId: 'm1' }),
      log({ id: '2', messageId: 'm2' }),
    ]);
    expect([...set].sort()).toEqual(['m1', 'm2']);
  });

  it('dedupes repeated messageIds — several logs can share one message', () => {
    const set = deriveMessagesWithLogs([
      log({ id: '1', messageId: 'm1' }),
      log({ id: '2', messageId: 'm1' }),
    ]);
    expect(set.size).toBe(1);
  });

  it('drops chat-level logs (null/undefined/empty messageId) — v4 filters on truthiness', () => {
    // A title-generation or compression log has no message to point at.
    const set = deriveMessagesWithLogs([
      log({ id: '1', messageId: null }),
      log({ id: '2' }),
      log({ id: '3', messageId: '' }),
      log({ id: '4', messageId: 'm4' }),
    ]);
    expect([...set]).toEqual(['m4']);
  });

  it('is empty for no logs', () => {
    expect(deriveMessagesWithLogs([]).size).toBe(0);
  });
});
