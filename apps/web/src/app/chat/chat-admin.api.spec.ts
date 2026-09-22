import { describe, expect, it } from 'vitest';

import type { CoreClient } from '../core/core-client';
import { rebuildChatSummary } from './chat-admin.api';

/**
 * `rebuildChatSummary` (§1 `ChatRebuildSummary`, bug 161, `e7821606f`) — the
 * `chat-rename-modal.spec.ts:134` precedent: pin the exact request bytes, not
 * just that dispatch fired.
 */

interface Req {
  type: string;
  [k: string]: unknown;
}

describe('rebuildChatSummary', () => {
  it('sends exactly {type: chatRebuildSummary, chatId} and returns the jobId', async () => {
    const sent: Req[] = [];
    const core = {
      dispatchData: (async (req: Req) => {
        sent.push(req);
        return { success: true, jobId: 'job-1' };
      }) as unknown as CoreClient['dispatchData'],
    } as CoreClient;

    const jobId = await rebuildChatSummary(core, 'chat-1');

    expect(sent).toEqual([{ type: 'chatRebuildSummary', chatId: 'chat-1' }]);
    expect(jobId).toBe('job-1');
  });

  it('rejects with the dispatch error when the server refuses', async () => {
    const core = {
      dispatchData: (async () => {
        throw new Error('Pause the room before rebuilding its summary.');
      }) as unknown as CoreClient['dispatchData'],
    } as CoreClient;

    await expect(rebuildChatSummary(core, 'chat-1')).rejects.toThrow(
      'Pause the room before rebuilding its summary.',
    );
  });
});
