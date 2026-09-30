import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../../core/core-client';
import { CoreDispatchError } from '../../core/core-contract';
import { QUEUE_CHANGE_EVENT } from '../../layout/queue-status.logic';
import {
  confirmAndDeleteChatMemories,
  confirmAndQueueMemories,
  NO_ACTIVE_CHARACTER_MESSAGE,
  QUEUE_CONFIRM,
} from './memory-sidebar.api';

/**
 * The Edit Content memory pair against v4 `useMemoryActions.ts` at `97b25fc53`
 * (`:17-63` delete, `:65-101` re-extract): every string is v4's bytes.
 */
function refusal(message: string): CoreDispatchError {
  return new CoreDispatchError({ kind: 'bad-request', message });
}

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

describe('confirmAndQueueMemories (Re-extract Memories — queue ONLY)', () => {
  let confirm: ReturnType<typeof vi.spyOn>;
  beforeEach(() => {
    confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
  });
  afterEach(() => vi.restoreAllMocks());

  it('refuses with v4 toast when no character is active, before any confirm', async () => {
    const { core, toasts, calls } = rig(async () => ({}));
    expect(await confirmAndQueueMemories(core, toasts, 'c1', false)).toBe(false);
    expect(toasts.showError).toHaveBeenCalledWith('Cannot re-extract memories: no active character in chat');
    expect(NO_ACTIVE_CHARACTER_MESSAGE).toBe('Cannot re-extract memories: no active character in chat');
    expect(confirm).not.toHaveBeenCalled();
    expect(calls).toEqual([]);
  });

  it('confirms with v4 sentence, queues, NEVER deletes (M3), toasts and wakes the queue', async () => {
    const { core, toasts, calls } = rig(async () => ({ jobCount: 4 }));
    const woke = vi.fn();
    window.addEventListener(QUEUE_CHANGE_EVENT, woke);
    try {
      expect(await confirmAndQueueMemories(core, toasts, 'c1', true)).toBe(true);
    } finally {
      window.removeEventListener(QUEUE_CHANGE_EVENT, woke);
    }
    expect(confirm).toHaveBeenCalledWith(QUEUE_CONFIRM);
    expect(QUEUE_CONFIRM).toBe(
      'Queue memory extraction jobs for all messages in this chat? This will process the entire conversation history.',
    );
    expect(calls).toEqual(['chatQueueMemories']);
    expect(calls).not.toContain('memoryDeleteByChat');
    expect(toasts.showSuccess).toHaveBeenCalledWith('Queued 4 memory extraction jobs');
    expect(woke).toHaveBeenCalledTimes(1);
  });

  it('dispatches nothing when declined', async () => {
    confirm.mockReturnValue(false);
    const { core, toasts, calls } = rig(async () => ({}));
    expect(await confirmAndQueueMemories(core, toasts, 'c1', true)).toBe(false);
    expect(calls).toEqual([]);
  });

  it('a refused queue is the colon-form toast; a non-dispatch throw the bare guard (no client path produces one)', async () => {
    const a = rig(async () => {
      throw refusal('Chat has no messages');
    });
    await confirmAndQueueMemories(a.core, a.toasts, 'c1', true);
    expect(a.toasts.showError).toHaveBeenCalledWith(
      'Failed to queue memory extraction: Chat has no messages',
    );
    const b = rig(async () => {
      throw new Error('offline');
    });
    await confirmAndQueueMemories(b.core, b.toasts, 'c1', true);
    expect(b.toasts.showError).toHaveBeenCalledWith('Failed to queue memory extraction');
  });

  it('DIVERGENCE (recorded): a transport failure is the synthetic CoreDispatchError, so it reads the colon form where v4 reads the bare toast', async () => {
    const lost = new CoreDispatchError({
      kind: 'internal',
      message: 'Connection lost. The server may still be starting.',
    });
    const q = rig(async () => {
      throw lost;
    });
    await confirmAndQueueMemories(q.core, q.toasts, 'c1', true);
    expect(q.toasts.showError).toHaveBeenCalledWith(
      'Failed to queue memory extraction: Connection lost. The server may still be starting.',
    );
    const d = rig(async (req) => {
      if (req.type === 'memoryCountByChat') return { memoryCount: 2 };
      throw lost;
    });
    await confirmAndDeleteChatMemories(d.core, d.toasts, 'c1', 2);
    expect(d.toasts.showError).toHaveBeenCalledWith(
      'Failed to delete memories: Connection lost. The server may still be starting.',
    );
  });
});

describe('confirmAndDeleteChatMemories (Delete Memories (n))', () => {
  let confirm: ReturnType<typeof vi.spyOn>;
  beforeEach(() => {
    confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
  });
  afterEach(() => vi.restoreAllMocks());

  it('re-reads the count FIRST: a fresh zero toasts and never confirms, even over a stale rendered 5', async () => {
    const { core, toasts, calls } = rig(async () => ({ chatId: 'c1', memoryCount: 0 }));
    expect(await confirmAndDeleteChatMemories(core, toasts, 'c1', 5)).toBe(false);
    expect(calls).toEqual(['memoryCountByChat']);
    expect(toasts.showError).toHaveBeenCalledWith('This chat has no memories to delete');
    expect(confirm).not.toHaveBeenCalled();
  });

  it('confirms quoting the RE-READ count, deletes, and toasts the server deletedCount', async () => {
    const { core, toasts, calls } = rig(async (req) =>
      req.type === 'memoryCountByChat' ? { memoryCount: 9 } : { deletedCount: 9 },
    );
    expect(await confirmAndDeleteChatMemories(core, toasts, 'c1', 2)).toBe(true);
    expect(confirm).toHaveBeenCalledWith(
      'Delete all 9 memories created from this chat? This action cannot be undone.',
    );
    expect(calls).toEqual(['memoryCountByChat', 'memoryDeleteByChat']);
    expect(toasts.showSuccess).toHaveBeenCalledWith('Deleted 9 memories');
  });

  it('a failed probe falls through to the rendered count', async () => {
    const { core, toasts } = rig(async (req) => {
      if (req.type === 'memoryCountByChat') throw new Error('offline');
      return { deletedCount: 3 };
    });
    expect(await confirmAndDeleteChatMemories(core, toasts, 'c1', 3)).toBe(true);
    expect(confirm).toHaveBeenCalledWith(
      'Delete all 3 memories created from this chat? This action cannot be undone.',
    );
  });

  it('declined confirm deletes nothing', async () => {
    confirm.mockReturnValue(false);
    const { core, toasts, calls } = rig(async () => ({ memoryCount: 2 }));
    expect(await confirmAndDeleteChatMemories(core, toasts, 'c1', 2)).toBe(false);
    expect(calls).toEqual(['memoryCountByChat']);
    expect(toasts.showSuccess).not.toHaveBeenCalled();
  });

  it('failure toasts use the colon form, the catch the bare one', async () => {
    const a = rig(async (req) => {
      if (req.type === 'memoryCountByChat') return { memoryCount: 2 };
      throw refusal('locked');
    });
    await confirmAndDeleteChatMemories(a.core, a.toasts, 'c1', 2);
    expect(a.toasts.showError).toHaveBeenCalledWith('Failed to delete memories: locked');
    const b = rig(async (req) => {
      if (req.type === 'memoryCountByChat') return { memoryCount: 2 };
      throw new Error('offline');
    });
    await confirmAndDeleteChatMemories(b.core, b.toasts, 'c1', 2);
    expect(b.toasts.showError).toHaveBeenCalledWith('Failed to delete memories');
  });
});
