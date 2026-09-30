/**
 * The Salon sidebar's Edit Content memory pair — v4's `useMemoryActions`
 * (`app/salon/[id]/hooks/useMemoryActions.ts` at `97b25fc53`), one handler per
 * button. NOT the chat card's badge flow (`memory-reextract.api.ts`): that one
 * deletes first and then queues; the sidebar's **Re-extract Memories queues
 * ONLY**, and **Delete Memories (n)** deletes only.
 *
 * v4's `fetch` calls are the dispatch verbs that are the same server code
 * (`chatQueueMemories`, `memoryCountByChat`, `memoryDeleteByChat`); a non-ok
 * answer is a `CoreDispatchError` whose message is the server's `error` text.
 * The gate is `window.confirm`, the established idiom (`memory-reextract.api.ts`).
 *
 * @module screens/salon/memory-sidebar.api
 */

import type { CoreClient } from '../../core/core-client';
import { CoreDispatchError } from '../../core/core-contract';
import { notifyQueueChange } from '../../layout/queue-status.logic';
import type { ReextractToasts } from './memory-reextract.api';

/** v4 `useMemoryActions.ts:68`. */
export const NO_ACTIVE_CHARACTER_MESSAGE = 'Cannot re-extract memories: no active character in chat';
/** v4 `useMemoryActions.ts:73`. */
export const QUEUE_CONFIRM =
  'Queue memory extraction jobs for all messages in this chat? This will process the entire conversation history.';

/**
 * Re-extract Memories (v4 `handleReextractMemories`, `:65-101`): guard on an
 * active character, confirm, queue the extraction (NO delete), toast, wake the
 * queue badges. Answers whether the extraction was queued.
 */
export async function confirmAndQueueMemories(
  core: CoreClient,
  toasts: ReextractToasts,
  chatId: string,
  hasActiveCharacter: boolean,
): Promise<boolean> {
  if (!hasActiveCharacter) {
    toasts.showError(NO_ACTIVE_CHARACTER_MESSAGE);
    return false;
  }
  if (!window.confirm(QUEUE_CONFIRM)) return false;
  try {
    const queued = await core.dispatchData({ type: 'chatQueueMemories', chatId });
    toasts.showSuccess(`Queued ${queued['jobCount']} memory extraction jobs`);
    notifyQueueChange();
    return true;
  } catch (err) {
    toasts.showError(
      err instanceof CoreDispatchError
        ? `Failed to queue memory extraction: ${err.message}`
        : 'Failed to queue memory extraction',
    );
    return false;
  }
}

/**
 * Delete Memories (n) (v4 `handleDeleteChatMemories`, `:17-63`): re-read the
 * live count (a dropped socket can leave a stale rendered zero), toast at zero,
 * confirm quoting the RE-READ count, delete, toast the server's own
 * `deletedCount`. A failed probe falls through to the rendered count. Answers
 * whether memories were deleted (the caller's cue to refresh the count).
 */
export async function confirmAndDeleteChatMemories(
  core: CoreClient,
  toasts: ReextractToasts,
  chatId: string,
  renderedCount: number,
): Promise<boolean> {
  let count = renderedCount;
  try {
    const probe = await core.dispatchData({ type: 'memoryCountByChat', chatId });
    count = Number(probe['memoryCount']) || 0;
  } catch {
    // A failed probe is not a reason to refuse a delete the user asked for.
  }
  if (count === 0) {
    toasts.showError('This chat has no memories to delete');
    return false;
  }
  if (!window.confirm(`Delete all ${count} memories created from this chat? This action cannot be undone.`)) {
    return false;
  }
  try {
    const res = await core.dispatchData({ type: 'memoryDeleteByChat', chatId });
    toasts.showSuccess(`Deleted ${res['deletedCount']} memories`);
    return true;
  } catch (err) {
    toasts.showError(
      err instanceof CoreDispatchError
        ? `Failed to delete memories: ${err.message}`
        : 'Failed to delete memories',
    );
    return false;
  }
}
