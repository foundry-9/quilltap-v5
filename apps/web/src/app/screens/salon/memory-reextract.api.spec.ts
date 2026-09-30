import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../../core/core-client';
import { CoreDispatchError } from '../../core/core-contract';
import { QUEUE_CHANGE_EVENT } from '../../layout/queue-status.logic';
import { confirmAndReextractMemories, REEXTRACT_CONFIRM } from './memory-reextract.api';

/**
 * The memory badge's action, pinned against v4 `handleReextractMemories`
 * (`app/salon/SalonListView.tsx:119-145` and
 * `components/character/character-conversations-tab.tsx:148-176` at
 * `97b25fc53`): confirm → DELETE memories → POST queue-memories → success toast
 * `Queued ${jobCount} memory extraction jobs` + `notifyQueueChange()`; the queue
 * failure is `data.error || 'Failed to queue memory extraction'`, the catch
 * `err.message` else `'Failed to re-extract memories'`. The DELETE's status is
 * never read.
 */
function rig(answer: (req: { type: string }) => Promise<Record<string, unknown>>) {
  const calls: string[] = [];
  const core = {
    dispatchData: vi.fn(async (req: { type: string }) => {
      calls.push(req.type);
      return answer(req);
    }),
  } as unknown as CoreClient;
  const toasts = { showSuccess: vi.fn(), showError: vi.fn() };
  return { core, toasts, calls };
}

describe('confirmAndReextractMemories', () => {
  let confirm: ReturnType<typeof vi.spyOn>;
  beforeEach(() => {
    confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
  });
  afterEach(() => vi.restoreAllMocks());

  it("asks v4's sentence and dispatches NOTHING when declined", async () => {
    confirm.mockReturnValue(false);
    const { core, toasts, calls } = rig(async () => ({}));
    expect(await confirmAndReextractMemories(core, toasts, 'c1')).toBe(false);
    expect(confirm).toHaveBeenCalledWith(REEXTRACT_CONFIRM);
    expect(REEXTRACT_CONFIRM).toBe(
      'This will delete all existing memories from this chat and re-extract them from the conversation. Are you sure?',
    );
    expect(calls).toEqual([]);
    expect(toasts.showSuccess).not.toHaveBeenCalled();
  });

  it('deletes THEN queues, in that order, then toasts the job count and wakes the queue', async () => {
    const { core, toasts, calls } = rig(async (req) =>
      req.type === 'chatQueueMemories' ? { success: true, jobCount: 7 } : { success: true },
    );
    const woke = vi.fn();
    window.addEventListener(QUEUE_CHANGE_EVENT, woke);
    try {
      expect(await confirmAndReextractMemories(core, toasts, 'c1')).toBe(true);
    } finally {
      window.removeEventListener(QUEUE_CHANGE_EVENT, woke);
    }
    // M3: swapping the two verbs reddens this line.
    expect(calls).toEqual(['memoryDeleteByChat', 'chatQueueMemories']);
    expect(core.dispatchData).toHaveBeenNthCalledWith(1, { type: 'memoryDeleteByChat', chatId: 'c1' });
    expect(core.dispatchData).toHaveBeenNthCalledWith(2, { type: 'chatQueueMemories', chatId: 'c1' });
    expect(toasts.showSuccess).toHaveBeenCalledWith('Queued 7 memory extraction jobs');
    expect(woke).toHaveBeenCalledTimes(1);
    expect(toasts.showError).not.toHaveBeenCalled();
  });

  it("never reads the DELETE's status: a refused delete still queues (v4 ignores it)", async () => {
    const { core, toasts, calls } = rig(async (req) => {
      if (req.type === 'memoryDeleteByChat') {
        throw new CoreDispatchError({ kind: 'not-found', message: 'nope' });
      }
      return { jobCount: 2 };
    });
    expect(await confirmAndReextractMemories(core, toasts, 'c1')).toBe(true);
    expect(calls).toEqual(['memoryDeleteByChat', 'chatQueueMemories']);
    expect(toasts.showSuccess).toHaveBeenCalledWith('Queued 2 memory extraction jobs');
  });

  it("shows the server's own error when the queue is refused, else v4's fallback", async () => {
    const refused = rig(async (req) => {
      if (req.type === 'chatQueueMemories') {
        throw new CoreDispatchError({ kind: 'bad-request', message: 'Chat has no messages' });
      }
      return {};
    });
    expect(await confirmAndReextractMemories(refused.core, refused.toasts, 'c1')).toBe(false);
    expect(refused.toasts.showError).toHaveBeenCalledWith('Chat has no messages');
    expect(refused.toasts.showSuccess).not.toHaveBeenCalled();

    const bare = rig(async (req) => {
      if (req.type === 'chatQueueMemories') {
        throw new CoreDispatchError({ kind: 'internal', message: '' });
      }
      return {};
    });
    await confirmAndReextractMemories(bare.core, bare.toasts, 'c1');
    expect(bare.toasts.showError).toHaveBeenCalledWith('Failed to queue memory extraction');
  });

  it('a transport failure toasts its message, else v4\'s catch-all, and does not queue', async () => {
    const down = rig(async () => {
      throw new Error('socket hang up');
    });
    expect(await confirmAndReextractMemories(down.core, down.toasts, 'c1')).toBe(false);
    expect(down.toasts.showError).toHaveBeenCalledWith('socket hang up');
    expect(down.calls).toEqual(['memoryDeleteByChat']);

    const mute = rig(async () => {
      throw new Error('');
    });
    await confirmAndReextractMemories(mute.core, mute.toasts, 'c1');
    expect(mute.toasts.showError).toHaveBeenCalledWith('Failed to re-extract memories');
  });
});
